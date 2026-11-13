use crate::field_output;
use crate::mesh2d::{self, Mesh2D};
use crate::model_expression;
use crate::scalar2d;
use crate::{C, MU0, SolverCache, c_add, c_mul, c_scale, solve_linear_system};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

const EPS0: f64 = 8.854_187_812_8e-12;

#[derive(Clone, Copy)]
struct Material {
    epsilon: C,
    mu: f64,
}

fn em_material_from_values(
    values: &HashMap<String, model_expression::Scalar>,
    frequency_hz: f64,
    formulation: &str,
) -> Result<Material, String> {
    let omega = std::f64::consts::TAU * frequency_hz;
    let epsilon = values
        .get("eps_r")
        .ok_or("Scenario material catalog omits eps_r.")?
        .value;
    let mu = values
        .get("mu_r")
        .ok_or("Scenario material catalog omits mu_r.")?
        .value;
    let conductivity = values
        .get("sigma")
        .ok_or("Scenario material catalog omits sigma.")?
        .value;
    if !epsilon.is_finite() || !mu.is_finite() || !conductivity.is_finite() {
        return Err("Relative permittivity, permeability and conductivity must be finite in the 2D EM solver.".into());
    }
    let epsilon = C {
        re: epsilon,
        im: -conductivity / (omega * EPS0),
    };
    if !epsilon.im.is_finite() {
        return Err("The complex electromagnetic permittivity must be finite.".into());
    }
    if formulation == "Ez" && mu == 0.0 {
        return Err("The Ez formulation requires nonzero permeability for its diffusion coefficient and magnetic-field recovery.".into());
    }
    if formulation == "Hz" && epsilon.re == 0.0 && epsilon.im == 0.0 {
        return Err("The Hz formulation requires nonzero complex permittivity for its diffusion coefficient and electric-field recovery.".into());
    }
    Ok(Material { epsilon, mu })
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
enum BoundaryCondition {
    Dirichlet(Value),
    Neumann(Value),
    Impedance {
        impedance: Value,
        incident: Option<model_expression::CompiledComplexQuantity>,
    },
}

pub(super) fn resolve_from(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn edge_key(edge: [usize; 2]) -> [usize; 2] {
    if edge[0] <= edge[1] {
        edge
    } else {
        [edge[1], edge[0]]
    }
}

fn adjacency(mesh: &Mesh2D) -> HashMap<[usize; 2], Vec<(usize, usize)>> {
    let mut output = HashMap::<[usize; 2], Vec<(usize, usize)>>::new();
    for (element, (triangle, tag)) in mesh.triangles.iter().enumerate() {
        for edge in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            output
                .entry(edge_key(edge))
                .or_default()
                .push((element, *tag));
        }
    }
    output
}

fn assemble(
    mesh: &Mesh2D,
    quadrature_materials: &[Vec<Material>],
    constitutive_quadrature: &str,
    boundaries: &HashMap<usize, BoundaryCondition>,
    frequency_hz: f64,
    formulation: &str,
    periodic: &scalar2d::PeriodicDofMap,
    parameters: &HashMap<String, model_expression::Scalar>,
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
    let k0 = omega / 299_792_458.0;
    let mut matrix = vec![BTreeMap::<usize, C>::new(); n];
    let mut rhs_re = vec![0.0; n];
    let mut rhs_im = vec![0.0; n];
    let mut dirichlet_nodes = HashMap::<usize, C>::new();
    let quadrature = triangle_quadrature(constitutive_quadrature)?;

    for (element, (triangle, material_tag)) in mesh.triangles.iter().enumerate() {
        let element_quadrature = quadrature_materials.get(element).ok_or_else(|| format!("Mesh triangle uses electromagnetic material tag {material_tag}, but no constitutive quadrature materials are assigned."))?;
        let points = triangle.map(|node| mesh.nodes[node]);
        let (area, gradients) = mesh2d::triangle_geometry(points)?;
        for (q_index, q) in quadrature.iter().enumerate() {
            let material = element_quadrature.get(q_index).ok_or(
                "Constitutive quadrature material count does not match the integration rule.",
            )?;
            let (diffusion, reaction) = if formulation == "Ez" {
                (
                    C {
                        re: 1.0 / material.mu,
                        im: 0.0,
                    },
                    c_scale(material.epsilon, -k0 * k0),
                )
            } else {
                (
                    C { re: 1.0, im: 0.0 }.div(material.epsilon),
                    C {
                        re: -k0 * k0 * material.mu,
                        im: 0.0,
                    },
                )
            };
            let weighted_area = area * q[3];
            for i in 0..3 {
                for j in 0..3 {
                    let gradient_product =
                        gradients[i][0] * gradients[j][0] + gradients[i][1] * gradients[j][1];
                    let value = c_add(
                        c_scale(diffusion, weighted_area * gradient_product),
                        c_scale(reaction, weighted_area * q[i] * q[j]),
                    );
                    let entry = matrix[triangle[i]].entry(triangle[j]).or_default();
                    *entry = c_add(*entry, value);
                }
            }
        }
    }

    let edge_adjacency = adjacency(mesh);
    let mut used_boundary_tags = HashSet::new();
    for (edge, tag) in &mesh.boundary_edges {
        if periodic.boundary_tags.contains(tag)
            || periodic.boundary_edges.contains(&edge_key(*edge))
        {
            if boundaries.contains_key(tag) {
                return Err(format!(
                    "Electromagnetic boundary tag {tag} is periodic and cannot also carry a physical boundary condition."
                ));
            }
            continue;
        }
        let Some(condition) = boundaries.get(tag) else {
            continue;
        };
        used_boundary_tags.insert(*tag);
        let edge_points = [mesh.nodes[edge[0]], mesh.nodes[edge[1]]];
        let delta = [
            edge_points[1][0] - edge_points[0][0],
            edge_points[1][1] - edge_points[0][1],
        ];
        let length = delta[0].hypot(delta[1]);
        if !length.is_finite() || length <= 0.0 {
            return Err("The electromagnetic boundary mesh contains a degenerate edge.".into());
        }
        let adjacent = edge_adjacency
            .get(&edge_key(*edge))
            .ok_or_else(|| format!("Boundary edge {:?} has no adjacent triangle.", edge))?;
        match condition {
            BoundaryCondition::Dirichlet(expression) => {
                if adjacent.len() != 1 {
                    return Err(format!(
                        "Dirichlet boundary selection tag {tag} must target an exterior edge."
                    ));
                }
                for node in edge {
                    let point = mesh.nodes[*node];
                    let symbols = scalar2d::symbols_at_point(parameters, frequency_hz, point);
                    let value = scalar2d::quantity(
                        expression,
                        &symbols,
                        &format!("boundary tag {tag}.value"),
                    )?;
                    if !value.is_finite() {
                        return Err(format!(
                            "Dirichlet boundary tag {tag} evaluated to a non-finite field value."
                        ));
                    }
                    scalar2d::insert_dirichlet_constraint(
                        &mut dirichlet_nodes,
                        periodic,
                        *node,
                        C { re: value, im: 0.0 },
                    )?;
                }
            }
            BoundaryCondition::Neumann(expression) => {
                if adjacent.len() != 1 {
                    return Err(format!(
                        "Neumann boundary selection tag {tag} must target an exterior edge."
                    ));
                }
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
                        edge_points[0][0] + t * delta[0],
                        edge_points[0][1] + t * delta[1],
                        0.0,
                    ];
                    let symbols = scalar2d::symbols_at_point(parameters, frequency_hz, point);
                    let value = scalar2d::quantity(
                        expression,
                        &symbols,
                        &format!("boundary tag {tag}.value"),
                    )?;
                    if !value.is_finite() {
                        return Err(format!(
                            "Neumann boundary tag {tag} evaluated to non-finite conormal boundary data."
                        ));
                    }
                    let weighted_length = length * weight * 0.5;
                    for (local, shape) in [(0, 1.0 - t), (1, t)] {
                        rhs_re[edge[local]] += weighted_length * shape * value;
                    }
                }
            }
            BoundaryCondition::Impedance {
                impedance: impedance_expression,
                incident,
            } => {
                if adjacent.len() != 1 {
                    return Err(format!(
                        "Impedance boundary selection tag {tag} must target an exterior edge."
                    ));
                }
                // Apply the prescribed physical surface impedance directly to
                // the conormal flux. No material square-root branch is needed.
                let incident_path = format!("boundary tag {tag}.incidentField");
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
                        edge_points[0][0] + t * delta[0],
                        edge_points[0][1] + t * delta[1],
                        0.0,
                    ];
                    let symbols = scalar2d::symbols_at_point(parameters, frequency_hz, point);
                    let z = scalar2d::quantity(
                        impedance_expression,
                        &symbols,
                        &format!("boundary tag {tag}.impedance"),
                    )?;
                    if !z.is_finite() || z == 0.0 {
                        return Err(format!(
                            "Impedance boundary tag {tag} must evaluate to a finite nonzero impedance."
                        ));
                    }
                    let beta = if formulation == "Ez" {
                        omega * MU0 / z
                    } else {
                        omega * EPS0 * z
                    };
                    if !beta.is_finite() {
                        return Err(format!(
                            "Impedance boundary tag {tag} produced a non-finite Robin coefficient."
                        ));
                    }
                    let i_beta = C { re: 0.0, im: beta };
                    let weighted_length = length * weight * 0.5;
                    let shapes = [1.0 - t, t];
                    for i in 0..2 {
                        for j in 0..2 {
                            let entry = matrix[edge[i]].entry(edge[j]).or_default();
                            *entry = c_add(
                                *entry,
                                c_scale(i_beta, weighted_length * shapes[i] * shapes[j]),
                            );
                        }
                    }
                    if let Some(incident_quantity) = incident {
                        // Evaluate the complete complex trace at each edge quadrature
                        // point, so an oblique incident wave can carry its spatial phase.
                        let incident_value =
                            incident_quantity.evaluate(&symbols, &incident_path)?;
                        if !incident_value.re.is_finite() || !incident_value.im.is_finite() {
                            return Err(format!(
                                "Incident field on impedance boundary tag {tag} evaluated to a non-finite value."
                            ));
                        }
                        let source = c_scale(
                            c_mul(
                                i_beta,
                                C {
                                    re: incident_value.re,
                                    im: incident_value.im,
                                },
                            ),
                            2.0,
                        );
                        for local in 0..2 {
                            rhs_re[edge[local]] += weighted_length * shapes[local] * source.re;
                            rhs_im[edge[local]] += weighted_length * shapes[local] * source.im;
                        }
                    }
                }
            }
        }
    }
    for tag in boundaries.keys() {
        if !used_boundary_tags.contains(tag) {
            return Err(format!(
                "Electromagnetic boundary selection tag {tag} has no boundary edges in the mesh."
            ));
        }
    }
    // Unspecified exterior edges retain the natural homogeneous Neumann term.
    Ok((matrix, rhs_re, rhs_im, dirichlet_nodes))
}

fn run_model(
    model: &Value,
    mesh: &Mesh2D,
    map: &Value,
    output_directory: &Path,
    frequency_hz: f64,
    formulation: &str,
    sweep_csv: &str,
    constitutive_quadrature: &str,
    solver_backend: &str,
    solver_ordering: &str,
    solver_threads: usize,
) -> Result<(), String> {
    // The native fullmeta-fem coordinator validates the full model before dispatch.
    // The request schema checks the FEM formulation; this path checks solve inputs.
    let component = model
        .pointer("/model")
        .ok_or("The electromagnetic model is missing.")?;
    let periodic = scalar2d::periodic_dof_map(model, map, mesh)?;
    let parameters = model
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let parameter_symbols = model_expression::resolve_parameters(parameters, model)?;
    let frequencies = [frequency_hz];

    let material_tags = scalar2d::selection_tags(map, "physicalGroups")?;
    let boundary_tags = scalar2d::selection_tags(map, "boundarySelections")?;
    let material_rows = component
        .get("materials")
        .and_then(Value::as_array)
        .ok_or("model.materials must be an array")?;
    let mut material_rows_by_tag = HashMap::new();
    for material in material_rows {
        let selection_id = material
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("Electromagnetic material needs selectionId")?;
        let tag = *material_tags.get(selection_id).ok_or_else(|| {
            format!("Material selection {selection_id:?} is absent from the mesh map.")
        })?;
        if material_rows_by_tag.insert(tag, material.clone()).is_some() {
            return Err(format!(
                "More than one electromagnetic material assignment targets physical tag {tag}."
            ));
        }
    }
    let compiled_materials =
        crate::material::CompiledMaterialAssignments::compile("em-2d", &material_rows_by_tag)?;
    let physics =
        crate::model_schema::physics(component).ok_or("model.physics must be an object")?;
    let mut boundary_rows_by_tag = Vec::new();
    for boundary in crate::model_schema::physics_boundary_conditions(physics) {
        if boundary.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let selection_id = boundary
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("Electromagnetic boundary needs selectionId")?;
        let tag = *boundary_tags.get(selection_id).ok_or_else(|| {
            format!("Boundary selection {selection_id:?} is absent from the mesh map.")
        })?;
        boundary_rows_by_tag.push((tag, boundary.clone()));
    }

    fs::create_dir_all(output_directory)
        .map_err(|error| format!("Could not create electromagnetic output folder: {error}"))?;
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
    let quadrature = triangle_quadrature(constitutive_quadrature)?;
    for frequency in frequencies.iter().copied() {
        let constant_values =
            compiled_materials.position_independent_values(&parameter_symbols, frequency)?;
        let constant_materials = constant_values
            .iter()
            .map(|(&tag, values)| {
                em_material_from_values(values, frequency, formulation)
                    .map(|material| (tag, material))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut materials = Vec::<Material>::with_capacity(mesh.triangles.len());
        let mut quadrature_materials = Vec::<Vec<Material>>::with_capacity(mesh.triangles.len());
        for (element, (triangle, tag)) in mesh.triangles.iter().enumerate() {
            if !material_rows_by_tag.contains_key(tag) {
                return Err(format!(
                    "Mesh triangle {element} uses electromagnetic material tag {tag}, but no material is assigned to that domain."
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
                em_material_from_values(&values, frequency, formulation)?
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
                    samples.push(em_material_from_values(&values, frequency, formulation)?);
                }
            }
            quadrature_materials.push(samples);
        }
        let mut boundaries = HashMap::<usize, BoundaryCondition>::new();
        for (tag, boundary) in &boundary_rows_by_tag {
            let path = format!(
                "physics.boundary.{}",
                boundary
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            );
            let kind = boundary.get("type").and_then(Value::as_str).unwrap_or("");
            let condition = match kind {
                "dirichlet" => BoundaryCondition::Dirichlet(
                    boundary
                        .get("value")
                        .cloned()
                        .ok_or_else(|| format!("{path}.value is required"))?,
                ),
                "neumann" => BoundaryCondition::Neumann(
                    boundary
                        .get("value")
                        .cloned()
                        .ok_or_else(|| format!("{path}.value is required"))?,
                ),
                "impedance" => BoundaryCondition::Impedance {
                    impedance: boundary
                        .get("impedance")
                        .cloned()
                        .ok_or_else(|| format!("{path}.impedance is required"))?,
                    incident: model_expression::scalar_incident_quantity(boundary, model, &path)?
                        .map(|expression| {
                            model_expression::CompiledComplexQuantity::compile(
                                &expression,
                                &format!("{path}.incidentField"),
                            )
                        })
                        .transpose()?,
                },
                _ => {
                    return Err(format!(
                        "Electromagnetic boundary type {kind:?} is not implemented in the 2D solver."
                    ));
                }
            };
            if boundaries.insert(*tag, condition).is_some() {
                return Err(format!(
                    "More than one electromagnetic boundary condition targets physical tag {tag}."
                ));
            }
        }
        crate::solver_common::stage_started("assembly");
        let (matrix, rhs_re, rhs_im, dirichlet_nodes) = assemble(
            mesh,
            &quadrature_materials,
            constitutive_quadrature,
            &boundaries,
            frequency,
            formulation,
            &periodic,
            &parameter_symbols,
        )?;
        let (mut matrix, mut rhs_re, mut rhs_im) =
            scalar2d::reduce_system(&matrix, &rhs_re, &rhs_im, &periodic)?;
        scalar2d::apply_complex_dirichlet(&mut matrix, &mut rhs_re, &mut rhs_im, &dirichlet_nodes)?;
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
        let max_field = solution_re
            .iter()
            .zip(&solution_im)
            .map(|(&re, &im)| re.hypot(im))
            .fold(0.0, f64::max);
        let declarations = crate::model_schema::field_variables_for_model(model, false)?;
        let primary = declarations
            .iter()
            .find(|declaration| declaration.role == "primary")
            .ok_or("The scenario schema does not declare a primary EM2D field.")?;
        let component_name = primary.symbol.clone();
        let unit = primary.unit.clone();
        let solved_field = solution_re
            .iter()
            .zip(&solution_im)
            .map(|(&re, &im)| C { re, im })
            .collect::<Vec<_>>();
        let node_count = mesh.nodes.len();
        let mut derived_electric_sums = vec![[C::default(); 3]; node_count];
        let mut derived_electric_counts = vec![0; node_count];
        let mut derived_magnetic_sums = vec![[C::default(); 3]; node_count];
        let mut derived_magnetic_counts = vec![0; node_count];
        let omega = std::f64::consts::TAU * frequency;
        for (element, (triangle, material_tag)) in mesh.triangles.iter().enumerate() {
            let material = materials.get(element).ok_or_else(|| format!("Mesh triangle uses electromagnetic material tag {material_tag}, which has no material assignment."))?;
            let points = triangle.map(|node| mesh.nodes[node]);
            let (_, gradients_2d) = mesh2d::triangle_geometry(points)?;
            let gradients = gradients_2d.map(|gradient| [gradient[0], gradient[1], 0.0]);
            let solved_gradient =
                field_output::p1_complex_gradient(triangle, &gradients, &solution_re, &solution_im);
            if formulation == "Ez" {
                // exp(+i omega t): H = i curl(E)/(omega mu), with E = Ez z-hat.
                let factor = C {
                    re: 0.0,
                    im: 1.0 / (omega * MU0 * material.mu),
                };
                let curl_e = [
                    solved_gradient[1],
                    C {
                        re: -solved_gradient[0].re,
                        im: -solved_gradient[0].im,
                    },
                    C::default(),
                ];
                let magnetic = curl_e.map(|component| c_mul(factor, component));
                field_output::accumulate_cell_vector(
                    &mut derived_magnetic_sums,
                    &mut derived_magnetic_counts,
                    triangle,
                    magnetic,
                )?;
            } else {
                // exp(+i omega t): E = -i curl(H)/(omega epsilon), including conductivity.
                let factor = C { re: 0.0, im: -1.0 }.div(c_scale(material.epsilon, omega * EPS0));
                let curl_h = [
                    solved_gradient[1],
                    C {
                        re: -solved_gradient[0].re,
                        im: -solved_gradient[0].im,
                    },
                    C::default(),
                ];
                let electric = curl_h.map(|component| c_mul(factor, component));
                field_output::accumulate_cell_vector(
                    &mut derived_electric_sums,
                    &mut derived_electric_counts,
                    triangle,
                    electric,
                )?;
            }
        }
        field_output::average_nodal_vectors(&mut derived_electric_sums, &derived_electric_counts);
        field_output::average_nodal_vectors(&mut derived_magnetic_sums, &derived_magnetic_counts);
        let zero_field = vec![C::default(); node_count];
        let electric = if formulation == "Ez" {
            [zero_field.clone(), zero_field.clone(), solved_field.clone()]
        } else {
            [
                derived_electric_sums.iter().map(|value| value[0]).collect(),
                derived_electric_sums.iter().map(|value| value[1]).collect(),
                zero_field.clone(),
            ]
        };
        let magnetic = if formulation == "Ez" {
            [
                derived_magnetic_sums.iter().map(|value| value[0]).collect(),
                derived_magnetic_sums.iter().map(|value| value[1]).collect(),
                zero_field.clone(),
            ]
        } else {
            [zero_field.clone(), zero_field.clone(), solved_field]
        };
        let projection = field_output::field_projection(&declarations)?;
        let scalar_families = HashMap::<&str, &[C]>::new();
        let vector_families = HashMap::from([
            (
                "electric",
                [
                    electric[0].as_slice(),
                    electric[1].as_slice(),
                    electric[2].as_slice(),
                ],
            ),
            (
                "magnetic",
                [
                    magnetic[0].as_slice(),
                    magnetic[1].as_slice(),
                    magnetic[2].as_slice(),
                ],
            ),
        ]);
        let values =
            field_output::project_nodal_fields(&projection, &scalar_families, &vector_families)?;
        let fields = field_output::declared_component_fields(model, &values)?;
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
        let edge_adjacency = adjacency(mesh);
        let boundary_cells = mesh
            .boundary_edges
            .iter()
            .chain(&mesh.integration_edges)
            .map(|(nodes, tag)| {
                let adjacent = edge_adjacency
                    .get(&edge_key(*nodes))
                    .and_then(|rows| rows.first())
                    .map(|row| row.1)
                    .ok_or_else(|| {
                        format!("Boundary edge {nodes:?} has no adjacent material domain.")
                    })?;
                Ok(crate::integration::Simplex {
                    nodes: nodes.to_vec(),
                    physical_tag: *tag,
                    material_tag: Some(adjacent),
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
            &values,
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
            "schema":"fullmeta-em2d-field/1",
            "frequencyHz":frequency,
            "dimension":2,
            "formulation":formulation,
            "fieldComponentCoordinateSystem":"model",
            "fieldComponent":component_name,
            "fieldUnit":unit,
            "nodes":mesh.nodes,
            "triangles":mesh.triangles.iter().map(|(nodes,tag)| json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>(),
            "fieldReal":solution_re,
            "fieldImaginary":solution_im,
            "fields":fields,
            "materialFieldsByCell":material_fields,
            "globalVariables":global_variables,
            "fieldConvention":"exp(+i omega t)",
            "derivedFieldRecovery":"arithmetic mean of incident triangle values at each node",
            "solver":{"backend":solver_backend,"ordering":solver_ordering,"threads":solver_threads,"constitutiveQuadrature":constitutive_quadrature,"residual":residual,"unknowns":periodic.dof_count},
            "maximumFieldMagnitude":max_field
        });
        let field_path = output_directory.join(format!("emfield_{frequency:.6}Hz.json"));
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
            max_field,
            variable_values,
        ));
        println!(
            "EM 2D {formulation} · {frequency:.3} Hz · {} unknowns · {} triangles · residual {:.2e} · max |{component_name}| {:.5e} {unit}",
            periodic.dof_count,
            mesh.triangles.len(),
            residual,
            max_field
        );
    }
    let real_flags = crate::integration::variable_real_flags(
        &csv_rows.iter().map(|row| row.4.clone()).collect::<Vec<_>>(),
    );
    let mut csv = String::from("frequency_Hz,unknowns,residual,max_field_magnitude");
    crate::integration::append_variable_csv_headers(&mut csv, &variable_symbols, &real_flags);
    csv.push('\n');
    for (frequency, unknowns, residual, max_field, values) in csv_rows {
        csv.push_str(&format!(
            "{frequency:.12},{unknowns},{residual:.8e},{max_field:.12e}"
        ));
        crate::integration::append_variable_csv_values(&mut csv, &values, &real_flags);
        csv.push('\n');
    }
    fs::write(output_directory.join(sweep_csv), csv)
        .map_err(|error| format!("Could not write EM 2D sweep CSV: {error}"))?;
    Ok(())
}

pub fn run_request(request_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let request: Value = serde_json::from_slice(&fs::read(request_path)?)?;
    crate::solver_request::validate_single_case_request(&request, "em-2d")
        .map_err(|error| format!("Solver request: {error}"))?;
    let formulation = request
        .get("fieldFormulation")
        .and_then(Value::as_str)
        .ok_or("request.fieldFormulation is required")?;
    let base = request_path.parent().unwrap_or(Path::new("."));
    let model_path = resolve_from(
        base,
        request
            .get("modelFile")
            .and_then(Value::as_str)
            .ok_or("request.modelFile is required")?,
    );
    let mesh_path = resolve_from(
        base,
        request
            .get("meshFile")
            .and_then(Value::as_str)
            .ok_or("request.meshFile is required")?,
    );
    let map_path = resolve_from(
        base,
        request
            .get("modelMapFile")
            .and_then(Value::as_str)
            .ok_or("request.modelMapFile is required")?,
    );
    let output_path = resolve_from(
        base,
        request
            .get("outputDirectory")
            .and_then(Value::as_str)
            .ok_or("request.outputDirectory is required")?,
    );
    let model: Value = serde_json::from_slice(&fs::read(&model_path)?)?;
    crate::solver_request::validate_study_reference(&model, &request)
        .map_err(|error| format!("Solver request: {error}"))?;
    let frequency = crate::solver_request::single_case_frequency(&request)
        .map_err(|error| format!("Solver request: {error}"))?;
    let map: Value = serde_json::from_slice(&fs::read(&map_path)?)?;
    if map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6") {
        return Err("The em-2d solver requires fullmeta-mesh-map/6.".into());
    }
    if map.get("dimension").and_then(Value::as_u64) != Some(2) {
        return Err("The em-2d solver requires a dimension-2 mesh map.".into());
    }
    let mesh = mesh2d::parse_mesh_v2(&fs::read_to_string(&mesh_path)?)?;
    let sweep_csv = request
        .get("sweepCsv")
        .and_then(Value::as_str)
        .ok_or("request.sweepCsv is required")?;
    if request.get("elementOrder").and_then(Value::as_u64) != Some(1) {
        return Err("The em-2d solver currently supports first-order elements only.".into());
    }
    let constitutive_quadrature = request
        .get("constitutiveQuadrature")
        .and_then(Value::as_str)
        .ok_or("request.constitutiveQuadrature is required")?;
    if constitutive_quadrature != "standard" && constitutive_quadrature != "centroid" {
        return Err("request.constitutiveQuadrature must be 'standard' or 'centroid'.".into());
    }
    let solver_backend = request
        .get("solverBackend")
        .and_then(Value::as_str)
        .ok_or("request.solverBackend is required")?;
    let solver_ordering = request
        .get("solverOrdering")
        .and_then(Value::as_str)
        .ok_or("request.solverOrdering is required")?;
    let solver_threads = request
        .get("solverThreads")
        .and_then(Value::as_u64)
        .ok_or("request.solverThreads is required")? as usize;
    run_model(
        &model,
        &mesh,
        &map,
        &output_path,
        frequency,
        formulation,
        sweep_csv,
        constitutive_quadrature,
        solver_backend,
        solver_ordering,
        solver_threads,
    )
    .map_err(Into::into)
}
