//! Native stationary 3D conduction: -div(k grad T)=0, q=-k grad T.
//! P1 tetrahedra, positive material-wise isotropic k, constant boundary data.
//! Expressions and SI units are evaluated through the common quantity engine.
//! Shared FEM code owns preflight, meshing, Study lifecycle and solver backends.
//! Periodic conditions, spatial coefficients and volume sources are unsupported.

use crate::solver_common::{C, SolverCache, solve_linear_system};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

type V3 = [f64; 3];

#[derive(Clone, Debug)]
struct Mesh3D {
    nodes: Vec<V3>,
    tetrahedra: Vec<([usize; 4], usize)>,
    boundary_faces: Vec<([usize; 3], usize)>,
}

fn parse_mesh(text: &str) -> Result<Mesh3D, String> {
    let mut lines = text.trim().lines();
    if lines.next().map(str::trim) != Some("FULLMETA_MESH/1") {
        return Err("Thermal 3D requires FULLMETA_MESH/1.".into());
    }
    let mut mesh = Mesh3D {
        nodes: vec![],
        tetrahedra: vec![],
        boundary_faces: vec![],
    };
    let mut ended = false;
    for (line, value) in lines.enumerate() {
        if ended {
            return Err("Unexpected data after mesh terminator.".into());
        }
        let row = value.trim().split(',').collect::<Vec<_>>();
        let index = |i: usize| -> Result<usize, String> {
            row.get(i)
                .ok_or_else(|| format!("Incomplete mesh line {}.", line + 2))?
                .parse()
                .map_err(|_| format!("Invalid integer in mesh line {}.", line + 2))
        };
        match row[0] {
            "V" if row.len() == 5 => {
                if index(1)? != mesh.nodes.len() {
                    return Err("Duplicate or noncontiguous mesh node ID.".into());
                }
                let mut p = [0.0; 3];
                for axis in 0..3 {
                    p[axis] = row[axis + 2].parse::<f64>().map_err(|e| e.to_string())?;
                    if !p[axis].is_finite() {
                        return Err("Non-finite mesh coordinate.".into());
                    }
                }
                mesh.nodes.push(p);
            }
            "T" if row.len() == 7 => {
                if index(1)? != mesh.tetrahedra.len() {
                    return Err("Duplicate or noncontiguous tetrahedron ID.".into());
                }
                mesh.tetrahedra
                    .push(([index(2)?, index(3)?, index(4)?, index(5)?], index(6)?));
            }
            "F" if row.len() == 6 => {
                if index(1)? != mesh.boundary_faces.len() {
                    return Err("Duplicate or noncontiguous boundary face ID.".into());
                }
                mesh.boundary_faces
                    .push(([index(2)?, index(3)?, index(4)?], index(5)?));
            }
            "L" | "I" | "P" => { /* Integration metadata does not alter this PDE. */ }
            "FULLMETA_MESH_DATA_END" if row.len() == 1 => ended = true,
            _ => return Err(format!("Unsupported or malformed mesh line {}.", line + 2)),
        }
    }
    if !ended {
        return Err("Mesh terminator is missing; mesh may be truncated.".into());
    }
    Ok(mesh)
}

fn dot(a: V3, b: V3) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
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
fn face_key(mut face: [usize; 3]) -> [usize; 3] {
    face.sort_unstable();
    face
}
fn root(parent: &mut [usize], node: usize) -> usize {
    let mut current = node;
    while parent[current] != current {
        current = parent[current];
    }
    let found = current;
    current = node;
    while parent[current] != current {
        let next = parent[current];
        parent[current] = found;
        current = next;
    }
    found
}
fn join(parent: &mut [usize], a: usize, b: usize) {
    let a = root(parent, a);
    let b = root(parent, b);
    parent[a.max(b)] = a.min(b);
}
fn tetra_geometry(p: [V3; 4]) -> Result<(f64, [V3; 4]), String> {
    let a = sub(p[1], p[0]);
    let b = sub(p[2], p[0]);
    let c = sub(p[3], p[0]);
    let determinant = dot(a, cross(b, c));
    let scale = dot(a, a).max(dot(b, b)).max(dot(c, c)).sqrt();
    if !determinant.is_finite() || scale == 0.0 || determinant.abs() <= 1e-14 * scale.powi(3) {
        return Err("Degenerate thermal tetrahedron.".into());
    }
    let mut gradients = [[0.0; 3]; 4];
    for (i, vector) in [cross(b, c), cross(c, a), cross(a, b)]
        .into_iter()
        .enumerate()
    {
        for axis in 0..3 {
            gradients[i + 1][axis] = vector[axis] / determinant;
        }
    }
    for axis in 0..3 {
        gradients[0][axis] = -gradients[1][axis] - gradients[2][axis] - gradients[3][axis];
    }
    Ok((determinant.abs() / 6.0, gradients))
}

#[derive(Clone, Copy, Debug)]
enum BoundaryCondition {
    Temperature(f64),
    OutwardHeatFlux(f64),
}

#[derive(Clone, Debug)]
struct SolverSettings {
    backend: String,
    ordering: String,
    threads: usize,
    constitutive_quadrature: String,
}

impl SolverSettings {
    fn validate(&self) -> Result<(), String> {
        let capabilities = crate::model_schema::engine_capabilities()?;
        let study = capabilities
            .get("study")
            .ok_or("Study capabilities missing.")?;
        let profile = study
            .pointer("/solverSettingsByProfile/thermal-3d")
            .ok_or("Thermal 3D solver settings missing.")?;
        crate::solver_options::resolve(
            study,
            profile,
            &json!({"backend":self.backend,
            "ordering":self.ordering,"threads":self.threads,"constitutiveQuadrature":self.constitutive_quadrature}),
            "T",
            1,
        )?;
        Ok(())
    }
}

#[derive(Debug)]
struct Solution {
    temperature_kelvin: Vec<f64>,
    heat_flux_by_cell: Vec<V3>,
    conductivity_by_cell: Vec<f64>,
    residual: f64,
    unknowns: usize,
    mean_constraints: usize,
    volume: f64,
    boundary_power: BTreeMap<usize, f64>,
    boundary_area: BTreeMap<usize, f64>,
}

/// Mean constraints use volume weights. Pure-Neumann components must have zero
/// integrated heat load; a gauge must never balance an incompatible load.
fn solve_with_mean(
    mesh: &Mesh3D,
    conductivity: &HashMap<usize, f64>,
    boundaries: &HashMap<usize, BoundaryCondition>,
    settings: &SolverSettings,
    mean_temperature: f64,
) -> Result<Solution, String> {
    if !mean_temperature.is_finite() {
        return Err("Mean temperature must be finite.".into());
    }
    let count = mesh.nodes.len();
    if count == 0 || mesh.tetrahedra.is_empty() {
        return Err("Thermal 3D needs nodes and tetrahedra.".into());
    }
    if mesh.nodes.iter().flatten().any(|v| !v.is_finite()) {
        return Err("Mesh coordinates must be finite.".into());
    }
    if conductivity
        .iter()
        .any(|(&tag, &k)| tag == 0 || !k.is_finite() || k <= 0.0)
    {
        return Err("Thermal conductivity must be positive and finite with nonzero tags.".into());
    }
    for (&tag, bc) in boundaries {
        let value = match bc {
            BoundaryCondition::Temperature(v) | BoundaryCondition::OutwardHeatFlux(v) => *v,
        };
        if tag == 0 || !value.is_finite() {
            return Err("Thermal boundary values must be finite with nonzero tags.".into());
        }
    }
    let mut matrix = vec![BTreeMap::<usize, C>::new(); count];
    let mut rhs = vec![0.0; count];
    let mut absolute_loads = vec![0.0; count];
    let mut parent = (0..count).collect::<Vec<_>>();
    let mut weights = vec![0.0; count];
    let mut cells = HashSet::new();
    // face -> (adjacency count, owning cell, opposite vertex)
    let mut faces = HashMap::<[usize; 3], (usize, usize, usize)>::new();
    let mut cell_gradients = Vec::new();
    let mut conductivity_by_cell = Vec::new();
    let mut volume = 0.0;
    for (cell, (tet, tag)) in mesh.tetrahedra.iter().enumerate() {
        if tet.iter().any(|&i| i >= count) {
            return Err(format!("Cell {cell} references a missing node."));
        }
        let mut key = *tet;
        key.sort_unstable();
        if key.windows(2).any(|w| w[0] == w[1]) || !cells.insert(key) {
            return Err("Repeated tetrahedron vertex or duplicate cell.".into());
        }
        let k = *conductivity
            .get(tag)
            .ok_or_else(|| format!("No conductivity for material tag {tag}."))?;
        let (v, g) = tetra_geometry(tet.map(|i| mesh.nodes[i]))?;
        volume += v;
        for i in 0..4 {
            weights[tet[i]] += v / 4.0;
            join(&mut parent, tet[0], tet[i]);
            for j in 0..4 {
                matrix[tet[i]].entry(tet[j]).or_default().re += k * v * dot(g[i], g[j]);
            }
            let mut face = [0; 3];
            let mut p = 0;
            for j in 0..4 {
                if j != i {
                    face[p] = tet[j];
                    p += 1;
                }
            }
            let entry = faces.entry(face_key(face)).or_insert((0, cell, tet[i]));
            entry.0 += 1;
            if entry.0 > 2 {
                return Err("Thermal mesh has a non-manifold face.".into());
            }
        }
        cell_gradients.push(g);
        conductivity_by_cell.push(k);
    }
    if weights.iter().any(|&v| !v.is_finite() || v <= 0.0) {
        return Err("Unused mesh node or invalid nodal volume.".into());
    }
    let mut constraints = HashMap::<usize, f64>::new();
    let mut seen = HashSet::new();
    let mut used_tags = HashSet::new();
    let mut boundary_geometry = Vec::<(usize, usize, V3, f64)>::new();
    for (face, tag) in &mesh.boundary_faces {
        if face.iter().any(|&i| i >= count) {
            return Err("Boundary face references a missing node.".into());
        }
        let key = face_key(*face);
        if key.windows(2).any(|w| w[0] == w[1]) || !seen.insert(key) {
            return Err("Repeated or overlapping thermal boundary face.".into());
        }
        let &(adjacent, cell, opposite) = faces
            .get(&key)
            .ok_or("Boundary face has no adjacent tetrahedron.")?;
        if adjacent != 1 {
            return Err("Thermal boundary conditions require exterior faces.".into());
        }
        let [a, b, c] = face.map(|i| mesh.nodes[i]);
        let mut normal = cross(sub(b, a), sub(c, a));
        if dot(normal, sub(mesh.nodes[opposite], a)) > 0.0 {
            normal = normal.map(|v| -v);
        }
        let length = dot(normal, normal).sqrt();
        let area = length / 2.0;
        if !area.is_finite() || area <= 0.0 {
            return Err("Degenerate thermal boundary triangle.".into());
        }
        normal = normal.map(|v| v / length);
        boundary_geometry.push((*tag, cell, normal, area));
        let Some(bc) = boundaries.get(tag) else {
            continue;
        };
        used_tags.insert(*tag);
        match *bc {
            BoundaryCondition::Temperature(t) => {
                for &node in face {
                    if let Some(old) = constraints.insert(node, t) {
                        if (old - t).abs() > 1e-10 * old.abs().max(t.abs()).max(1.0) {
                            return Err(format!(
                                "Conflicting prescribed temperatures at node {node}."
                            ));
                        }
                    }
                }
            }
            BoundaryCondition::OutwardHeatFlux(q) => {
                for &node in face {
                    let load = q * area / 3.0;
                    rhs[node] -= load;
                    absolute_loads[node] += load.abs();
                }
            }
        }
    }
    for tag in boundaries.keys() {
        if !used_tags.contains(tag) {
            return Err(format!("Thermal boundary tag {tag} has no exterior faces."));
        }
    }
    let anchored = constraints
        .keys()
        .map(|&node| root(&mut parent, node))
        .collect::<HashSet<_>>();
    let mut mean_weights = BTreeMap::<usize, Vec<(usize, f64)>>::new();
    let mut component_loads = BTreeMap::<usize, (f64, f64)>::new();
    for node in 0..count {
        let component = root(&mut parent, node);
        if !anchored.contains(&component) {
            mean_weights
                .entry(component)
                .or_default()
                .push((node, weights[node]));
            let entry = component_loads.entry(component).or_default();
            entry.0 += rhs[node];
            entry.1 += absolute_loads[node];
        }
    }
    for (&component, &(net, absolute)) in &component_loads {
        if !net.is_finite() || !absolute.is_finite() || net.abs() > 1e-10 * absolute {
            return Err(format!(
                "Pure-flux component {component} has incompatible net heat load {net:e} W."
            ));
        }
    }
    let mut imaginary_rhs = vec![0.0; count];
    crate::scalar2d::apply_dirichlet(&mut matrix, &mut rhs, &mut imaginary_rhs, &constraints)?;
    let mean_constraints = mean_weights.len();
    for component in mean_weights.values() {
        let v = component.iter().map(|(_, w)| w).sum::<f64>();
        let multiplier = matrix.len();
        if !v.is_finite() || v <= 0.0 {
            return Err("Mean-temperature reference needs positive finite volume.".into());
        }
        matrix.push(BTreeMap::new());
        rhs.push(mean_temperature);
        imaginary_rhs.push(0.0);
        for &(node, w) in component {
            let coefficient = C { re: w / v, im: 0.0 };
            matrix[node].insert(multiplier, coefficient);
            matrix[multiplier].insert(node, coefficient);
        }
    }
    if !volume.is_finite()
        || rhs.iter().any(|v| !v.is_finite())
        || matrix
            .iter()
            .flat_map(|r| r.values())
            .any(|v| !v.re.is_finite())
    {
        return Err("Non-finite thermal stiffness or heat load.".into());
    }
    let mut cache = SolverCache::default();
    let (temperature, imaginary, residual) = solve_linear_system(
        &matrix,
        &rhs,
        &imaginary_rhs,
        1e-9,
        &settings.backend,
        &settings.ordering,
        settings.threads,
        &mut cache,
    )?;
    if temperature.len() != matrix.len() || imaginary.len() != matrix.len() {
        return Err("Thermal solution length mismatch.".into());
    }
    for i in 0..matrix.len() {
        if !temperature[i].is_finite()
            || !imaginary[i].is_finite()
            || imaginary[i].abs() > 1e-10 * temperature[i].abs().max(1.0)
        {
            return Err("Non-finite or non-real thermal solution.".into());
        }
    }
    let temperature_kelvin = temperature[..count].to_vec();
    let heat_flux_by_cell = mesh
        .tetrahedra
        .iter()
        .enumerate()
        .map(|(cell, (tet, _))| {
            let reference = temperature_kelvin[tet[0]];
            let mut q = [0.0; 3];
            for axis in 0..3 {
                for i in 0..4 {
                    q[axis] -= conductivity_by_cell[cell]
                        * cell_gradients[cell][i][axis]
                        * (temperature_kelvin[tet[i]] - reference);
                }
            }
            q
        })
        .collect::<Vec<_>>();
    if heat_flux_by_cell.iter().flatten().any(|v| !v.is_finite()) {
        return Err("Non-finite thermal heat flux.".into());
    }
    let mut boundary_power = BTreeMap::<usize, f64>::new();
    let mut boundary_area = BTreeMap::<usize, f64>::new();
    for (tag, cell, normal, area) in boundary_geometry {
        *boundary_power.entry(tag).or_default() += dot(heat_flux_by_cell[cell], normal) * area;
        *boundary_area.entry(tag).or_default() += area;
    }
    Ok(Solution {
        temperature_kelvin,
        heat_flux_by_cell,
        conductivity_by_cell,
        residual,
        unknowns: count,
        mean_constraints,
        volume,
        boundary_power,
        boundary_area,
    })
}

// The quantity/request adapter below follows the existing thermal2d module;
// it consumes shared prepared requests and delegates the project lifecycle.
fn text<'a>(value: &'a Value, key: &str, path: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{path}.{key} must be a nonempty string."))
}

fn array<'a>(value: &'a Value, key: &str, path: &str) -> Result<&'a [Value], String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{path}.{key} must be an array."))
}

/// The common kernel owns expression syntax, parameter resolution and five SI
/// dimensions. This adapter only states where the constant-element thermal
/// assembly needs a constant value rather than a spatial field.
struct ExpressionContext {
    symbols: Map<String, Value>,
    allowed_functions: Vec<String>,
}

impl ExpressionContext {
    fn for_model(model: &Value) -> Result<Self, String> {
        let capabilities = crate::model_schema::engine_capabilities()?;
        let allowed_functions = capabilities
            .pointer("/expressions/modelValueFunctions")
            .and_then(Value::as_array)
            .ok_or("The model schema must declare expressions.modelValueFunctions.")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "Model expression functions must be strings.".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut reserved = crate::model_schema::reserved_symbols_for_model(model)?;
        reserved.extend(
            ["pi", "e", "i", "x", "y", "z", "freq", "omega", "t"]
                .into_iter()
                .map(str::to_string),
        );
        let parameters = model
            .pointer("/model/parameters")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let symbols = fullmeta_core::quantity::resolve_parameters(
            parameters,
            &reserved,
            &allowed_functions,
            "model.parameters",
        )?;
        Ok(Self {
            symbols,
            allowed_functions,
        })
    }

    fn quantity(&self, value: &Value, expected_unit: &str, path: &str) -> Result<f64, String> {
        let actual = fullmeta_core::quantity::get_unit(text(value, "unit", path)?)?;
        let expected = fullmeta_core::quantity::get_unit(expected_unit)?;
        if actual
            .dimension
            .iter()
            .zip(expected.dimension)
            .any(|(left, right)| (left - right).abs() > 1e-10)
        {
            return Err(format!(
                "{path}.unit must be compatible with {expected_unit:?}."
            ));
        }
        let expression = text(value, "expression", path)?;
        let inspected = fullmeta_core::expression::inspect_expression(expression)?;
        if inspected
            .symbols
            .iter()
            .any(|symbol| matches!(symbol.as_str(), "x" | "y" | "z"))
        {
            return Err(format!(
                "{path}: spatial thermal material and boundary expressions are not implemented by the constant-element thermal adapter; use an expression of parameters only."
            ));
        }
        let number = fullmeta_core::quantity::evaluate_quantity(
            value,
            &self.symbols,
            path,
            &self.allowed_functions,
        )?;
        if !number.is_finite() {
            return Err(format!("{path} must evaluate to a finite value."));
        }
        Ok(number)
    }
}

fn boundary_condition(
    boundary: &Value,
    path: &str,
    expressions: &ExpressionContext,
) -> Result<BoundaryCondition, String> {
    let value = boundary
        .get("value")
        .ok_or_else(|| format!("{path}.value is required."))?;
    match text(boundary, "type", path)? {
        "dirichlet" => Ok(BoundaryCondition::Temperature(expressions.quantity(
            value,
            "K",
            &format!("{path}.value"),
        )?)),
        "neumann" => Ok(BoundaryCondition::OutwardHeatFlux(expressions.quantity(
            value,
            "W/m^2",
            &format!("{path}.value"),
        )?)),
        other => Err(format!(
            "{path}.type {other:?} is unsupported; use 'dirichlet' or 'neumann'."
        )),
    }
}

/// Delegate schema and whole-model FEM checks to the shared dispatcher.
/// The client reuses the dispatcher receipt when this exact model has already
/// passed preflight before the scenario process was launched.
pub(crate) fn validate_model(model: &Value) -> Result<(), String> {
    crate::preflight_client::validate(model)
}

fn resolve_from(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn selection_tags(map: &Value, property: &str) -> Result<HashMap<String, usize>, String> {
    let mut tags = HashMap::new();
    let mut seen_tags = HashSet::new();
    for (index, row) in array(map, property, "mesh map")?.iter().enumerate() {
        let path = format!("mesh map.{property}[{index}]");
        let id = row
            .get("selectionId")
            .or_else(|| row.get("id"))
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| format!("{path} needs a selection ID."))?;
        let tag = row
            .get("physicalTag")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|&tag| tag > 0)
            .ok_or_else(|| format!("{path}.physicalTag must be a positive integer."))?;
        if tags.insert(id.to_string(), tag).is_some() || !seen_tags.insert(tag) {
            return Err(format!("{path} duplicates a selection ID or physical tag."));
        }
    }
    Ok(tags)
}

/// Execute one stationary request using the profile contract in fem.schema.json.
/// It has no frequency cases or frequency CSV. Common preflight completes
/// before mesh assembly and output creation; numerical input guards stay here.
pub(crate) fn run_request(request_path: &Path) -> Result<(), Box<dyn Error>> {
    let request: Value = serde_json::from_slice(&fs::read(request_path)?)?;
    let allowed = [
        "schema",
        "solver",
        "studyId",
        "fieldFormulation",
        "elementOrder",
        "constitutiveQuadrature",
        "solverBackend",
        "solverOrdering",
        "solverThreads",
        "csvFilename",
        "fieldFiles",
        "fieldFilename",
        "modelFile",
        "meshFile",
        "modelMapFile",
        "outputDirectory",
    ];
    for key in request
        .as_object()
        .ok_or("The thermal solver request must be an object.")?
        .keys()
    {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("Unsupported thermal solver request property {key:?}.").into());
        }
    }
    if text(&request, "schema", "request")? != "fem-solver-request/1"
        || text(&request, "solver", "request")? != "thermal-3d"
        || text(&request, "fieldFormulation", "request")? != "T"
        || request.get("elementOrder").and_then(Value::as_u64) != Some(1)
    {
        return Err("The thermal request must use fem-solver-request/1, solver 'thermal-3d', formulation 'T' and first-order elements.".into());
    }
    let csv_filename = text(&request, "csvFilename", "request")?;
    if !safe_output_filename(csv_filename, ".csv") {
        return Err("request.csvFilename must be one safe .csv filename.".into());
    }
    let field_files = request
        .get("fieldFiles")
        .and_then(Value::as_bool)
        .ok_or("request.fieldFiles must be a boolean.")?;
    let field_filename = request
        .get("fieldFilename")
        .and_then(Value::as_str)
        .filter(|filename| safe_output_filename(filename, ".json"));
    if field_files && field_filename.is_none() {
        return Err("request.fieldFilename is required when fieldFiles is true.".into());
    }
    let study_id = text(&request, "studyId", "request")?;
    let settings = SolverSettings {
        backend: text(&request, "solverBackend", "request")?.to_string(),
        ordering: text(&request, "solverOrdering", "request")?.to_string(),
        threads: request
            .get("solverThreads")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("request.solverThreads must be an integer.")?,
        constitutive_quadrature: text(&request, "constitutiveQuadrature", "request")?.to_string(),
    };
    settings.validate()?;
    let base = request_path.parent().unwrap_or(Path::new("."));
    let mut paths = HashMap::new();
    for key in ["modelFile", "meshFile", "modelMapFile", "outputDirectory"] {
        let value = text(&request, key, "request")?;
        if value.chars().count() > 4096 || value.contains('\0') {
            return Err(format!("request.{key} is not a valid file reference.").into());
        }
        paths.insert(key, resolve_from(base, value));
    }
    let model: Value = serde_json::from_slice(&fs::read(&paths["modelFile"])?)?;
    validate_model(&model).map_err(|error| format!("Thermal model validation:\n{error}"))?;
    let study = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .ok_or("model.model.study must be an array.")?
        .iter()
        .find(|study| study.get("id").and_then(Value::as_str) == Some(study_id))
        .ok_or_else(|| format!("request.studyId {study_id:?} is absent from the model."))?;
    let expressions = ExpressionContext::for_model(&model)?;
    let mean_temperature = study
        .get("meanTemperature")
        .map(|quantity| expressions.quantity(quantity, "K", "study.meanTemperature"))
        .transpose()?
        .unwrap_or(0.0);
    let map: Value = serde_json::from_slice(&fs::read(&paths["modelMapFile"])?)?;
    if map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6")
        || map.get("dimension").and_then(Value::as_u64) != Some(3)
    {
        return Err("Thermal 3D requires a dimension-3 fullmeta-mesh-map/6 map.".into());
    }
    let mesh = parse_mesh(&fs::read_to_string(&paths["meshFile"])?)?;
    let component = &model["model"];
    let material_tags = selection_tags(&map, "physicalGroups")?;
    let boundary_tags = selection_tags(&map, "boundarySelections")?;
    let mut conductivity = HashMap::new();
    for (index, material) in array(component, "materials", "component")?
        .iter()
        .enumerate()
    {
        let path = format!("model.materials[{index}]");
        let id = text(material, "selectionId", &path)?;
        let tag = *material_tags
            .get(id)
            .ok_or_else(|| format!("Material selection {id:?} is absent from the mesh map."))?;
        let k = expressions.quantity(
            &material["thermalConductivity"],
            "W/(m*K)",
            &format!("{path}.thermalConductivity"),
        )?;
        if conductivity.insert(tag, k).is_some() {
            return Err(format!(
                "More than one thermal conductivity assignment targets physical tag {tag}."
            )
            .into());
        }
    }
    let mut boundaries = HashMap::new();
    let physics =
        crate::model_schema::physics(component).ok_or("model.physics must be an object.")?;
    for boundary in crate::model_schema::physics_boundary_conditions(physics) {
        if boundary.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let path = format!(
            "physics.{}",
            boundary
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("condition")
        );
        let id = text(boundary, "selectionId", &path)?;
        let tag = *boundary_tags
            .get(id)
            .ok_or_else(|| format!("Boundary selection {id:?} is absent from the mesh map."))?;
        if boundaries
            .insert(tag, boundary_condition(boundary, &path, &expressions)?)
            .is_some()
        {
            return Err(format!(
                "More than one thermal boundary condition targets physical tag {tag}."
            )
            .into());
        }
    }
    if component
        .pointer("/mesh/periodicPairs")
        .and_then(Value::as_array)
        .is_some_and(|p| !p.is_empty())
        || crate::model_schema::physics_periodic_conditions(physics)
            .any(|p| p.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        return Err("Periodic thermal 3D conditions are not implemented by this module.".into());
    }
    if text(study, "type", "Study")? != "stationary" {
        return Err("Thermal 3D requires a stationary Study.".into());
    }
    crate::solver_common::stage_started("assembly");
    let solution = solve_with_mean(
        &mesh,
        &conductivity,
        &boundaries,
        &settings,
        mean_temperature,
    )?;
    let minimum = solution
        .temperature_kelvin
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let maximum = solution
        .temperature_kelvin
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let output = json!({
        "schema":"fullmeta-thermal-field/1", "dimension":3, "studyId":study_id,
        "fieldComponentCoordinateSystem":"model", "nodes":mesh.nodes,
        "tetrahedra":mesh.tetrahedra.iter().map(|(nodes,tag)| json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>(),
        "temperatureK":solution.temperature_kelvin,
        "fields":{"T":{"real":solution.temperature_kelvin,"imag":vec![0.0;mesh.nodes.len()],"unit":"K","source":"solved"}},
        "heatFluxByCell":{"values":solution.heat_flux_by_cell,"unit":"W/m^2","source":"-k grad(T)","coordinateSystem":"model"},
        "materialFieldsByCell":{"k":{
            "real":solution.conductivity_by_cell.iter().map(|&value| [value;4]).collect::<Vec<_>>(),
            "imag":vec![[0.0;4];mesh.tetrahedra.len()],"unit":"W/(m*K)"
        }},
        "minimumTemperatureK":minimum, "maximumTemperatureK":maximum,
        "solver":{"backend":settings.backend,"ordering":settings.ordering,"threads":settings.threads,"constitutiveQuadrature":settings.constitutive_quadrature,"residual":solution.residual,"unknowns":solution.unknowns,
            "meanTemperatureConstraints":solution.mean_constraints,"meanTemperatureK":mean_temperature,"systemUnknowns":solution.unknowns+solution.mean_constraints},
        "fieldConvention":"stationary", "heatFluxConvention":"positive outward normal flux removes heat",
        "derivedFieldRecovery":"constant per tetrahedron; material interface jumps preserved",
        "volumeM3":solution.volume,
        "boundaryOutwardPowerWByTag":solution.boundary_power,
        "boundaryAreaM2ByTag":solution.boundary_area,
        "netBoundaryOutwardPowerW":solution.boundary_power.values().sum::<f64>()
    });
    let output_directory = &paths["outputDirectory"];
    fs::create_dir_all(output_directory)?;
    crate::solver_common::stage_started("output");
    if field_files {
        fs::write(
            output_directory.join(field_filename.unwrap()),
            serde_json::to_vec_pretty(&output)?,
        )?;
    }
    fs::write(
        output_directory.join(csv_filename),
        format!(
            "case,unknowns,residual,minimum_temperature_K,maximum_temperature_K\n0,{},{:.12e},{minimum:.12e},{maximum:.12e}\n",
            solution.unknowns, solution.residual
        ),
    )?;
    println!(
        "Thermal 3D · {} unknowns · {} tetrahedra · residual {:.2e} · T [{minimum:.6e}, {maximum:.6e}] K",
        solution.unknowns,
        mesh.tetrahedra.len(),
        solution.residual
    );
    Ok(())
}

fn safe_output_filename(value: &str, extension: &str) -> bool {
    value.len() <= 120
        && value.ends_with(extension)
        && !value.starts_with('.')
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
}

pub(crate) fn run(arguments: &[String]) -> Result<i32, String> {
    crate::scenario_solver_cli::run_with_fallback(
        arguments,
        "thermal-3d",
        |request, _cache| run_request(request),
        crate::run_shared_operation,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> SolverSettings {
        SolverSettings {
            backend: "faer".into(),
            ordering: "default".into(),
            threads: 1,
            constitutive_quadrature: "standard".into(),
        }
    }
    fn cube(n: usize) -> Mesh3D {
        let mut mesh = Mesh3D {
            nodes: vec![],
            tetrahedra: vec![],
            boundary_faces: vec![],
        };
        let id = |i, j, k| i + (n + 1) * (j + (n + 1) * k);
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    mesh.nodes.push([
                        i as f64 / n as f64,
                        j as f64 / n as f64,
                        k as f64 / n as f64,
                    ]);
                }
            }
        }
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    let v = [
                        id(i, j, k),
                        id(i + 1, j, k),
                        id(i + 1, j + 1, k),
                        id(i, j + 1, k),
                        id(i, j, k + 1),
                        id(i + 1, j, k + 1),
                        id(i + 1, j + 1, k + 1),
                        id(i, j + 1, k + 1),
                    ];
                    for local in [
                        [0, 1, 2, 6],
                        [0, 2, 3, 6],
                        [0, 3, 7, 6],
                        [0, 7, 4, 6],
                        [0, 4, 5, 6],
                        [0, 5, 1, 6],
                    ] {
                        mesh.tetrahedra
                            .push((local.map(|p| v[p]), if i < n / 2 { 1 } else { 2 }));
                    }
                }
            }
        }
        let mut faces = BTreeMap::<[usize; 3], usize>::new();
        for (tet, _) in &mesh.tetrahedra {
            for i in 0..4 {
                let mut f = [0; 3];
                let mut p = 0;
                for j in 0..4 {
                    if i != j {
                        f[p] = tet[j];
                        p += 1;
                    }
                }
                *faces.entry(face_key(f)).or_default() += 1;
            }
        }
        for (face, count) in faces {
            if count == 1 {
                let tag = if face.iter().all(|&i| mesh.nodes[i][0] == 0.0) {
                    1
                } else if face.iter().all(|&i| mesh.nodes[i][0] == 1.0) {
                    2
                } else {
                    3
                };
                mesh.boundary_faces.push((face, tag));
            }
        }
        mesh
    }
    fn conductivity(k: f64) -> HashMap<usize, f64> {
        HashMap::from([(1, k), (2, k)])
    }
    fn fixed(cold: f64, hot: f64) -> HashMap<usize, BoundaryCondition> {
        HashMap::from([
            (1, BoundaryCondition::Temperature(cold)),
            (2, BoundaryCondition::Temperature(hot)),
            (3, BoundaryCondition::OutwardHeatFlux(0.0)),
        ])
    }
    fn check(
        mesh: &Mesh3D,
        solution: &Solution,
        temperature: impl Fn(V3) -> f64,
        flux: impl Fn(usize) -> V3,
    ) {
        for (i, &p) in mesh.nodes.iter().enumerate() {
            assert!(
                (solution.temperature_kelvin[i] - temperature(p)).abs() < 1e-8,
                "temperature at {i}"
            );
        }
        for (cell, q) in solution.heat_flux_by_cell.iter().enumerate() {
            let expected = flux(cell);
            assert!(
                dot(sub(*q, expected), sub(*q, expected)).sqrt() < 1e-7,
                "flux at cell {cell}"
            );
        }
        assert!((solution.volume - 1.0).abs() < 1e-10);
        assert!(solution.boundary_power.values().sum::<f64>().abs() < 1e-7);
        assert!(solution.residual.is_finite() && solution.residual < 1e-9);
    }
    #[test]
    fn cube_temperature_flux_and_power() {
        let mesh = cube(4);
        let s = solve_with_mean(
            &mesh,
            &conductivity(2.5),
            &fixed(300.0, 400.0),
            &settings(),
            0.0,
        )
        .unwrap();
        check(&mesh, &s, |p| 300.0 + 100.0 * p[0], |_| [-250.0, 0.0, 0.0]);
        assert!((s.boundary_power[&1] - 250.0).abs() < 1e-7);
        assert!((s.boundary_power[&2] + 250.0).abs() < 1e-7);
        assert!(s.boundary_power[&3].abs() < 1e-7);
        assert_eq!(s.mean_constraints, 0);
    }
    #[test]
    fn reversed_temperature_and_scaled_conductivity() {
        let mesh = cube(4);
        for (cold, hot, k) in [(400.0, 300.0, 2.5), (300.0, 400.0, 5.0)] {
            let s = solve_with_mean(&mesh, &conductivity(k), &fixed(cold, hot), &settings(), 0.0)
                .unwrap();
            check(
                &mesh,
                &s,
                |p| cold + (hot - cold) * p[0],
                |_| [-k * (hot - cold), 0.0, 0.0],
            );
        }
    }
    #[test]
    fn two_material_series_preserves_flux_jump_in_gradient() {
        let mesh = cube(4);
        let s = solve_with_mean(
            &mesh,
            &HashMap::from([(1, 1.0), (2, 4.0)]),
            &fixed(300.0, 400.0),
            &settings(),
            0.0,
        )
        .unwrap();
        check(
            &mesh,
            &s,
            |p| {
                if p[0] <= 0.5 {
                    300.0 + 160.0 * p[0]
                } else {
                    380.0 + 40.0 * (p[0] - 0.5)
                }
            },
            |_| [-160.0, 0.0, 0.0],
        );
    }
    #[test]
    fn outward_neumann_sign() {
        let mesh = cube(4);
        let mut bc = fixed(300.0, 400.0);
        bc.insert(2, BoundaryCondition::OutwardHeatFlux(-250.0));
        let s = solve_with_mean(&mesh, &conductivity(2.5), &bc, &settings(), 0.0).unwrap();
        check(&mesh, &s, |p| 300.0 + 100.0 * p[0], |_| [-250.0, 0.0, 0.0]);
    }
    #[test]
    fn rotated_cube_checks_all_flux_components() {
        let mut mesh = cube(4);
        let (sin, cos) = 0.61f64.sin_cos();
        for p in &mut mesh.nodes {
            *p = [cos * p[0] - sin * p[1], sin * p[0] + cos * p[1], p[2]];
        }
        let s = solve_with_mean(
            &mesh,
            &conductivity(2.5),
            &fixed(300.0, 400.0),
            &settings(),
            0.0,
        )
        .unwrap();
        check(
            &mesh,
            &s,
            |p| 300.0 + 100.0 * (cos * p[0] + sin * p[1]),
            |_| [-250.0 * cos, -250.0 * sin, 0.0],
        );
    }
    #[test]
    fn pure_neumann_uses_volume_mean_without_pinning_a_node() {
        let mesh = cube(4);
        for driven in [false, true] {
            let bc = if driven {
                HashMap::from([
                    (1, BoundaryCondition::OutwardHeatFlux(250.0)),
                    (2, BoundaryCondition::OutwardHeatFlux(-250.0)),
                ])
            } else {
                HashMap::new()
            };
            let s = solve_with_mean(&mesh, &conductivity(2.5), &bc, &settings(), 350.0).unwrap();
            check(
                &mesh,
                &s,
                |p| if driven { 300.0 + 100.0 * p[0] } else { 350.0 },
                |_| if driven { [-250.0, 0.0, 0.0] } else { [0.0; 3] },
            );
            assert_eq!(s.mean_constraints, 1);
        }
    }
    #[test]
    fn reject_unbalanced_pure_flux() {
        let mesh = cube(2);
        let bc = HashMap::from([(2, BoundaryCondition::OutwardHeatFlux(100.0))]);
        assert!(
            solve_with_mean(&mesh, &conductivity(2.5), &bc, &settings(), 350.0)
                .unwrap_err()
                .contains("incompatible")
        );
    }
    #[test]
    fn reject_invalid_material_and_boundary_data() {
        let mesh = cube(2);
        for k in [0.0, -1.0, f64::NAN] {
            assert!(
                solve_with_mean(
                    &mesh,
                    &conductivity(k),
                    &fixed(300.0, 400.0),
                    &settings(),
                    0.0
                )
                .is_err()
            );
        }
        let mut bc = fixed(300.0, 400.0);
        bc.insert(3, BoundaryCondition::Temperature(350.0));
        assert!(
            solve_with_mean(&mesh, &conductivity(1.0), &bc, &settings(), 0.0)
                .unwrap_err()
                .contains("Conflicting")
        );
        let mut bc = fixed(300.0, 400.0);
        bc.insert(99, BoundaryCondition::OutwardHeatFlux(0.0));
        assert!(
            solve_with_mean(&mesh, &conductivity(1.0), &bc, &settings(), 0.0)
                .unwrap_err()
                .contains("no exterior")
        );
    }
    #[test]
    fn reject_degenerate_duplicate_and_disconnected_mesh_data() {
        let mut mesh = cube(2);
        mesh.nodes[mesh.tetrahedra[0].0[1]] = mesh.nodes[mesh.tetrahedra[0].0[0]];
        assert!(
            solve_with_mean(
                &mesh,
                &conductivity(1.0),
                &fixed(300.0, 400.0),
                &settings(),
                0.0
            )
            .is_err()
        );
        let mut mesh = cube(2);
        mesh.tetrahedra.push(mesh.tetrahedra[0]);
        assert!(
            solve_with_mean(
                &mesh,
                &conductivity(1.0),
                &fixed(300.0, 400.0),
                &settings(),
                0.0
            )
            .is_err()
        );
        let mut mesh = cube(2);
        mesh.nodes.push([2.0, 2.0, 2.0]);
        assert!(
            solve_with_mean(
                &mesh,
                &conductivity(1.0),
                &fixed(300.0, 400.0),
                &settings(),
                0.0
            )
            .unwrap_err()
            .contains("Unused")
        );
        let mut mesh = cube(2);
        mesh.boundary_faces.push(mesh.boundary_faces[0]);
        assert!(
            solve_with_mean(
                &mesh,
                &conductivity(1.0),
                &fixed(300.0, 400.0),
                &settings(),
                0.0
            )
            .unwrap_err()
            .contains("overlapping")
        );
    }
    #[test]
    fn mesh_parser_rejects_truncation_nonfinite_and_duplicate_ids() {
        assert!(parse_mesh("FULLMETA_MESH/1\nV,0,0,0,0").is_err());
        assert!(parse_mesh("FULLMETA_MESH/1\nV,0,NaN,0,0\nFULLMETA_MESH_DATA_END").is_err());
        assert!(
            parse_mesh("FULLMETA_MESH/1\nV,0,0,0,0\nV,0,1,0,0\nFULLMETA_MESH_DATA_END").is_err()
        );
    }
}
