use crate::C;
use fullmeta_core::expression::{AstNode, parse_expression};
use fullmeta_core::pointwise::{ComplexValue, DefinitionPlan};
use crate::core_language::{Context, Number, Program};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(super) struct Simplex {
    pub(super) nodes: Vec<usize>,
    pub(super) physical_tag: usize,
    pub(super) material_tag: Option<usize>,
    pub(super) operator_id: Option<String>,
    pub(super) element_index: Option<usize>,
    pub(super) adjacent_element_index: Option<usize>,
}

#[derive(Clone)]
pub(super) struct IntegrationOperator {
    pub(super) id: String,
    pub(super) kind: String,
    pub(super) physical_tag: usize,
    pub(super) host_material_tag: Option<usize>,
    pub(super) point_m: Option<[f64; 3]>,
}

fn complex_add(a: ComplexValue, b: ComplexValue) -> ComplexValue {
    ComplexValue {
        re: a.re + b.re,
        im: a.im + b.im,
    }
}
fn selection_tags(map: &Value, collection: &str) -> Result<HashMap<String, usize>, String> {
    let rows = map
        .get(collection)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("mesh map.{collection} must be an array."))?;
    let mut result = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        let id = row
            .get("selectionId")
            .and_then(Value::as_str)
            .or_else(|| row.get("id").and_then(Value::as_str))
            .ok_or_else(|| format!("mesh map.{collection}[{index}] has no selection ID."))?;
        let tag = row
            .get("physicalTag")
            .and_then(Value::as_u64)
            .filter(|tag| *tag > 0)
            .ok_or_else(|| {
                format!("mesh map.{collection}[{index}] has no positive physical tag.")
            })? as usize;
        if result.insert(id.to_string(), tag).is_some() {
            return Err(format!("mesh map.{collection} repeats selection {id:?}."));
        }
    }
    Ok(result)
}

pub(crate) fn parse_operators(
    model: &Value,
    component: &Value,
    map: &Value,
) -> Result<Vec<IntegrationOperator>, String> {
    let domains = selection_tags(map, "physicalGroups")?;
    let boundaries = selection_tags(map, "boundarySelections")?;
    let host_material_tags = map
        .get("boundarySelections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some((
                row.get("id")
                    .or_else(|| row.get("selectionId"))?
                    .as_str()?
                    .to_string(),
                row.get("hostSelectionId")?.as_str()?.to_string(),
            ))
        })
        .filter_map(|(selection_id, host_id)| {
            domains
                .get(&host_id)
                .copied()
                .map(|tag| (selection_id, tag))
        })
        .collect::<HashMap<_, _>>();
    let edges = map
        .get("edgeSelections")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let id = row
                .get("selectionId")
                .and_then(Value::as_str)
                .or_else(|| row.get("id").and_then(Value::as_str))
                .ok_or_else(|| format!("mesh map.edgeSelections[{index}] has no selection ID."))?;
            let tag = row
                .get("physicalTag")
                .and_then(Value::as_u64)
                .filter(|tag| *tag > 0)
                .ok_or_else(|| {
                    format!("mesh map.edgeSelections[{index}] has no positive physical tag.")
                })? as usize;
            Ok((id.to_string(), tag))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    let points = map
        .get("pointSelections")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut ids = HashSet::new();
    let mut result = Vec::new();
    let operators = model
        .pointer("/model/integrationOperators")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    for (index, item) in operators.into_iter().enumerate() {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("model.integrationOperators[{index}].id is required."))?
            .to_string();
        if !ids.insert(id.clone()) {
            return Err(format!("Duplicate integration operator ID {id:?}."));
        }
        let selection_id = item
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Integration operator {id} has no selectionId."))?;
        let selection = component
            .get("selections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|selection| selection.get("id").and_then(Value::as_str) == Some(selection_id))
            .ok_or_else(|| {
                format!("Integration operator {id} references unknown selection {selection_id:?}.")
            })?;
        let kind = crate::model_schema::integration_kind_for_selection(selection)
            .ok_or_else(|| format!("Integration operator {id} selection {selection_id:?} has no schema-declared integration kind."))?;
        let (tag, point_m) = match kind.as_str() {
            "domainIntegration" => (*domains.get(selection_id).ok_or_else(|| format!("Integration operator {id} selection {selection_id:?} is missing from mesh-map physical groups."))?, None),
            "boundaryIntegration" => (*boundaries.get(selection_id).ok_or_else(|| format!("Integration operator {id} selection {selection_id:?} is missing from mesh-map boundary selections."))?, None),
            "edgeIntegration" => (*edges.get(selection_id).ok_or_else(|| format!("Integration operator {id} selection {selection_id:?} is missing from mesh-map edge selections."))?, None),
            "pointEvaluation" => {
                let point = points.iter().find(|point| point.get("id").and_then(Value::as_str) == Some(selection_id))
                    .ok_or_else(|| format!("Point evaluation operator {id} selection {selection_id:?} is missing from the mesh map."))?;
                let coordinates = point.get("coordinateM").and_then(Value::as_array).ok_or_else(|| {
                    let count = point.get("meshNodeCount").and_then(Value::as_u64);
                    match count {
                        Some(count) => format!("Point selection {selection_id:?} resolves to {count} mesh nodes; point evaluation requires exactly one."),
                        None => format!("Point selection {selection_id:?} has no single mesh-node coordinate; point evaluation requires exactly one selected mesh node."),
                    }
                })?;
                if coordinates.len() != 2 && coordinates.len() != 3 { return Err(format!("Point selection {selection_id:?} coordinateM must have two or three components.")); }
                let mut point_m = [0.0; 3];
                for (axis, coordinate) in coordinates.iter().enumerate() {
                    point_m[axis] = coordinate.as_f64().filter(|value| value.is_finite()).ok_or_else(|| format!("Point selection {selection_id:?} coordinateM[{axis}] must be finite."))?;
                }
                (0, Some(point_m))
            }
            other => return Err(format!("Integration operator {id} has unsupported type {other:?}.")),
        };
        let host_material_tag = if kind == "boundaryIntegration" || kind == "edgeIntegration" {
            host_material_tags.get(selection_id).copied()
        } else {
            None
        };
        result.push(IntegrationOperator {
            id,
            kind,
            physical_tag: tag,
            host_material_tag,
            point_m,
        });
    }
    Ok(result)
}

fn point_on_simplex(
    nodes: &[[f64; 3]],
    cell: &Simplex,
    barycentric: &[f64],
) -> Result<[f64; 3], String> {
    if cell.nodes.len() != barycentric.len() {
        return Err("Integration quadrature and mesh simplex dimensions do not match.".into());
    }
    let mut point = [0.0; 3];
    for (node, weight) in cell.nodes.iter().zip(barycentric) {
        let coordinate = nodes
            .get(*node)
            .ok_or_else(|| format!("Integration simplex references missing node {node}."))?;
        for axis in 0..3 {
            point[axis] += coordinate[axis] * weight;
        }
    }
    Ok(point)
}

fn interpolate_fields(
    fields: &HashMap<String, Vec<C>>,
    cell: &Simplex,
    barycentric: &[f64],
) -> Result<HashMap<String, ComplexValue>, String> {
    let mut result = HashMap::new();
    for (name, values) in fields {
        let mut value = ComplexValue::default();
        for (node, weight) in cell.nodes.iter().zip(barycentric) {
            let sample = values.get(*node).ok_or_else(|| {
                format!("Solver field {name} has no nodal value for node {node}.")
            })?;
            value.re += sample.re * weight;
            value.im += sample.im * weight;
        }
        result.insert(name.clone(), value);
    }
    Ok(result)
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn length(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
fn area(points: [[f64; 3]; 3]) -> f64 {
    0.5 * length(cross(sub(points[1], points[0]), sub(points[2], points[0])))
}
fn volume(points: [[f64; 3]; 4]) -> f64 {
    let a = sub(points[1], points[0]);
    let b = sub(points[2], points[0]);
    let c = sub(points[3], points[0]);
    let cross = cross(a, b);
    (cross[0] * c[0] + cross[1] * c[1] + cross[2] * c[2]).abs() / 6.0
}

pub(super) fn variable_real_flags(rows: &[Vec<C>]) -> Vec<bool> {
    let count = rows.first().map_or(0, Vec::len);
    (0..count)
        .map(|index| {
            rows.iter().all(|row| {
                row.get(index)
                    .is_some_and(|value| value.im.abs() <= 1e-12 * value.re.abs().max(1.0))
            })
        })
        .collect()
}

pub(super) fn append_variable_csv_headers(
    csv: &mut String,
    symbols: &[String],
    real_flags: &[bool],
) {
    for (index, symbol) in symbols.iter().enumerate() {
        if real_flags.get(index).copied().unwrap_or(false) {
            csv.push_str(&format!(",{symbol}"));
        } else {
            csv.push_str(&format!(",{symbol}_re,{symbol}_im"));
        }
    }
}

pub(super) fn append_variable_csv_values(csv: &mut String, values: &[C], real_flags: &[bool]) {
    for (index, value) in values.iter().enumerate() {
        csv.push_str(&format!(",{:.14e}", value.re));
        if !real_flags.get(index).copied().unwrap_or(false) {
            csv.push_str(&format!(",{:.14e}", value.im));
        }
    }
}

struct EvalContext<'a> {
    dimension: usize,
    nodes: &'a [[f64; 3]],
    domains: &'a [Simplex],
    boundaries: &'a [Simplex],
    pointwise: Option<&'a DefinitionPlan>,
    operators: &'a [IntegrationOperator],
    field_sampler: &'a (
            dyn Fn(&Simplex, &[f64], [f64; 3]) -> Result<HashMap<String, ComplexValue>, String> + 'a
        ),
    material_sampler:
        &'a (dyn Fn(&Simplex, [f64; 3]) -> Result<HashMap<String, ComplexValue>, String> + 'a),
    coordinate_order: [usize; 3],
    coordinate_origin: [f64; 3],
}

fn with_point_values(
    context: &EvalContext<'_>,
    cell: &Simplex,
    barycentric: &[f64],
    outer: &HashMap<String, ComplexValue>,
) -> Result<(HashMap<String, ComplexValue>, [f64; 3]), String> {
    let point = point_on_simplex(context.nodes, cell, barycentric)?;
    let mut values = outer.clone();
    let model_point = std::array::from_fn(|axis| {
        point[context.coordinate_order[axis]] + context.coordinate_origin[axis]
    });
    for axis in 0..3 {
        let coordinate = model_point[axis];
        values.insert(
            ["x", "y", "z"][axis].to_string(),
            ComplexValue {
                re: coordinate,
                im: 0.0,
            },
        );
    }
    values.extend((context.field_sampler)(cell, barycentric, point)?);
    values.extend((context.material_sampler)(cell, model_point)?);
    Ok((values, point))
}

fn eval_program_batch(
    program: &Program,
    environments: &[HashMap<String, ComplexValue>],
    context: &EvalContext<'_>,
) -> Result<Vec<ComplexValue>, String> {
    let mut symbols = Vec::with_capacity(environments.len() * program.symbols.len());
    let mut operators = Vec::with_capacity(environments.len() * program.operators.len());
    for environment in environments {
        for name in &program.symbols {
            let value = environment.get(name)
                .ok_or_else(|| format!("Expression uses undefined symbol {name:?}."))?;
            symbols.push(Number { re: value.re, im: value.im });
        }
        for host in &program.operators {
            let operator = context.operators.iter().find(|operator| operator.id == host.name)
                .ok_or_else(|| format!("Unknown integration operator {:?}.", host.name))?;
            let value = integrate_operator(operator, &host.argument, environment, context)?;
            operators.push(Number { re: value.re, im: value.im });
        }
    }
    Ok(program.evaluate_batch(environments.len(), &symbols, &operators)?.into_iter()
        .map(|value| ComplexValue { re: value.re, im: value.im }).collect())
}

fn eval_program(program: &Program, environment: &HashMap<String, ComplexValue>,
    context: &EvalContext<'_>) -> Result<ComplexValue, String> {
    Ok(eval_program_batch(program, std::slice::from_ref(environment), context)?[0])
}

fn flush_quadrature(program: &Program, points: &mut Vec<HashMap<String, ComplexValue>>,
    slots: &mut Vec<Number>, weights: &mut Vec<[f64; 3]>, total: &mut ComplexValue,
    context: &EvalContext<'_>) -> Result<(), String> {
    if weights.is_empty() { return Ok(()); }
    if let Some(plan) = &context.pointwise {
        plan.evaluate_batch(points).map_err(|error| format!("Pointwise field expression at integration point: {error}"))?;
    }
    let values = if program.operators.is_empty() && context.pointwise.is_none() {
        program.evaluate_batch(weights.len(), slots, &[])?.into_iter()
            .map(|value| ComplexValue { re: value.re, im: value.im }).collect()
    } else {
        eval_program_batch(program, points, context)?
    };
    for (value, weight) in values.into_iter().zip(weights.drain(..)) {
        *total = complex_add(*total, ComplexValue {
            re: value.re * weight[0] * weight[1] * weight[2],
            im: value.im * weight[0] * weight[1] * weight[2],
        });
    }
    points.clear();
    slots.clear();
    Ok(())
}

fn integrate_operator(
    operator: &IntegrationOperator,
    integrand: &Program,
    environment: &HashMap<String, ComplexValue>,
    context: &EvalContext<'_>,
) -> Result<ComplexValue, String> {
    if operator.kind == "pointEvaluation" {
        let model_point = operator.point_m.ok_or_else(|| {
            format!(
                "Point evaluation operator {} has no coordinate.",
                operator.id
            )
        })?;
        let mut mesh_point = [0.0; 3];
        for axis in 0..3 {
            mesh_point[context.coordinate_order[axis]] =
                model_point[axis] - context.coordinate_origin[axis];
        }
        for cell in context.domains.iter().filter(|cell| {
            cell.operator_id
                .as_deref()
                .map_or(true, |id| id == operator.id)
        }) {
            let Some(barycentric) =
                barycentric_at_point(context.nodes, cell, mesh_point, context.dimension)?
            else {
                continue;
            };
            let (values, _) = with_point_values(context, cell, &barycentric, environment)?;
            let mut rows = [values];
            if let Some(plan) = &context.pointwise {
                plan.evaluate_batch(&mut rows).map_err(|error| format!("Pointwise field expression at integration point: {error}"))?;
            }
            return eval_program(integrand, &rows[0], context);
        }
        return Err(format!(
            "Point evaluation operator {} is outside the meshed domain or its selected domain.",
            operator.id
        ));
    }
    let cells = if operator.kind == "domainIntegration" {
        context.domains
    } else {
        context.boundaries
    };
    let mut integral = ComplexValue::default();
    let mut selected = 0usize;
    let mut points = Vec::with_capacity(128);
    let mut slots = Vec::with_capacity(128 * integrand.symbols.len());
    let mut weights = Vec::with_capacity(128);
    macro_rules! sample {
        ($cell:expr, $bary:expr, $weight:expr) => {{
            let (values, _) = with_point_values(context, $cell, &$bary, environment)?;
            if integrand.operators.is_empty() && context.pointwise.is_none() {
                for name in &integrand.symbols {
                    let value = values.get(name)
                        .ok_or_else(|| format!("Expression uses undefined symbol {name:?}."))?;
                    slots.push(Number { re: value.re, im: value.im });
                }
            } else {
                points.push(values);
            }
            weights.push($weight);
            if weights.len() == 128 {
                flush_quadrature(integrand, &mut points, &mut slots, &mut weights, &mut integral, context)?;
            }
        }};
    }
    for cell in cells.iter().filter(|cell| {
        cell.operator_id
            .as_deref()
            .map_or(cell.physical_tag == operator.physical_tag, |id| {
                id == operator.id.as_str()
            })
    }) {
        let mut evaluation_cell = cell.clone();
        if let Some(material_tag) = operator.host_material_tag {
            evaluation_cell.material_tag = Some(material_tag);
        }
        let cell = &evaluation_cell;
        selected += 1;
        match (context.dimension, operator.kind.as_str(), cell.nodes.len()) {
            (2, "domainIntegration", 3) => {
                let p = cell
                    .nodes
                    .iter()
                    .map(|node| {
                        context.nodes.get(*node).copied().ok_or_else(|| {
                            format!("Integration cell references missing node {node}.")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let area = area([p[0], p[1], p[2]]);
                for q in crate::TRI_Q {
                    let bary = [q[0], q[1], q[2]];
                    sample!(cell, bary, [area, q[3], 1.0]);
                }
            }
            (3, "domainIntegration", 4) => {
                let p = cell
                    .nodes
                    .iter()
                    .map(|node| {
                        context.nodes.get(*node).copied().ok_or_else(|| {
                            format!("Integration cell references missing node {node}.")
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let jacobian = 6.0 * volume([p[0], p[1], p[2], p[3]]);
                for q in crate::TET_Q {
                    let bary = [q[0], q[1], q[2], q[3]];
                    sample!(cell, bary, [jacobian, q[4], 1.0]);
                }
            }
            (2, "boundaryIntegration", 2) => {
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
                let a = *cell.nodes.first().unwrap();
                let b = *cell.nodes.get(1).unwrap();
                let p0 = *context
                    .nodes
                    .get(a)
                    .ok_or_else(|| format!("Boundary edge references missing node {a}."))?;
                let p1 = *context
                    .nodes
                    .get(b)
                    .ok_or_else(|| format!("Boundary edge references missing node {b}."))?;
                let edge_length = length(sub(p1, p0));
                for (position, weight) in POSITIONS.into_iter().zip(WEIGHTS) {
                    let t = (position + 1.0) * 0.5;
                    let bary = [1.0 - t, t];
                    sample!(cell, bary, [edge_length, weight, 0.5]);
                }
            }
            (3, "boundaryIntegration", 3) => {
                let p =
                    cell.nodes
                        .iter()
                        .map(|node| {
                            context.nodes.get(*node).copied().ok_or_else(|| {
                                format!("Boundary face references missing node {node}.")
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                let face_area = area([p[0], p[1], p[2]]);
                for q in crate::TRI_Q {
                    let bary = [q[0], q[1], q[2]];
                    sample!(cell, bary, [face_area, q[3], 1.0]);
                }
            }
            (3, "edgeIntegration", 2) => {
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
                let a = *cell.nodes.first().unwrap();
                let b = *cell.nodes.get(1).unwrap();
                let p0 = *context
                    .nodes
                    .get(a)
                    .ok_or_else(|| format!("Integration edge references missing node {a}."))?;
                let p1 = *context
                    .nodes
                    .get(b)
                    .ok_or_else(|| format!("Integration edge references missing node {b}."))?;
                let edge_length = length(sub(p1, p0));
                for (position, weight) in POSITIONS.into_iter().zip(WEIGHTS) {
                    let t = (position + 1.0) * 0.5;
                    let bary = [1.0 - t, t];
                    sample!(cell, bary, [edge_length, weight, 0.5]);
                }
            }
            _ => {
                return Err(format!(
                    "Integration operator {} has a mesh entity incompatible with the {}D {} measure.",
                    operator.id, context.dimension, operator.kind
                ));
            }
        }
    }
    if selected == 0 {
        return Err(format!(
            "Integration operator {} selected no mesh entities.",
            operator.id
        ));
    }
    flush_quadrature(integrand, &mut points, &mut slots, &mut weights, &mut integral, context)?;
    Ok(integral)
}

fn barycentric_at_point(
    nodes: &[[f64; 3]],
    cell: &Simplex,
    point: [f64; 3],
    dimension: usize,
) -> Result<Option<Vec<f64>>, String> {
    if dimension == 2 && cell.nodes.len() == 3 {
        let p = cell
            .nodes
            .iter()
            .map(|node| {
                nodes
                    .get(*node)
                    .copied()
                    .ok_or_else(|| format!("Point selection references missing node {node}."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let det =
            (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
        if det.abs() <= 1e-24 {
            return Err("Point evaluation encountered a degenerate triangle.".into());
        }
        let l1 = ((point[0] - p[0][0]) * (p[2][1] - p[0][1])
            - (p[2][0] - p[0][0]) * (point[1] - p[0][1]))
            / det;
        let l2 = ((p[1][0] - p[0][0]) * (point[1] - p[0][1])
            - (point[0] - p[0][0]) * (p[1][1] - p[0][1]))
            / det;
        let bary = vec![1.0 - l1 - l2, l1, l2];
        return Ok(bary
            .iter()
            .all(|weight| *weight >= -1e-9 && *weight <= 1.0 + 1e-9)
            .then_some(bary));
    }
    if dimension == 3 && cell.nodes.len() == 4 {
        let p = cell
            .nodes
            .iter()
            .map(|node| {
                nodes
                    .get(*node)
                    .copied()
                    .ok_or_else(|| format!("Point selection references missing node {node}."))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let a = sub(p[1], p[0]);
        let b = sub(p[2], p[0]);
        let c = sub(p[3], p[0]);
        let r = sub(point, p[0]);
        let det = dot(a, cross(b, c));
        if det.abs() <= 1e-30 {
            return Err("Point evaluation encountered a degenerate tetrahedron.".into());
        }
        let l1 = dot(r, cross(b, c)) / det;
        let l2 = dot(a, cross(r, c)) / det;
        let l3 = dot(a, cross(b, r)) / det;
        let bary = vec![1.0 - l1 - l2 - l3, l1, l2, l3];
        return Ok(bary
            .iter()
            .all(|weight| *weight >= -1e-9 && *weight <= 1.0 + 1e-9)
            .then_some(bary));
    }
    Ok(None)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Shared expression and quadrature pipeline. Each FEM formulation supplies
/// scalar field symbols selected from the scenario-profile declarations. Any
/// solver-internal vector ordering must be converted to model x, y, z before
/// those symbols are sampled. Geometry measures, pointwise values, dependency
/// ordering, and expression evaluation stay common across scenarios.
pub(super) fn evaluate_variables_with_sampler(
    variables: &[(String, AstNode)],
    pointwise: &[(String, AstNode)],
    operators: &[IntegrationOperator],
    dimension: usize,
    nodes: &[[f64; 3]],
    domain_cells: &[Simplex],
    boundary_cells: &[Simplex],
    mut environment: HashMap<String, ComplexValue>,
    field_sampler: &(
         dyn Fn(&Simplex, &[f64], [f64; 3]) -> Result<HashMap<String, ComplexValue>, String> + '_
     ),
    material_sampler: &(
         dyn Fn(&Simplex, [f64; 3]) -> Result<HashMap<String, ComplexValue>, String> + '_
     ),
    coordinate_order: [usize; 3],
    coordinate_origin: [f64; 3],
) -> Result<Map<String, Value>, String> {
    if variables.is_empty() {
        return Ok(Map::new());
    }
    let operator_names = operators.iter().map(|operator| operator.id.clone()).collect::<Vec<_>>();
    let compiled = Context::compile(variables, pointwise, &operator_names)?;
    let context = EvalContext {
        dimension,
        nodes,
        domains: domain_cells,
        boundaries: boundary_cells,
        pointwise: compiled.pointwise.as_ref(),
        operators,
        field_sampler,
        material_sampler,
        coordinate_order,
        coordinate_origin,
    };
    let mut output = Map::new();
    for (symbol, program) in &compiled.expressions {
        let value = eval_program(program, &environment, &context)
            .map_err(|error| format!("Global variable {symbol}: {error}"))?;
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err(format!(
                "Global variable {symbol} evaluated to a non-finite value."
            ));
        }
        environment.insert(symbol.clone(), value);
        output.insert(symbol.clone(), json!({"re":value.re,"im":value.im}));
    }
    Ok(output)
}

/// Model 2 wrapper for nodal-field solvers; oriented-element solvers use the
/// same evaluator with their sampler through `evaluate_variables_with_sampler`.
pub(super) fn evaluate_model_variables(
    model: &Value,
    component: &Value,
    map: &Value,
    dimension: usize,
    nodes: &[[f64; 3]],
    domain_cells: &[Simplex],
    boundary_cells: &[Simplex],
    fields: &HashMap<String, Vec<C>>,
    frequency_hz: f64,
) -> Result<Map<String, Value>, String> {
    let sampler = |cell: &Simplex, barycentric: &[f64], _point: [f64; 3]| {
        interpolate_fields(fields, cell, barycentric)
    };
    evaluate_model_variables_with_field_sampler(model, component, map, dimension, nodes,
        domain_cells, boundary_cells, frequency_hz, &sampler)
}

/// Parse configured global and pointwise definitions for any study kind.
pub(crate) fn model_expression_definitions(model:&Value) -> Result<(Vec<(String,AstNode)>,Vec<(String,AstNode)>),String> {
    let mut variables = Vec::new();
    for (index, definition) in model
        .pointer("/model/variables")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let symbol = definition
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("model.variables[{index}].symbol is required."))?
            .to_string();
        let expression = definition
            .get("expression")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("model.variables[{index}].expression is required."))?;
        variables.push((
            symbol,
            parse_expression(expression)
                .map_err(|error| format!("model.variables[{index}].expression: {error}"))?,
        ));
    }
    let mut pointwise = Vec::new();
    for (index, definition) in model
        .pointer("/model/pointwiseVariables")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let symbol = definition
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("model.pointwiseVariables[{index}].symbol is required."))?
            .to_string();
        let expression = definition
            .get("expression")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("model.pointwiseVariables[{index}].expression is required."))?;
        pointwise.push((
            symbol,
            parse_expression(expression).map_err(|error| {
                format!("model.pointwiseVariables[{index}].expression: {error}")
            })?,
        ));
    }
    Ok((variables,pointwise))
}

/// Evaluate the same model declarations with a caller-provided physical field
/// sampler. `nodes` and the point passed to the sampler use model coordinates.
/// This permits higher-order and oriented bases without nodal interpolation.
pub(super) fn evaluate_model_variables_with_field_sampler(
    model: &Value,
    component: &Value,
    map: &Value,
    dimension: usize,
    nodes: &[[f64; 3]],
    domain_cells: &[Simplex],
    boundary_cells: &[Simplex],
    frequency_hz: f64,
    field_sampler: &(dyn Fn(&Simplex, &[f64], [f64; 3]) -> Result<HashMap<String, ComplexValue>, String> + '_),
) -> Result<Map<String, Value>, String> {
    let operators = parse_operators(model, component, map)?;
    let (variables,pointwise)=model_expression_definitions(model)?;
    let parameters = model
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let resolved = crate::model_expression::resolve_parameters(parameters, model)?;
    let mut environment = resolved
        .iter()
        .map(|(symbol, value)| {
            (
                symbol.clone(),
                ComplexValue {
                    re: value.value,
                    im: 0.0,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    environment.insert(
        "freq".into(),
        ComplexValue {
            re: frequency_hz,
            im: 0.0,
        },
    );
    environment.insert(
        "frequency".into(),
        ComplexValue {
            re: frequency_hz,
            im: 0.0,
        },
    );
    environment.insert(
        "omega".into(),
        ComplexValue {
            re: std::f64::consts::TAU * frequency_hz,
            im: 0.0,
        },
    );
    let assignments = crate::material::assignments_by_physical_tag(model, map)?;
    let profile_id = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let material_cache = std::cell::RefCell::new(HashMap::<usize, crate::material::CompiledMaterialEvaluator>::new());
    let material_sampler = |cell: &Simplex,
                            point: [f64; 3]|
     -> Result<HashMap<String, ComplexValue>, String> {
        let tag = cell.material_tag.unwrap_or(cell.physical_tag);
        let material = assignments.get(&tag).ok_or_else(|| {
            format!("Integration entity uses material physical tag {tag}, which has no assignment.")
        })?;
        let mut cache = material_cache.borrow_mut();
        if !cache.contains_key(&tag) {
            cache.insert(tag, crate::material::CompiledMaterialEvaluator::compile(&profile_id, material, "integration material")?);
        }
        let values = cache.get(&tag).expect("material evaluator inserted")
            .evaluate(&resolved, frequency_hz, point, "integration material")?;
        Ok(values
            .into_iter()
            .map(|(symbol, value)| {
                (
                    symbol,
                    ComplexValue {
                        re: value.value,
                        im: 0.0,
                    },
                )
            })
            .collect())
    };
    evaluate_variables_with_sampler(
        &variables,
        &pointwise,
        &operators,
        dimension,
        nodes,
        domain_cells,
        boundary_cells,
        environment,
        field_sampler,
        &material_sampler,
        [0, 1, 2],
        [0.0; 3],
    )
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    fn domain_integral(cells: usize) -> Result<Map<String, Value>, String> {
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
        let simplex = Simplex { nodes: vec![0,1,2], physical_tag: 1, material_tag: None,
            operator_id: None, element_index: None, adjacent_element_index: None };
        let domains = vec![simplex; cells];
        let operator = IntegrationOperator { id: "intop".into(), kind: "domainIntegration".into(),
            physical_tag: 1, host_material_tag: None, point_m: None };
        let variables = vec![("result".into(), parse_expression("offset + intop(x * E)").unwrap())];
        let environment = HashMap::from([("offset".into(), ComplexValue { re: 5., im: 0. })]);
        let fields = |_: &Simplex, _: &[f64], _: [f64;3]| Ok(HashMap::from([
            ("E".into(), ComplexValue { re: 2., im: 3. })]));
        let material = |_: &Simplex, _: [f64;3]| Ok(HashMap::new());
        evaluate_variables_with_sampler(&variables, &[], &[operator], 2, &nodes, &domains, &[],
            environment, &fields, &material, [0,1,2], [0.;3])
    }

    #[test]
    fn complex_integrand_on_triangle_matches_exact_integral() {
        let result = domain_integral(1).unwrap();
        assert!((result["result"]["re"].as_f64().unwrap() - (5. + 2./6.)).abs() < 1e-12);
        assert!((result["result"]["im"].as_f64().unwrap() - 3./6.).abs() < 1e-12);
    }

    #[test]
    fn point_edge_and_nested_integrals_are_evaluated() {
        let nodes = [[0.,0.,0.], [1.,0.,0.], [0.,1.,0.]];
        let triangle = Simplex { nodes: vec![0,1,2], physical_tag: 1, material_tag: None,
            operator_id: None, element_index: None, adjacent_element_index: None };
        let edge = Simplex { nodes: vec![0,1], physical_tag: 2, material_tag: None,
            operator_id: None, element_index: None, adjacent_element_index: None };
        let operators = [
            IntegrationOperator { id: "domain".into(), kind: "domainIntegration".into(), physical_tag: 1, host_material_tag: None, point_m: None },
            IntegrationOperator { id: "edge".into(), kind: "boundaryIntegration".into(), physical_tag: 2, host_material_tag: None, point_m: None },
            IntegrationOperator { id: "point".into(), kind: "pointEvaluation".into(), physical_tag: 1, host_material_tag: None, point_m: Some([0.25,0.25,0.]) },
        ];
        let variables = vec![
            ("edgeValue".into(), parse_expression("edge(x)").unwrap()),
            ("pointValue".into(), parse_expression("point(x+y)").unwrap()),
            ("nestedValue".into(), parse_expression("domain(domain(x))").unwrap()),
        ];
        let fields = |_: &Simplex, _: &[f64], _: [f64;3]| Ok(HashMap::new());
        let material = |_: &Simplex, _: [f64;3]| Ok(HashMap::new());
        let result = evaluate_variables_with_sampler(&variables, &[], &operators, 2, &nodes,
            &[triangle], &[edge], HashMap::new(), &fields, &material, [0,1,2], [0.;3]).unwrap();
        assert!((result["edgeValue"]["re"].as_f64().unwrap() - 0.5).abs() < 1e-12);
        assert!((result["pointValue"]["re"].as_f64().unwrap() - 0.5).abs() < 1e-12);
        assert!((result["nestedValue"]["re"].as_f64().unwrap() - 1./12.).abs() < 1e-12);
    }

    #[test]
    fn undefined_integrand_symbols_report_an_error() {
        let nodes = [[0.,0.,0.], [1.,0.,0.], [0.,1.,0.]];
        let triangle = Simplex { nodes: vec![0,1,2], physical_tag: 1, material_tag: None,
            operator_id: None, element_index: None, adjacent_element_index: None };
        let operator = IntegrationOperator { id: "intop".into(), kind: "domainIntegration".into(),
            physical_tag: 1, host_material_tag: None, point_m: None };
        let variables = vec![("result".into(), parse_expression("intop(missing)").unwrap())];
        let fields = |_: &Simplex, _: &[f64], _: [f64;3]| Ok(HashMap::new());
        let material = |_: &Simplex, _: [f64;3]| Ok(HashMap::new());
        let error = evaluate_variables_with_sampler(&variables, &[], &[operator], 2, &nodes, &[triangle], &[],
            HashMap::new(), &fields, &material, [0,1,2], [0.;3]).unwrap_err();
        assert!(error.contains("missing"), "{error}");
    }

    #[test]
    fn three_dimensional_measures_and_pointwise_variables_work() {
        let nodes = [[0.,0.,0.], [1.,0.,0.], [0.,1.,0.], [0.,0.,1.]];
        let cell = |indices: Vec<usize>, tag| Simplex { nodes: indices, physical_tag: tag, material_tag: None,
            operator_id: None, element_index: None, adjacent_element_index: None };
        let operators = [
            IntegrationOperator { id: "volume".into(), kind: "domainIntegration".into(), physical_tag: 1, host_material_tag: None, point_m: None },
            IntegrationOperator { id: "face".into(), kind: "boundaryIntegration".into(), physical_tag: 2, host_material_tag: None, point_m: None },
            IntegrationOperator { id: "line".into(), kind: "edgeIntegration".into(), physical_tag: 3, host_material_tag: None, point_m: None },
        ];
        let variables = [("volumeValue".into(), parse_expression("volume(1)").unwrap()),
            ("faceValue".into(), parse_expression("face(1)").unwrap()),
            ("lineValue".into(), parse_expression("line(p)").unwrap())];
        let pointwise = [("p".into(), parse_expression("2*x").unwrap())];
        let fields = |_: &Simplex, _: &[f64], _: [f64;3]| Ok(HashMap::new());
        let material = |_: &Simplex, _: [f64;3]| Ok(HashMap::new());
        let result = evaluate_variables_with_sampler(&variables, &pointwise, &operators, 3, &nodes,
            &[cell(vec![0,1,2,3], 1)], &[cell(vec![0,1,2], 2), cell(vec![0,1], 3)],
            HashMap::new(), &fields, &material, [0,1,2], [0.;3]).unwrap();
        assert!((result["volumeValue"]["re"].as_f64().unwrap() - 1./6.).abs() < 1e-12);
        assert!((result["faceValue"]["re"].as_f64().unwrap() - 0.5).abs() < 1e-12);
        assert!((result["lineValue"]["re"].as_f64().unwrap() - 1.).abs() < 1e-12);
    }

    pub(crate) fn validate_synthetic_cases() {
        complex_integrand_on_triangle_matches_exact_integral();
        point_edge_and_nested_integrals_are_evaluated();
        undefined_integrand_symbols_report_an_error();
        three_dimensional_measures_and_pointwise_variables_work();
    }

    #[test]
    #[ignore = "explicit synthetic integration timing"]
    fn integration_timing() {
        let start = std::time::Instant::now();
        let result = domain_integral(30000).unwrap();
        println!("synthetic integration seconds={:.6} result={}", start.elapsed().as_secs_f64(), result["result"]);
    }
}
