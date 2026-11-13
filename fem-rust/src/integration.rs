use crate::C;
use fullmeta_core::expression::{AstNode, node_symbols, parse_expression};
use fullmeta_core::pointwise::{ComplexValue, evaluate_complex_call, evaluate_complex_definitions};
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
fn complex_sub(a: ComplexValue, b: ComplexValue) -> ComplexValue {
    ComplexValue {
        re: a.re - b.re,
        im: a.im - b.im,
    }
}
fn complex_mul(a: ComplexValue, b: ComplexValue) -> ComplexValue {
    ComplexValue {
        re: a.re * b.re - a.im * b.im,
        im: a.re * b.im + a.im * b.re,
    }
}
fn complex_div(a: ComplexValue, b: ComplexValue) -> ComplexValue {
    let d = b.re * b.re + b.im * b.im;
    ComplexValue {
        re: (a.re * b.re + a.im * b.im) / d,
        im: (a.im * b.re - a.re * b.im) / d,
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

fn parse_operators(
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
    pointwise: &'a [(String, AstNode)],
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
    let pointwise = evaluate_complex_definitions(context.pointwise, &values)
        .map_err(|error| format!("Pointwise field expression at integration point: {error}"))?;
    values.extend(pointwise);
    Ok((values, point))
}

fn eval_ast(
    node: &AstNode,
    environment: &HashMap<String, ComplexValue>,
    context: &EvalContext<'_>,
) -> Result<ComplexValue, String> {
    match node {
        AstNode::Number { value } => Ok(ComplexValue {
            re: *value,
            im: 0.0,
        }),
        AstNode::Symbol { name } => match name.as_str() {
            "pi" => Ok(ComplexValue {
                re: std::f64::consts::PI,
                im: 0.0,
            }),
            "e" => Ok(ComplexValue {
                re: std::f64::consts::E,
                im: 0.0,
            }),
            _ => environment
                .get(name)
                .copied()
                .ok_or_else(|| format!("Expression uses undefined symbol {name:?}.")),
        },
        AstNode::Unary { op, value } => {
            let value = eval_ast(value, environment, context)?;
            match op.as_str() {
                "+" => Ok(value),
                "-" => Ok(ComplexValue {
                    re: -value.re,
                    im: -value.im,
                }),
                _ => Err(format!("Unsupported unary expression operator {op:?}.")),
            }
        }
        AstNode::Binary { op, left, right } => {
            let a = eval_ast(left, environment, context)?;
            let b = eval_ast(right, environment, context)?;
            match op.as_str() {
                "+" => Ok(complex_add(a, b)),
                "-" => Ok(complex_sub(a, b)),
                "*" => Ok(complex_mul(a, b)),
                "/" => Ok(complex_div(a, b)),
                "^" => evaluate_complex_call("pow", &[a, b]),
                _ => Err(format!("Unsupported binary expression operator {op:?}.")),
            }
        }
        AstNode::Call { name, args } => {
            if let Some(operator) = context
                .operators
                .iter()
                .find(|operator| operator.id == *name)
            {
                if args.len() != 1 {
                    return Err(format!(
                        "Integration operator {name} expects exactly one expression."
                    ));
                }
                return integrate_operator(operator, &args[0], environment, context);
            }
            let values = args
                .iter()
                .map(|argument| eval_ast(argument, environment, context))
                .collect::<Result<Vec<_>, _>>()?;
            evaluate_complex_call(name, &values)
        }
    }
}

fn integrate_operator(
    operator: &IntegrationOperator,
    integrand: &AstNode,
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
            return eval_ast(integrand, &values, context);
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
                    let (values, _) = with_point_values(context, cell, &bary, environment)?;
                    let value = eval_ast(integrand, &values, context)?;
                    integral = complex_add(
                        integral,
                        ComplexValue {
                            re: value.re * area * q[3],
                            im: value.im * area * q[3],
                        },
                    );
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
                    let (values, _) = with_point_values(context, cell, &bary, environment)?;
                    let value = eval_ast(integrand, &values, context)?;
                    integral = complex_add(
                        integral,
                        ComplexValue {
                            re: value.re * jacobian * q[4],
                            im: value.im * jacobian * q[4],
                        },
                    );
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
                    let (values, _) = with_point_values(context, cell, &bary, environment)?;
                    let value = eval_ast(integrand, &values, context)?;
                    integral = complex_add(
                        integral,
                        ComplexValue {
                            re: value.re * edge_length * weight * 0.5,
                            im: value.im * edge_length * weight * 0.5,
                        },
                    );
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
                    let (values, _) = with_point_values(context, cell, &bary, environment)?;
                    let value = eval_ast(integrand, &values, context)?;
                    integral = complex_add(
                        integral,
                        ComplexValue {
                            re: value.re * face_area * q[3],
                            im: value.im * face_area * q[3],
                        },
                    );
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
                    let (values, _) = with_point_values(context, cell, &bary, environment)?;
                    let value = eval_ast(integrand, &values, context)?;
                    integral = complex_add(
                        integral,
                        ComplexValue {
                            re: value.re * edge_length * weight * 0.5,
                            im: value.im * edge_length * weight * 0.5,
                        },
                    );
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
    let mut definitions = HashMap::<String, AstNode>::new();
    let mut declaration_order = Vec::new();
    for (symbol, ast) in variables {
        if definitions.insert(symbol.clone(), ast.clone()).is_some() {
            return Err(format!("Duplicate model variable symbol {symbol:?}."));
        }
        declaration_order.push(symbol.clone());
    }
    if declaration_order.is_empty() {
        return Ok(Map::new());
    }
    let mut order = Vec::new();
    let mut done = HashSet::new();
    let mut active = Vec::new();
    fn visit(
        symbol: &str,
        definitions: &HashMap<String, AstNode>,
        done: &mut HashSet<String>,
        active: &mut Vec<String>,
        order: &mut Vec<String>,
    ) -> Result<(), String> {
        if done.contains(symbol) {
            return Ok(());
        }
        if let Some(index) = active.iter().position(|value| value == symbol) {
            let mut cycle = active[index..].to_vec();
            cycle.push(symbol.to_string());
            return Err(format!(
                "Global variable dependency cycle: {}.",
                cycle.join(" → ")
            ));
        }
        active.push(symbol.to_string());
        let ast = definitions
            .get(symbol)
            .ok_or_else(|| format!("Missing global variable definition {symbol:?}."))?;
        for dependency in node_symbols(ast) {
            if definitions.contains_key(&dependency) {
                visit(&dependency, definitions, done, active, order)?;
            }
        }
        active.pop();
        done.insert(symbol.to_string());
        order.push(symbol.to_string());
        Ok(())
    }
    for symbol in &declaration_order {
        visit(symbol, &definitions, &mut done, &mut active, &mut order)?;
    }

    let context = EvalContext {
        dimension,
        nodes,
        domains: domain_cells,
        boundaries: boundary_cells,
        pointwise,
        operators,
        field_sampler,
        material_sampler,
        coordinate_order,
        coordinate_origin,
    };
    let mut output = Map::new();
    for symbol in order {
        let ast = definitions.get(&symbol).unwrap();
        let value = eval_ast(ast, &environment, &context)
            .map_err(|error| format!("Global variable {symbol}: {error}"))?;
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err(format!(
                "Global variable {symbol} evaluated to a non-finite value."
            ));
        }
        environment.insert(symbol.clone(), value);
        output.insert(symbol, json!({"re":value.re,"im":value.im}));
    }
    Ok(output)
}

/// Model 2 wrapper for nodal-field solvers. EM3D uses the same evaluator with
/// its Nedelec basis sampler through `evaluate_variables_with_sampler`.
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
    let operators = parse_operators(model, component, map)?;
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
    let sampler = |cell: &Simplex, barycentric: &[f64], _point: [f64; 3]| {
        interpolate_fields(fields, cell, barycentric)
    };
    let assignments = crate::material::assignments_by_physical_tag(model, map)?;
    let profile_id = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let material_sampler = |cell: &Simplex,
                            point: [f64; 3]|
     -> Result<HashMap<String, ComplexValue>, String> {
        let tag = cell.material_tag.unwrap_or(cell.physical_tag);
        let material = assignments.get(&tag).ok_or_else(|| {
            format!("Integration entity uses material physical tag {tag}, which has no assignment.")
        })?;
        let values = crate::material::values_for_assignment(
            &profile_id,
            material,
            &resolved,
            frequency_hz,
            point,
            "integration material",
        )?;
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
        &sampler,
        &material_sampler,
        [0, 1, 2],
        [0.0; 3],
    )
}
