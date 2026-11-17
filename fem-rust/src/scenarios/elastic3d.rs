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
                if nodes[id].is_some() {
                    return Err(format!("Duplicate mesh node {id}."));
                }
                nodes[id] = Some(point);
            }
            "F" if row.len() >= 6 => {
                let id = row[1].parse::<usize>().map_err(|error| error.to_string())?;
                if faces.len() <= id {
                    faces.resize(id + 1, None);
                }
                if faces[id].is_some() {
                    return Err(format!("Duplicate mesh boundary face {id}."));
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
                if edges[id].is_some() {
                    return Err(format!("Duplicate mesh edge {id}."));
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
                if tetrahedra[id].is_some() {
                    return Err(format!("Duplicate mesh tetrahedron {id}."));
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
        return Err("The elastic 3D solver requires a FULLMETA_MESH/1 file.".into());
    }
    if nodes.is_empty()
        || nodes.iter().any(Option::is_none)
        || tetrahedra.is_empty()
        || tetrahedra.iter().any(Option::is_none)
        || faces.iter().any(Option::is_none)
        || edges.iter().any(Option::is_none)
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

#[derive(Clone, Copy, Debug)]
struct Material {
    density: f64,
    lambda: f64,
    mu: f64,
    loss: f64,
}

fn material_from_values(
    values: &HashMap<String, model_expression::Scalar>,
) -> Result<Material, String> {
    let get = |key: &str| {
        values
            .get(key)
            .map(|v| v.value)
            .ok_or_else(|| format!("Elastic material is missing {key}."))
    };
    let density = get("rho")?;
    let young = get("young")?;
    let poisson = get("nu")?;
    let loss = values.get("densityLoss").map_or(0.0, |v| v.value);
    if !density.is_finite() || density <= 0.0 {
        return Err("Elastic density must be finite and positive.".into());
    }
    if !young.is_finite() || young <= 0.0 {
        return Err("Young's modulus must be finite and positive.".into());
    }
    if !poisson.is_finite() || poisson <= -1.0 || poisson >= 0.5 {
        return Err("Poisson ratio must be finite and strictly between -1 and 0.5.".into());
    }
    if !loss.is_finite() || loss < 0.0 {
        return Err("Elastic density loss factor must be finite and nonnegative.".into());
    }
    let mu = young / (2.0 * (1.0 + poisson));
    let lambda = young * poisson / ((1.0 + poisson) * (1.0 - 2.0 * poisson));
    if !lambda.is_finite() || !mu.is_finite() {
        return Err("Elastic Lamé coefficients must be finite.".into());
    }
    Ok(Material {
        density,
        lambda,
        mu,
        loss,
    })
}

#[derive(Clone)]
enum Boundary {
    Displacement {
        scalar: model_expression::CompiledComplexQuantity,
        field: Option<[model_expression::CompiledComplexQuantity; 3]>,
    },
    Traction {
        scalar: model_expression::CompiledComplexQuantity,
        field: Option<[model_expression::CompiledComplexQuantity; 3]>,
    },
    Impedance(model_expression::CompiledComplexQuantity),
}

fn boundary_quantity(
    value: &Value,
    unit: &str,
    path: &str,
) -> Result<model_expression::CompiledComplexQuantity, String> {
    let actual = value
        .get("unit")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.unit is required."))?;
    if model_expression::unit(actual)?
        .1
        .iter()
        .zip(model_expression::unit(unit)?.1)
        .any(|(a, b)| (a - b).abs() > 1e-10)
    {
        return Err(format!("{path} must have units compatible with {unit}."));
    }
    model_expression::CompiledComplexQuantity::compile(value, path)
}

fn compile_boundary(value: &Value, path: &str) -> Result<Boundary, String> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.type is required."))?;
    if kind == "impedance" {
        if value.get("incidentField").is_some() || value.get("polarization").is_some() {
            return Err(format!(
                "{path}: elastic impedance incident excitation is not implemented; use a Neumann traction."
            ));
        }
        return Ok(Boundary::Impedance(boundary_quantity(
            value
                .get("impedance")
                .ok_or_else(|| format!("{path}.impedance is required."))?,
            "Pa*s/m",
            &format!("{path}.impedance"),
        )?));
    }
    let unit = match kind {
        "dirichlet" => "m",
        "neumann" => "Pa",
        _ => {
            return Err(format!(
                "{path}: unsupported elastic boundary type {kind:?}."
            ));
        }
    };
    let scalar = boundary_quantity(
        value
            .get("value")
            .ok_or_else(|| format!("{path}.value is required."))?,
        unit,
        &format!("{path}.value"),
    )?;
    let field = value
        .get("fieldValues")
        .map(|value| {
            let a = value.as_array().filter(|a| a.len() == 3).ok_or_else(|| {
                format!("{path}.fieldValues requires three Cartesian components.")
            })?;
            Ok::<_, String>([
                boundary_quantity(&a[0], unit, &format!("{path}.fieldValues[0]"))?,
                boundary_quantity(&a[1], unit, &format!("{path}.fieldValues[1]"))?,
                boundary_quantity(&a[2], unit, &format!("{path}.fieldValues[2]"))?,
            ])
        })
        .transpose()?;
    Ok(if kind == "dirichlet" {
        Boundary::Displacement { scalar, field }
    } else {
        Boundary::Traction { scalar, field }
    })
}

const FACE_Q: [[f64; 3]; 3] = [
    [2.0 / 3.0, 1.0 / 6.0, 1.0 / 6.0],
    [1.0 / 6.0, 2.0 / 3.0, 1.0 / 6.0],
    [1.0 / 6.0, 1.0 / 6.0, 2.0 / 3.0],
];

fn face_area(points: [V3; 3]) -> f64 {
    let n = cross(sub(points[1], points[0]), sub(points[2], points[0]));
    0.5 * dot(n, n).sqrt()
}

fn elastic_element(
    gradients: [V3; 4],
    volume: f64,
    material: Material,
    omega: f64,
) -> [[C; 12]; 12] {
    let mut matrix = [[C::default(); 12]; 12];
    for a in 0..4 {
        for b in 0..4 {
            for i in 0..3 {
                for j in 0..3 {
                    let g = gradients[a];
                    let h = gradients[b];
                    let stiffness = volume
                        * (material.lambda * g[i] * h[j]
                            + material.mu * (g[j] * h[i] + if i == j { dot(g, h) } else { 0.0 }));
                    let mass = if i == j {
                        material.density * volume * (if a == b { 2.0 } else { 1.0 }) / 20.0
                    } else {
                        0.0
                    };
                    matrix[3 * a + i][3 * b + j] = C {
                        re: stiffness - omega * omega * mass,
                        im: omega * omega * mass * material.loss,
                    };
                }
            }
        }
    }
    matrix
}

fn stress(gradients: [V3; 4], displacement: [[C; 3]; 4], material: Material) -> [C; 6] {
    let mut gradient = [[C::default(); 3]; 3];
    for a in 0..4 {
        for i in 0..3 {
            for j in 0..3 {
                gradient[i][j].re += displacement[a][i].re * gradients[a][j];
                gradient[i][j].im += displacement[a][i].im * gradients[a][j];
            }
        }
    }
    let trace = C {
        re: (0..3).map(|i| gradient[i][i].re).sum(),
        im: (0..3).map(|i| gradient[i][i].im).sum(),
    };
    [(0, 0), (1, 1), (2, 2), (0, 1), (1, 2), (0, 2)].map(|(i, j)| C {
        re: material.mu * (gradient[i][j].re + gradient[j][i].re)
            + if i == j {
                material.lambda * trace.re
            } else {
                0.0
            },
        im: material.mu * (gradient[i][j].im + gradient[j][i].im)
            + if i == j {
                material.lambda * trace.im
            } else {
                0.0
            },
    })
}

type System = (
    Vec<BTreeMap<usize, C>>,
    Vec<f64>,
    Vec<f64>,
    HashMap<usize, C>,
);

fn assemble(
    mesh: &Mesh3D,
    samples: &[Vec<Material>],
    boundaries: &HashMap<usize, Boundary>,
    frequency: f64,
    parameters: &HashMap<String, model_expression::Scalar>,
) -> Result<System, String> {
    let omega = std::f64::consts::TAU * frequency;
    let count = 3 * mesh.nodes.len();
    let mut matrix = vec![BTreeMap::<usize, C>::new(); count];
    let mut rhs_re = vec![0.0; count];
    let mut rhs_im = vec![0.0; count];
    let mut constraints = HashMap::<usize, C>::new();
    let mut owners = HashMap::<[usize; 3], usize>::new();
    for (nodes, _) in &mesh.tetrahedra {
        for opposite in 0..4 {
            let mut face = [0; 3];
            let mut k = 0;
            for (i, &node) in nodes.iter().enumerate() {
                if i != opposite {
                    face[k] = node;
                    k += 1;
                }
            }
            *owners.entry(face_key(face)).or_default() += 1;
        }
    }
    for (index, (nodes, _)) in mesh.tetrahedra.iter().enumerate() {
        let (volume, gradients) = tetra_geometry(nodes.map(|node| mesh.nodes[node]))?;
        let local_samples = samples
            .get(index)
            .filter(|v| v.len() == crate::TET_Q.len())
            .ok_or("Elastic material quadrature samples are missing.")?;
        for (q, material) in crate::TET_Q.iter().zip(local_samples) {
            let weight = volume * q[4] * 6.0;
            for a in 0..4 {
                for b in 0..4 {
                    for i in 0..3 {
                        for j in 0..3 {
                            let ga = gradients[a];
                            let gb = gradients[b];
                            let stiffness = weight
                                * (material.lambda * ga[i] * gb[j]
                                    + material.mu
                                        * (ga[j] * gb[i] + if i == j { dot(ga, gb) } else { 0.0 }));
                            let mass = if i == j {
                                weight * material.density * q[a] * q[b]
                            } else {
                                0.0
                            };
                            scalar2d::add_entry(
                                &mut matrix,
                                3 * nodes[a] + i,
                                3 * nodes[b] + j,
                                C {
                                    re: stiffness - omega * omega * mass,
                                    im: omega * omega * mass * material.loss,
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    let mut used = HashSet::new();
    for (face, tag) in &mesh.boundary_faces {
        let Some(boundary) = boundaries.get(tag) else {
            continue;
        };
        used.insert(*tag);
        if owners.get(&face_key(*face)) != Some(&1) {
            return Err(format!(
                "Elastic boundary tag {tag} must target an exterior tetrahedral face."
            ));
        }
        let points = face.map(|node| mesh.nodes[node]);
        let area = face_area(points);
        if !area.is_finite() || area <= 0.0 {
            return Err("Elastic boundary has a degenerate face.".into());
        }
        match boundary {
            Boundary::Displacement { scalar, field } => {
                for &node in face {
                    let symbols =
                        scalar2d::symbols_at_point(parameters, frequency, mesh.nodes[node]);
                    let legacy = scalar.evaluate(&symbols, &format!("boundary[{tag}].value"))?;
                    if legacy.re.hypot(legacy.im) > 1e-14 {
                        return Err(format!(
                            "Elastic Dirichlet scalar value must be zero on tag {tag}; use fieldValues."
                        ));
                    }
                    for axis in 0..3 {
                        let v = if let Some(field) = field {
                            field[axis].evaluate(
                                &symbols,
                                &format!("boundary[{tag}].fieldValues[{axis}]"),
                            )?
                        } else {
                            legacy
                        };
                        if !v.re.is_finite() || !v.im.is_finite() {
                            return Err(format!("Elastic Dirichlet tag {tag} is non-finite."));
                        }
                        let prescribed = C { re: v.re, im: v.im };
                        let dof = 3 * node + axis;
                        if let Some(previous) = constraints.get(&dof) {
                            let scale = previous
                                .re
                                .hypot(previous.im)
                                .max(prescribed.re.hypot(prescribed.im))
                                .max(1e-12);
                            if (previous.re - prescribed.re).hypot(previous.im - prescribed.im)
                                > 1e-9 * scale
                            {
                                return Err(format!(
                                    "Conflicting displacement constraints at node {node}, axis {axis}."
                                ));
                            }
                        } else {
                            constraints.insert(dof, prescribed);
                        }
                    }
                }
            }
            Boundary::Traction { scalar, field } => {
                for q in FACE_Q {
                    let point =
                        std::array::from_fn(|axis| (0..3).map(|a| q[a] * points[a][axis]).sum());
                    let symbols = scalar2d::symbols_at_point(parameters, frequency, point);
                    let legacy = scalar.evaluate(&symbols, &format!("boundary[{tag}].value"))?;
                    if legacy.re.hypot(legacy.im) > 1e-14 {
                        return Err(format!(
                            "Elastic Neumann scalar value must be zero on tag {tag}; use fieldValues."
                        ));
                    }
                    for axis in 0..3 {
                        let v = if let Some(field) = field {
                            field[axis].evaluate(
                                &symbols,
                                &format!("boundary[{tag}].fieldValues[{axis}]"),
                            )?
                        } else {
                            legacy
                        };
                        if !v.re.is_finite() || !v.im.is_finite() {
                            return Err(format!("Elastic traction on tag {tag} is non-finite."));
                        }
                        for a in 0..3 {
                            let dof = 3 * face[a] + axis;
                            let weight = area * q[a] / 3.0;
                            rhs_re[dof] += weight * v.re;
                            rhs_im[dof] += weight * v.im;
                        }
                    }
                }
            }
            Boundary::Impedance(impedance) => {
                for q in FACE_Q {
                    let point =
                        std::array::from_fn(|axis| (0..3).map(|a| q[a] * points[a][axis]).sum());
                    let symbols = scalar2d::symbols_at_point(parameters, frequency, point);
                    let z = impedance.evaluate(&symbols, &format!("boundary[{tag}].impedance"))?;
                    if !z.re.is_finite() || !z.im.is_finite() || z.re < 0.0 {
                        return Err(format!(
                            "Elastic impedance on tag {tag} must be finite with nonnegative real part."
                        ));
                    }
                    for a in 0..3 {
                        for b in 0..3 {
                            for axis in 0..3 {
                                let weight = omega * area * q[a] * q[b] / 3.0;
                                scalar2d::add_entry(
                                    &mut matrix,
                                    3 * face[a] + axis,
                                    3 * face[b] + axis,
                                    C {
                                        re: -weight * z.im,
                                        im: weight * z.re,
                                    },
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    for tag in boundaries.keys() {
        if !used.contains(tag) {
            return Err(format!(
                "Elastic boundary selection tag {tag} has no exterior mesh faces."
            ));
        }
    }
    Ok((matrix, rhs_re, rhs_im, constraints))
}

fn run_model(
    model: &Value,
    mesh: &Mesh3D,
    map: &Value,
    output_directory: &Path,
    frequency: f64,
    sweep_csv: &str,
    solver_backend: &str,
    solver_ordering: &str,
    solver_threads: usize,
    field_files: bool,
    cache: &mut SolverCache,
) -> Result<(), String> {
    let component = model
        .pointer("/model")
        .ok_or("The elastic model is missing.")?;
    let physics =
        crate::model_schema::physics(component).ok_or("model.physics must be an object.")?;
    if crate::model_schema::physics_periodic_conditions(physics)
        .any(|v| v.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        return Err("Elastic periodic conditions are not implemented.".into());
    }
    let parameter_rows = component
        .get("parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let parameters = model_expression::resolve_parameters(parameter_rows, model)?;
    let material_tags = scalar2d::selection_tags(map, "physicalGroups")?;
    let boundary_tags = scalar2d::selection_tags(map, "boundarySelections")?;
    let mut material_rows = HashMap::new();
    for row in component
        .get("materials")
        .and_then(Value::as_array)
        .ok_or("model.materials must be an array.")?
    {
        let selection = row
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("Elastic material needs selectionId.")?;
        let tag = *material_tags.get(selection).ok_or_else(|| {
            format!("Material selection {selection:?} is absent from the mesh map.")
        })?;
        if material_rows.insert(tag, row.clone()).is_some() {
            return Err(format!("Duplicate elastic material tag {tag}."));
        }
    }
    let compiled =
        crate::material::CompiledMaterialAssignments::compile("elastic-3d", &material_rows)?;
    let constant = compiled.position_independent_values(&parameters, frequency)?;
    let constant = constant
        .iter()
        .map(|(&tag, v)| material_from_values(v).map(|m| (tag, m)))
        .collect::<Result<HashMap<_, _>, _>>()?;
    let mut samples = Vec::with_capacity(mesh.tetrahedra.len());
    let mut center_materials = Vec::with_capacity(mesh.tetrahedra.len());
    for (element, (nodes, tag)) in mesh.tetrahedra.iter().enumerate() {
        if let Some(&m) = constant.get(tag) {
            samples.push(vec![m; crate::TET_Q.len()]);
            center_materials.push(m);
            continue;
        }
        let points = nodes.map(|n| mesh.nodes[n]);
        let center = std::array::from_fn(|axis| points.iter().map(|p| p[axis]).sum::<f64>() / 4.0);
        center_materials.push(material_from_values(&compiled.evaluate(
            *tag,
            &parameters,
            frequency,
            center,
            &format!("tetrahedron {element}"),
        )?)?);
        let q_points = crate::TET_Q
            .iter()
            .map(|q| std::array::from_fn(|axis| (0..4).map(|a| q[a] * points[a][axis]).sum()))
            .collect::<Vec<_>>();
        let vals = compiled.evaluate_batch(
            *tag,
            &parameters,
            frequency,
            &q_points,
            &format!("tetrahedron {element}"),
        )?;
        samples.push(
            vals.iter()
                .map(material_from_values)
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    let mut boundaries = HashMap::new();
    for value in crate::model_schema::physics_boundary_conditions(physics)
        .filter(|b| b.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        let selection = value
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("Elastic boundary requires selectionId.")?;
        let tag = *boundary_tags.get(selection).ok_or_else(|| {
            format!("Boundary selection {selection:?} is absent from the mesh map.")
        })?;
        if boundaries
            .insert(
                tag,
                compile_boundary(value, &format!("boundary {selection}"))?,
            )
            .is_some()
        {
            return Err(format!("More than one elastic boundary targets tag {tag}."));
        }
    }
    crate::solver_common::stage_started("assembly");
    let (mut matrix, mut rhs_re, mut rhs_im, constraints) =
        assemble(mesh, &samples, &boundaries, frequency, &parameters)?;
    scalar2d::apply_complex_dirichlet(&mut matrix, &mut rhs_re, &mut rhs_im, &constraints)?;
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
    let mut displacement: [Vec<C>; 3] =
        std::array::from_fn(|_| Vec::with_capacity(mesh.nodes.len()));
    for node in 0..mesh.nodes.len() {
        for axis in 0..3 {
            let i = 3 * node + axis;
            displacement[axis].push(C {
                re: solution_re[i],
                im: solution_im[i],
            });
        }
    }
    let max_uz = displacement[2]
        .iter()
        .map(|v| v.re.hypot(v.im))
        .fold(0.0, f64::max);
    fs::create_dir_all(output_directory)
        .map_err(|e| format!("Could not create elastic output folder: {e}"))?;
    crate::solver_common::stage_started("output");
    if field_files {
        let mut cell_stresses = Vec::<[C; 6]>::with_capacity(mesh.tetrahedra.len());
        let mut sums = vec![[C::default(); 6]; mesh.nodes.len()];
        let mut counts = vec![0usize; mesh.nodes.len()];
        for (cell, (nodes, _)) in mesh.tetrahedra.iter().enumerate() {
            let (_, gradients) = tetra_geometry(nodes.map(|n| mesh.nodes[n]))?;
            let local = nodes.map(|n| std::array::from_fn(|axis| displacement[axis][n]));
            let s = stress(gradients, local, center_materials[cell]);
            for &n in nodes {
                counts[n] += 1;
                for k in 0..6 {
                    sums[n][k].re += s[k].re;
                    sums[n][k].im += s[k].im;
                }
            }
            cell_stresses.push(s);
        }
        let mut fields = HashMap::<String, Vec<C>>::new();
        for (axis, name) in ["ux", "uy", "uz"].iter().enumerate() {
            fields.insert((*name).into(), displacement[axis].clone());
        }
        for (k, name) in [
            "sigma_xx", "sigma_yy", "sigma_zz", "sigma_xy", "sigma_yz", "sigma_xz",
        ]
        .iter()
        .enumerate()
        {
            let vals = sums
                .iter()
                .zip(&counts)
                .map(|(s, &count)| {
                    if count > 0 {
                        C {
                            re: s[k].re / count as f64,
                            im: s[k].im / count as f64,
                        }
                    } else {
                        C::default()
                    }
                })
                .collect();
            fields.insert((*name).into(), vals);
        }
        let declared = field_output::declared_component_fields(model, &fields)?;
        let material_fields = crate::material::fields_by_cell(
            model,
            map,
            frequency,
            &mesh.nodes,
            &mesh.tetrahedra,
            [0, 1, 2],
            [0.0; 3],
        )?;
        let output = json!({"schema":"fullmeta-elastic-field/1","frequencyHz":frequency,"dimension":3,
        "fieldComponentCoordinateSystem":"model","nodes":mesh.nodes,
        "tetrahedra":mesh.tetrahedra.iter().map(|(nodes,tag)|json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>(),
        "fields":declared,"materialFieldsByCell":material_fields,
        "stressByCell":{"components":["sigma_xx","sigma_yy","sigma_zz","sigma_xy","sigma_yz","sigma_xz"],
            "real":cell_stresses.iter().map(|s|s.map(|v|v.re)).collect::<Vec<_>>(),
            "imag":cell_stresses.iter().map(|s|s.map(|v|v.im)).collect::<Vec<_>>(),"unit":"Pa"},
        "fieldConvention":"exp(+i omega t)","derivedFieldRecovery":"arithmetic mean of incident tetrahedron centroid stresses at each node; raw stressByCell retained",
        "solver":{"backend":solver_backend,"ordering":solver_ordering,"threads":solver_threads,"constitutiveQuadrature":"standard","residual":residual,"unknowns":3*mesh.nodes.len()},"maximumUzMagnitudeM":max_uz});
        fs::write(
            output_directory.join(format!("displacement_{frequency:.6}Hz.json")),
            serde_json::to_vec(&output).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("Could not write elastic field: {e}"))?;
    }
    fs::write(output_directory.join(sweep_csv),format!("frequency_Hz,unknowns,residual,max_uz_magnitude_m\n{frequency:.12},{},{residual:.8e},{max_uz:.12e}\n",3*mesh.nodes.len())).map_err(|e|format!("Could not write elastic sweep: {e}"))?;
    println!(
        "Elastic 3D · {frequency:.3} Hz · {} displacement DOFs · {} tetrahedra · residual {residual:.2e} · max |uz| {max_uz:.5e} m",
        3 * mesh.nodes.len(),
        mesh.tetrahedra.len()
    );
    Ok(())
}

pub fn run_request_with_cache(
    request_path: &Path,
    cache: &mut SolverCache,
) -> Result<(), Box<dyn std::error::Error>> {
    let request: Value = serde_json::from_slice(&fs::read(request_path)?)?;
    crate::solver_request::validate_single_case_request(&request, "elastic-3d")
        .map_err(|e| format!("Solver request: {e}"))?;
    let base = request_path.parent().unwrap_or(Path::new("."));
    let path = |key: &str| -> Result<PathBuf, String> {
        let value = request
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("request.{key} is required."))?;
        Ok(scalar2d::resolve_from(base, value))
    };
    let model: Value = serde_json::from_slice(&fs::read(path("modelFile")?)?)?;
    crate::solver_request::validate_study_reference(&model, &request)
        .map_err(|e| format!("Solver request: {e}"))?;
    let frequency = crate::solver_request::single_case_frequency(&request)
        .map_err(|e| format!("Solver request: {e}"))?;
    let map: Value = serde_json::from_slice(&fs::read(path("modelMapFile")?)?)?;
    if map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6")
        || map.get("dimension").and_then(Value::as_u64) != Some(3)
    {
        return Err("Elastic 3D requires a dimension-3 fullmeta-mesh-map/6.".into());
    }
    let mesh = parse_mesh(&fs::read_to_string(path("meshFile")?)?)?;
    if request.get("fieldFormulation").and_then(Value::as_str) != Some("displacement")
        || request.get("elementOrder").and_then(Value::as_u64) != Some(1)
    {
        return Err(
            "Elastic 3D requires displacement formulation and first-order elements.".into(),
        );
    }
    if request
        .get("constitutiveQuadrature")
        .and_then(Value::as_str)
        != Some("standard")
    {
        return Err("Elastic 3D requires standard quadrature; centroid quadrature underintegrates the mass matrix.".into());
    }
    let string = |key: &str| -> Result<&str, String> {
        request
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("request.{key} is required."))
    };
    let threads = request
        .get("solverThreads")
        .and_then(Value::as_u64)
        .ok_or("request.solverThreads is required.")? as usize;
    let study_id = string("studyId")?;
    let field_files = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .and_then(|studies| {
            studies
                .iter()
                .find(|study| study.get("id").and_then(Value::as_str) == Some(study_id))
        })
        .and_then(|study| study.pointer("/output/fieldFiles"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    run_model(
        &model,
        &mesh,
        &map,
        &path("outputDirectory")?,
        frequency,
        string("sweepCsv")?,
        string("solverBackend")?,
        string("solverOrdering")?,
        threads,
        field_files,
        cache,
    )
    .map_err(Into::into)
}

pub(crate) fn run(arguments: &[String]) -> Result<i32, String> {
    crate::scenario_solver_cli::run_with_fallback(
        arguments,
        "elastic-3d",
        |request, cache| run_request_with_cache(request, cache),
        crate::run_shared_operation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // Collapse a tetrahedral waveguide onto uniform cross-section motion.
    // The omitted components are constrained, so this is a normal-incidence
    // plane-strain check of the actual 3D volume and boundary assembly.
    fn plane_wave_reflection(axis: usize, z_factor: f64) -> f64 {
        const N: usize = 40;
        let node = |i: usize, y: usize, z: usize| 4 * i + 2 * y + z;
        let mut nodes = Vec::new();
        for i in 0..=N {
            for y in 0..2 {
                for z in 0..2 {
                    nodes.push([i as f64 / N as f64, y as f64, z as f64]);
                }
            }
        }
        let mut tetrahedra = Vec::new();
        for i in 0..N {
            // Six tetrahedra about the 000--111 body diagonal.
            for p in [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ] {
                let mut xyz = [0usize; 3];
                let mut v = [node(i, 0, 0); 4];
                for step in 0..3 {
                    xyz[p[step]] = 1;
                    v[step + 1] = node(i + xyz[0], xyz[1], xyz[2]);
                }
                tetrahedra.push((v, 1));
            }
        }
        let mesh = Mesh3D {
            nodes,
            tetrahedra,
            boundary_faces: vec![
                ([node(N, 0, 0), node(N, 1, 0), node(N, 1, 1)], 2),
                ([node(N, 0, 0), node(N, 1, 1), node(N, 0, 1)], 2),
            ],
            edge_selections: vec![],
        };
        let material = Material {
            density: 1.0,
            lambda: 2.0,
            mu: 1.0,
            loss: 0.0,
        };
        let speed = if axis == 0 { 2.0 } else { 1.0 };
        let z = speed * z_factor;
        let bc =
            json!({"type":"impedance", "impedance":{"expression":z.to_string(),"unit":"Pa*s/m"}});
        let boundaries = HashMap::from([(2, compile_boundary(&bc, "impedance").unwrap())]);
        let samples = vec![vec![material; crate::TET_Q.len()]; mesh.tetrahedra.len()];
        let (matrix, _, _, _) =
            assemble(&mesh, &samples, &boundaries, speed, &HashMap::new()).unwrap();
        let mut reduced = vec![BTreeMap::new(); N + 1];
        for (row, entries) in matrix.iter().enumerate().filter(|(i, _)| i % 3 == axis) {
            for (&column, &value) in entries.iter().filter(|(i, _)| *i % 3 == axis) {
                scalar2d::add_entry(&mut reduced, row / 12, column / 12, value);
            }
        }
        // Prescribe u(0)=1, retaining free complex amplitudes in the interior.
        let mut rhs_re = vec![0.0; N + 1];
        let mut rhs_im = vec![0.0; N + 1];
        for row in 1..=N {
            if let Some(value) = reduced[row].remove(&0) {
                rhs_re[row] = -value.re;
                rhs_im[row] = -value.im;
            }
        }
        reduced[0].clear();
        reduced[0].insert(0, C { re: 1.0, im: 0.0 });
        rhs_re[0] = 1.0;
        let (real, imag, _) = solve_linear_system(
            &reduced,
            &rhs_re,
            &rhs_im,
            1e-10,
            "faer",
            "default",
            1,
            &mut SolverCache::default(),
        )
        .unwrap();
        let u = |i| C {
            re: real[i],
            im: imag[i],
        };
        let wave = |i: usize, sign: f64| {
            let phase = sign * std::f64::consts::TAU * i as f64 / N as f64;
            C {
                re: phase.cos(),
                im: phase.sin(),
            }
        };
        let subtract = |a: C, b: C| C {
            re: a.re - b.re,
            im: a.im - b.im,
        };
        let mul = crate::solver_common::c_mul;
        let numerator = subtract(mul(u(10), wave(20, -1.0)), mul(u(20), wave(10, -1.0)));
        let denominator = subtract(mul(u(20), wave(10, 1.0)), mul(u(10), wave(20, 1.0)));
        numerator.re.hypot(numerator.im) / denominator.re.hypot(denominator.im)
    }

    #[test]
    fn impedance_reflects_longitudinal_and_shear_waves_as_expected() {
        for axis in [0, 1] {
            let matched = plane_wave_reflection(axis, 1.0);
            let mismatched = plane_wave_reflection(axis, 2.0);
            assert!(matched < 0.01, "axis {axis}: matched reflection {matched}");
            assert!(
                (mismatched - 1.0 / 3.0).abs() < 0.01,
                "axis {axis}: mismatched reflection {mismatched}"
            );
        }
    }

    fn tetra() -> (Mesh3D, Material) {
        (
            Mesh3D {
                nodes: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
                tetrahedra: vec![([0, 1, 2, 3], 1)],
                boundary_faces: vec![([0, 1, 2], 2)],
                edge_selections: vec![],
            },
            Material {
                density: 2.,
                lambda: 3.,
                mu: 4.,
                loss: 0.05,
            },
        )
    }

    #[test]
    fn rigid_motion_has_no_elastic_stress_or_stiffness() {
        let (mesh, m) = tetra();
        let nodes = mesh.tetrahedra[0].0;
        let (volume, g) = tetra_geometry(nodes.map(|n| mesh.nodes[n])).unwrap();
        let k = elastic_element(g, volume, m, 0.0);
        let translation = [1.3, -2.1, 0.7];
        let rotation = [0.4, -0.2, 0.8];
        let local = nodes.map(|n| {
            let x = mesh.nodes[n];
            let r = cross(rotation, x);
            std::array::from_fn(|i| C {
                re: translation[i] + r[i],
                im: 0.0,
            })
        });
        let s = stress(g, local, m);
        assert!(s.iter().all(|v| v.re.abs() < 1e-12));
        for row in 0..12 {
            let v = (0..12)
                .map(|j| k[row][j].re * local[j / 3][j % 3].re)
                .sum::<f64>();
            assert!(v.abs() < 1e-12);
        }
    }

    #[test]
    fn affine_displacement_recovers_analytical_stress() {
        let (mesh, m) = tetra();
        let nodes = mesh.tetrahedra[0].0;
        let (_, g) = tetra_geometry(nodes.map(|n| mesh.nodes[n])).unwrap();
        // u=(2x+3y, -y, 4z); epsilon_xy=3/2, trace=5.
        let local = nodes.map(|n| {
            let x = mesh.nodes[n];
            [
                C {
                    re: 2.0 * x[0] + 3.0 * x[1],
                    im: 0.,
                },
                C { re: -x[1], im: 0. },
                C {
                    re: 4.0 * x[2],
                    im: 0.,
                },
            ]
        });
        let s = stress(g, local, m);
        for (value, expected) in s.iter().zip([31., 7., 47., 12., 0., 0.]) {
            assert!((value.re - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn consistent_mass_and_density_loss_have_correct_sign() {
        let (mesh, m) = tetra();
        let nodes = mesh.tetrahedra[0].0;
        let (volume, g) = tetra_geometry(nodes.map(|n| mesh.nodes[n])).unwrap();
        let undamped = elastic_element(g, volume, Material { loss: 0., ..m }, 0.0);
        let omega = 3.;
        let damped = elastic_element(g, volume, m, omega);
        let lossless = elastic_element(g, volume, Material { loss: 0., ..m }, omega);
        let mut imaginary_work = 0.0;
        for a in 0..4 {
            for b in 0..4 {
                let mass = m.density * volume * (if a == b { 2. } else { 1. }) / 20.;
                assert!(
                    (damped[3 * a][3 * b].re - undamped[3 * a][3 * b].re + omega * omega * mass)
                        .abs()
                        < 1e-12
                );
                assert!((damped[3 * a][3 * b].im - omega * omega * mass * m.loss).abs() < 1e-12);
                assert_eq!(lossless[3 * a][3 * b].im, 0.0);
                assert_eq!(lossless[3 * a][3 * b].re, damped[3 * a][3 * b].re);
                imaginary_work += damped[3 * a][3 * b].im;
            }
        }
        // For unit uniform displacement, mean dissipated power is
        // omega/2 times the imaginary quadratic form of the operator.
        let power = omega * imaginary_work / 2.0;
        let expected = omega.powi(3) * m.density * volume * m.loss / 2.0;
        assert!(power > 0.0 && (power - expected).abs() < 1e-12);
        let samples = vec![vec![m; crate::TET_Q.len()]];
        let (assembled, _, _, _) = assemble(
            &mesh,
            &samples,
            &HashMap::new(),
            omega / std::f64::consts::TAU,
            &HashMap::new(),
        )
        .unwrap();
        for i in 0..12 {
            for j in 0..12 {
                let got = assembled[i].get(&j).copied().unwrap_or_default();
                assert!((got.re - damped[i][j].re).abs() < 1e-11);
                assert!((got.im - damped[i][j].im).abs() < 1e-11);
            }
        }
    }

    #[test]
    fn degenerate_tetrahedron_is_rejected() {
        assert!(tetra_geometry([[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 0.]]).is_err());
    }

    #[test]
    fn material_ranges_are_checked() {
        let scalar = |value| model_expression::Scalar {
            value,
            dimension: [0.0; 5],
        };
        let mut properties = HashMap::from([
            ("rho".to_string(), scalar(2700.0)),
            ("young".to_string(), scalar(70e9)),
            ("nu".to_string(), scalar(0.33)),
            ("densityLoss".to_string(), scalar(0.01)),
        ]);
        assert!(material_from_values(&properties).is_ok());
        for (name, bad) in [
            ("rho", 0.0),
            ("young", -1.0),
            ("nu", 0.5),
            ("nu", -1.0),
            ("densityLoss", -0.1),
        ] {
            let previous = properties.insert(name.into(), scalar(bad)).unwrap();
            assert!(material_from_values(&properties).is_err(), "{name}={bad}");
            properties.insert(name.into(), previous);
        }
    }

    fn quantity(expression: &str, unit: &str) -> Value {
        json!({"expression":expression,"unit":unit})
    }

    #[test]
    fn traction_resultant_is_cartesian_complex_and_orientation_independent() {
        let (mut mesh, m) = tetra();
        let data = json!({"type":"neumann","value":quantity("0","Pa"),"fieldValues":[quantity("0","Pa"),quantity("0","Pa"),quantity("2+4*i","Pa")]});
        let boundary = compile_boundary(&data, "traction").unwrap();
        let samples = vec![vec![m; crate::TET_Q.len()]];
        let boundaries = HashMap::from([(2, boundary)]);
        let (_, re, im, _) = assemble(&mesh, &samples, &boundaries, 1.0, &HashMap::new()).unwrap();
        assert!((re.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((im.iter().sum::<f64>() - 2.0).abs() < 1e-12);
        assert!(
            (0..mesh.nodes.len())
                .all(|node| re[3 * node].abs() < 1e-12 && re[3 * node + 1].abs() < 1e-12)
        );
        mesh.boundary_faces[0].0.reverse();
        let (_, reversed_re, reversed_im, _) =
            assemble(&mesh, &samples, &boundaries, 1.0, &HashMap::new()).unwrap();
        for i in 0..re.len() {
            assert!((re[i] - reversed_re[i]).abs() < 1e-12);
            assert!((im[i] - reversed_im[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn impedance_is_passive_and_zero_neumann_matches_free() {
        let (mesh, m) = tetra();
        let samples = vec![vec![m; crate::TET_Q.len()]];
        let zero = json!({"type":"neumann","value":quantity("0","Pa")});
        let impedance = json!({"type":"impedance","impedance":quantity("5","Pa*s/m")});
        let b0 = HashMap::from([(2, compile_boundary(&zero, "zero").unwrap())]);
        let bz = HashMap::from([(2, compile_boundary(&impedance, "impedance").unwrap())]);
        let (free, free_re, free_im, _) =
            assemble(&mesh, &samples, &HashMap::new(), 1.0, &HashMap::new()).unwrap();
        let (zero, zero_re, zero_im, _) =
            assemble(&mesh, &samples, &b0, 1.0, &HashMap::new()).unwrap();
        assert_eq!(free_re, zero_re);
        assert_eq!(free_im, zero_im);
        for i in 0..12 {
            for j in 0..12 {
                let x = free[i].get(&j).copied().unwrap_or_default();
                let y = zero[i].get(&j).copied().unwrap_or_default();
                assert!((x.re - y.re).abs() < 1e-12 && (x.im - y.im).abs() < 1e-12);
            }
        }
        let (loaded, _, _, _) = assemble(&mesh, &samples, &bz, 1.0, &HashMap::new()).unwrap();
        let dissipative: f64 = (0..3)
            .flat_map(|a| (0..3).map(move |b| (3 * a, 3 * b)))
            .map(|(i, j)| {
                loaded[i].get(&j).map_or(0., |v| v.im) - free[i].get(&j).map_or(0., |v| v.im)
            })
            .sum();
        assert!((dissipative - std::f64::consts::TAU * 5.0 * 0.5).abs() < 1e-11);
    }

    #[test]
    fn incompatible_displacements_at_shared_nodes_are_rejected() {
        let (mut mesh, m) = tetra();
        mesh.boundary_faces.push(([0, 1, 3], 3));
        let data = |x| {
            json!({"type":"dirichlet","value":quantity("0","m"),
            "fieldValues":[quantity(x,"m"),quantity("0","m"),quantity("0","m")]})
        };
        let boundaries = HashMap::from([
            (2, compile_boundary(&data("0"), "left").unwrap()),
            (3, compile_boundary(&data("1"), "right").unwrap()),
        ]);
        let message = assemble(
            &mesh,
            &[vec![m; crate::TET_Q.len()]],
            &boundaries,
            1.0,
            &HashMap::new(),
        )
        .err()
        .unwrap();
        assert!(message.contains("Conflicting displacement"), "{message}");
    }
}
