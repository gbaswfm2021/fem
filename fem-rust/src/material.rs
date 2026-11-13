use crate::model_expression::{self, Scalar};
use fullmeta_core::quantity::Dimension;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

#[derive(Clone)]
struct CompiledProperty {
    symbol: String,
    expression: model_expression::CompiledQuantity,
    dimension: Dimension,
    minimum: Option<(f64, bool)>,
    implemented_values: Option<Vec<f64>>,
    unit: String,
}

#[derive(Clone)]
pub(super) struct CompiledMaterialEvaluator {
    properties: Vec<CompiledProperty>,
    uses_position: bool,
}

struct CompiledMaterialAssignment {
    id: String,
    evaluator: CompiledMaterialEvaluator,
}

/// Compiles each domain material once and reuses the expression trees across
/// all elements and frequencies in a solver run.
pub(super) struct CompiledMaterialAssignments {
    by_tag: HashMap<usize, CompiledMaterialAssignment>,
}

impl CompiledMaterialAssignments {
    pub(super) fn compile(
        profile_id: &str,
        assignments: &HashMap<usize, Value>,
    ) -> Result<Self, String> {
        let mut tags = assignments.keys().copied().collect::<Vec<_>>();
        tags.sort_unstable();
        let mut by_tag = HashMap::with_capacity(assignments.len());
        for tag in tags {
            let material = assignments
                .get(&tag)
                .ok_or("Material assignment disappeared while compiling.")?;
            let id = material
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("assignment")
                .to_string();
            let evaluator = CompiledMaterialEvaluator::compile(
                profile_id,
                material,
                &format!("material {id}"),
            )?;
            by_tag.insert(tag, CompiledMaterialAssignment { id, evaluator });
        }
        Ok(Self { by_tag })
    }

    /// Evaluate properties independent of x/y/z once per material tag for a
    /// frequency case. Frequency and sweep parameters can still change values.
    pub(super) fn position_independent_values(
        &self,
        base_symbols: &HashMap<String, Scalar>,
        frequency_hz: f64,
    ) -> Result<HashMap<usize, HashMap<String, Scalar>>, String> {
        let mut values_by_tag = HashMap::new();
        for (&tag, assignment) in &self.by_tag {
            if assignment.evaluator.uses_position() {
                continue;
            }
            let values = assignment.evaluator.evaluate(
                base_symbols,
                frequency_hz,
                [0.0; 3],
                &format!("material {}", assignment.id),
            )?;
            values_by_tag.insert(tag, values);
        }
        Ok(values_by_tag)
    }

    pub(super) fn evaluate(
        &self,
        tag: usize,
        base_symbols: &HashMap<String, Scalar>,
        frequency_hz: f64,
        point: [f64; 3],
        context: &str,
    ) -> Result<HashMap<String, Scalar>, String> {
        let assignment = self.by_tag.get(&tag).ok_or_else(|| {
            format!("{context} uses physical material tag {tag}, which has no material assignment.")
        })?;
        assignment.evaluator.evaluate(
            base_symbols,
            frequency_hz,
            point,
            &format!("material {} in {context}", assignment.id),
        )
    }
}

impl CompiledMaterialEvaluator {
    pub(super) fn compile(profile_id: &str, material: &Value, path: &str) -> Result<Self, String> {
        let definitions = crate::model_schema::material_properties_for_profile(profile_id)?;
        if definitions.is_empty() {
            return Err(format!(
                "Scenario profile {profile_id:?} declares no material properties."
            ));
        }
        let mut properties = Vec::with_capacity(definitions.len());
        let mut uses_position = false;
        for definition in definitions {
            let key = definition
                .get("key")
                .and_then(Value::as_str)
                .ok_or("Scenario material property is missing key.")?;
            let symbol = definition
                .get("symbol")
                .and_then(Value::as_str)
                .ok_or("Scenario material property is missing symbol.")?;
            let unit = definition
                .get("unit")
                .and_then(Value::as_str)
                .unwrap_or("1")
                .to_string();
            let representation = definition
                .get("representation")
                .and_then(Value::as_str)
                .unwrap_or("expression");
            let raw = material.get(key).or_else(|| definition.get("default"));
            let Some(raw) = raw else {
                if definition
                    .get("optional")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    continue;
                }
                return Err(format!(
                    "{path}.{key} is required by scenario profile {profile_id:?}."
                ));
            };
            let quantity = if representation == "quantity" {
                raw.clone()
            } else {
                let source = raw
                    .as_str()
                    .ok_or_else(|| format!("{path}.{key} must be an expression string."))?;
                json!({"expression":source,"unit":unit})
            };
            let expression =
                model_expression::CompiledQuantity::compile(&quantity, &format!("{path}.{key}"))?;
            uses_position |= ["x", "y", "z"]
                .iter()
                .any(|axis| expression.uses_symbol(axis));
            let dimension = model_expression::unit(&unit)?.1;
            let minimum = definition
                .get("minimum")
                .and_then(Value::as_f64)
                .map(|value| {
                    (
                        value,
                        definition
                            .get("minimumExclusive")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    )
                });
            let implemented_values = definition.get("implementedValues").map(|values| {
                let values = values.as_array().ok_or_else(|| format!("Material property {key:?} implementedValues must be an array."))?;
                values.iter().map(|value| value.as_f64().filter(|value| value.is_finite())
                    .ok_or_else(|| format!("Material property {key:?} implementedValues must contain finite numbers.")))
                    .collect::<Result<Vec<_>, String>>()
            }).transpose()?;
            properties.push(CompiledProperty {
                symbol: symbol.to_string(),
                expression,
                dimension,
                minimum,
                implemented_values,
                unit,
            });
        }
        Ok(Self {
            properties,
            uses_position,
        })
    }

    pub(super) fn uses_position(&self) -> bool {
        self.uses_position
    }

    pub(super) fn evaluate(
        &self,
        base_symbols: &HashMap<String, Scalar>,
        frequency_hz: f64,
        point: [f64; 3],
        path: &str,
    ) -> Result<HashMap<String, Scalar>, String> {
        let mut symbols = base_symbols.clone();
        for (axis, value) in ["x", "y", "z"].into_iter().zip(point) {
            if !value.is_finite() {
                return Err(format!("{path}: coordinate {axis} is not finite."));
            }
            symbols.insert(
                axis.to_string(),
                Scalar {
                    value,
                    dimension: [1.0, 0.0, 0.0, 0.0, 0.0],
                },
            );
        }
        let hz_dimension = model_expression::unit("Hz")?.1;
        symbols.insert(
            "freq".into(),
            Scalar {
                value: frequency_hz,
                dimension: hz_dimension,
            },
        );
        symbols.insert(
            "frequency".into(),
            Scalar {
                value: frequency_hz,
                dimension: hz_dimension,
            },
        );
        symbols.insert(
            "omega".into(),
            Scalar {
                value: std::f64::consts::TAU * frequency_hz,
                dimension: hz_dimension,
            },
        );

        let mut output = HashMap::with_capacity(self.properties.len());
        for property in &self.properties {
            let property_path = format!("{path}.{}", property.symbol);
            let value = property.expression.evaluate(&symbols, &property_path)?;
            if let Some((minimum, exclusive)) = property.minimum {
                if if exclusive {
                    value <= minimum
                } else {
                    value < minimum
                } {
                    return Err(format!(
                        "{property_path} must be {} {minimum} {} at ({}, {}, {}).",
                        if exclusive {
                            "greater than"
                        } else {
                            "at least"
                        },
                        property.unit,
                        point[0],
                        point[1],
                        point[2]
                    ));
                }
            }
            if let Some(implemented) = &property.implemented_values {
                if !implemented.contains(&value) {
                    return Err(format!(
                        "{property_path}: the implemented material model supports only {} {} at ({}, {}, {}).",
                        json!(implemented),
                        property.unit,
                        point[0],
                        point[1],
                        point[2]
                    ));
                }
            }
            let scalar = Scalar {
                value,
                dimension: property.dimension,
            };
            output.insert(property.symbol.clone(), scalar);
            symbols.insert(property.symbol.clone(), scalar);
        }
        Ok(output)
    }
}

pub(super) fn assignments_by_physical_tag(
    model: &Value,
    map: &Value,
) -> Result<HashMap<usize, Value>, String> {
    let material_rows = model
        .pointer("/model/materials")
        .and_then(Value::as_array)
        .ok_or("model.materials must be an array.")?;
    let physical_groups = map
        .get("physicalGroups")
        .and_then(Value::as_array)
        .ok_or("mesh map.physicalGroups must be an array.")?;
    let mut result = HashMap::new();
    for (index, material) in material_rows.iter().enumerate() {
        let selection_id = material
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("model.materials[{index}].selectionId is required."))?;
        let group = physical_groups
            .iter()
            .find(|group| group.get("selectionId").and_then(Value::as_str) == Some(selection_id))
            .ok_or_else(|| {
                format!(
                    "Material selection {selection_id:?} is missing from mesh map.physicalGroups."
                )
            })?;
        let tag = group
            .get("physicalTag")
            .and_then(Value::as_u64)
            .filter(|tag| *tag > 0)
            .ok_or_else(|| {
                format!("Material selection {selection_id:?} has no positive physical tag.")
            })? as usize;
        if result.insert(tag, material.clone()).is_some() {
            return Err(format!(
                "More than one material assignment maps to physical tag {tag}."
            ));
        }
    }
    Ok(result)
}

pub(super) fn values_for_assignment(
    profile_id: &str,
    material: &Value,
    base_symbols: &HashMap<String, Scalar>,
    frequency_hz: f64,
    point: [f64; 3],
    path: &str,
) -> Result<HashMap<String, Scalar>, String> {
    CompiledMaterialEvaluator::compile(profile_id, material, path)?.evaluate(
        base_symbols,
        frequency_hz,
        point,
        path,
    )
}

pub(super) fn fields_by_cell<const N: usize>(
    model: &Value,
    map: &Value,
    frequency_hz: f64,
    nodes: &[[f64; 3]],
    cells: &[([usize; N], usize)],
    coordinate_order: [usize; 3],
    coordinate_origin: [f64; 3],
) -> Result<Map<String, Value>, String> {
    let profile_id = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let assignments = assignments_by_physical_tag(model, map)?;
    let parameters = model
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let base_symbols = model_expression::resolve_parameters(parameters, model)?;
    fields_by_assignment_cells(
        profile_id,
        &assignments
            .iter()
            .map(|(tag, material)| (*tag, material.clone()))
            .collect::<HashMap<_, _>>(),
        &base_symbols,
        frequency_hz,
        nodes,
        cells,
        coordinate_order,
        coordinate_origin,
    )
}

pub(super) fn fields_by_assignment_cells<const N: usize>(
    profile_id: &str,
    assignments: &HashMap<usize, Value>,
    base_symbols: &HashMap<String, Scalar>,
    frequency_hz: f64,
    nodes: &[[f64; 3]],
    cells: &[([usize; N], usize)],
    coordinate_order: [usize; 3],
    coordinate_origin: [f64; 3],
) -> Result<Map<String, Value>, String> {
    let properties = crate::model_schema::material_properties_for_profile(profile_id)?;
    let mut output = Map::new();
    for property in properties {
        if let Some(symbol) = property.get("symbol").and_then(Value::as_str) {
            output.insert(symbol.to_string(), json!({"real":Vec::<Vec<f64>>::with_capacity(cells.len()),"imag":Vec::<Vec<f64>>::with_capacity(cells.len())}));
        }
    }
    let compiled_assignments = assignments
        .iter()
        .map(|(tag, material)| {
            let path = format!(
                "material[{}]",
                material
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("assignment")
            );
            CompiledMaterialEvaluator::compile(profile_id, material, &path)
                .map(|compiled| (*tag, compiled))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    for (cell_index, (cell_nodes, tag)) in cells.iter().enumerate() {
        let material = assignments.get(tag).ok_or_else(|| format!("Mesh cell {cell_index} uses physical material tag {tag}, which has no material assignment."))?;
        let compiled = compiled_assignments.get(tag).ok_or_else(|| format!("Mesh cell {cell_index} uses physical material tag {tag}, which has no compiled material assignment."))?;
        let mut per_symbol = HashMap::<String, (Vec<f64>, Vec<f64>)>::new();
        for &node in cell_nodes {
            let point = *nodes
                .get(node)
                .ok_or_else(|| format!("Mesh cell {cell_index} references missing node {node}."))?;
            let model_point =
                std::array::from_fn(|axis| point[coordinate_order[axis]] + coordinate_origin[axis]);
            let values = compiled.evaluate(
                &base_symbols,
                frequency_hz,
                model_point,
                &format!(
                    "material[{}]",
                    material
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("assignment")
                ),
            )?;
            for (symbol, value) in values {
                let arrays = per_symbol.entry(symbol).or_default();
                arrays.0.push(value.value);
                arrays.1.push(0.0);
            }
        }
        for (symbol, (real, imag)) in per_symbol {
            if let Some(entry) = output.get_mut(&symbol).and_then(Value::as_object_mut) {
                entry
                    .get_mut("real")
                    .and_then(Value::as_array_mut)
                    .ok_or("Internal material field output is invalid.")?
                    .push(json!(real));
                entry
                    .get_mut("imag")
                    .and_then(Value::as_array_mut)
                    .ok_or("Internal material field output is invalid.")?
                    .push(json!(imag));
            }
        }
    }
    Ok(output)
}
