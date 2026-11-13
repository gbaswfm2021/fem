use crate::acoustic2d::{AcousticMaterial, BoundaryCondition, compile_incident_fields};
use crate::field_output;
use crate::model_expression;
use crate::scalar2d;
use crate::{C, SolverCache, solve_linear_system};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

type V3 = [f64; 3];

struct Mesh3D {
    nodes: Vec<V3>,
    tetrahedra: Vec<([usize; 4], usize)>,
    boundary_faces: Vec<([usize; 3], usize)>,
    edge_selections: Vec<([usize; 2], usize)>,
}

fn face_key(mut nodes: [usize; 3]) -> [usize; 3] {
    nodes.sort_unstable();
    nodes
}
fn adjacent_material_tag(mesh: &Mesh3D, face: [usize; 3]) -> Result<usize, String> {
    for (tetrahedron, tag) in &mesh.tetrahedra {
        let mut local = tetrahedron
            .iter()
            .copied()
            .filter(|node| face.contains(node))
            .collect::<Vec<_>>();
        local.sort_unstable();
        if local.len() == 3 && local.as_slice() == face_key(face) {
            return Ok(*tag);
        }
    }
    Err(format!(
        "Boundary face {face:?} has no adjacent tetrahedron."
    ))
}
fn adjacent_material_tag_for_edge(mesh: &Mesh3D, edge: [usize; 2]) -> Result<usize, String> {
    mesh.tetrahedra
        .iter()
        .find_map(|(tetrahedron, tag)| {
            edge.iter()
                .all(|node| tetrahedron.contains(node))
                .then_some(*tag)
        })
        .ok_or_else(|| format!("Integration edge {edge:?} has no adjacent tetrahedron."))
}

fn parse_mesh(text: &str) -> Result<Mesh3D, String> {
    let mut nodes = Vec::<Option<V3>>::new();
    let mut tetrahedra = Vec::<Option<([usize; 4], usize)>>::new();
    let mut faces = Vec::<Option<([usize; 3], usize)>>::new();
    let mut edges = Vec::<Option<([usize; 2], usize)>>::new();
    let mut header = false;
    for (line_index, line) in text.lines().enumerate() {
        let row: Vec<_> = line.trim().split(',').collect();
        match row.first().copied().unwrap_or("") {
            "FULLMETA_MESH/1" => header = true,
            "V" if row.len() >= 5 => {
                let id = row[1]
                    .parse::<usize>()
                    .map_err(|error| format!("mesh line {} node ID: {error}", line_index + 1))?;
                if nodes.len() <= id {
                    nodes.resize(id + 1, None);
                }
                let point = [
                    row[2].parse::<f64>().map_err(|error| error.to_string())?,
                    row[3].parse::<f64>().map_err(|error| error.to_string())?,
                    row[4].parse::<f64>().map_err(|error| error.to_string())?,
                ];
                if point.iter().any(|value| !value.is_finite()) {
                    return Err(format!("mesh node {id} must be finite."));
                }
                nodes[id] = Some(point);
            }
            "F" if row.len() >= 6 => {
                let id = row[1].parse::<usize>().map_err(|error| error.to_string())?;
                if faces.len() <= id {
                    faces.resize(id + 1, None);
                }
                faces[id] = Some((
                    [
                        row[2].parse::<usize>().map_err(|error| error.to_string())?,
                        row[3].parse::<usize>().map_err(|error| error.to_string())?,
                        row[4].parse::<usize>().map_err(|error| error.to_string())?,
                    ],
                    row[5].parse::<usize>().map_err(|error| error.to_string())?,
                ));
            }
            "L" if row.len() >= 5 => {
                let id = row[1].parse::<usize>().map_err(|error| error.to_string())?;
                if edges.len() <= id {
                    edges.resize(id + 1, None);
                }
                edges[id] = Some((
                    [
                        row[2].parse::<usize>().map_err(|error| error.to_string())?,
                        row[3].parse::<usize>().map_err(|error| error.to_string())?,
                    ],
                    row[4].parse::<usize>().map_err(|error| error.to_string())?,
                ));
            }
            "T" if row.len() >= 7 => {
                let id = row[1].parse::<usize>().map_err(|error| error.to_string())?;
                if tetrahedra.len() <= id {
                    tetrahedra.resize(id + 1, None);
                }
                tetrahedra[id] = Some((
                    [
                        row[2].parse::<usize>().map_err(|error| error.to_string())?,
                        row[3].parse::<usize>().map_err(|error| error.to_string())?,
                        row[4].parse::<usize>().map_err(|error| error.to_string())?,
                        row[5].parse::<usize>().map_err(|error| error.to_string())?,
                    ],
                    row[6].parse::<usize>().map_err(|error| error.to_string())?,
                ));
            }
            _ => {}
        }
    }
    if !header {
        return Err("The acoustic 3D solver requires a FULLMETA_MESH/1 file.".into());
    }
    if nodes.is_empty()
        || nodes.iter().any(Option::is_none)
        || tetrahedra.is_empty()
        || tetrahedra.iter().any(Option::is_none)
    {
        return Err("The tetrahedral mesh has incomplete node or cell records.".into());
    }
    let nodes: Vec<V3> = nodes.into_iter().map(Option::unwrap).collect();
    let tetrahedra: Vec<([usize; 4], usize)> = tetrahedra.into_iter().map(Option::unwrap).collect();
    let boundary_faces: Vec<([usize; 3], usize)> = faces.into_iter().map(Option::unwrap).collect();
    let edge_selections: Vec<([usize; 2], usize)> = edges.into_iter().map(Option::unwrap).collect();
    if boundary_faces
        .iter()
        .any(|(face, _)| face.iter().any(|node| *node >= nodes.len()))
        || edge_selections
            .iter()
            .any(|(edge, _)| edge.iter().any(|node| *node >= nodes.len()))
        || tetrahedra
            .iter()
            .any(|(tetrahedron, _)| tetrahedron.iter().any(|node| *node >= nodes.len()))
    {
        return Err("The tetrahedral mesh references an out-of-range node.".into());
    }
    Ok(Mesh3D {
        nodes,
        tetrahedra,
        boundary_faces,
        edge_selections,
    })
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn tetra_geometry(points: [V3; 4]) -> Result<(f64, [V3; 4]), String> {
    let [a, b, c, d] = points;
    let e0 = sub(b, a);
    let e1 = sub(c, a);
    let e2 = sub(d, a);
    let j = [
        [e0[0], e1[0], e2[0]],
        [e0[1], e1[1], e2[1]],
        [e0[2], e1[2], e2[2]],
    ];
    let det = j[0][0] * (j[1][1] * j[2][2] - j[1][2] * j[2][1])
        - j[0][1] * (j[1][0] * j[2][2] - j[1][2] * j[2][0])
        + j[0][2] * (j[1][0] * j[2][1] - j[1][1] * j[2][0]);
    if !det.is_finite() || det.abs() < 1e-30 {
        return Err("The tetrahedral mesh contains a degenerate cell.".into());
    }
    let inverse = [
        [
            (j[1][1] * j[2][2] - j[1][2] * j[2][1]) / det,
            (j[0][2] * j[2][1] - j[0][1] * j[2][2]) / det,
            (j[0][1] * j[1][2] - j[0][2] * j[1][1]) / det,
        ],
        [
            (j[1][2] * j[2][0] - j[1][0] * j[2][2]) / det,
            (j[0][0] * j[2][2] - j[0][2] * j[2][0]) / det,
            (j[0][2] * j[1][0] - j[0][0] * j[1][2]) / det,
        ],
        [
            (j[1][0] * j[2][1] - j[1][1] * j[2][0]) / det,
            (j[0][1] * j[2][0] - j[0][0] * j[2][1]) / det,
            (j[0][0] * j[1][1] - j[0][1] * j[1][0]) / det,
        ],
    ];
    let gradients = [
        [
            -(inverse[0][0] + inverse[1][0] + inverse[2][0]),
            -(inverse[0][1] + inverse[1][1] + inverse[2][1]),
            -(inverse[0][2] + inverse[1][2] + inverse[2][2]),
        ],
        inverse[0],
        inverse[1],
        inverse[2],
    ];
    Ok((det.abs() / 6.0, gradients))
}

fn recover_nodal_velocity(
    mesh: &Mesh3D,
    materials: &[AcousticMaterial],
    pressure_real: &[f64],
    pressure_imaginary: &[f64],
    omega: f64,
) -> Result<Vec<[C; 3]>, String> {
    let mut sums = vec![[C::default(); 3]; mesh.nodes.len()];
    let mut counts = vec![0; mesh.nodes.len()];
    for (element, (tetrahedron, material_tag)) in mesh.tetrahedra.iter().enumerate() {
        let material = materials.get(element).ok_or_else(|| format!("Mesh tetrahedron uses acoustic material tag {material_tag}, which has no acoustic material assignment."))?;
        let points = tetrahedron.map(|node| mesh.nodes[node]);
        let (_, gradients) = tetra_geometry(points)?;
        let pressure_gradient = field_output::p1_complex_gradient(
            tetrahedron,
            &gradients,
            pressure_real,
            pressure_imaginary,
        );
        // The acoustic solvers use exp(+i omega t): v = i grad(p)/(omega rho).
        let velocity =
            field_output::acoustic_particle_velocity(pressure_gradient, omega, material.density);
        field_output::accumulate_cell_vector(&mut sums, &mut counts, tetrahedron, velocity)?;
    }
    field_output::average_nodal_vectors(&mut sums, &counts);
    Ok(sums)
}

fn face_area(points: [V3; 3]) -> f64 {
    0.5 * dot(
        cross(sub(points[1], points[0]), sub(points[2], points[0])),
        cross(sub(points[1], points[0]), sub(points[2], points[0])),
    )
    .sqrt()
}

fn tetrahedron_quadrature(rule: &str) -> Result<Vec<[f64; 5]>, String> {
    match rule {
        "standard" => Ok(crate::TET_Q
            .iter()
            .map(|q| [q[0], q[1], q[2], q[3], q[4] * 6.0])
            .collect()),
        "centroid" => Ok(vec![[0.25, 0.25, 0.25, 0.25, 1.0]]),
        _ => Err(format!(
            "Unknown constitutive quadrature rule {rule:?}; use 'standard' or 'centroid'."
        )),
    }
}

fn assemble(
    mesh: &Mesh3D,
    quadrature_materials: &[Vec<AcousticMaterial>],
    constitutive_quadrature: &str,
    boundaries: &HashMap<usize, BoundaryCondition>,
    frequency_hz: f64,
    parameters: &HashMap<String, model_expression::Scalar>,
    periodic: &scalar2d::PeriodicDofMap,
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
    let quadrature = tetrahedron_quadrature(constitutive_quadrature)?;
    let incident_fields = compile_incident_fields(boundaries)?;
    for (element, (tetrahedron, material_tag)) in mesh.tetrahedra.iter().enumerate() {
        let element_quadrature = quadrature_materials.get(element).ok_or_else(|| format!("Mesh tetrahedron uses material tag {material_tag}, which has no constitutive quadrature materials."))?;
        let points = tetrahedron.map(|node| mesh.nodes[node]);
        let (volume, gradients) = tetra_geometry(points)?;
        for (q_index, q) in quadrature.iter().enumerate() {
            let material = element_quadrature.get(q_index).ok_or(
                "Constitutive quadrature material count does not match the integration rule.",
            )?;
            let reaction =
                -omega * omega / (material.density * material.sound_speed * material.sound_speed);
            for i in 0..4 {
                for j in 0..4 {
                    let gradient_product = dot(gradients[i], gradients[j]);
                    let coefficient = volume
                        * q[4]
                        * (gradient_product / material.density + reaction * q[i] * q[j]);
                    scalar2d::add_entry(
                        &mut matrix,
                        tetrahedron[i],
                        tetrahedron[j],
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
    for (face, tag) in &mesh.boundary_faces {
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
        let area = face_area(face.map(|node| mesh.nodes[node]));
        if !area.is_finite() || area <= 0.0 {
            return Err("The boundary mesh contains a degenerate triangle.".into());
        }
        match condition {
            BoundaryCondition::Dirichlet(value) => {
                for node in face {
                    let symbols =
                        scalar2d::symbols_at_point(parameters, frequency_hz, mesh.nodes[*node]);
                    let prescribed =
                        scalar2d::quantity(value, &symbols, &format!("boundary[{tag}].value"))?;
                    scalar2d::insert_dirichlet_constraint(
                        &mut dirichlet,
                        periodic,
                        *node,
                        C {
                            re: prescribed,
                            im: 0.0,
                        },
                    )?;
                }
            }
            BoundaryCondition::Neumann(value) => {
                const QUADRATURE: [[f64; 4]; 3] = [
                    [2.0 / 3.0, 1.0 / 6.0, 1.0 / 6.0, 1.0 / 3.0],
                    [1.0 / 6.0, 2.0 / 3.0, 1.0 / 6.0, 1.0 / 3.0],
                    [1.0 / 6.0, 1.0 / 6.0, 2.0 / 3.0, 1.0 / 3.0],
                ];
                for q in QUADRATURE {
                    let point = (0..3).fold([0.0; 3], |mut p, i| {
                        for axis in 0..3 {
                            p[axis] += q[i] * mesh.nodes[face[i]][axis];
                        }
                        p
                    });
                    let symbols = scalar2d::symbols_at_point(parameters, frequency_hz, point);
                    let prescribed =
                        scalar2d::quantity(value, &symbols, &format!("boundary[{tag}].value"))?;
                    for i in 0..3 {
                        rhs_im[face[i]] -= omega * area * q[3] * q[i] * prescribed;
                    }
                }
            }
            BoundaryCondition::Impedance { impedance, .. } => {
                const QUADRATURE: [[f64; 4]; 3] = [
                    [2.0 / 3.0, 1.0 / 6.0, 1.0 / 6.0, 1.0 / 3.0],
                    [1.0 / 6.0, 2.0 / 3.0, 1.0 / 6.0, 1.0 / 3.0],
                    [1.0 / 6.0, 1.0 / 6.0, 2.0 / 3.0, 1.0 / 3.0],
                ];
                for q in QUADRATURE {
                    let point = (0..3).fold([0.0; 3], |mut p, i| {
                        for axis in 0..3 {
                            p[axis] += q[i] * mesh.nodes[face[i]][axis];
                        }
                        p
                    });
                    let symbols = scalar2d::symbols_at_point(parameters, frequency_hz, point);
                    let impedance = scalar2d::quantity(
                        impedance,
                        &symbols,
                        &format!("boundary[{tag}].impedance"),
                    )?;
                    if !impedance.is_finite() || impedance == 0.0 {
                        return Err(format!(
                            "Acoustic impedance on boundary tag {tag} must be finite and nonzero at every integration point."
                        ));
                    }
                    let scale = omega / impedance;
                    let weight = area * q[3];
                    for i in 0..3 {
                        for j in 0..3 {
                            scalar2d::add_entry(
                                &mut matrix,
                                face[i],
                                face[j],
                                C {
                                    re: 0.0,
                                    im: scale * weight * q[i] * q[j],
                                },
                            );
                        }
                    }
                    if let Some(expression) = incident_fields.get(tag) {
                        let pressure = expression
                            .evaluate(&symbols, &format!("boundary[{tag}].incidentField"))?;
                        for i in 0..3 {
                            let source_weight = 2.0 * scale * weight * q[i];
                            rhs_re[face[i]] -= source_weight * pressure.im;
                            rhs_im[face[i]] += source_weight * pressure.re;
                        }
                    }
                }
            }
        }
    }
    for tag in boundaries.keys() {
        if !used_boundary_tags.contains(tag) {
            return Err(format!(
                "Acoustic boundary selection tag {tag} has no boundary faces in the mesh."
            ));
        }
    }
    Ok((matrix, rhs_re, rhs_im, dirichlet))
}

fn run_model(
    model: &Value,
    mesh: &Mesh3D,
    map: &Value,
    output_directory: &Path,
    frequency_hz: f64,
    sweep_csv: &str,
    constitutive_quadrature: &str,
    solver_backend: &str,
    solver_ordering: &str,
    solver_threads: usize,
    cache: &mut SolverCache,
) -> Result<(), String> {
    // The native fullmeta-fem coordinator validates the full model before dispatch.
    // Keep this path focused on extracting solve inputs and checking FEM constraints.
    let component = model
        .pointer("/model")
        .ok_or("The acoustic model is missing.")?;
    let parameters = model
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let parameter_symbols = model_expression::resolve_parameters(parameters, model)?;
    let frequencies = [frequency_hz];
    let material_tags = scalar2d::selection_tags(map, "physicalGroups")?;
    let boundary_tags = scalar2d::selection_tags(map, "boundarySelections")?;
    let periodic_entities = mesh
        .boundary_faces
        .iter()
        .map(|(face, tag)| (face.to_vec(), *tag))
        .collect::<Vec<_>>();
    let periodic =
        scalar2d::periodic_dof_map_for_entities(model, map, &mesh.nodes, &periodic_entities, 3)?;
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
        "acoustic-3d",
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
    let quadrature = tetrahedron_quadrature(constitutive_quadrature)?;
    for frequency in frequencies.iter().copied() {
        let constant_values =
            compiled_materials.position_independent_values(&parameter_symbols, frequency)?;
        let constant_materials = constant_values
            .iter()
            .map(|(&tag, values)| {
                crate::acoustic2d::acoustic_material_from_values(values)
                    .map(|material| (tag, material))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut materials = Vec::<AcousticMaterial>::with_capacity(mesh.tetrahedra.len());
        let mut quadrature_materials =
            Vec::<Vec<AcousticMaterial>>::with_capacity(mesh.tetrahedra.len());
        for (element, (tetrahedron, tag)) in mesh.tetrahedra.iter().enumerate() {
            if !material_rows_by_tag.contains_key(tag) {
                return Err(format!(
                    "Mesh tetrahedron {element} uses material tag {tag}, which has no acoustic material assignment."
                ));
            }
            let points = tetrahedron.map(|node| mesh.nodes[node]);
            let centroid: [f64; 3] = std::array::from_fn(|axis| {
                (points[0][axis] + points[1][axis] + points[2][axis] + points[3][axis]) / 4.0
            });
            let centroid_material = if let Some(&material) = constant_materials.get(tag) {
                material
            } else {
                let values = compiled_materials.evaluate(
                    *tag,
                    &parameter_symbols,
                    frequency,
                    centroid,
                    &format!("tetrahedron {element}"),
                )?;
                crate::acoustic2d::acoustic_material_from_values(&values)?
            };
            materials.push(centroid_material);
            let mut samples = Vec::with_capacity(quadrature.len());
            if let Some(&material) = constant_materials.get(tag) {
                samples.resize(quadrature.len(), material);
            } else {
                for q in &quadrature {
                    let point = (0..4).fold([0.0; 3], |mut point, local| {
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
                        &format!("tetrahedron {element} quadrature point"),
                    )?;
                    samples.push(crate::acoustic2d::acoustic_material_from_values(&values)?);
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
            cache,
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
        let velocity_z = velocity.iter().map(|value| value[2]).collect::<Vec<_>>();
        let pressure = solution_re
            .iter()
            .zip(&solution_im)
            .map(|(&re, &im)| C { re, im })
            .collect::<Vec<_>>();
        let field_values = crate::acoustic2d::acoustic_field_values(
            model,
            &pressure,
            [&velocity_x, &velocity_y, &velocity_z],
        )?;
        let fields = field_output::declared_component_fields(model, &field_values)?;
        let component = model.pointer("/model").ok_or("model is required.")?;
        let domain_cells = mesh
            .tetrahedra
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
        let mut boundary_cells = mesh
            .boundary_faces
            .iter()
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
        boundary_cells.extend(
            mesh.edge_selections
                .iter()
                .map(|(nodes, tag)| {
                    Ok(crate::integration::Simplex {
                        nodes: nodes.to_vec(),
                        physical_tag: *tag,
                        material_tag: Some(adjacent_material_tag_for_edge(mesh, *nodes)?),
                        operator_id: None,
                        element_index: None,
                        adjacent_element_index: None,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        );
        let global_variables = crate::integration::evaluate_model_variables(
            model,
            component,
            map,
            3,
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
            &mesh.tetrahedra,
            [0, 1, 2],
            [0.0; 3],
        )?;
        let output = json!({
            "schema":"fullmeta-acoustic-field/1", "frequencyHz":frequency, "dimension":3, "fieldComponentCoordinateSystem":"model", "nodes":mesh.nodes,
            "tetrahedra":mesh.tetrahedra.iter().map(|(nodes,tag)| json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>(),
            "pressureRealPa":solution_re,"pressureImaginaryPa":solution_im,
            "fields":fields,
            "materialFieldsByCell":material_fields,
            "globalVariables":global_variables,
            "fieldConvention":"exp(+i omega t)",
            "derivedFieldRecovery":"arithmetic mean of incident tetrahedron values at each node",
            "solver":{"backend":solver_backend,"ordering":solver_ordering,"threads":solver_threads,"constitutiveQuadrature":constitutive_quadrature,"residual":residual,"unknowns":periodic.dof_count},"maximumPressureMagnitudePa":max_pressure
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
            "Acoustic 3D · {frequency:.3} Hz · {} scalar trace DOFs · {} tetrahedra · residual {:.2e} · max |p| {:.5e} Pa",
            periodic.dof_count,
            mesh.tetrahedra.len(),
            residual,
            max_pressure
        );
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

pub fn run_request(request_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut cache = SolverCache::default();
    run_request_with_cache(request_path, &mut cache)
}

pub fn run_request_with_cache(
    request_path: &Path,
    cache: &mut SolverCache,
) -> Result<(), Box<dyn std::error::Error>> {
    let request: Value = serde_json::from_slice(&fs::read(request_path)?)?;
    crate::solver_request::validate_single_case_request(&request, "acoustic-3d")
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
    let model_path = scalar2d::resolve_from(base, model_path);
    let mesh_path = scalar2d::resolve_from(base, mesh_path);
    let map_path = scalar2d::resolve_from(base, map_path);
    let output_directory = PathBuf::from(output_directory)
        .is_absolute()
        .then(|| PathBuf::from(output_directory))
        .unwrap_or_else(|| base.join(output_directory));
    let model: Value = serde_json::from_slice(&fs::read(&model_path)?)?;
    crate::solver_request::validate_study_reference(&model, &request)
        .map_err(|error| format!("Solver request: {error}"))?;
    let frequency = crate::solver_request::single_case_frequency(&request)
        .map_err(|error| format!("Solver request: {error}"))?;
    let map: Value = serde_json::from_slice(&fs::read(&map_path)?)?;
    if map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6") {
        return Err("The acoustic-3d solver requires fullmeta-mesh-map/6.".into());
    }
    if map.get("dimension").and_then(Value::as_u64) != Some(3) {
        return Err("The acoustic-3d solver requires a dimension-3 mesh map.".into());
    }
    let mesh = parse_mesh(&fs::read_to_string(&mesh_path)?)?;
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
            "The acoustic-3d solver currently supports the pressure formulation only.".into(),
        );
    }
    if request.get("elementOrder").and_then(Value::as_u64) != Some(1) {
        return Err("The acoustic-3d solver currently supports first-order elements only.".into());
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
        cache,
    )
    .map_err(Into::into)
}
