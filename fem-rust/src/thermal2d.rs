//! Stationary 2D conduction: -div(k grad(T)) = 0 with q = -k grad(T).
//!
//! This is a source-stage scenario implementation. The numerical kernel uses
//! prescribed, constant isotropic conductivity on each material selection and
//! linear triangles. Material and boundary quantities use the shared expression
//! and unit engine, including parameter references and thermal dimensions.
//! Whole-model preflight is delegated to the common FEM dispatcher. The
//! numerical adapter retains checks on its request, mesh, constitutive values
//! and assembled system.
//! Project meshing and run bookkeeping live in thermal2d_project. Sweeps and
//! user-defined field expressions remain separate extensions.

use crate::mesh2d::{self, Mesh2D};
use crate::scalar2d::{self, PeriodicDofMap};
use crate::solver_common::{C, SolverCache, solve_linear_system};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug)]
pub(crate) enum BoundaryCondition {
    /// Absolute temperature, in kelvin.
    Temperature(f64),
    /// q dot n in W/m^2. Positive values remove heat; zero is insulation.
    OutwardHeatFlux(f64),
}

#[derive(Clone, Debug)]
pub(crate) struct SolverSettings {
    pub(crate) backend: String,
    pub(crate) ordering: String,
    pub(crate) threads: usize,
    pub(crate) constitutive_quadrature: String,
}

impl SolverSettings {
    fn from_study(study: &Value) -> Result<Self, String> {
        Self::from_options(study.get("solver").unwrap_or(&Value::Null))
    }

    fn from_options(solver: &Value) -> Result<Self, String> {
        let capabilities = crate::model_schema::engine_capabilities()?;
        let study = capabilities
            .get("study")
            .ok_or("The model schema must declare Study capabilities.")?;
        let profile = study
            .pointer("/solverSettingsByProfile/thermal-2d")
            .ok_or("The model schema must declare thermal solver settings.")?;
        let options = Value::Object(crate::solver_options::resolve(
            study, profile, solver, "T", 1,
        )?);
        Ok(Self {
            backend: text(&options, "backend", "resolved solver")?.to_string(),
            ordering: text(&options, "ordering", "resolved solver")?.to_string(),
            threads: options
                .get("threads")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or("Resolved solver threads are invalid.")?,
            constitutive_quadrature: options
                .get("constitutiveQuadrature")
                .and_then(Value::as_str)
                .ok_or("Resolved constitutive quadrature is absent.")?
                .to_string(),
        })
    }

    fn validate(&self) -> Result<(), String> {
        Self::from_options(&json!({"backend":self.backend,"ordering":self.ordering,
            "threads":self.threads,"constitutiveQuadrature":self.constitutive_quadrature}))
        .map(|_| ())
    }
}

pub(crate) struct Solution {
    pub(crate) temperature_kelvin: Vec<f64>,
    /// Cell values preserve flux jumps across different materials.
    pub(crate) heat_flux_by_cell: Vec<[f64; 3]>,
    pub(crate) conductivity_by_cell: Vec<f64>,
    pub(crate) residual: f64,
    /// Scalar degrees of freedom after identifying periodic trace nodes.
    pub(crate) unknowns: usize,
    pub(crate) mean_constraints: usize,
}

fn edge_key(mut edge: [usize; 2]) -> [usize; 2] {
    edge.sort_unstable();
    edge
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

fn join(parent: &mut [usize], first: usize, second: usize) {
    let first = root(parent, first);
    let second = root(parent, second);
    if first != second {
        parent[first.max(second)] = first.min(second);
    }
}

/// Solve a real stationary system. Coordinates are metres; conductivity is
/// W/(m*K), temperature is K and outward boundary heat flux is W/m^2. The 2D
/// weak form is per unit out-of-plane thickness. Unspecified exterior edges
/// have zero flux. Both supported quadrature rules integrate constant-k P1
/// stiffness exactly, so no harmonic mass/reaction term or frequency is used.
pub(crate) fn solve(
    mesh: &Mesh2D,
    conductivity: &HashMap<usize, f64>,
    boundaries: &HashMap<usize, BoundaryCondition>,
    settings: &SolverSettings,
    periodic: &PeriodicDofMap,
) -> Result<Solution, String> {
    solve_with_mean(mesh, conductivity, boundaries, settings, periodic, 0.0)
}

/// Pure-flux connected components use an area-weighted mean temperature to fix
/// their additive constant. This adds a gauge without changing compatible heat
/// loads; an incompatible net outward flux is rejected before factorization.
fn solve_with_mean(
    mesh: &Mesh2D,
    conductivity: &HashMap<usize, f64>,
    boundaries: &HashMap<usize, BoundaryCondition>,
    settings: &SolverSettings,
    periodic: &PeriodicDofMap,
    mean_temperature: f64,
) -> Result<Solution, String> {
    settings.validate()?;
    if !mean_temperature.is_finite() {
        return Err("The mean temperature reference must be finite.".into());
    }
    let count = mesh.nodes.len();
    if count == 0 || mesh.triangles.is_empty() {
        return Err("Thermal conduction requires nodes and triangular cells.".into());
    }
    if periodic.node_to_dof.len() != count
        || periodic.node_weights.len() != count
        || periodic.dof_count == 0
        || periodic.dof_count > count
        || periodic
            .node_to_dof
            .iter()
            .any(|&dof| dof >= periodic.dof_count)
    {
        return Err("The thermal periodic mapping does not match the mesh nodes.".into());
    }
    if periodic.node_weights.iter().any(|weight| {
        !weight.re.is_finite()
            || !weight.im.is_finite()
            || (weight.re - 1.0).abs() > 1e-12
            || weight.im.abs() > 1e-12
    }) {
        return Err("Stationary thermal conduction supports zero-phase periodicity only.".into());
    }
    for (node, point) in mesh.nodes.iter().enumerate() {
        if point.iter().any(|value| !value.is_finite()) || point[2].abs() > 1e-10 {
            return Err(format!(
                "Thermal mesh node {node} must be finite and lie in z=0."
            ));
        }
    }
    for (&tag, &value) in conductivity {
        if tag == 0 || !value.is_finite() {
            return Err(format!(
                "Thermal conductivity for material tag {tag} must be finite and have a nonzero material tag."
            ));
        }
    }
    for (&tag, condition) in boundaries {
        let valid = match condition {
            BoundaryCondition::Temperature(value) => value.is_finite(),
            BoundaryCondition::OutwardHeatFlux(value) => value.is_finite(),
        };
        if tag == 0 || !valid {
            return Err(format!(
                "Boundary tag {tag} requires a finite heat flux or temperature and a nonzero boundary tag."
            ));
        }
    }

    let mut matrix = vec![BTreeMap::<usize, C>::new(); count];
    let mut rhs = vec![0.0; count];
    // Preserve the scale of integrated loads before opposite boundary terms
    // cancel at a shared node; compatibility is checked against this scale.
    let mut absolute_loads = vec![0.0; count];
    let mut parent = (0..count).collect::<Vec<_>>();
    let mut used_nodes = vec![false; count];
    let mut cells = HashSet::new();
    let mut edge_cells = HashMap::<[usize; 2], usize>::new();
    let mut cell_gradients = Vec::with_capacity(mesh.triangles.len());
    let mut conductivity_by_cell = Vec::with_capacity(mesh.triangles.len());
    let mut node_areas = vec![0.0; count];

    for (cell, (triangle, tag)) in mesh.triangles.iter().enumerate() {
        if triangle.iter().any(|&node| node >= count) {
            return Err(format!("Thermal cell {cell} references a missing node."));
        }
        let mut key = *triangle;
        key.sort_unstable();
        if key[0] == key[1] || key[1] == key[2] || !cells.insert(key) {
            return Err(format!(
                "Thermal cell {cell} repeats a vertex or duplicates another triangle."
            ));
        }
        let k = *conductivity.get(tag).ok_or_else(|| {
            format!("Thermal cell {cell} has no conductivity for material tag {tag}.")
        })?;
        let (area, gradients) = mesh2d::triangle_geometry(triangle.map(|node| mesh.nodes[node]))?;
        for local in 0..3 {
            used_nodes[triangle[local]] = true;
            node_areas[triangle[local]] += area / 3.0;
            let edge = edge_key([triangle[local], triangle[(local + 1) % 3]]);
            let adjacent = edge_cells.entry(edge).or_default();
            *adjacent += 1;
            if *adjacent > 2 {
                return Err(format!(
                    "Thermal mesh edge {edge:?} has more than two adjacent cells."
                ));
            }
            join(&mut parent, edge[0], edge[1]);
            for other in 0..3 {
                let gradient_product = gradients[local][0] * gradients[other][0]
                    + gradients[local][1] * gradients[other][1];
                let entry = matrix[triangle[local]].entry(triangle[other]).or_default();
                entry.re += k * area * gradient_product;
                if !entry.re.is_finite() {
                    return Err(format!(
                        "Thermal cell {cell} produced a non-finite stiffness coefficient."
                    ));
                }
            }
        }
        cell_gradients.push(gradients);
        conductivity_by_cell.push(k);
    }
    if let Some(node) = used_nodes.iter().position(|used| !used) {
        return Err(format!(
            "Thermal mesh node {node} is not connected to a triangular cell."
        ));
    }

    // Periodic equality can connect otherwise separate conduction components.
    // Keep one original node per reduced degree of freedom for the reference
    // check below, before reducing the assembled stiffness and heat loads.
    let mut representatives = vec![usize::MAX; periodic.dof_count];
    for (node, &dof) in periodic.node_to_dof.iter().enumerate() {
        if representatives[dof] == usize::MAX {
            representatives[dof] = node;
        } else {
            join(&mut parent, representatives[dof], node);
        }
    }
    if representatives.contains(&usize::MAX) {
        return Err("The thermal periodic mapping contains an unused degree of freedom.".into());
    }

    let mut constraints = HashMap::<usize, f64>::new();
    let mut boundary_edges = HashSet::new();
    let mut used_boundary_tags = HashSet::new();
    for (edge, tag) in &mesh.boundary_edges {
        if edge.iter().any(|&node| node >= count) {
            return Err(format!(
                "Thermal boundary tag {tag} references a missing node."
            ));
        }
        let key = edge_key(*edge);
        if !boundary_edges.insert(key) {
            return Err(format!(
                "Thermal boundary edge {key:?} is duplicated; overlapping boundary selections are unsupported."
            ));
        }
        let adjacent = edge_cells
            .get(&key)
            .ok_or_else(|| format!("Thermal boundary edge {key:?} has no adjacent cell."))?;
        if periodic.boundary_tags.contains(tag) || periodic.boundary_edges.contains(&key) {
            if boundaries.contains_key(tag) {
                return Err(format!(
                    "Thermal boundary tag {tag} is periodic and cannot also carry a prescribed temperature or heat flux."
                ));
            }
            if *adjacent != 1 {
                return Err(format!(
                    "Thermal periodic boundary tag {tag} targets an interior edge; periodic conditions require exterior edges."
                ));
            }
            continue;
        }
        let Some(condition) = boundaries.get(tag) else {
            continue;
        };
        if *adjacent != 1 {
            return Err(format!(
                "Thermal boundary tag {tag} targets an interior edge; temperature and flux conditions require exterior edges."
            ));
        }
        used_boundary_tags.insert(*tag);
        let [first, second] = edge.map(|node| mesh.nodes[node]);
        let length = (second[0] - first[0]).hypot(second[1] - first[1]);
        if !length.is_finite() || length <= 0.0 {
            return Err(format!(
                "Thermal boundary tag {tag} contains a degenerate edge."
            ));
        }
        match *condition {
            BoundaryCondition::Temperature(value) => {
                for &node in edge {
                    let dof = periodic.node_to_dof[node];
                    if let Some(previous) = constraints.insert(dof, value) {
                        let tolerance = 1e-10 * previous.abs().max(value.abs()).max(1.0);
                        if (previous - value).abs() > tolerance {
                            return Err(format!(
                                "Conflicting prescribed temperatures meet at mesh node {node} or its periodic equivalent (degree of freedom {dof})."
                            ));
                        }
                    }
                }
            }
            BoundaryCondition::OutwardHeatFlux(value) => {
                // Integrating -div(k grad(T))=0 by parts gives
                // K*T = -integral(q dot n * N). Each linear edge basis
                // integrates to half the edge length for constant flux.
                for &node in edge {
                    let load = 0.5 * length * value;
                    rhs[node] -= load;
                    absolute_loads[node] += load.abs();
                    if !rhs[node].is_finite() || !absolute_loads[node].is_finite() {
                        return Err(format!(
                            "Thermal boundary tag {tag} produced a non-finite heat load."
                        ));
                    }
                }
            }
        }
    }
    for tag in boundaries.keys() {
        if !used_boundary_tags.contains(tag) {
            return Err(format!(
                "Thermal boundary tag {tag} has no exterior edges in the mesh."
            ));
        }
    }
    let anchored = constraints
        .keys()
        .map(|&dof| root(&mut parent, representatives[dof]))
        .collect::<HashSet<_>>();
    let mut mean_weights = BTreeMap::<usize, BTreeMap<usize, f64>>::new();
    let mut component_loads = BTreeMap::<usize, (f64, f64)>::new();
    for node in 0..count {
        let component = root(&mut parent, node);
        if !anchored.contains(&component) {
            *mean_weights
                .entry(component)
                .or_default()
                .entry(periodic.node_to_dof[node])
                .or_default() += node_areas[node];
            let load = component_loads.entry(component).or_default();
            load.0 += rhs[node];
            load.1 += absolute_loads[node];
        }
    }
    for (&component, &(net, absolute)) in &component_loads {
        if !net.is_finite() || !absolute.is_finite() || net.abs() > 1e-10 * absolute {
            return Err(format!(
                "The pure-flux conduction component containing node {component} has incompatible net heat load {net:.12e} W/m. Its total outward heat flux must be zero; a mean-temperature reference cannot balance an applied heat source."
            ));
        }
    }

    // Identify the paired trace nodes before applying temperature constraints,
    // retaining symmetric stiffness and a zero imaginary thermal load.
    let imaginary_rhs = vec![0.0; count];
    let (mut matrix, mut rhs, mut imaginary_rhs) =
        scalar2d::reduce_system(&matrix, &rhs, &imaginary_rhs, periodic)?;
    scalar2d::apply_dirichlet(&mut matrix, &mut rhs, &mut imaginary_rhs, &constraints)?;
    let mean_constraints = mean_weights.len();
    for (component, weights) in mean_weights {
        let area = weights.values().sum::<f64>();
        if !area.is_finite() || area <= 0.0 {
            return Err(format!(
                "The conduction component containing node {component} has no finite positive area for its mean-temperature reference."
            ));
        }
        let multiplier = matrix.len();
        matrix.push(BTreeMap::new());
        rhs.push(mean_temperature);
        imaginary_rhs.push(0.0);
        for (dof, weight) in weights {
            let weight = C {
                re: weight / area,
                im: 0.0,
            };
            matrix[dof].insert(multiplier, weight);
            matrix[multiplier].insert(dof, weight);
        }
    }
    if rhs
        .iter()
        .chain(&imaginary_rhs)
        .any(|value| !value.is_finite())
        || matrix
            .iter()
            .flat_map(|row| row.values())
            .any(|value| !value.re.is_finite() || !value.im.is_finite())
    {
        return Err(
            "Periodic reduction or prescribed temperatures produced a non-finite thermal system."
                .into(),
        );
    }
    let mut cache = SolverCache::default();
    let (temperature_kelvin, imaginary, residual) = solve_linear_system(
        &matrix,
        &rhs,
        &imaginary_rhs,
        1e-7,
        &settings.backend,
        &settings.ordering,
        settings.threads,
        &mut cache,
    )?;
    let (temperature_kelvin, imaginary) = scalar2d::expand_complex_solution(
        &temperature_kelvin[..periodic.dof_count],
        &imaginary[..periodic.dof_count],
        periodic,
    )?;
    for (node, (&temperature, &imaginary)) in temperature_kelvin.iter().zip(&imaginary).enumerate()
    {
        if !temperature.is_finite()
            || !imaginary.is_finite()
            || imaginary.abs() > 1e-10 * temperature.abs().max(1.0)
        {
            return Err(format!(
                "Thermal solve returned a non-finite or non-real temperature at node {node}."
            ));
        }
    }
    let mut heat_flux_by_cell = Vec::with_capacity(mesh.triangles.len());
    for (cell, (triangle, _)) in mesh.triangles.iter().enumerate() {
        let mut flux = [0.0; 3];
        for local in 0..3 {
            for axis in 0..2 {
                flux[axis] -= conductivity_by_cell[cell]
                    * cell_gradients[cell][local][axis]
                    * temperature_kelvin[triangle[local]];
            }
        }
        if flux.iter().any(|value| !value.is_finite()) {
            return Err(format!(
                "Thermal cell {cell} produced non-finite heat flux."
            ));
        }
        heat_flux_by_cell.push(flux);
    }
    Ok(Solution {
        temperature_kelvin,
        heat_flux_by_cell,
        conductivity_by_cell,
        residual,
        unknowns: periodic.dof_count,
        mean_constraints,
    })
}

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

/// Reject a stale or unrelated mesh-map pair before using its trace tags and
/// translation to identify nodes. Coordinate matching itself is shared.
fn periodic_dof_map(
    model: &Value,
    map: &Value,
    mesh: &Mesh2D,
    expressions: &ExpressionContext,
) -> Result<PeriodicDofMap, String> {
    let component = &model["model"];
    let physics =
        crate::model_schema::physics(component).ok_or("model.physics must be an object.")?;
    let active = crate::model_schema::physics_periodic_conditions(physics)
        .filter(|feature| feature.get("enabled").and_then(Value::as_bool) != Some(false))
        .collect::<Vec<_>>();
    if !active.is_empty() {
        let pairs = component
            .pointer("/mesh/periodicPairs")
            .and_then(Value::as_array)
            .ok_or("Thermal mesh periodicPairs must be an array.")?;
        let mapped_pairs = array(map, "periodicPairs", "mesh map")?;
        let tolerance = map
            .get("toleranceM")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(1e-8)
            .max(1e-10);
        for &feature in &active {
            let id = text(feature, "periodicPairId", "thermal periodic condition")?;
            if let Some(phase) = feature.get("phaseVector") {
                let values = phase.as_array().filter(|values| values.len() == 3).ok_or(
                    "A thermal periodic condition requires three phase-vector quantities.",
                )?;
                for (axis, value) in values.iter().enumerate() {
                    if expressions
                        .quantity(
                            value,
                            "rad/m",
                            &format!("thermal periodic condition {id}.phaseVector[{axis}]"),
                        )?
                        .abs()
                        > 1e-12
                    {
                        return Err(
                            "Stationary thermal conduction supports zero-phase periodicity only."
                                .into(),
                        );
                    }
                }
            }
            let pair = pairs
                .iter()
                .find(|pair| pair.get("id").and_then(Value::as_str) == Some(id))
                .ok_or_else(|| format!("Thermal periodic pair {id:?} is absent from the model."))?;
            let matches = mapped_pairs
                .iter()
                .filter(|pair| pair.get("id").and_then(Value::as_str) == Some(id))
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "Thermal periodic pair {id:?} must occur exactly once in the mesh map."
                ));
            }
            let mapped = matches[0];
            for side in ["lowerSelectionId", "upperSelectionId"] {
                if mapped.get(side) != pair.get(side) {
                    return Err(format!(
                        "Thermal periodic pair {id:?}.{side} does not match its mesh-map declaration; regenerate the mesh."
                    ));
                }
            }
            let translation = array(pair, "translation", "mesh periodic pair")?;
            let mapped_translation = array(mapped, "translation", "mesh-map periodic pair")?;
            if translation.len() != 3 || mapped_translation.len() != 3 {
                return Err(format!(
                    "Thermal periodic pair {id:?} requires three translation quantities in both model and mesh map."
                ));
            }
            for axis in 0..3 {
                let value = expressions.quantity(
                    &translation[axis],
                    "m",
                    &format!("periodic pair {id}.translation[{axis}]"),
                )?;
                let mapped_value = expressions.quantity(
                    &mapped_translation[axis],
                    "m",
                    &format!("mesh-map periodic pair {id}.translation[{axis}]"),
                )?;
                if (value - mapped_value).abs() > tolerance {
                    return Err(format!(
                        "Thermal periodic pair {id:?}.translation[{axis}] does not match its mesh-map declaration; regenerate the mesh."
                    ));
                }
            }
        }
    }
    // The shared coordinate matcher only needs SI phase and translation data.
    // Resolve those through the five-dimensional kernel first so unrelated
    // kelvin-valued parameters never enter the legacy wave expression adapter.
    let mut resolved_model = model.clone();
    resolved_model["model"]["parameters"] = json!([]);
    if let Some(physics) = resolved_model.pointer_mut("/model/physics") {
        for feature in crate::model_schema::physics_conditions_mut(physics).filter(|feature| {
            feature.get("enabled").and_then(Value::as_bool) != Some(false)
                && feature.get("type").and_then(Value::as_str) == Some("periodicCondition")
        }) {
            if let Some(phase) = feature.get_mut("phaseVector").and_then(Value::as_array_mut) {
                for (axis, quantity) in phase.iter_mut().enumerate() {
                    let value = expressions.quantity(
                        quantity,
                        "rad/m",
                        &format!("thermal phaseVector[{axis}]"),
                    )?;
                    *quantity = json!({"expression":value.to_string(),"unit":"rad/m"});
                }
            }
        }
    }
    let mut resolved_map = map.clone();
    let active_ids = active
        .iter()
        .filter_map(|feature| feature.get("periodicPairId").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    if let Some(pairs) = resolved_map
        .get_mut("periodicPairs")
        .and_then(Value::as_array_mut)
    {
        for pair in pairs.iter_mut().filter(|pair| {
            pair.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| active_ids.contains(id))
        }) {
            if let Some(translation) = pair.get_mut("translation").and_then(Value::as_array_mut) {
                for (axis, quantity) in translation.iter_mut().enumerate() {
                    let value = expressions.quantity(
                        quantity,
                        "m",
                        &format!("thermal mesh-map translation[{axis}]"),
                    )?;
                    *quantity = json!({"expression":value.to_string(),"unit":"m"});
                }
            }
        }
    }
    scalar2d::periodic_dof_map(&resolved_model, &resolved_map, mesh)
}

/// Delegate schema and whole-model FEM checks to the shared dispatcher.
/// The client reuses the dispatcher receipt when this exact model has already
/// passed preflight before the scenario process was launched.
pub(crate) fn validate_model(model: &Value) -> Result<(), String> {
    crate::preflight_client::validate(model)
}

/// Prepare one stationary case without opening mesh input files. The server
/// uses a placeholder mesh path for preflight and supplies real paths when
/// the scenario runner creates its self-contained run directory.
pub(crate) fn prepare_study(envelope: &Value) -> Result<Value, String> {
    let model = envelope
        .get("model")
        .ok_or("Study preparation requires model JSON.")?;
    validate_model(model)?;
    let study_id = text(envelope, "studyId", "prepare request")?;
    let mesh_file = text(envelope, "meshFile", "prepare request")?;
    let study = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .ok_or("model.model.study must be an array.")?
        .iter()
        .find(|study| study.get("id").and_then(Value::as_str) == Some(study_id))
        .ok_or_else(|| format!("Unknown stationary Study {study_id:?}."))?;
    let settings = SolverSettings::from_study(study)?;
    let csv = envelope
        .get("csvFilename")
        .and_then(Value::as_str)
        .or_else(|| study.pointer("/output/csvFilename").and_then(Value::as_str))
        .unwrap_or("thermal-summary.csv");
    if csv.len() > 103
        || !csv.ends_with(".csv")
        || csv.len() <= 4
        || !csv.chars().enumerate().all(|(index, character)| {
            if index == 0 {
                character.is_ascii_alphanumeric()
            } else {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            }
        })
    {
        return Err("The stationary summary CSV must be one safe .csv filename.".into());
    }
    let component = &model["model"];
    let request = json!({
        "schema":"fem-solver-request/1", "solver":"thermal-2d", "studyId":study_id,
        "fieldFormulation":"T", "elementOrder":1,
        "constitutiveQuadrature":settings.constitutive_quadrature,
        "solverBackend":settings.backend, "solverOrdering":settings.ordering,
        "solverThreads":settings.threads,
        "modelFile":envelope.get("modelFile").and_then(Value::as_str).unwrap_or("model.json"),
        "meshFile":mesh_file,
        "modelMapFile":envelope.get("modelMapFile").and_then(Value::as_str).unwrap_or("mesh/model-model-map.json"),
        "outputDirectory":envelope.get("outputDirectory").and_then(Value::as_str).unwrap_or("output")
    });
    Ok(json!({
        "schema":"fullmeta-study-plan/1", "profileId":"thermal-2d",
        "studyId":study_id, "studyType":"stationary", "totalPoints":1,
        "solverSettings":{
            "fieldFormulation":"T", "elementOrder":1,
            "constitutiveQuadrature":settings.constitutive_quadrature,
            "solverBackend":settings.backend, "solverOrdering":settings.ordering,
            "solverThreads":settings.threads
        },
        "request":request, "study":study, "component":component,
        "csvFilename":csv,
        "fieldFiles":study.pointer("/output/fieldFiles").and_then(Value::as_bool).unwrap_or(true)
    }))
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
        || text(&request, "solver", "request")? != "thermal-2d"
        || text(&request, "fieldFormulation", "request")? != "T"
        || request.get("elementOrder").and_then(Value::as_u64) != Some(1)
    {
        return Err("The thermal request must use fem-solver-request/1, solver 'thermal-2d', formulation 'T' and first-order elements.".into());
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
        || map.get("dimension").and_then(Value::as_u64) != Some(2)
    {
        return Err("Thermal 2D requires a dimension-2 fullmeta-mesh-map/6 map.".into());
    }
    let mesh = mesh2d::parse_mesh_v2(&fs::read_to_string(&paths["meshFile"])?)?;
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
    let periodic = periodic_dof_map(&model, &map, &mesh, &expressions)?;
    crate::solver_common::stage_started("assembly");
    let solution = solve_with_mean(
        &mesh,
        &conductivity,
        &boundaries,
        &settings,
        &periodic,
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
        "schema":"fullmeta-thermal-field/1", "dimension":2, "studyId":study_id,
        "fieldComponentCoordinateSystem":"model", "nodes":mesh.nodes,
        "triangles":mesh.triangles.iter().map(|(nodes,tag)| json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>(),
        "temperatureK":solution.temperature_kelvin,
        "fields":{"T":{"real":solution.temperature_kelvin,"imag":vec![0.0;mesh.nodes.len()],"unit":"K","source":"solved"}},
        "heatFluxByCell":{"values":solution.heat_flux_by_cell,"unit":"W/m^2","source":"-k grad(T)","coordinateSystem":"model"},
        "materialFieldsByCell":{"k":{
            "real":solution.conductivity_by_cell.iter().map(|&value| [value;3]).collect::<Vec<_>>(),
            "imag":vec![[0.0;3];mesh.triangles.len()],"unit":"W/(m*K)"
        }},
        "minimumTemperatureK":minimum, "maximumTemperatureK":maximum,
        "solver":{"backend":settings.backend,"ordering":settings.ordering,"threads":settings.threads,"constitutiveQuadrature":settings.constitutive_quadrature,"residual":solution.residual,"unknowns":solution.unknowns,
            "meanTemperatureConstraints":solution.mean_constraints,"meanTemperatureK":mean_temperature,"systemUnknowns":solution.unknowns+solution.mean_constraints},
        "fieldConvention":"stationary", "heatFluxConvention":"positive outward normal flux removes heat",
        "derivedFieldRecovery":"constant per triangle; material interface jumps preserved"
    });
    let output_directory = &paths["outputDirectory"];
    fs::create_dir_all(output_directory)?;
    crate::solver_common::stage_started("output");
    fs::write(
        output_directory.join("temperature.json"),
        serde_json::to_vec_pretty(&output)?,
    )?;
    println!(
        "Thermal 2D · {} unknowns · {} triangles · residual {:.2e} · T [{minimum:.6e}, {maximum:.6e}] K",
        solution.unknowns,
        mesh.triangles.len(),
        solution.residual
    );
    Ok(())
}
