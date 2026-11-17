use crate::field_output;
use crate::mesh2d::{self, Mesh2D};
use crate::model_expression;
use crate::scalar2d;
use crate::{C, SolverCache, solve_linear_system};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

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
        field: Option<[model_expression::CompiledComplexQuantity; 2]>,
    },
    Traction {
        scalar: model_expression::CompiledComplexQuantity,
        field: Option<[model_expression::CompiledComplexQuantity; 2]>,
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
            let a = value
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or_else(|| format!("{path}.fieldValues requires two Cartesian components."))?;
            Ok::<_, String>([
                boundary_quantity(&a[0], unit, &format!("{path}.fieldValues[0]"))?,
                boundary_quantity(&a[1], unit, &format!("{path}.fieldValues[1]"))?,
            ])
        })
        .transpose()?;
    Ok(if kind == "dirichlet" {
        Boundary::Displacement { scalar, field }
    } else {
        Boundary::Traction { scalar, field }
    })
}

fn reduced_lambda(material: Material, formulation: &str) -> Result<f64, String> {
    match formulation {
        "planeStrain" => Ok(material.lambda),
        "planeStress" => {
            Ok(2.0 * material.lambda * material.mu / (material.lambda + 2.0 * material.mu))
        }
        _ => Err(format!(
            "Unsupported elastic 2D formulation {formulation:?}."
        )),
    }
}

type System = (
    Vec<BTreeMap<usize, C>>,
    Vec<f64>,
    Vec<f64>,
    HashMap<usize, C>,
);

fn assemble(
    mesh: &Mesh2D,
    samples: &[Vec<Material>],
    boundaries: &HashMap<usize, Boundary>,
    formulation: &str,
    frequency: f64,
    parameters: &HashMap<String, model_expression::Scalar>,
) -> Result<System, String> {
    let omega = std::f64::consts::TAU * frequency;
    let mut matrix = vec![BTreeMap::new(); 2 * mesh.nodes.len()];
    let mut rhs_re = vec![0.0; matrix.len()];
    let mut rhs_im = rhs_re.clone();
    let mut constraints = HashMap::new();
    let mut owners = HashMap::<[usize; 2], usize>::new();
    for (nodes, _) in &mesh.triangles {
        for mut edge in [
            [nodes[0], nodes[1]],
            [nodes[1], nodes[2]],
            [nodes[2], nodes[0]],
        ] {
            edge.sort_unstable();
            *owners.entry(edge).or_default() += 1;
        }
    }
    for (cell, (nodes, _)) in mesh.triangles.iter().enumerate() {
        let (area, gradients) = mesh2d::triangle_geometry(nodes.map(|n| mesh.nodes[n]))?;
        let values = samples
            .get(cell)
            .filter(|v| v.len() == crate::TRI_Q.len())
            .ok_or("Elastic triangle quadrature materials are missing.")?;
        for (q, material) in crate::TRI_Q.iter().zip(values) {
            let lambda = reduced_lambda(*material, formulation)?;
            let weight = area * q[3];
            for a in 0..3 {
                for b in 0..3 {
                    for i in 0..2 {
                        for j in 0..2 {
                            let ga = gradients[a];
                            let gb = gradients[b];
                            let stiffness = weight
                                * (lambda * ga[i] * gb[j]
                                    + material.mu
                                        * (ga[j] * gb[i]
                                            + if i == j {
                                                ga[0] * gb[0] + ga[1] * gb[1]
                                            } else {
                                                0.0
                                            }));
                            let mass = if i == j {
                                weight * material.density * q[a] * q[b]
                            } else {
                                0.0
                            };
                            scalar2d::add_entry(
                                &mut matrix,
                                2 * nodes[a] + i,
                                2 * nodes[b] + j,
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
    let edge_q = [
        (0.5 - 0.5 / 3.0_f64.sqrt(), 0.5),
        (0.5 + 0.5 / 3.0_f64.sqrt(), 0.5),
    ];
    for (edge, tag) in &mesh.boundary_edges {
        let Some(bc) = boundaries.get(tag) else {
            continue;
        };
        used.insert(*tag);
        let mut key = *edge;
        key.sort_unstable();
        if owners.get(&key) != Some(&1) {
            return Err(format!(
                "Elastic boundary tag {tag} must target an exterior edge."
            ));
        }
        let points = edge.map(|n| mesh.nodes[n]);
        let length = (points[1][0] - points[0][0]).hypot(points[1][1] - points[0][1]);
        if !length.is_finite() || length <= 0.0 {
            return Err("Elastic boundary has a degenerate edge.".into());
        }
        match bc {
            Boundary::Displacement { scalar, field } => {
                for &node in edge {
                    let symbols =
                        scalar2d::symbols_at_point(parameters, frequency, mesh.nodes[node]);
                    let legacy = scalar.evaluate(&symbols, &format!("boundary[{tag}].value"))?;
                    if !legacy.re.is_finite()
                        || !legacy.im.is_finite()
                        || legacy.re.hypot(legacy.im) > 1e-14
                    {
                        return Err(format!(
                            "Elastic Dirichlet scalar must be zero on tag {tag}; use fieldValues."
                        ));
                    }
                    for axis in 0..2 {
                        let value = if let Some(f) = field {
                            f[axis].evaluate(
                                &symbols,
                                &format!("boundary[{tag}].fieldValues[{axis}]"),
                            )?
                        } else {
                            legacy
                        };
                        if !value.re.is_finite() || !value.im.is_finite() {
                            return Err(format!("Non-finite displacement on tag {tag}."));
                        }
                        let next = C {
                            re: value.re,
                            im: value.im,
                        };
                        let dof = 2 * node + axis;
                        if let Some(prev) = constraints.get(&dof) {
                            let prev: &C = prev;
                            let scale = prev
                                .re
                                .hypot(prev.im)
                                .max(next.re.hypot(next.im))
                                .max(1e-12);
                            if (prev.re - next.re).hypot(prev.im - next.im) > 1e-9 * scale {
                                return Err(format!(
                                    "Conflicting displacement constraints at node {node}, axis {axis}."
                                ));
                            }
                        } else {
                            constraints.insert(dof, next);
                        }
                    }
                }
            }
            Boundary::Traction { scalar, field } => {
                for (s, weight) in edge_q {
                    let shape = [1.0 - s, s];
                    let point =
                        std::array::from_fn(|i| shape[0] * points[0][i] + shape[1] * points[1][i]);
                    let symbols = scalar2d::symbols_at_point(parameters, frequency, point);
                    let legacy = scalar.evaluate(&symbols, &format!("boundary[{tag}].value"))?;
                    if !legacy.re.is_finite()
                        || !legacy.im.is_finite()
                        || legacy.re.hypot(legacy.im) > 1e-14
                    {
                        return Err(format!(
                            "Elastic Neumann scalar must be zero on tag {tag}; use fieldValues."
                        ));
                    }
                    for axis in 0..2 {
                        let value = if let Some(f) = field {
                            f[axis].evaluate(
                                &symbols,
                                &format!("boundary[{tag}].fieldValues[{axis}]"),
                            )?
                        } else {
                            legacy
                        };
                        if !value.re.is_finite() || !value.im.is_finite() {
                            return Err(format!("Non-finite traction on tag {tag}."));
                        }
                        for a in 0..2 {
                            let dof = 2 * edge[a] + axis;
                            rhs_re[dof] += length * weight * shape[a] * value.re;
                            rhs_im[dof] += length * weight * shape[a] * value.im;
                        }
                    }
                }
            }
            Boundary::Impedance(z) => {
                for (s, weight) in edge_q {
                    let shape = [1.0 - s, s];
                    let point =
                        std::array::from_fn(|i| shape[0] * points[0][i] + shape[1] * points[1][i]);
                    let symbols = scalar2d::symbols_at_point(parameters, frequency, point);
                    let value = z.evaluate(&symbols, &format!("boundary[{tag}].impedance"))?;
                    if !value.re.is_finite() || !value.im.is_finite() || value.re < 0.0 {
                        return Err(format!(
                            "Elastic impedance on tag {tag} must be finite and passive."
                        ));
                    }
                    for a in 0..2 {
                        for b in 0..2 {
                            for axis in 0..2 {
                                let w = omega * length * weight * shape[a] * shape[b];
                                scalar2d::add_entry(
                                    &mut matrix,
                                    2 * edge[a] + axis,
                                    2 * edge[b] + axis,
                                    C {
                                        re: -w * value.im,
                                        im: w * value.re,
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
                "Elastic boundary selection tag {tag} has no exterior mesh edges."
            ));
        }
    }
    Ok((matrix, rhs_re, rhs_im, constraints))
}

fn stress(
    gradients: [[f64; 2]; 3],
    values: [[C; 2]; 3],
    material: Material,
    formulation: &str,
) -> Result<[C; 4], String> {
    let lambda = reduced_lambda(material, formulation)?;
    let mut result = [C::default(); 4];
    for (part, imaginary) in [false, true].into_iter().enumerate() {
        let mut g = [[0.0; 2]; 2];
        for a in 0..3 {
            for i in 0..2 {
                for j in 0..2 {
                    g[i][j] += if imaginary {
                        values[a][i].im
                    } else {
                        values[a][i].re
                    } * gradients[a][j];
                }
            }
        }
        let trace = g[0][0] + g[1][1];
        let s = [
            lambda * trace + 2.0 * material.mu * g[0][0],
            lambda * trace + 2.0 * material.mu * g[1][1],
            if formulation == "planeStrain" {
                material.lambda * trace
            } else {
                0.0
            },
            material.mu * (g[0][1] + g[1][0]),
        ];
        for i in 0..4 {
            if part == 0 {
                result[i].re = s[i]
            } else {
                result[i].im = s[i]
            }
        }
    }
    Ok(result)
}

fn run_model(
    model: &Value,
    mesh: &Mesh2D,
    map: &Value,
    output: &Path,
    frequency: f64,
    formulation: &str,
    sweep_csv: &str,
    backend: &str,
    ordering: &str,
    threads: usize,
    field_files: bool,
    cache: &mut SolverCache,
) -> Result<(), String> {
    reduced_lambda(
        Material {
            density: 1.0,
            lambda: 1.0,
            mu: 1.0,
            loss: 0.0,
        },
        formulation,
    )?;
    let component = model
        .pointer("/model")
        .ok_or("The elastic model is missing.")?;
    let physics =
        crate::model_schema::physics(component).ok_or("model.physics must be an object.")?;
    if physics.get("formulation").and_then(Value::as_str) != Some(formulation) {
        return Err("Elastic 2D Physics and Study formulations must match.".into());
    }
    if crate::model_schema::physics_periodic_conditions(physics)
        .any(|v| v.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        return Err("Elastic periodic conditions are not implemented.".into());
    }
    let rows = component
        .get("parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let parameters = model_expression::resolve_parameters(rows, model)?;
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
        let tag = *material_tags
            .get(selection)
            .ok_or_else(|| format!("Material selection {selection:?} is absent from mesh map."))?;
        if material_rows.insert(tag, row.clone()).is_some() {
            return Err(format!("Duplicate elastic material tag {tag}."));
        }
    }
    let compiled =
        crate::material::CompiledMaterialAssignments::compile("elastic-2d", &material_rows)?;
    let constants = compiled
        .position_independent_values(&parameters, frequency)?
        .iter()
        .map(|(&tag, v)| material_from_values(v).map(|m| (tag, m)))
        .collect::<Result<HashMap<_, _>, _>>()?;
    let mut samples = Vec::with_capacity(mesh.triangles.len());
    let mut centers = Vec::with_capacity(mesh.triangles.len());
    for (cell, (nodes, tag)) in mesh.triangles.iter().enumerate() {
        if let Some(&m) = constants.get(tag) {
            samples.push(vec![m; crate::TRI_Q.len()]);
            centers.push(m);
            continue;
        }
        let points = nodes.map(|n| mesh.nodes[n]);
        let center = std::array::from_fn(|axis| points.iter().map(|p| p[axis]).sum::<f64>() / 3.0);
        centers.push(material_from_values(&compiled.evaluate(
            *tag,
            &parameters,
            frequency,
            center,
            &format!("triangle {cell}"),
        )?)?);
        let qpoints = crate::TRI_Q
            .iter()
            .map(|q| std::array::from_fn(|axis| (0..3).map(|a| q[a] * points[a][axis]).sum()))
            .collect::<Vec<_>>();
        let vals = compiled.evaluate_batch(
            *tag,
            &parameters,
            frequency,
            &qpoints,
            &format!("triangle {cell}"),
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
        let tag = *boundary_tags
            .get(selection)
            .ok_or_else(|| format!("Boundary selection {selection:?} is absent from mesh map."))?;
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
    let (mut matrix, mut rhs_re, mut rhs_im, constraints) = assemble(
        mesh,
        &samples,
        &boundaries,
        formulation,
        frequency,
        &parameters,
    )?;
    scalar2d::apply_complex_dirichlet(&mut matrix, &mut rhs_re, &mut rhs_im, &constraints)?;
    let (re, im, residual) = solve_linear_system(
        &matrix, &rhs_re, &rhs_im, 1e-7, backend, ordering, threads, cache,
    )?;
    let displacement: [Vec<C>; 2] = std::array::from_fn(|axis| {
        (0..mesh.nodes.len())
            .map(|n| C {
                re: re[2 * n + axis],
                im: im[2 * n + axis],
            })
            .collect()
    });
    let max_uy = displacement[1]
        .iter()
        .map(|v| v.re.hypot(v.im))
        .fold(0.0, f64::max);
    fs::create_dir_all(output).map_err(|e| format!("Could not create elastic output: {e}"))?;
    crate::solver_common::stage_started("output");
    if field_files {
        let mut cell_stresses = Vec::<[C; 4]>::new();
        let mut sums = vec![[C::default(); 4]; mesh.nodes.len()];
        let mut counts = vec![0usize; mesh.nodes.len()];
        for (cell, (nodes, _)) in mesh.triangles.iter().enumerate() {
            let (_, g) = mesh2d::triangle_geometry(nodes.map(|n| mesh.nodes[n]))?;
            let s = stress(
                g,
                nodes.map(|n| std::array::from_fn(|axis| displacement[axis][n])),
                centers[cell],
                formulation,
            )?;
            for &n in nodes {
                counts[n] += 1;
                for k in 0..4 {
                    sums[n][k].re += s[k].re;
                    sums[n][k].im += s[k].im;
                }
            }
            cell_stresses.push(s);
        }
        let mut fields = HashMap::<String, Vec<C>>::new();
        for (axis, name) in ["ux", "uy"].iter().enumerate() {
            fields.insert((*name).into(), displacement[axis].clone());
        }
        for (k, name) in ["sigma_xx", "sigma_yy", "sigma_zz", "sigma_xy"]
            .iter()
            .enumerate()
        {
            fields.insert(
                (*name).into(),
                sums.iter()
                    .zip(&counts)
                    .map(|(s, &n)| {
                        if n > 0 {
                            C {
                                re: s[k].re / n as f64,
                                im: s[k].im / n as f64,
                            }
                        } else {
                            C::default()
                        }
                    })
                    .collect(),
            );
        }
        let declared = field_output::declared_component_fields(model, &fields)?;
        let material_fields = crate::material::fields_by_cell(
            model,
            map,
            frequency,
            &mesh.nodes,
            &mesh.triangles,
            [0, 1, 2],
            [0.0; 3],
        )?;
        let data = json!({"schema":"fullmeta-elastic-field/1","frequencyHz":frequency,"dimension":2,"formulation":formulation,
            "fieldComponentCoordinateSystem":"model","nodes":mesh.nodes,
            "triangles":mesh.triangles.iter().map(|(nodes,tag)|json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>(),
            "fields":declared,"materialFieldsByCell":material_fields,
            "stressByCell":{"components":["sigma_xx","sigma_yy","sigma_zz","sigma_xy"],"real":cell_stresses.iter().map(|s|s.map(|v|v.re)).collect::<Vec<_>>(),"imag":cell_stresses.iter().map(|s|s.map(|v|v.im)).collect::<Vec<_>>(),"unit":"Pa"},
            "fieldConvention":"exp(+i omega t)","derivedFieldRecovery":"arithmetic mean of incident triangle centroid stresses at each node; raw stressByCell retained",
            "solver":{"backend":backend,"ordering":ordering,"threads":threads,"constitutiveQuadrature":"standard","residual":residual,"unknowns":2*mesh.nodes.len()},"maximumUyMagnitudeM":max_uy});
        fs::write(
            output.join(format!("displacement_{frequency:.6}Hz.json")),
            serde_json::to_vec(&data).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("Could not write elastic field: {e}"))?;
    }
    fs::write(output.join(sweep_csv),format!("frequency_Hz,unknowns,residual,max_uy_magnitude_m\n{frequency:.12},{},{residual:.8e},{max_uy:.12e}\n",2*mesh.nodes.len())).map_err(|e|format!("Could not write elastic sweep: {e}"))?;
    println!(
        "Elastic 2D {formulation} · {frequency:.3} Hz · {} DOFs · {} triangles · residual {residual:.2e} · max |uy| {max_uy:.5e} m",
        2 * mesh.nodes.len(),
        mesh.triangles.len()
    );
    Ok(())
}

pub fn run_request_with_cache(
    request_path: &Path,
    cache: &mut SolverCache,
) -> Result<(), Box<dyn std::error::Error>> {
    let request: Value = serde_json::from_slice(&fs::read(request_path)?)?;
    crate::solver_request::validate_single_case_request(&request, "elastic-2d")
        .map_err(|e| format!("Solver request: {e}"))?;
    let base = request_path.parent().unwrap_or(Path::new("."));
    let path = |key: &str| -> Result<PathBuf, String> {
        Ok(scalar2d::resolve_from(
            base,
            request
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("request.{key} is required."))?,
        ))
    };
    let model: Value = serde_json::from_slice(&fs::read(path("modelFile")?)?)?;
    crate::solver_request::validate_study_reference(&model, &request)
        .map_err(|e| format!("Solver request: {e}"))?;
    let frequency = crate::solver_request::single_case_frequency(&request)
        .map_err(|e| format!("Solver request: {e}"))?;
    let map: Value = serde_json::from_slice(&fs::read(path("modelMapFile")?)?)?;
    if map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6")
        || map.get("dimension").and_then(Value::as_u64) != Some(2)
    {
        return Err("Elastic 2D requires a dimension-2 fullmeta-mesh-map/6.".into());
    }
    let mesh = mesh2d::parse_mesh_v2(&fs::read_to_string(path("meshFile")?)?)?;
    if request.get("elementOrder").and_then(Value::as_u64) != Some(1) {
        return Err("Elastic 2D requires first-order elements.".into());
    }
    if request
        .get("constitutiveQuadrature")
        .and_then(Value::as_str)
        != Some("standard")
    {
        return Err(
            "Elastic 2D requires standard quadrature; centroid quadrature underintegrates mass."
                .into(),
        );
    }
    let string = |key: &str| -> Result<&str, String> {
        request
            .get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("request.{key} is required."))
    };
    let formulation = string("fieldFormulation")?;
    if formulation != "planeStress" && formulation != "planeStrain" {
        return Err(format!("Unsupported elastic 2D formulation {formulation:?}.").into());
    }
    let threads = request
        .get("solverThreads")
        .and_then(Value::as_u64)
        .ok_or("request.solverThreads is required.")? as usize;
    let study_id = string("studyId")?;
    let field_files = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .find(|s| s.get("id").and_then(Value::as_str) == Some(study_id))
        })
        .and_then(|s| s.pointer("/output/fieldFiles"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    run_model(
        &model,
        &mesh,
        &map,
        &path("outputDirectory")?,
        frequency,
        formulation,
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
        "elastic-2d",
        |request, cache| run_request_with_cache(request, cache),
        crate::run_shared_operation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn material() -> Material {
        Material {
            density: 2.0,
            lambda: 3.0,
            mu: 4.0,
            loss: 0.05,
        }
    }
    fn mesh() -> Mesh2D {
        Mesh2D {
            nodes: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            triangles: vec![([0, 1, 2], 1)],
            boundary_edges: vec![([0, 1], 2)],
            integration_edges: vec![],
        }
    }
    fn quantity(s: &str, unit: &str) -> Value {
        json!({"expression":s,"unit":unit})
    }
    #[test]
    fn plane_stress_and_strain_affine_and_rigid_motion() {
        let m = material();
        let mesh = mesh();
        let (_, g) = mesh2d::triangle_geometry(mesh.triangles[0].0.map(|n| mesh.nodes[n])).unwrap();
        let values = mesh
            .nodes
            .iter()
            .map(|p| {
                [
                    C {
                        re: 2. * p[0] + 3. * p[1],
                        im: 0.,
                    },
                    C { re: -p[1], im: 0. },
                ]
            })
            .collect::<Vec<_>>();
        for form in ["planeStress", "planeStrain"] {
            let s = stress(g, [values[0], values[1], values[2]], m, form).unwrap();
            let l = reduced_lambda(m, form).unwrap();
            assert!((s[0].re - (l + 4. * m.mu)).abs() < 1e-12);
            assert!((s[1].re - (l - 2. * m.mu)).abs() < 1e-12);
            assert!((s[3].re - 3. * m.mu).abs() < 1e-12);
            assert!((s[2].re - if form == "planeStrain" { m.lambda } else { 0. }).abs() < 1e-12);
            let rigid = mesh
                .nodes
                .iter()
                .map(|p| {
                    [
                        C {
                            re: 1. - p[1],
                            im: 0.,
                        },
                        C {
                            re: 2. + p[0],
                            im: 0.,
                        },
                    ]
                })
                .collect::<Vec<_>>();
            assert!(
                stress(g, [rigid[0], rigid[1], rigid[2]], m, form)
                    .unwrap()
                    .iter()
                    .all(|v| v.re.abs() < 1e-12)
            );
        }
        assert!(reduced_lambda(m, "wrong").is_err());
    }
    #[test]
    fn consistent_mass_damping_and_reversal() {
        let m = material();
        let mut mesh = mesh();
        let samples = vec![vec![m; crate::TRI_Q.len()]];
        let (dynamic, _, _, _) = assemble(
            &mesh,
            &samples,
            &HashMap::new(),
            "planeStress",
            1.0,
            &HashMap::new(),
        )
        .unwrap();
        let (stat, _, _, _) = assemble(
            &mesh,
            &samples,
            &HashMap::new(),
            "planeStress",
            0.0,
            &HashMap::new(),
        )
        .unwrap();
        let omega = std::f64::consts::TAU;
        for a in 0..3 {
            for b in 0..3 {
                let mass = m.density * 0.5 * (if a == b { 2. } else { 1. }) / 12.;
                assert!(
                    (dynamic[2 * a][&(2 * b)].re - stat[2 * a][&(2 * b)].re + omega * omega * mass)
                        .abs()
                        < 1e-11
                );
                assert!(
                    (dynamic[2 * a][&(2 * b)].im - omega * omega * mass * m.loss).abs() < 1e-11
                );
            }
        }
        let lossless = vec![vec![Material { loss: 0.0, ..m }; crate::TRI_Q.len()]];
        let (no_loss, _, _, _) = assemble(
            &mesh,
            &lossless,
            &HashMap::new(),
            "planeStress",
            1.0,
            &HashMap::new(),
        )
        .unwrap();
        let mut quadratic = 0.0;
        for a in 0..3 {
            for b in 0..3 {
                let damped = dynamic[2 * a][&(2 * b)];
                let undamped = no_loss[2 * a][&(2 * b)];
                assert_eq!(undamped.im, 0.0);
                assert_eq!(undamped.re, damped.re);
                quadratic += damped.im;
            }
        }
        let power = omega * quadratic / 2.0;
        let expected = omega.powi(3) * m.density * 0.5 * m.loss / 2.0;
        assert!(power > 0.0 && (power - expected).abs() < 1e-11);
        mesh.triangles[0].0.swap(1, 2);
        let (reverse, _, _, _) = assemble(
            &mesh,
            &samples,
            &HashMap::new(),
            "planeStress",
            1.0,
            &HashMap::new(),
        )
        .unwrap();
        for i in 0..6 {
            for (&j, v) in &dynamic[i] {
                assert!((v.re - reverse[i][&j].re).abs() < 1e-11);
                assert!((v.im - reverse[i][&j].im).abs() < 1e-11);
            }
        }
    }
    #[test]
    fn traction_free_impedance_and_constraint_checks() {
        let mut mesh = mesh();
        let m = material();
        let samples = vec![vec![m; crate::TRI_Q.len()]];
        let traction = json!({"type":"neumann","value":quantity("0","Pa"),"fieldValues":[quantity("2+i","Pa"),quantity("3","Pa")]});
        let boundaries = HashMap::from([(2, compile_boundary(&traction, "load").unwrap())]);
        let (_, r, i, _) = assemble(
            &mesh,
            &samples,
            &boundaries,
            "planeStrain",
            1.,
            &HashMap::new(),
        )
        .unwrap();
        assert!((r.iter().step_by(2).sum::<f64>() - 2.).abs() < 1e-12);
        assert!((i.iter().step_by(2).sum::<f64>() - 1.).abs() < 1e-12);
        mesh.boundary_edges[0].0.reverse();
        let (_, r2, i2, _) = assemble(
            &mesh,
            &samples,
            &boundaries,
            "planeStrain",
            1.,
            &HashMap::new(),
        )
        .unwrap();
        assert!(r.iter().zip(&r2).all(|(a, b)| (a - b).abs() < 1e-12));
        assert!(i.iter().zip(&i2).all(|(a, b)| (a - b).abs() < 1e-12));
        let zero = json!({"type":"impedance","impedance":quantity("0","Pa*s/m")});
        let z = HashMap::from([(2, compile_boundary(&zero, "impedance").unwrap())]);
        let (a, _, _, _) =
            assemble(&mesh, &samples, &z, "planeStrain", 1., &HashMap::new()).unwrap();
        let (b, _, _, _) = assemble(
            &mesh,
            &samples,
            &HashMap::new(),
            "planeStrain",
            1.,
            &HashMap::new(),
        )
        .unwrap();
        for i in 0..6 {
            for (&j, v) in &a[i] {
                let w = b[i].get(&j).copied().unwrap_or_default();
                assert!((v.re - w.re).abs() < 1e-12 && (v.im - w.im).abs() < 1e-12);
            }
        }
        let bad = json!({"type":"impedance","impedance":quantity("-1","Pa*s/m")});
        let z = HashMap::from([(2, compile_boundary(&bad, "impedance").unwrap())]);
        assert!(assemble(&mesh, &samples, &z, "planeStrain", 1., &HashMap::new()).is_err());
    }

    fn reflected_wave(formulation: &str, axis: usize, impedance_factor: f64) -> f64 {
        const N: usize = 40;
        let mut mesh = Mesh2D {
            nodes: vec![],
            triangles: vec![],
            boundary_edges: vec![],
            integration_edges: vec![],
        };
        for i in 0..=N {
            for y in 0..2 {
                mesh.nodes.push([i as f64 / N as f64, y as f64, 0.]);
            }
        }
        for i in 0..N {
            let a = 2 * i;
            let b = a + 2;
            let c = a + 1;
            let d = b + 1;
            mesh.triangles.push(([a, b, d], 1));
            mesh.triangles.push(([a, d, c], 1));
        }
        mesh.boundary_edges.push(([2 * N, 2 * N + 1], 2));
        let m = Material {
            density: 1.,
            lambda: 2.,
            mu: 1.,
            loss: 0.,
        };
        let speed = if axis == 0 {
            (reduced_lambda(m, formulation).unwrap() + 2. * m.mu).sqrt()
        } else {
            m.mu.sqrt()
        };
        let z = json!({"type":"impedance","impedance":quantity(&(speed*impedance_factor).to_string(),"Pa*s/m")});
        let boundaries = HashMap::from([(2, compile_boundary(&z, "impedance").unwrap())]);
        let samples = vec![vec![m; crate::TRI_Q.len()]; mesh.triangles.len()];
        let (matrix, _, _, _) = assemble(
            &mesh,
            &samples,
            &boundaries,
            formulation,
            speed,
            &HashMap::new(),
        )
        .unwrap();
        let mut reduced = vec![BTreeMap::new(); N + 1];
        for (row, entries) in matrix.iter().enumerate().filter(|(i, _)| i % 2 == axis) {
            for (&column, &value) in entries.iter().filter(|(i, _)| *i % 2 == axis) {
                scalar2d::add_entry(&mut reduced, row / 4, column / 4, value);
            }
        }
        let mut re = vec![0.; N + 1];
        let mut im = vec![0.; N + 1];
        for row in 1..=N {
            if let Some(value) = reduced[row].remove(&0) {
                re[row] = -value.re;
                im[row] = -value.im;
            }
        }
        reduced[0].clear();
        reduced[0].insert(0, C { re: 1., im: 0. });
        re[0] = 1.;
        let (real, imag, _) = solve_linear_system(
            &reduced,
            &re,
            &im,
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
        let sub = |a: C, b: C| C {
            re: a.re - b.re,
            im: a.im - b.im,
        };
        let mul = crate::c_mul;
        let numerator = sub(mul(u(10), wave(20, -1.)), mul(u(20), wave(10, -1.)));
        let denominator = sub(mul(u(20), wave(10, 1.)), mul(u(10), wave(20, 1.)));
        numerator.re.hypot(numerator.im) / denominator.re.hypot(denominator.im)
    }

    #[test]
    fn normal_incidence_impedance_reflection_for_both_formulations() {
        for formulation in ["planeStress", "planeStrain"] {
            for axis in [0, 1] {
                let matched = reflected_wave(formulation, axis, 1.0);
                let mismatched = reflected_wave(formulation, axis, 2.0);
                assert!(
                    matched < 0.01,
                    "{formulation}, axis {axis}, matched {matched}"
                );
                assert!(
                    (mismatched - 1. / 3.).abs() < 0.01,
                    "{formulation}, axis {axis}, mismatched {mismatched}"
                );
            }
        }
    }

    #[test]
    fn invalid_materials_and_conflicting_constraints_are_rejected() {
        let scalar = |value| model_expression::Scalar {
            value,
            dimension: [0.; 5],
        };
        let mut values = HashMap::from([
            ("rho".to_string(), scalar(2700.)),
            ("young".to_string(), scalar(70e9)),
            ("nu".to_string(), scalar(0.33)),
            ("densityLoss".to_string(), scalar(0.01)),
        ]);
        assert!(material_from_values(&values).is_ok());
        for (name, bad) in [
            ("rho", 0.),
            ("young", -1.),
            ("nu", 0.5),
            ("nu", -1.),
            ("densityLoss", -0.1),
        ] {
            let old = values.insert(name.to_string(), scalar(bad)).unwrap();
            assert!(material_from_values(&values).is_err());
            values.insert(name.to_string(), old);
        }
        let mut m = mesh();
        m.boundary_edges.push(([0, 2], 3));
        let displacement = |x: &str| json!({"type":"dirichlet","value":quantity("0","m"),"fieldValues":[quantity(x,"m"),quantity("0","m")]});
        let conditions = HashMap::from([
            (2, compile_boundary(&displacement("0"), "a").unwrap()),
            (3, compile_boundary(&displacement("1"), "b").unwrap()),
        ]);
        assert!(
            assemble(
                &m,
                &[vec![material(); crate::TRI_Q.len()]],
                &conditions,
                "planeStress",
                1.,
                &HashMap::new()
            )
            .unwrap_err()
            .contains("Conflicting")
        );
        let incident =
            json!({"type":"impedance","impedance":quantity("1","Pa*s/m"),"incidentField":"1"});
        assert!(compile_boundary(&incident, "incident").is_err());
    }
}
