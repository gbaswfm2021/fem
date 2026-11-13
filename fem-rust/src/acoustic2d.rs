use crate::field_output;
use crate::mesh2d::{self, Mesh2D, adjacent_material_tag, triangle_geometry};
use crate::model_expression;
use crate::scalar2d::{self, add_entry, quantity, resolve_from, selection_tags};
use crate::{C, SolverCache, solve_linear_system};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

type V3 = [f64; 3];

#[cfg(test)]
mod floquet_tests {
    use super::*;
    use crate::solver_common::{c_abs, c_mul};

    #[test]
    fn acoustic_scalar_solve_preserves_nonzero_floquet_trace() {
        let mesh = Mesh2D {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            triangles: vec![([0, 1, 2], 1), ([1, 3, 2], 1)],
            boundary_edges: vec![([0, 2], 1), ([1, 3], 2), ([0, 1], 3), ([2, 3], 4)],
            integration_edges: vec![],
        };
        let phase = C { re: 0.0, im: -1.0 };
        let periodic = scalar2d::PeriodicDofMap {
            node_to_dof: vec![0, 0, 1, 1],
            node_weights: vec![C { re: 1.0, im: 0.0 }, phase, C { re: 1.0, im: 0.0 }, phase],
            dof_count: 2,
            boundary_tags: HashSet::from([1, 2]),
            boundary_edges: HashSet::from([[0, 2], [1, 3]]),
        };
        let impedance = json!({"expression":"1","unit":"Pa*s/m"});
        let boundaries = HashMap::from([
            (
                3,
                BoundaryCondition::Impedance {
                    impedance: impedance.clone(),
                    incident: Some(json!({"expression":"1","unit":"Pa"})),
                },
            ),
            (
                4,
                BoundaryCondition::Impedance {
                    impedance,
                    incident: None,
                },
            ),
        ]);
        let quadrature_materials = vec![
            vec![
                AcousticMaterial {
                    density: 1.0,
                    sound_speed: 1.0
                };
                crate::TRI_Q.len()
            ];
            2
        ];
        let (matrix, rhs_re, rhs_im, constraints) = assemble(
            &mesh,
            &quadrature_materials,
            "standard",
            &boundaries,
            0.1,
            &HashMap::new(),
            &periodic,
        )
        .unwrap();
        assert!(constraints.is_empty());
        let (reduced, real, imaginary) =
            scalar2d::reduce_system(&matrix, &rhs_re, &rhs_im, &periodic).unwrap();
        let (solution_re, solution_im, residual) = solve_linear_system(
            &reduced,
            &real,
            &imaginary,
            1e-10,
            "faer",
            "amd",
            1,
            &mut SolverCache::default(),
        )
        .unwrap();
        assert!(residual < 1e-10);
        let (full_re, full_im) =
            scalar2d::expand_complex_solution(&solution_re, &solution_im, &periodic).unwrap();
        for (lower, upper) in [(0, 1), (2, 3)] {
            let expected = c_mul(
                phase,
                C {
                    re: full_re[lower],
                    im: full_im[lower],
                },
            );
            assert!((expected.re - full_re[upper]).hypot(expected.im - full_im[upper]) < 1e-12);
        }
        assert!(
            c_abs(C {
                re: full_re[0],
                im: full_im[0]
            }) > 1e-3
        );
    }
}

#[derive(Clone, Copy)]
pub(super) struct AcousticMaterial {
    pub(super) density: f64,
    pub(super) sound_speed: f64,
}

pub(super) fn acoustic_material_from_values(
    values: &HashMap<String, model_expression::Scalar>,
) -> Result<AcousticMaterial, String> {
    let density = values
        .get("rho")
        .ok_or("Scenario material catalog omits rho.")?
        .value;
    let sound_speed = values
        .get("c")
        .ok_or("Scenario material catalog omits c.")?
        .value;
    if let Some(loss) = values.get("lossFactor") {
        if loss.value.abs() > 1e-14 {
            return Err(
                "Nonzero acoustic lossFactor is not implemented in the pressure solver.".into(),
            );
        }
    }
    if !density.is_finite() || density == 0.0 || !sound_speed.is_finite() || sound_speed == 0.0 {
        return Err("Acoustic density and sound speed must be finite and nonzero because the pressure equation divides by them.".into());
    }
    Ok(AcousticMaterial {
        density,
        sound_speed,
    })
}

fn triangle_quadrature(rule: &str) -> Result<Vec<[f64; 4]>, String> {
    match rule {
        "standard" => Ok(crate::TRI_Q.to_vec()),
        "centroid" => Ok(vec![[1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 1.0]]),
        _ => Err(format!(
            "Unknown constitutive quadrature rule {rule:?}; use 'standard' or 'centroid'."
        )),
    }
}

#[derive(Clone)]
pub(super) enum BoundaryCondition {
    Dirichlet(Value),
    Neumann(Value),
    Impedance {
        impedance: Value,
        incident: Option<Value>,
    },
}

pub(super) fn compile_incident_fields(
    boundaries: &HashMap<usize, BoundaryCondition>,
) -> Result<HashMap<usize, model_expression::CompiledComplexQuantity>, String> {
    let mut fields = HashMap::new();
    for (tag, condition) in boundaries {
        if let BoundaryCondition::Impedance {
            incident: Some(expression),
            ..
        } = condition
        {
            fields.insert(
                *tag,
                model_expression::CompiledComplexQuantity::compile(
                    expression,
                    &format!("boundary[{tag}].incidentField"),
                )?,
            );
        }
    }
    Ok(fields)
}

fn recover_nodal_velocity(
    mesh: &Mesh2D,
    materials: &[AcousticMaterial],
    pressure_real: &[f64],
    pressure_imaginary: &[f64],
    omega: f64,
) -> Result<Vec<[C; 3]>, String> {
    let mut sums = vec![[C::default(); 3]; mesh.nodes.len()];
    let mut counts = vec![0; mesh.nodes.len()];
    for (element, (triangle, material_tag)) in mesh.triangles.iter().enumerate() {
        let material = materials.get(element).ok_or_else(|| format!("Mesh triangle uses acoustic material tag {material_tag}, which has no acoustic material assignment."))?;
        let points = triangle.map(|node| mesh.nodes[node]);
        let (_, gradients_2d) = triangle_geometry(points)?;
        let gradients = gradients_2d.map(|gradient| [gradient[0], gradient[1], 0.0]);
        let pressure_gradient = field_output::p1_complex_gradient(
            triangle,
            &gradients,
            pressure_real,
            pressure_imaginary,
        );
        // The acoustic solvers use exp(+i omega t): v = i grad(p)/(omega rho).
        let velocity =
            field_output::acoustic_particle_velocity(pressure_gradient, omega, material.density);
        field_output::accumulate_cell_vector(&mut sums, &mut counts, triangle, velocity)?;
    }
    field_output::average_nodal_vectors(&mut sums, &counts);
    Ok(sums)
}

fn edge_expression_integrals(
    points: [V3; 2],
    expression: &Value,
    parameters: &HashMap<String, model_expression::Scalar>,
    frequency_hz: f64,
    path: &str,
) -> Result<[f64; 2], String> {
    const POSITIONS: [f64; 5] = [
        -0.906179845938664,
        -0.538469310105683,
        0.0,
        0.538469310105683,
        0.906179845938664,
    ];
    const WEIGHTS: [f64; 5] = [
        0.236926885056189,
        0.478628670499366,
        0.568888888888889,
        0.478628670499366,
        0.236926885056189,
    ];
    let length = (points[1][0] - points[0][0]).hypot(points[1][1] - points[0][1]);
    let mut integrals = [0.0; 2];
    for (position, weight) in POSITIONS.into_iter().zip(WEIGHTS) {
        let t = (position + 1.0) * 0.5;
        let point = [
            points[0][0] + t * (points[1][0] - points[0][0]),
            points[0][1] + t * (points[1][1] - points[0][1]),
            0.0,
        ];
        let symbols = scalar2d::symbols_at_point(parameters, frequency_hz, point);
        let value = scalar2d::quantity(expression, &symbols, path)?;
        let weighted_length = length * weight * 0.5;
        integrals[0] += weighted_length * (1.0 - t) * value;
        integrals[1] += weighted_length * t * value;
    }
    Ok(integrals)
}

fn assemble(
    mesh: &Mesh2D,
    quadrature_materials: &[Vec<AcousticMaterial>],
    constitutive_quadrature: &str,
    boundaries: &HashMap<usize, BoundaryCondition>,
    frequency_hz: f64,
    parameters: &HashMap<String, model_expression::Scalar>,
    periodic: &crate::scalar2d::PeriodicDofMap,
) -> Result<
    (
        Vec<BTreeMap<usize, C>>,
        Vec<f64>,
        Vec<f64>,
        HashMap<usize, C>,
    ),
    String,
> {
    let n = mesh.nodes.len();
    let omega = std::f64::consts::TAU * frequency_hz;
    let mut matrix = vec![BTreeMap::<usize, C>::new(); n];
    let mut rhs_re = vec![0.0; n];
    let mut rhs_im = vec![0.0; n];
    let mut dirichlet = HashMap::<usize, C>::new();
    let quadrature = triangle_quadrature(constitutive_quadrature)?;
    let incident_fields = compile_incident_fields(boundaries)?;

    for (element, (triangle, material_tag)) in mesh.triangles.iter().enumerate() {
        let element_quadrature = quadrature_materials.get(element).ok_or_else(|| format!("Mesh triangle uses material tag {material_tag}, which has no constitutive quadrature materials."))?;
        let points = triangle.map(|node| mesh.nodes[node]);
        let (area, gradients) = triangle_geometry(points)?;
        for (q_index, q) in quadrature.iter().enumerate() {
            let material = element_quadrature.get(q_index).ok_or(
                "Constitutive quadrature material count does not match the integration rule.",
            )?;
            let reaction =
                -omega * omega / (material.density * material.sound_speed * material.sound_speed);
            let weighted_area = area * q[3];
            for i in 0..3 {
                for j in 0..3 {
                    let gradient_product =
                        gradients[i][0] * gradients[j][0] + gradients[i][1] * gradients[j][1];
                    let coefficient = weighted_area
                        * (gradient_product / material.density + reaction * q[i] * q[j]);
                    add_entry(
                        &mut matrix,
                        triangle[i],
                        triangle[j],
                        C {
                            re: coefficient,
                            im: 0.0,
                        },
                    );
                }
            }
        }
    }

    let mut used_boundary_tags = HashSet::new();
    for (edge, tag) in &mesh.boundary_edges {
        if periodic.boundary_tags.contains(tag) {
            if boundaries.contains_key(tag) {
                return Err(format!(
                    "Acoustic boundary tag {tag} is periodic and cannot also carry a physical boundary condition."
                ));
            }
            continue;
        }
        let Some(condition) = boundaries.get(tag) else {
            continue;
        };
        used_boundary_tags.insert(*tag);
        let p0 = mesh.nodes[edge[0]];
        let p1 = mesh.nodes[edge[1]];
        let length = ((p1[0] - p0[0]).powi(2) + (p1[1] - p0[1]).powi(2)).sqrt();
        if length <= 0.0 || !length.is_finite() {
            return Err("The boundary mesh contains a degenerate edge.".into());
        }
        match condition {
            BoundaryCondition::Dirichlet(value) => {
                for node in edge {
                    let point = mesh.nodes[*node];
                    let symbols = scalar2d::symbols_at_point(parameters, frequency_hz, point);
                    let value = quantity(value, &symbols, &format!("boundary[{tag}].value"))?;
                    scalar2d::insert_dirichlet_constraint(
                        &mut dirichlet,
                        periodic,
                        *node,
                        C { re: value, im: 0.0 },
                    )?;
                }
            }
            BoundaryCondition::Neumann(value) => {
                let integrals = edge_expression_integrals(
                    [p0, p1],
                    value,
                    parameters,
                    frequency_hz,
                    &format!("boundary[{tag}].value"),
                )?;
                for local in 0..2 {
                    rhs_im[edge[local]] -= omega * integrals[local];
                }
            }
            BoundaryCondition::Impedance { impedance, .. } => {
                const POSITIONS: [f64; 5] = [
                    -0.906179845938664,
                    -0.538469310105683,
                    0.0,
                    0.538469310105683,
                    0.906179845938664,
                ];
                const WEIGHTS: [f64; 5] = [
                    0.236926885056189,
                    0.478628670499366,
                    0.568888888888889,
                    0.478628670499366,
                    0.236926885056189,
                ];
                for (position, weight) in POSITIONS.into_iter().zip(WEIGHTS) {
                    let t = (position + 1.0) * 0.5;
                    let point = [
                        p0[0] + t * (p1[0] - p0[0]),
                        p0[1] + t * (p1[1] - p0[1]),
                        0.0,
                    ];
                    let symbols = scalar2d::symbols_at_point(parameters, frequency_hz, point);
                    let impedance =
                        quantity(impedance, &symbols, &format!("boundary[{tag}].impedance"))?;
                    if !impedance.is_finite() || impedance == 0.0 {
                        return Err(format!(
                            "Acoustic impedance on boundary tag {tag} must be finite and nonzero at every integration point."
                        ));
                    }
                    let scale = omega / impedance;
                    let weighted_length = length * weight * 0.5;
                    let shape = [1.0 - t, t];
                    for i in 0..2 {
                        for j in 0..2 {
                            add_entry(
                                &mut matrix,
                                edge[i],
                                edge[j],
                                C {
                                    re: 0.0,
                                    im: scale * weighted_length * shape[i] * shape[j],
                                },
                            );
                        }
                    }
                    if let Some(expression) = incident_fields.get(tag) {
                        let value = expression
                            .evaluate(&symbols, &format!("boundary[{tag}].incidentField"))?;
                        for i in 0..2 {
                            let weight = 2.0 * scale * weighted_length * shape[i];
                            rhs_re[edge[i]] -= weight * value.im;
                            rhs_im[edge[i]] += weight * value.re;
                        }
                    }
                }
            }
        }
    }
    for tag in boundaries.keys() {
        if !used_boundary_tags.contains(tag) {
            return Err(format!(
                "Acoustic boundary selection tag {tag} has no boundary edges in the mesh."
            ));
        }
    }

    Ok((matrix, rhs_re, rhs_im, dirichlet))
}

fn run_model(
    model: &Value,
    mesh: &Mesh2D,
    map: &Value,
    output_directory: &Path,
    frequency_hz: f64,
    sweep_csv: &str,
    constitutive_quadrature: &str,
    solver_backend: &str,
    solver_ordering: &str,
    solver_threads: usize,
) -> Result<(), String> {
    // The native fullmeta-fem coordinator validates the full model before dispatch.
    // Keep this path focused on extracting solve inputs and checking FEM constraints.
    let component = model
        .pointer("/model")
        .ok_or("The acoustic model is missing.")?;
    let periodic = scalar2d::periodic_dof_map(model, map, mesh)?;
    let parameters = model
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let parameter_symbols = model_expression::resolve_parameters(parameters, model)?;
    let frequencies = [frequency_hz];

    let material_tags = selection_tags(map, "physicalGroups")?;
    let boundary_tags = selection_tags(map, "boundarySelections")?;
    let material_rows = component
        .get("materials")
        .and_then(Value::as_array)
        .ok_or("model.materials must be an array.")?;
    let mut material_rows_by_tag = HashMap::new();
    for material in material_rows {
        let selection_id = material
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("Acoustic material needs selectionId.")?;
        let tag = *material_tags.get(selection_id).ok_or_else(|| {
            format!("Material selection {selection_id:?} is absent from the mesh map.")
        })?;
        if material_rows_by_tag.insert(tag, material.clone()).is_some() {
            return Err(format!(
                "More than one acoustic material assignment targets physical tag {tag}."
            ));
        }
    }
    let compiled_materials = crate::material::CompiledMaterialAssignments::compile(
        "acoustic-2d",
        &material_rows_by_tag,
    )?;
    let physics =
        crate::model_schema::physics(component).ok_or("model.physics must be an object.")?;
    let mut boundary_rows_by_tag = Vec::new();
    for boundary in crate::model_schema::physics_boundary_conditions(physics) {
        if boundary.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let selection_id = boundary
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("Acoustic boundary needs selectionId.")?;
        let tag = *boundary_tags.get(selection_id).ok_or_else(|| {
            format!("Boundary selection {selection_id:?} is absent from the mesh map.")
        })?;
        boundary_rows_by_tag.push((tag, boundary.clone()));
    }

    fs::create_dir_all(output_directory)
        .map_err(|error| format!("Could not create acoustic output folder: {error}"))?;
    let variable_symbols = model
        .pointer("/model/variables")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            entry
                .get("symbol")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    let mut csv_rows = Vec::<(f64, usize, f64, f64, Vec<C>)>::new();
    let mut cache = SolverCache::default();
    for (frequency_index, frequency) in frequencies.iter().copied().enumerate() {
        let constant_values =
            compiled_materials.position_independent_values(&parameter_symbols, frequency)?;
        let constant_materials = constant_values
            .iter()
            .map(|(&tag, values)| {
                crate::acoustic2d::acoustic_material_from_values(values)
                    .map(|material| (tag, material))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut materials = Vec::<AcousticMaterial>::with_capacity(mesh.triangles.len());
        let quadrature = triangle_quadrature(constitutive_quadrature)?;
        let mut quadrature_materials =
            Vec::<Vec<AcousticMaterial>>::with_capacity(mesh.triangles.len());
        for (element, (triangle, tag)) in mesh.triangles.iter().enumerate() {
            if !material_rows_by_tag.contains_key(tag) {
                return Err(format!(
                    "Mesh triangle {element} uses material tag {tag}, which has no acoustic material assignment."
                ));
            }
            let points = triangle.map(|node| mesh.nodes[node]);
            let centroid = [
                (points[0][0] + points[1][0] + points[2][0]) / 3.0,
                (points[0][1] + points[1][1] + points[2][1]) / 3.0,
                (points[0][2] + points[1][2] + points[2][2]) / 3.0,
            ];
            let centroid_material = if let Some(&material) = constant_materials.get(tag) {
                material
            } else {
                let values = compiled_materials.evaluate(
                    *tag,
                    &parameter_symbols,
                    frequency,
                    centroid,
                    &format!("triangle {element}"),
                )?;
                acoustic_material_from_values(&values)?
            };
            materials.push(centroid_material);
            let mut samples = Vec::with_capacity(quadrature.len());
            if let Some(&material) = constant_materials.get(tag) {
                samples.resize(quadrature.len(), material);
            } else {
                for q in &quadrature {
                    let point = (0..3).fold([0.0; 3], |mut point, local| {
                        for axis in 0..3 {
                            point[axis] += q[local] * points[local][axis];
                        }
                        point
                    });
                    let values = compiled_materials.evaluate(
                        *tag,
                        &parameter_symbols,
                        frequency,
                        point,
                        &format!("triangle {element} quadrature point"),
                    )?;
                    samples.push(acoustic_material_from_values(&values)?);
                }
            }
            quadrature_materials.push(samples);
        }
        let mut boundaries = HashMap::<usize, BoundaryCondition>::new();
        for (tag, boundary) in &boundary_rows_by_tag {
            let kind = boundary.get("type").and_then(Value::as_str).unwrap_or("");
            let condition = match kind {
                "dirichlet" => BoundaryCondition::Dirichlet(
                    boundary
                        .get("value")
                        .cloned()
                        .ok_or("Dirichlet boundary needs value.")?,
                ),
                "neumann" => BoundaryCondition::Neumann(
                    boundary
                        .get("value")
                        .cloned()
                        .ok_or("Neumann boundary needs value.")?,
                ),
                "impedance" => BoundaryCondition::Impedance {
                    impedance: boundary
                        .get("impedance")
                        .cloned()
                        .ok_or("Impedance boundary needs impedance.")?,
                    incident: model_expression::scalar_incident_quantity(
                        boundary,
                        model,
                        &format!("boundary tag {tag}"),
                    )?,
                },
                _ => {
                    return Err(format!(
                        "Acoustic boundary type {kind:?} is not implemented."
                    ));
                }
            };
            if boundaries.insert(*tag, condition).is_some() {
                return Err(format!(
                    "More than one acoustic boundary condition targets physical tag {tag}."
                ));
            }
        }
        crate::solver_common::stage_started("assembly");
        let (matrix, rhs_re, rhs_im, dirichlet) = assemble(
            mesh,
            &quadrature_materials,
            constitutive_quadrature,
            &boundaries,
            frequency,
            &parameter_symbols,
            &periodic,
        )?;
        let (mut matrix, mut rhs_re, mut rhs_im) =
            scalar2d::reduce_system(&matrix, &rhs_re, &rhs_im, &periodic)?;
        scalar2d::apply_complex_dirichlet(&mut matrix, &mut rhs_re, &mut rhs_im, &dirichlet)?;
        let (solution_re, solution_im, residual) = solve_linear_system(
            &matrix,
            &rhs_re,
            &rhs_im,
            1e-7,
            solver_backend,
            solver_ordering,
            solver_threads,
            &mut cache,
        )?;
        let (solution_re, solution_im) =
            scalar2d::expand_complex_solution(&solution_re, &solution_im, &periodic)?;
        let max_pressure = solution_re
            .iter()
            .zip(&solution_im)
            .map(|(&re, &im)| re.hypot(im))
            .fold(0.0, f64::max);
        let omega = std::f64::consts::TAU * frequency;
        let velocity = recover_nodal_velocity(mesh, &materials, &solution_re, &solution_im, omega)?;
        let velocity_x = velocity.iter().map(|value| value[0]).collect::<Vec<_>>();
        let velocity_y = velocity.iter().map(|value| value[1]).collect::<Vec<_>>();
        // The 2D pressure formulation has no out-of-plane pressure gradient;
        // publish vz explicitly as zero rather than relying on recovery arithmetic.
        let velocity_z = vec![C::default(); mesh.nodes.len()];
        let pressure = solution_re
            .iter()
            .zip(&solution_im)
            .map(|(&re, &im)| C { re, im })
            .collect::<Vec<_>>();
        let field_values =
            acoustic_field_values(model, &pressure, [&velocity_x, &velocity_y, &velocity_z])?;
        let fields = field_output::declared_component_fields(model, &field_values)?;
        let component = model.pointer("/model").ok_or("model is required.")?;
        let domain_cells = mesh
            .triangles
            .iter()
            .map(|(nodes, tag)| crate::integration::Simplex {
                nodes: nodes.to_vec(),
                physical_tag: *tag,
                material_tag: Some(*tag),
                operator_id: None,
                element_index: None,
                adjacent_element_index: None,
            })
            .collect::<Vec<_>>();
        let boundary_cells = mesh
            .boundary_edges
            .iter()
            .chain(&mesh.integration_edges)
            .map(|(nodes, tag)| {
                Ok(crate::integration::Simplex {
                    nodes: nodes.to_vec(),
                    physical_tag: *tag,
                    material_tag: Some(adjacent_material_tag(mesh, *nodes)?),
                    operator_id: None,
                    element_index: None,
                    adjacent_element_index: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let global_variables = crate::integration::evaluate_model_variables(
            model,
            component,
            map,
            2,
            &mesh.nodes,
            &domain_cells,
            &boundary_cells,
            &field_values,
            frequency,
        )?;
        let material_fields = crate::material::fields_by_cell(
            model,
            map,
            frequency,
            &mesh.nodes,
            &mesh.triangles,
            [0, 1, 2],
            [0.0; 3],
        )?;
        let output = json!({
            "schema":"fullmeta-acoustic-field/1",
            "frequencyHz":frequency,
            "dimension":2,
            "fieldComponentCoordinateSystem":"model",
            "nodes":mesh.nodes,
            "triangles":mesh.triangles.iter().map(|(nodes,tag)| json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>(),
            "pressureRealPa":solution_re,
            "pressureImaginaryPa":solution_im,
            "fields":fields,
            "materialFieldsByCell":material_fields,
            "globalVariables":global_variables,
            "fieldConvention":"exp(+i omega t)",
            "derivedFieldRecovery":"arithmetic mean of incident triangle values at each node",
            "solver":{"backend":solver_backend,"ordering":solver_ordering,"threads":solver_threads,"constitutiveQuadrature":constitutive_quadrature,"residual":residual,"unknowns":periodic.dof_count},
            "maximumPressureMagnitudePa":max_pressure
        });
        let field_path = output_directory.join(format!("pressure_{frequency:.6}Hz.json"));
        crate::solver_common::stage_started("output");
        fs::write(
            &field_path,
            serde_json::to_vec_pretty(&output).map_err(|error| error.to_string())?,
        )
        .map_err(|error| format!("Could not write {}: {error}", field_path.display()))?;
        let variable_values = variable_symbols
            .iter()
            .map(|symbol| {
                let value = global_variables.get(symbol).ok_or_else(|| {
                    format!("Global variable {symbol} was not returned by the shared evaluator.")
                })?;
                Ok(C {
                    re: value
                        .get("re")
                        .and_then(Value::as_f64)
                        .ok_or_else(|| format!("Global variable {symbol} has no real value."))?,
                    im: value.get("im").and_then(Value::as_f64).ok_or_else(|| {
                        format!("Global variable {symbol} has no imaginary value.")
                    })?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        csv_rows.push((
            frequency,
            periodic.dof_count,
            residual,
            max_pressure,
            variable_values,
        ));
        println!(
            "Acoustic 2D · {frequency:.3} Hz · {} unknowns · {} triangles · residual {:.2e} · max |p| {:.5e} Pa",
            periodic.dof_count,
            mesh.triangles.len(),
            residual,
            max_pressure
        );
        let _ = frequency_index;
    }
    let real_flags = crate::integration::variable_real_flags(
        &csv_rows.iter().map(|row| row.4.clone()).collect::<Vec<_>>(),
    );
    let mut csv = String::from("frequency_Hz,unknowns,residual,max_pressure_magnitude_Pa");
    crate::integration::append_variable_csv_headers(&mut csv, &variable_symbols, &real_flags);
    csv.push('\n');
    for (frequency, unknowns, residual, max_pressure, values) in csv_rows {
        csv.push_str(&format!(
            "{frequency:.12},{unknowns},{residual:.8e},{max_pressure:.12e}"
        ));
        crate::integration::append_variable_csv_values(&mut csv, &values, &real_flags);
        csv.push('\n');
    }
    fs::write(output_directory.join(sweep_csv), csv)
        .map_err(|error| format!("Could not write acoustic sweep CSV: {error}"))?;
    Ok(())
}

/// Shared acoustic output assembly for 2D and 3D solvers. Variable names,
/// units, and zero-field declarations come from the registered scenario schema.
pub(super) fn acoustic_field_values(
    model: &Value,
    pressure: &[C],
    velocity: [&[C]; 3],
) -> Result<HashMap<String, Vec<C>>, String> {
    let declarations = crate::model_schema::field_variables_for_model(model, false)?;
    let projection = field_output::field_projection(&declarations)?;
    let scalar_families = HashMap::from([("pressure", pressure)]);
    let vector_families = HashMap::from([("velocity", velocity)]);
    field_output::project_nodal_fields(&projection, &scalar_families, &vector_families)
}

pub fn run_request(request_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let request: Value = serde_json::from_slice(&fs::read(request_path)?)?;
    crate::solver_request::validate_single_case_request(&request, "acoustic-2d")
        .map_err(|error| format!("Solver request: {error}"))?;
    let base = request_path.parent().unwrap_or(Path::new("."));
    let model_path = request
        .get("modelFile")
        .and_then(Value::as_str)
        .ok_or("request.modelFile is required.")?;
    let mesh_path = request
        .get("meshFile")
        .and_then(Value::as_str)
        .ok_or("request.meshFile is required.")?;
    let map_path = request
        .get("modelMapFile")
        .and_then(Value::as_str)
        .ok_or("request.modelMapFile is required.")?;
    let output_directory = request
        .get("outputDirectory")
        .and_then(Value::as_str)
        .ok_or("request.outputDirectory is required.")?;
    let model_path = resolve_from(base, model_path);
    let mesh_path = resolve_from(base, mesh_path);
    let map_path = resolve_from(base, map_path);
    let output_directory = resolve_from(base, output_directory);
    let model: Value = serde_json::from_slice(&fs::read(&model_path)?)?;
    crate::solver_request::validate_study_reference(&model, &request)
        .map_err(|error| format!("Solver request: {error}"))?;
    let frequency = crate::solver_request::single_case_frequency(&request)
        .map_err(|error| format!("Solver request: {error}"))?;
    let map: Value = serde_json::from_slice(&fs::read(&map_path)?)?;
    if map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6") {
        return Err("The acoustic-2d solver requires fullmeta-mesh-map/6.".into());
    }
    if map.get("dimension").and_then(Value::as_u64) != Some(2) {
        return Err("The acoustic-2d solver requires a dimension-2 mesh map.".into());
    }
    let mesh = mesh2d::parse_mesh_v2(&fs::read_to_string(&mesh_path)?)?;
    let sweep_csv = request
        .get("sweepCsv")
        .and_then(Value::as_str)
        .ok_or("request.sweepCsv is required.")?;
    let formulation = request
        .get("fieldFormulation")
        .and_then(Value::as_str)
        .ok_or("request.fieldFormulation is required.")?;
    if formulation != "pressure" {
        return Err(
            "The acoustic-2d solver currently supports the pressure formulation only.".into(),
        );
    }
    if request.get("elementOrder").and_then(Value::as_u64) != Some(1) {
        return Err("The acoustic-2d solver currently supports first-order elements only.".into());
    }
    let constitutive_quadrature = request
        .get("constitutiveQuadrature")
        .and_then(Value::as_str)
        .ok_or("request.constitutiveQuadrature is required.")?;
    if constitutive_quadrature != "standard" && constitutive_quadrature != "centroid" {
        return Err("request.constitutiveQuadrature must be 'standard' or 'centroid'.".into());
    }
    let solver_backend = request
        .get("solverBackend")
        .and_then(Value::as_str)
        .ok_or("request.solverBackend is required.")?;
    let solver_ordering = request
        .get("solverOrdering")
        .and_then(Value::as_str)
        .ok_or("request.solverOrdering is required.")?;
    let solver_threads = request
        .get("solverThreads")
        .and_then(Value::as_u64)
        .ok_or("request.solverThreads is required.")? as usize;
    run_model(
        &model,
        &mesh,
        &map,
        &output_directory,
        frequency,
        sweep_csv,
        constitutive_quadrature,
        solver_backend,
        solver_ordering,
        solver_threads,
    )
    .map_err(Into::into)
}
