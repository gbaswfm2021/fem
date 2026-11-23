//! In-memory, declaration-driven frequency-domain scalar P1 simplex driver.
//!
//! This numerical layer accepts a prepared case and periodic mapping; generic
//! request orchestration and result publication are handled separately.
use crate::coefficient_expr::CoefficientExpressions;
use crate::model_expression::Scalar;
use crate::nodal_p1::{self, SimplexBoundaryPoint, SimplexCell, SimplexVolumePoint};
use crate::prepared_case::{CaseKind, PreparedCase};
use crate::scenario_plan::ScenarioPlan;
use crate::simplex_topology::{
    self, Facet, SelectedFacetDuplicates, SelectedFacetOwnership, SelectedFacetPolicy,
    SimplexTopology, TopologyPolicy,
};
use crate::{SolverCache, C};
use serde_json::Value;
use std::collections::HashMap;

pub(crate) enum Mesh<'a> {
    Triangle {
        nodes: &'a [[f64; 3]],
        cells: &'a [([usize; 3], usize)],
        facets: &'a [([usize; 2], usize)],
    },
    Tetrahedron {
        nodes: &'a [[f64; 3]],
        cells: &'a [([usize; 4], usize)],
        facets: &'a [([usize; 3], usize)],
    },
}

pub(crate) struct FrequencyResult {
    pub(crate) solution: nodal_p1::Solution,
    pub(crate) recovered_nodes: Vec<[C; 3]>,
    pub(crate) longitudinal_wavenumber: C,
}

enum FrequencyExecution {
    Solved(FrequencyResult),
    Assembled(nodal_p1::LinearSystem),
}

struct Policy {
    topology: TopologyPolicy,
    volume_inputs: Vec<String>,
    derived: Option<CoefficientExpressions>,
    nonzero: Vec<String>,
    boundary_quadrature: String,
    boundary_constants: HashMap<String, C>,
}

impl Policy {
    fn from_package(package: &Value, formulation: &str) -> Result<Self, String> {
        let root = package
            .get("preparedFrequencyScalar")
            .and_then(|v| v.get(formulation))
            .ok_or("preparedFrequencyScalar formulation policy is required.")?;
        let topology = root
            .get("topology")
            .ok_or("Frequency scalar topology policy is required.")?;
        let flag = |key: &str| {
            topology
                .get(key)
                .and_then(Value::as_bool)
                .ok_or_else(|| format!("topology.{key} must be boolean."))
        };
        let tol = topology
            .get("determinantTolerance")
            .ok_or("A determinant tolerance is required.")?;
        let threshold = tol
            .get("value")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.)
            .ok_or("Determinant tolerance value must be finite and nonnegative.")?;
        let reject_equal = tol
            .get("rejectEqual")
            .and_then(Value::as_bool)
            .ok_or("determinantTolerance.rejectEqual must be boolean.")?;
        let determinant_tolerance = match tol.get("kind").and_then(Value::as_str) {
            Some("absolute") => simplex_topology::DeterminantTolerance::Absolute {
                minimum: threshold,
                reject_equal,
            },
            Some("relative") => simplex_topology::DeterminantTolerance::Relative {
                fraction: threshold,
                reject_equal,
            },
            _ => return Err("Unsupported determinant tolerance policy.".into()),
        };
        let names = |key: &str, required: bool| -> Result<Vec<String>, String> {
            let Some(value) = root.get(key) else {
                return if required {
                    Err(format!("{key} is required."))
                } else {
                    Ok(Vec::new())
                };
            };
            let mut seen = std::collections::HashSet::new();
            value
                .as_array()
                .filter(|v| !required || !v.is_empty())
                .ok_or_else(|| format!("{key} must be an array."))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| format!("{key} entries must be nonempty strings."))
                })
                .map(|v| {
                    v.and_then(|name| {
                        if seen.insert(name.to_owned()) {
                            Ok(name.to_owned())
                        } else {
                            Err(format!("{key} contains duplicate {name:?}."))
                        }
                    })
                })
                .collect()
        };
        let boundary_quadrature = root
            .get("boundaryQuadrature")
            .and_then(Value::as_str)
            .filter(|v| matches!(*v, "gauss5" | "triangle3"))
            .ok_or("Unsupported frequency boundary quadrature.")?
            .to_string();
        let mut boundary_constants = HashMap::new();
        if let Some(constants) = root.get("boundaryConstants") {
            for (name, value) in constants
                .as_object()
                .ok_or("boundaryConstants must be an object.")?
            {
                let value = value
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or("Boundary constant must be finite.")?;
                if name == "omega" || name == "frequency" {
                    return Err("Boundary constant shadows a frequency symbol.".into());
                }
                boundary_constants.insert(name.clone(), C { re: value, im: 0. });
            }
        }
        Ok(Self {
            topology: TopologyPolicy {
                reject_duplicate_cells: flag("rejectDuplicateCells")?,
                reject_non_manifold_facets: flag("rejectNonManifoldFacets")?,
                require_all_nodes_used: flag("requireAllNodesUsed")?,
                require_nonempty_cells: flag("requireNonemptyCells")?,
                determinant_tolerance: Some(determinant_tolerance),
            },
            volume_inputs: names("volumeInputs", true)?,
            nonzero: names("nonzeroInputs", false)?,
            boundary_quadrature,
            boundary_constants,
            derived: root
                .get("derivedInputs")
                .map(CoefficientExpressions::from_json)
                .transpose()?,
        })
    }
}

fn sample_values(
    bindings: &crate::material_bindings::MaterialBindings,
    policy: &Policy,
    material: &HashMap<String, Scalar>,
    parameters: &HashMap<String, Scalar>,
    frequency: f64,
    point: [f64; 3],
    path: &str,
) -> Result<HashMap<String, C>, String> {
    let context = crate::scalar2d::symbols_at_point(parameters, frequency, point);
    let bound = bindings.evaluate(material, &context, path)?;
    let mut values: HashMap<String, C> = context
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                C {
                    re: value.value,
                    im: 0.,
                },
            )
        })
        .collect();
    values.extend(bound.into_iter().map(|(name, value)| {
        (
            name,
            C {
                re: value.value,
                im: 0.,
            },
        )
    }));
    if let Some(derived) = &policy.derived {
        for (name, value) in derived.evaluate(&values)? {
            if values.insert(name.clone(), value).is_some() {
                return Err(format!(
                    "Derived input {name:?} shadows an existing symbol."
                ));
            }
        }
    }
    for name in &policy.nonzero {
        let value = values
            .get(name)
            .ok_or_else(|| format!("Nonzero input {name:?} is missing."))?;
        if value.re == 0. && value.im == 0. {
            return Err(format!("Input {name:?} must be nonzero."));
        }
    }
    Ok(values)
}

pub(crate) fn solve(
    case: &PreparedCase,
    package: &Value,
    model: &Value,
    map: &Value,
    mesh: Mesh<'_>,
    parameters: &HashMap<String, Scalar>,
    periodic: Option<&crate::scalar2d::PeriodicDofMap>,
    cache: &mut SolverCache,
) -> Result<FrequencyResult, String> {
    match execute(case,package,model,map,mesh,parameters,periodic,cache,false)? {
        FrequencyExecution::Solved(result)=>Ok(result),
        FrequencyExecution::Assembled(_)=>Err("Frequency scalar solve returned an assembly-only result.".into()),
    }
}

/// Assemble the selected scalar frequency operator into sparse master rows
/// without solving it. This is shared by the propagation-mode adapter, which
/// samples the declared quadratic longitudinal operator at three wavenumbers.
pub(crate) fn assemble(
    case: &PreparedCase,
    package: &Value,
    model: &Value,
    map: &Value,
    mesh: Mesh<'_>,
    parameters: &HashMap<String, Scalar>,
    periodic: Option<&crate::scalar2d::PeriodicDofMap>,
    cache: &mut SolverCache,
) -> Result<nodal_p1::LinearSystem, String> {
    match execute(case,package,model,map,mesh,parameters,periodic,cache,true)? {
        FrequencyExecution::Assembled(system)=>Ok(system),
        FrequencyExecution::Solved(_)=>Err("Frequency scalar assembly unexpectedly solved the operator.".into()),
    }
}

fn execute(
    case: &PreparedCase,
    package: &Value,
    model: &Value,
    map: &Value,
    mesh: Mesh<'_>,
    parameters: &HashMap<String, Scalar>,
    periodic: Option<&crate::scalar2d::PeriodicDofMap>,
    cache: &mut SolverCache,
    assemble_only: bool,
) -> Result<FrequencyExecution, String> {
    let frequency = match &case.kind {
        CaseKind::Frequency { frequency_hz, .. }
            if frequency_hz.is_finite() && *frequency_hz > 0. =>
        {
            *frequency_hz
        }
        _ => return Err("Frequency scalar driver requires one positive frequency case.".into()),
    };
    if package.get("id").and_then(Value::as_str) != Some(case.profile.as_str()) {
        return Err("Prepared case and package identities differ.".into());
    }
    if case.settings.element_order != 1 {
        return Err("Frequency scalar driver supports first-order elements only.".into());
    }
    let mut plan = ScenarioPlan::from_package(package, &case.settings.formulation)?;
    let longitudinal_wavenumber = crate::longitudinal::prescribed_wavenumber(
        package, model, &case.study_id, &case.settings.formulation, parameters,
    )?;
    if let CaseKind::Frequency { case: prepared_case, .. } = &case.kind {
        crate::longitudinal::validate_prepared_case_value(prepared_case, longitudinal_wavenumber)?;
    }
    plan.set_longitudinal_wavenumber(longitudinal_wavenumber)?;
    if plan.volume.shape().1 != 1 || !plan.coefficient_matrices.is_empty() {
        return Err(
            "Frequency scalar driver requires one scalar field and scalar coefficients.".into(),
        );
    }
    let policy = Policy::from_package(package, &case.settings.formulation)?;
    crate::solver_request::validate_study_reference(model, &case.request)?;
    match mesh {
        Mesh::Triangle {
            nodes,
            cells,
            facets,
        } => {
            plan.volume.require_discretization(
                crate::discretization::Cell::Triangle,
                crate::discretization::Family::Lagrange,
                1,
                1,
            )?;
            if policy.boundary_quadrature != "gauss5" {
                return Err("Triangle facets require gauss5 quadrature.".into());
            }
            let connectivity = cells.iter().map(|(cell, _)| *cell).collect::<Vec<_>>();
            let topology = simplex_topology::triangles(nodes, &connectivity, &[], policy.topology)?;
            let quadrature = crate::quadrature::triangle(&case.settings.quadrature)?
                .into_iter()
                .map(|q| ([q[0], q[1], q[2]], q[3]))
                .collect();
            solve_simplex::<3, 2, 2>(
                case,
                package,
                model,
                map,
                nodes,
                cells,
                facets,
                parameters,
                periodic,
                cache,
                frequency,
                longitudinal_wavenumber,
                &plan,
                &policy,
                &topology,
                quadrature,
                &|facet| simplex_topology::edge_normal(nodes, facet, 0),
                assemble_only,
            )
        }
        Mesh::Tetrahedron {
            nodes,
            cells,
            facets,
        } => {
            plan.volume.require_discretization(
                crate::discretization::Cell::Tetrahedron,
                crate::discretization::Family::Lagrange,
                1,
                1,
            )?;
            if policy.boundary_quadrature != "triangle3" {
                return Err("Tetrahedron facets require triangle3 quadrature.".into());
            }
            let connectivity = cells.iter().map(|(cell, _)| *cell).collect::<Vec<_>>();
            let topology =
                simplex_topology::tetrahedra(nodes, &connectivity, &[], policy.topology)?;
            let quadrature = crate::quadrature::tetrahedron(&case.settings.quadrature)?
                .into_iter()
                .map(|q| ([q[0], q[1], q[2], q[3]], q[4]))
                .collect();
            solve_simplex::<4, 3, 3>(
                case,
                package,
                model,
                map,
                nodes,
                cells,
                facets,
                parameters,
                periodic,
                cache,
                frequency,
                longitudinal_wavenumber,
                &plan,
                &policy,
                &topology,
                quadrature,
                &|facet| simplex_topology::face_normal(nodes, facet, 0),
                assemble_only,
            )
        }
    }
}

fn solve_simplex<const N: usize, const D: usize, const F: usize>(
    case: &PreparedCase,
    package: &Value,
    model: &Value,
    map: &Value,
    nodes: &[[f64; 3]],
    cells: &[([usize; N], usize)],
    facets: &[([usize; F], usize)],
    parameters: &HashMap<String, Scalar>,
    periodic: Option<&crate::scalar2d::PeriodicDofMap>,
    cache: &mut SolverCache,
    frequency: f64,
    longitudinal_wavenumber: C,
    plan: &ScenarioPlan,
    policy: &Policy,
    topology: &SimplexTopology<F>,
    quadrature: Vec<([f64; N], f64)>,
    normal: &dyn Fn(&Facet<F>) -> Result<([f64; 3], f64), String>,
    assemble_only: bool,
) -> Result<FrequencyExecution, String> {
    if map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6")
        || map.get("dimension").and_then(Value::as_u64) != Some(D as u64)
    {
        return Err("Mesh map schema or dimension does not match the simplex element.".into());
    }
    let mesh_tags = HashMap::from([
        ("domainSources".into(), cells.iter().map(|(_, tag)| *tag).collect()),
        (
            "materials".into(),
            cells.iter().map(|(_, tag)| *tag).collect(),
        ),
        (
            "boundaries".into(),
            facets.iter().map(|(_, tag)| *tag).collect(),
        ),
    ]);
    let selection_plan = plan.selections.as_ref().ok_or("Scenario selectionPlan is required.")?;
    let source_by_cell = selection_plan.uses_cell_map("domainSources", map);
    let selected = selection_plan.bind(model, map, &mesh_tags)?;
    let source_data = crate::domain_source::prepare_routes(&plan.domain_sources, selected.get("domainSources").map(Vec::as_slice))?;
    let mut source_samples = source_data.iter().map(|_| Vec::with_capacity(cells.len())).collect::<Vec<Vec<Vec<Vec<C>>>>>();
    let material_rows = selected
        .get("materials")
        .ok_or("Selection plan must declare materials.")?
        .iter()
        .map(|row| (row.tag, row.row.clone()))
        .collect::<HashMap<_, _>>();
    let compiled =
        crate::material::CompiledMaterialAssignments::compile(&case.profile, &material_rows)?;
    let constant_values = compiled.position_independent_values(parameters, frequency)?;
    let bindings = plan
        .material_bindings
        .as_ref()
        .ok_or("Scenario materialBindings are required.")?;
    let mut generic_cells = Vec::with_capacity(cells.len());
    let mut recovery_symbols = Vec::with_capacity(cells.len());
    for (index, (cell, tag)) in cells.iter().enumerate() {
        if cell.iter().any(|&node| node >= nodes.len()) {
            return Err(format!("Cell {index} references a missing node."));
        }
        let positions = quadrature
            .iter()
            .map(|(basis, _)| {
                std::array::from_fn(|axis| {
                    (0..N).map(|i| basis[i] * nodes[cell[i]][axis]).sum::<f64>()
                })
            })
            .collect::<Vec<[f64; 3]>>();
        let samples = if let Some(values) = constant_values.get(tag) {
            vec![values.clone(); positions.len()]
        } else {
            compiled.evaluate_batch(
                *tag,
                parameters,
                frequency,
                &positions,
                &format!("cell {index} quadrature"),
            )?
        };
        let mut points = Vec::with_capacity(quadrature.len());
        let mut loads = source_data.iter().map(|_| Vec::with_capacity(quadrature.len())).collect::<Vec<Vec<Vec<C>>>>();
        for (i, ((basis, weight), position)) in quadrature.iter().zip(&positions).enumerate() {
            let values = sample_values(
                bindings,
                policy,
                &samples[i],
                parameters,
                frequency,
                *position,
                &format!("cell {index} quadrature {i}"),
            )?;
            for (route_index, (_, load_plan, sources)) in source_data.iter().enumerate() {
                let source_key = if source_by_cell { index } else { *tag };
                loads[route_index].push(crate::domain_source::sample_load(load_plan, sources, source_key, *position, parameters, Some(frequency), &values)?);
            }
            let symbols = policy
                .volume_inputs
                .iter()
                .map(|name| {
                    values
                        .get(name)
                        .copied()
                        .ok_or_else(|| format!("Volume input {name:?} is missing."))
                })
                .collect::<Result<Vec<_>, _>>()?;
            points.push(SimplexVolumePoint {
                basis: *basis,
                weight: *weight,
                symbols,
            });
        }
        let centroid = std::array::from_fn(|axis| {
            cell.iter().map(|&node| nodes[node][axis]).sum::<f64>() / N as f64
        });
        let raw = if let Some(values) = constant_values.get(tag) {
            values.clone()
        } else {
            compiled.evaluate(
                *tag,
                parameters,
                frequency,
                centroid,
                &format!("cell {index} centroid"),
            )?
        };
        recovery_symbols.push(sample_values(
            bindings,
            policy,
            &raw,
            parameters,
            frequency,
            centroid,
            &format!("cell {index} centroid"),
        )?);
        generic_cells.push(SimplexCell {
            nodes: *cell,
            quadrature: points,
        });
        for (all, loads) in source_samples.iter_mut().zip(loads) { all.push(loads); }
    }
    let selected_boundaries = selected
        .get("boundaries")
        .ok_or("Selection plan must declare boundaries.")?;
    let inputs = crate::declared_boundary::compile_rows(
        package,
        &case.settings.formulation,
        selected_boundaries.iter().map(|row| (row.tag, row.row)),
    )?;
    let tags = selected_boundaries
        .iter()
        .map(|row| row.tag)
        .collect::<Vec<_>>();
    let selected_facets = facets
        .iter()
        .copied()
        .filter(|(_, tag)| tags.contains(tag))
        .collect::<Vec<_>>();
    topology.bind_selected_facets(
        &selected_facets,
        &tags,
        SelectedFacetPolicy {
            ownership: SelectedFacetOwnership::ExteriorOnly,
            duplicates: SelectedFacetDuplicates::KeepAll,
            require_positive_tag: false,
        },
    )?;
    let mut essential_plan: Option<&crate::essential_form::EssentialFormPlan> = None;
    let mut essential = Vec::new();
    let mut boundaries: Vec<SimplexBoundaryPoint<'_, F, D>> = Vec::new();
    let mut coefficient_inputs = policy.boundary_constants.clone();
    coefficient_inputs.insert(
        "omega".into(),
        C {
            re: std::f64::consts::TAU * frequency,
            im: 0.,
        },
    );
    coefficient_inputs.insert(
        "frequency".into(),
        C {
            re: frequency,
            im: 0.,
        },
    );
    for (facet_nodes, tag) in selected_facets {
        if let Some(periodic) = periodic {
            let mut edge = [0usize; 2];
            if F == 2 {
                edge.copy_from_slice(&facet_nodes[..2]);
                edge.sort_unstable();
            }
            if periodic.boundary_tags.contains(&tag)
                || F == 2 && periodic.boundary_edges.contains(&edge)
            {
                return Err(format!(
                    "Periodic boundary tag {tag} also has a physical condition."
                ));
            }
        }
        let input = inputs.get(&tag).ok_or("Missing compiled boundary input.")?;
        let facet = topology
            .facet(facet_nodes)
            .ok_or("Selected facet has no adjacent cell.")?;
        let (unit_normal, measure) = normal(facet)?;
        if let Some(key) = input.form_key() {
            let form = plan
                .boundaries
                .get(key)
                .ok_or_else(|| format!("Boundary form {key:?} is missing."))?;
            let points: Vec<([f64; F], f64)> = if D == 2 {
                let positions = [
                    -0.906179845938664,
                    -0.538469310105683,
                    0.,
                    0.538469310105683,
                    0.906179845938664,
                ];
                let weights = [
                    0.236926885056189,
                    0.478628670499366,
                    0.568888888888889,
                    0.478628670499366,
                    0.236926885056189,
                ];
                positions
                    .into_iter()
                    .zip(weights)
                    .map(|(position, weight)| {
                        let t = (position + 1.) * 0.5;
                        (
                            std::array::from_fn(|i| if i == 0 { 1. - t } else { t }),
                            weight * 0.5,
                        )
                    })
                    .collect()
            } else {
                (0..3)
                    .map(|major| {
                        (
                            std::array::from_fn(|i| if i == major { 2. / 3. } else { 1. / 6. }),
                            1. / 3.,
                        )
                    })
                    .collect()
            };
            for (basis, weight) in points {
                let point = std::array::from_fn(|axis| {
                    (0..F)
                        .map(|i| basis[i] * nodes[facet_nodes[i]][axis])
                        .sum::<f64>()
                });
                let symbols = crate::scalar2d::symbols_at_point(parameters, frequency, point);
                let values =
                    input.evaluate(&symbols, Some(point), &coefficient_inputs, Some(form))?;
                boundaries.push(SimplexBoundaryPoint {
                    nodes: facet_nodes,
                    basis,
                    normal: std::array::from_fn(|axis| unit_normal[axis]),
                    weight: measure * weight,
                    plan: form,
                    coefficients: values.coefficients,
                    sources: values.sources,
                });
            }
        } else {
            let row = selected_boundaries
                .iter()
                .find(|row| row.tag == tag)
                .ok_or("Boundary selection disappeared.")?;
            let kind = row
                .row
                .get("type")
                .and_then(Value::as_str)
                .ok_or("Boundary type is required.")?;
            let form = plan
                .essential
                .get(kind)
                .ok_or_else(|| format!("Essential form {kind:?} is missing."))?;
            if essential_plan.is_some_and(|old| !std::ptr::eq(old, form)) {
                return Err("Multiple essential forms are not supported by this driver.".into());
            }
            essential_plan = Some(form);
            for node in facet_nodes {
                let point = nodes[node];
                let symbols = crate::scalar2d::symbols_at_point(parameters, frequency, point);
                let value = input
                    .evaluate(&symbols, Some(point), &coefficient_inputs, None)?
                    .essential
                    .ok_or("Essential value is missing.")?;
                let values = form.point_values(value, None, "boundary point")?;
                if values.len() != 1 {
                    return Err("Scalar essential form must return one value.".into());
                }
                essential.push((node, values[0]));
            }
        }
    }
    let names = policy
        .volume_inputs
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let periodic_map = periodic.map(|map| nodal_p1::PeriodicMap {
        representative: &map.node_to_dof,
        weights: &map.node_weights,
        count: map.dof_count,
        zero_phase_tolerance: 0.,
        essential_mapping: nodal_p1::EssentialMapping::WeightedPhysical,
    });
    let problem = nodal_p1::SimplexProblem {
        nodes,
        cells: &generic_cells,
        topology: policy.topology,
        form: &plan.volume,
        symbol_names: &names,
        boundaries: &boundaries,
        essential: &essential,
        essential_plan,
        periodic: periodic_map,
        mean: None,
    };
    let load_routes = source_data.iter().zip(&source_samples)
        .map(|((_, load_plan, _), samples)| (&load_plan.form, samples.as_slice()))
        .collect::<Vec<_>>();
    if assemble_only {
        let system=problem.assemble_with_volume_load_routes(&mut cache.assembly,&load_routes)?;
        return Ok(FrequencyExecution::Assembled(system));
    }
    let solution = problem.solve_with_volume_load_routes(
        &nodal_p1::SolveOptions {
            backend: &case.settings.backend,
            ordering: &case.settings.ordering,
            threads: case.settings.threads,
            residual_tolerance: 1e-7,
        },
        cache,
        &load_routes,
    )?;
    let recovery = plan
        .recovery
        .as_ref()
        .ok_or("Frequency recovery is required.")?;
    let sampling = package
        .get("recoveries")
        .and_then(|v| v.get(&case.settings.formulation))
        .and_then(|v| v.get("sampling"))
        .and_then(Value::as_str)
        .ok_or("Recovery sampling is required.")?;
    if sampling != "cellCentroid" {
        return Err("Frequency recovery requires centroid material sampling.".into());
    }
    let recovered_nodes =
        problem.recover_nodes(&solution, recovery, sampling, &recovery_symbols)?;
    Ok(FrequencyExecution::Solved(FrequencyResult {
        solution,
        recovered_nodes,
        longitudinal_wavenumber,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    
    fn scalar(value: f64, unit: &str) -> Scalar {
        Scalar {
            value,
            dimension: crate::model_expression::unit(unit).unwrap().1,
        }
    }
    
    
    fn policy() -> Value {
        json!({"topology":{"rejectDuplicateCells":true,"rejectNonManifoldFacets":true,
        "requireAllNodesUsed":true,"requireNonemptyCells":true,
        "determinantTolerance":{"kind":"absolute","value":0.,"rejectEqual":true}},
        "volumeInputs":["coefficientA","coefficientB","omega"],"nonzeroInputs":["coefficientA","coefficientB"],"boundaryQuadrature":"gauss5"})
    }

    

    

    

    #[test]
    fn complex_material_derivation_is_data_driven_and_checks_nonzero() {
        let bindings = crate::material_bindings::MaterialBindings::from_json(&json!([
            {"target":"a","source":"a","unit":"1"},{"target":"b","source":"b","unit":"1"}]))
        .unwrap();
        let mut package = json!({"preparedFrequencyScalar":{"f":policy()}});
        package["preparedFrequencyScalar"]["f"]["derivedInputs"] =
            json!({"complexValue":{"complex":[{"symbol":"a"},{"multiply":[-1,{"symbol":"b"}]}]}});
        package["preparedFrequencyScalar"]["f"]["nonzeroInputs"] = json!(["complexValue"]);
        let policy = Policy::from_package(&package, "f").unwrap();
        let raw = HashMap::from([("a".into(), scalar(2., "1")), ("b".into(), scalar(3., "1"))]);
        let values = sample_values(
            &bindings,
            &policy,
            &raw,
            &HashMap::new(),
            1.,
            [0.; 3],
            "material",
        )
        .unwrap();
        assert_eq!(
            (values["complexValue"].re, values["complexValue"].im),
            (2., -3.)
        );
        let zero = HashMap::from([("a".into(), scalar(0., "1")), ("b".into(), scalar(0., "1"))]);
        assert!(sample_values(
            &bindings,
            &policy,
            &zero,
            &HashMap::new(),
            1.,
            [0.; 3],
            "material"
        )
        .is_err());
    }

    #[test]
    fn finite_kz_acoustic_uniform_plane_wave_has_expected_pressure_and_axial_velocity() {
        use crate::prepared_case::{Output, SolverSettings};
        let package: Value = serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let kz = 2.0_f64;
        let sound_speed = 343.0_f64;
        let density = 1.225_f64;
        let omega = sound_speed * kz;
        let frequency = omega / std::f64::consts::TAU;
        let quantity = |expression: &str, unit: &str| json!({"expression":expression,"unit":unit});
        let model = json!({"scenario":{"profileId":"acoustic-2d"},"model":{
            "parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],
            "mesh":{"periodicPairIds":[]},
            "study":[{"id":"harmonic","type":"frequencyDomain","physicsId":"wave",
                "longitudinalWavenumber":{"real":quantity("2","rad/m"),"imaginary":quantity("0","rad/m")}}],
            "materials":[{"id":"fluid","selectionId":"solid","density":quantity("1.225","kg/m^3"),
                "soundSpeed":quantity("343","m/s"),"lossFactor":"0"}],
            "physics":{"id":"wave","type":"acoustics","formulation":"pressure","dirichlet":{
                "id":"dirichlet","type":"dirichlet","selectionId":"outer","value":quantity("1","Pa")}}
        }});
        let map = json!({"schema":"fullmeta-mesh-map/6","dimension":2,
            "physicalGroups":[{"selectionId":"solid","physicalTag":1}],
            "boundarySelections":[{"selectionId":"outer","physicalTag":2}]});
        let nodes = [[0.,0.,0.],[1.,0.,0.],[1.,1.,0.],[0.,1.,0.],[0.5,0.5,0.]];
        let cells = [([0,1,4],1),([1,2,4],1),([2,3,4],1),([3,0,4],1)];
        let facets = [([0,1],2),([1,2],2),([2,3],2),([3,0],2)];
        let case = PreparedCase {request:json!({"studyId":"harmonic"}),profile:"acoustic-2d".into(),study_id:"harmonic".into(),
            kind:CaseKind::Frequency {frequency_hz:frequency,case:json!({"longitudinalWavenumber":{"real":kz,"imaginary":0.,"unit":"rad/m"}})},
            settings:SolverSettings {formulation:"pressure".into(),element_order:1,quadrature:"standard".into(),
                backend:"faer".into(),ordering:"amd".into(),threads:1},output:Output::Frequency {sweep_csv:"result.csv".into()},
            model_path:Default::default(),mesh_path:Default::default(),map_path:Default::default(),output_directory:Default::default()};
        let parameters = crate::model_expression::resolve_parameters(&[],&model).unwrap();
        let result = solve(&case,&package,&model,&map,Mesh::Triangle {nodes:&nodes,cells:&cells,facets:&facets},
            &parameters,None,&mut crate::SolverCache::default()).unwrap();
        assert!(result.solution.residual < 1e-7);
        let expected_vz = kz / (omega * density);
        for (index,value) in result.solution.real.iter().enumerate() {assert!((value-1.).abs()<1e-8,"pressure node {index}: {value}");}
        for (index,value) in result.solution.imaginary.iter().enumerate() {assert!(value.abs()<1e-8,"pressure imaginary node {index}: {value}");}
        for value in &result.recovered_nodes {
            assert!(value[0].re.hypot(value[0].im)<1e-8);
            assert!(value[1].re.hypot(value[1].im)<1e-8);
            assert!((value[2].re-expected_vz).hypot(value[2].im)<1e-8);
        }

        // Exercise the active finite-kz force-density route and its physical
        // units through the current FEM-local core runtime. A constant axial
        // force couples through the reduced test gradient and moves the
        // unconstrained interior pressure degree of freedom.
        let mut forced_model=model.clone();
        forced_model["model"]["physics"]["force"]=json!({"id":"force","name":"Axial body force",
            "type":"additionalDomainSource","selectionId":"solid","values":[
                quantity("0","N/m^3"),quantity("0","N/m^3"),quantity("1","N/m^3")
            ]});
        let forced=solve(&case,&package,&forced_model,&map,Mesh::Triangle {nodes:&nodes,cells:&cells,facets:&facets},
            &parameters,None,&mut crate::SolverCache::default()).unwrap();
        assert!(forced.solution.residual<1e-7);
        assert!((forced.solution.real[4]-1.).hypot(forced.solution.imaginary[4])>1e-6,
            "active axial force did not change the interior pressure: {:?}+i{:?}",
            forced.solution.real,forced.solution.imaginary);
    }
}
