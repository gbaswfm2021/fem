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
    uses_time: bool,
}

struct CompiledMaterialAssignment {
    id: String,
    evaluator: CompiledMaterialEvaluator,
}

/// Select only properties applicable to the requested Study kind. An absent
/// `studyTypes` declaration keeps the legacy all-Study behavior.
fn material_definitions_for_study(definitions: &[Value], study_type: &str) -> Vec<Value> {
    definitions
        .iter()
        .filter(|definition| {
            definition
                .get("studyTypes")
                .and_then(Value::as_array)
                .is_none_or(|types| types.iter().any(|kind| kind.as_str() == Some(study_type)))
        })
        .cloned()
        .collect()
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
        Self::compile_for_study(profile_id, assignments, "frequencyDomain")
    }

    pub(super) fn compile_for_study(
        profile_id: &str,
        assignments: &HashMap<usize, Value>,
        study_type: &str,
    ) -> Result<Self, String> {
        let definitions = material_definitions_for_study(
            crate::model_schema::material_properties_for_profile(profile_id)?,
            study_type,
        );
        Self::compile_with_definitions(profile_id, assignments, &definitions)
    }

    /// Compile a study-specific material contract declared by a scenario.
    /// Existing harmonic and stationary callers continue using the profile
    /// catalogue through `compile`.
    pub(super) fn compile_with_definitions(
        profile_id: &str,
        assignments: &HashMap<usize, Value>,
        definitions: &[Value],
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
            let evaluator = CompiledMaterialEvaluator::compile_with_definitions(
                profile_id,
                material,
                &format!("material {id}"),
                definitions,
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

    /// Evaluate a material assignment at the current transient quadrature point.
    pub(super) fn evaluate_at_time(
        &self,
        tag: usize,
        base_symbols: &HashMap<String, Scalar>,
        time: f64,
        point: [f64; 3],
        context: &str,
    ) -> Result<HashMap<String, Scalar>, String> {
        let assignment = self.by_tag.get(&tag).ok_or_else(|| {
            format!("{context} uses physical material tag {tag}, which has no material assignment.")
        })?;
        assignment.evaluator.evaluate_at_time(
            base_symbols,
            time,
            point,
            &format!("material {} in {context}", assignment.id),
        )
    }

    /// True only when no compiled assigned property directly references time.
    /// Parameter and constitutive dependencies must also be checked by callers
    /// before caching any numerical operator or factorization.
    pub(super) fn has_explicit_time_dependence(&self) -> bool {
        self.by_tag
            .values()
            .any(|assignment| assignment.evaluator.uses_time())
    }

    pub(super) fn evaluate_batch(
        &self,
        tag: usize,
        base_symbols: &HashMap<String, Scalar>,
        frequency_hz: f64,
        points: &[[f64; 3]],
        context: &str,
    ) -> Result<Vec<HashMap<String, Scalar>>, String> {
        let assignment = self.by_tag.get(&tag).ok_or_else(|| {
            format!("{context} uses physical material tag {tag}, which has no material assignment.")
        })?;
        assignment.evaluator.evaluate_batch(
            base_symbols,
            frequency_hz,
            points,
            &format!("material {} in {context}", assignment.id),
        )
    }
}

impl CompiledMaterialEvaluator {
    pub(super) fn compile(profile_id: &str, material: &Value, path: &str) -> Result<Self, String> {
        let definitions = material_definitions_for_study(
            crate::model_schema::material_properties_for_profile(profile_id)?,
            "frequencyDomain",
        );
        Self::compile_with_definitions(profile_id, material, path, &definitions)
    }

    fn compile_with_definitions(
        profile_id: &str,
        material: &Value,
        path: &str,
        definitions: &[Value],
    ) -> Result<Self, String> {
        if definitions.is_empty() {
            return Err(format!(
                "Scenario profile {profile_id:?} declares no material properties."
            ));
        }
        let mut properties = Vec::with_capacity(definitions.len());
        let mut uses_position = false;
        let mut uses_time = false;
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
            uses_time |= expression.uses_symbol("t");
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
            uses_time,
        })
    }

    pub(super) fn uses_position(&self) -> bool {
        self.uses_position
    }

    /// An operator depending on this material cannot be treated as time-invariant.
    pub(super) fn uses_time(&self) -> bool {
        self.uses_time
    }

    pub(super) fn evaluate_batch(
        &self,
        base_symbols: &HashMap<String, Scalar>,
        frequency_hz: f64,
        points: &[[f64; 3]],
        path: &str,
    ) -> Result<Vec<HashMap<String, Scalar>>, String> {
        if points.is_empty() {
            return Ok(Vec::new());
        }
        let hz_dimension = model_expression::unit("Hz")?.1;
        let mut rows = Vec::with_capacity(points.len());
        for point in points {
            let mut symbols = base_symbols.clone();
            for (axis, value) in ["x", "y", "z"].into_iter().zip(*point) {
                if !value.is_finite() {
                    return Err(format!("{path}: coordinate {axis} is not finite."));
                }
                symbols.insert(
                    axis.into(),
                    Scalar {
                        value,
                        dimension: [1., 0., 0., 0., 0.],
                    },
                );
            }
            for (name, value) in [
                ("freq", frequency_hz),
                ("frequency", frequency_hz),
                ("omega", std::f64::consts::TAU * frequency_hz),
            ] {
                symbols.insert(
                    name.into(),
                    Scalar {
                        value,
                        dimension: hz_dimension,
                    },
                );
            }
            rows.push(symbols);
        }
        let mut output = vec![HashMap::with_capacity(self.properties.len()); points.len()];
        for property in &self.properties {
            let property_path = format!("{path}.{}", property.symbol);
            let values = match property.expression.evaluate_batch(&rows, &property_path) {
                Ok(values) => values,
                Err(error) => {
                    for point in points {
                        if let Err(first) = self.evaluate(base_symbols, frequency_hz, *point, path)
                        {
                            return Err(first);
                        }
                    }
                    return Err(error);
                }
            };
            for (index, value) in values.into_iter().enumerate() {
                let invalid_min = property.minimum.is_some_and(|(minimum, exclusive)| {
                    if exclusive {
                        value <= minimum
                    } else {
                        value < minimum
                    }
                });
                let invalid_set = property
                    .implemented_values
                    .as_ref()
                    .is_some_and(|set| !set.contains(&value));
                if invalid_min || invalid_set {
                    for point in points {
                        if let Err(first) = self.evaluate(base_symbols, frequency_hz, *point, path)
                        {
                            return Err(first);
                        }
                    }
                    return Err(format!(
                        "{property_path}: invalid material value at batch row {index}."
                    ));
                }
                let scalar = Scalar {
                    value,
                    dimension: property.dimension,
                };
                output[index].insert(property.symbol.clone(), scalar);
                rows[index].insert(property.symbol.clone(), scalar);
            }
        }
        Ok(output)
    }

    pub(super) fn evaluate(
        &self,
        base_symbols: &HashMap<String, Scalar>,
        frequency_hz: f64,
        point: [f64; 3],
        path: &str,
    ) -> Result<HashMap<String, Scalar>, String> {
        self.evaluate_in_context(base_symbols, Some(frequency_hz), None, point, path)
    }

    /// Evaluate prescribed transient material expressions without introducing
    /// harmonic frequency symbols into the transient context.
    pub(super) fn evaluate_at_time(
        &self,
        base_symbols: &HashMap<String, Scalar>,
        time: f64,
        point: [f64; 3],
        path: &str,
    ) -> Result<HashMap<String, Scalar>, String> {
        if !time.is_finite() {
            return Err(format!("{path}: time must be finite."));
        }
        if base_symbols.contains_key("t") {
            return Err(format!(
                "{path}: parameter t conflicts with transient time."
            ));
        }
        if self
            .properties
            .iter()
            .any(|property| property.symbol == "t")
        {
            return Err(format!(
                "{path}: material symbol t conflicts with transient time."
            ));
        }
        if self.properties.iter().any(|property| {
            ["freq", "frequency", "omega"]
                .iter()
                .any(|name| property.expression.uses_symbol(name))
        }) {
            return Err(format!(
                "{path}: frequency-dependent material needs a declared causal time-domain model."
            ));
        }
        self.evaluate_in_context(base_symbols, None, Some(time), point, path)
    }

    fn evaluate_in_context(
        &self,
        base_symbols: &HashMap<String, Scalar>,
        frequency_hz: Option<f64>,
        time: Option<f64>,
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
        if let Some(frequency_hz) = frequency_hz {
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
        }
        if let Some(time) = time {
            symbols.insert(
                "t".into(),
                Scalar {
                    value: time,
                    dimension: model_expression::unit("s")?.1,
                },
            );
        }

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

#[cfg(test)]
mod batch_tests {
    use super::*;
    #[test]
    fn legacy_material_evaluator_skips_transient_only_capacity() {
        let materials = HashMap::from([(
            7,
            json!({"id":"thermal","volumetricHeatCapacity":{"expression":"C0*(1+t/tau)","unit":"W*s/(m^3*K)"}}),
        )]);
        let compiled = CompiledMaterialAssignments::compile("thermal-2d", &materials).unwrap();
        let values = compiled
            .evaluate(7, &HashMap::new(), 1., [0.; 3], "cell")
            .unwrap();
        assert!(values.contains_key("k"));
        assert!(!values.contains_key("Cvol"));
    }
    #[test]
    fn material_can_be_sampled_at_position_and_dimensioned_time() {
        let property = CompiledProperty {
            symbol: "a".into(),
            expression: model_expression::CompiledQuantity::compile(
                &json!({"expression":"x*(1+t/tau)","unit":"m"}),
                "material",
            )
            .unwrap(),
            dimension: model_expression::unit("m").unwrap().1,
            minimum: Some((0., false)),
            implemented_values: None,
            unit: "m".into(),
        };
        let evaluator = CompiledMaterialEvaluator {
            properties: vec![property],
            uses_position: true,
            uses_time: true,
        };
        assert!(evaluator.uses_time());
        let base = HashMap::from([(
            "tau".into(),
            Scalar {
                value: 2.,
                dimension: model_expression::unit("s").unwrap().1,
            },
        )]);
        let values = evaluator
            .evaluate_at_time(&base, 3., [4., 0., 0.], "material")
            .unwrap();
        assert!((values["a"].value - 10.).abs() < 1e-12);
        assert!(
            evaluator
                .evaluate_at_time(&base, f64::NAN, [4., 0., 0.], "material")
                .is_err()
        );
        let dispersive = CompiledMaterialEvaluator {
            properties: vec![CompiledProperty {
                symbol: "eps".into(),
                expression: model_expression::CompiledQuantity::compile(
                    &json!({"expression":"frequency/f0","unit":"1"}),
                    "material",
                )
                .unwrap(),
                dimension: model_expression::unit("1").unwrap().1,
                minimum: None,
                implemented_values: None,
                unit: "1".into(),
            }],
            uses_position: false,
            uses_time: false,
        };
        assert!(
            dispersive
                .evaluate_at_time(&base, 1., [0.; 3], "material")
                .unwrap_err()
                .contains("causal")
        );
    }
    #[test]
    fn spatial_quantity_batches_match_scalar_dependencies_and_errors() {
        let dimension = [1., 0., 0., 0., 0.];
        let make =
            |symbol: &str, expression: &str, minimum: Option<(f64, bool)>| CompiledProperty {
                symbol: symbol.into(),
                expression: model_expression::CompiledQuantity::compile(
                    &json!({"expression":expression,"unit":"m"}),
                    "material",
                )
                .unwrap(),
                dimension,
                minimum,
                implemented_values: None,
                unit: "m".into(),
            };
        let evaluator = CompiledMaterialEvaluator {
            properties: vec![make("a", "2*x", Some((0., false))), make("b", "a+x", None)],
            uses_position: true,
            uses_time: false,
        };
        let points = [[0.5, 0., 0.], [1., 0., 0.], [1.5, 0., 0.]];
        let base = HashMap::new();
        let batch = evaluator
            .evaluate_batch(&base, 1., &points, "material")
            .unwrap();
        for (i, point) in points.iter().enumerate() {
            let scalar = evaluator.evaluate(&base, 1., *point, "material").unwrap();
            assert_eq!(batch[i], scalar);
        }
        assert_eq!((batch[0]["a"].value, batch[0]["b"].value), (1., 1.5));
        let invalid = [[0.5, 0., 0.], [-1., 0., 0.]];
        let expected = evaluator
            .evaluate(&base, 1., invalid[1], "material")
            .unwrap_err();
        assert_eq!(
            evaluator
                .evaluate_batch(&base, 1., &invalid, "material")
                .unwrap_err(),
            expected
        );
    }
}
