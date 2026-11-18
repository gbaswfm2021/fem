//! Scenario-neutral bindings from evaluated dimensioned material symbols to
//! weak-form inputs. Expressions use the existing FullMeta quantity runtime;
//! this layer does not evaluate untrusted native code or select a scenario.
//!
//! Binding arrays are ordered: an expression or alias may refer to material
//! properties, caller symbols, or an earlier binding. Missing later/cyclic
//! references are rejected by the dimensioned expression evaluator.

use crate::model_expression::{self, CompiledExpression, Scalar};
use fullmeta_core::quantity::Dimension;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

enum Source {
    Alias(String),
    Expression(CompiledExpression),
    Constant(f64),
}

struct Bounds {
    minimum: Option<(f64, bool)>,
    maximum: Option<(f64, bool)>,
    nonzero: bool,
    allowed: Option<Vec<f64>>,
    allowed_tolerance: f64,
}

struct Binding {
    target: String,
    unit: String,
    scale: f64,
    dimension: Dimension,
    source: Source,
    optional: bool,
    default: Option<f64>,
    bounds: Bounds,
}

pub(crate) struct MaterialBindings {
    bindings: Vec<Binding>,
    uses_position: bool,
}

impl MaterialBindings {
    /// Each array entry declares a unique `target`, `unit`, and exactly one
    /// `source`, `expression`, or numeric `constant`. Alias defaults and
    /// numeric constants are expressed in the declared unit. Bounds and the
    /// optional implemented-value tolerance are in that same unit.
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let rows = value
            .as_array()
            .filter(|rows| !rows.is_empty())
            .ok_or("Material bindings must be a nonempty array.")?;
        let mut bindings = Vec::with_capacity(rows.len());
        let mut targets = HashSet::new();
        let mut uses_position = false;
        for (index, row) in rows.iter().enumerate() {
            let path = format!("materialBindings[{index}]");
            let text = |key: &str| -> Result<&str, String> {
                row.get(key)
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && s.len() <= 128)
                    .ok_or_else(|| {
                        format!("{path}.{key} must be a nonempty string of at most 128 characters.")
                    })
            };
            let target = text("target")?.to_string();
            if !targets.insert(target.clone()) {
                return Err(format!("{path}.target duplicates {target:?}."));
            }
            let unit = text("unit")?.to_string();
            let (scale, dimension) =
                model_expression::unit(&unit).map_err(|error| format!("{path}.unit: {error}"))?;
            if !scale.is_finite() || scale <= 0.0 {
                return Err(format!("{path}.unit has an invalid scale."));
            }
            let source_count = ["source", "expression", "constant"]
                .iter()
                .filter(|key| row.get(**key).is_some())
                .count();
            if source_count != 1 {
                return Err(format!(
                    "{path} needs exactly one of source, expression, or constant."
                ));
            }
            let source = if row.get("source").is_some() {
                let name = text("source")?;
                uses_position |= ["x", "y", "z"].contains(&name);
                Source::Alias(name.to_string())
            } else if row.get("expression").is_some() {
                let source = row
                    .get("expression")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && s.len() <= 512)
                    .ok_or_else(|| {
                        format!(
                            "{path}.expression must be a nonempty string of at most 512 characters."
                        )
                    })?;
                let compiled = CompiledExpression::compile(source)
                    .map_err(|error| format!("{path}.expression: {error}"))?;
                uses_position |= ["x", "y", "z"]
                    .iter()
                    .any(|symbol| compiled.uses_symbol(symbol));
                Source::Expression(compiled)
            } else {
                Source::Constant(number(row.get("constant"), &format!("{path}.constant"))? * scale)
            };
            let optional = match row.get("optional") {
                None => false,
                Some(v) => v
                    .as_bool()
                    .ok_or_else(|| format!("{path}.optional must be a boolean."))?,
            };
            let default = row
                .get("default")
                .map(|value| number(Some(value), &format!("{path}.default")).map(|v| v * scale))
                .transpose()?;
            if matches!(&source, Source::Constant(value) if !value.is_finite())
                || default.is_some_and(|value| !value.is_finite())
            {
                return Err(format!(
                    "{path}: constant or default overflows in SI units."
                ));
            }
            if default.is_some() && !matches!(source, Source::Alias(_)) {
                return Err(format!(
                    "{path}.default is only supported for an alias source."
                ));
            }
            let bounds = row.get("bounds").unwrap_or(&Value::Null);
            if !bounds.is_null() && !bounds.is_object() {
                return Err(format!("{path}.bounds must be an object."));
            }
            let min = read_bound(bounds, "minimum", "exclusiveMinimum", scale, &path)?;
            let max = read_bound(bounds, "maximum", "exclusiveMaximum", scale, &path)?;
            if min.is_some_and(|(value, _)| !value.is_finite())
                || max.is_some_and(|(value, _)| !value.is_finite())
            {
                return Err(format!("{path}.bounds: value overflows in SI units."));
            }
            if let (Some((lo, _)), Some((hi, _))) = (min, max) {
                if lo > hi {
                    return Err(format!("{path}.bounds minimum exceeds maximum."));
                }
            }
            let nonzero = match row.get("nonzero") {
                None => false,
                Some(v) => v
                    .as_bool()
                    .ok_or_else(|| format!("{path}.nonzero must be a boolean."))?,
            };
            let allowed = row
                .get("implementedValues")
                .map(|values| {
                    let values = values.as_array().filter(|v| !v.is_empty()).ok_or_else(|| {
                        format!("{path}.implementedValues must be a nonempty array.")
                    })?;
                    values
                        .iter()
                        .map(|value| {
                            number(Some(value), &format!("{path}.implementedValues"))
                                .map(|v| v * scale)
                        })
                        .collect::<Result<Vec<_>, String>>()
                })
                .transpose()?;
            if allowed
                .as_ref()
                .is_some_and(|values| values.iter().any(|value| !value.is_finite()))
            {
                return Err(format!(
                    "{path}.implementedValues: value overflows in SI units."
                ));
            }
            let tolerance = row
                .get("implementedTolerance")
                .map(|v| number(Some(v), &format!("{path}.implementedTolerance")))
                .transpose()?
                .unwrap_or(0.0)
                * scale;
            if tolerance < 0.0 || !tolerance.is_finite() {
                return Err(format!(
                    "{path}.implementedTolerance must be non-negative and finite."
                ));
            }
            bindings.push(Binding {
                target,
                unit,
                scale,
                dimension,
                source,
                optional,
                default,
                bounds: Bounds {
                    minimum: min,
                    maximum: max,
                    nonzero,
                    allowed,
                    allowed_tolerance: tolerance,
                },
            });
        }
        Ok(Self {
            bindings,
            uses_position,
        })
    }

    /// True if a declared derived expression reads a coordinate directly.
    /// The caller must separately check dependencies of raw material inputs.
    pub(crate) fn uses_position(&self) -> bool {
        self.uses_position
    }

    /// Resolve aliases and derived real symbols, checking each declared unit and
    /// constraint. Material symbols override caller symbols of the same name;
    /// earlier bindings then override both for subsequent expressions.
    pub(crate) fn evaluate(
        &self,
        material: &HashMap<String, Scalar>,
        context: &HashMap<String, Scalar>,
        path: &str,
    ) -> Result<HashMap<String, Scalar>, String> {
        let mut available = context.clone();
        available.extend(material.iter().map(|(name, value)| (name.clone(), *value)));
        let mut output = HashMap::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            let label = format!("{path}.{}", binding.target);
            let value = match &binding.source {
                Source::Alias(name) => match available.get(name).copied() {
                    Some(value) => value,
                    None if binding.default.is_some() => Scalar {
                        value: binding.default.unwrap(),
                        dimension: binding.dimension,
                    },
                    None if binding.optional => continue,
                    None => return Err(format!("{label}: missing material input {name:?}.")),
                },
                Source::Expression(expression) => expression.evaluate(&available, &label)?,
                Source::Constant(value) => Scalar {
                    value: *value,
                    dimension: binding.dimension,
                },
            };
            if !value.value.is_finite() || value.dimension.iter().any(|v| !v.is_finite()) {
                return Err(format!("{label}: material value must be finite."));
            }
            if value
                .dimension
                .iter()
                .zip(binding.dimension)
                .any(|(actual, expected)| (actual - expected).abs() > 1e-10)
            {
                return Err(format!(
                    "{label}: material value must have units compatible with {}.",
                    binding.unit
                ));
            }
            let invalid_minimum = binding.bounds.minimum.is_some_and(|(v, exclusive)| {
                if exclusive {
                    value.value <= v
                } else {
                    value.value < v
                }
            });
            let invalid_maximum = binding.bounds.maximum.is_some_and(|(v, exclusive)| {
                if exclusive {
                    value.value >= v
                } else {
                    value.value > v
                }
            });
            if invalid_minimum || invalid_maximum || binding.bounds.nonzero && value.value == 0.0 {
                return Err(format!(
                    "{label}: value {} {} violates its declared bounds.",
                    value.value / binding.scale,
                    binding.unit
                ));
            }
            if binding.bounds.allowed.as_ref().is_some_and(|values| {
                !values.iter().any(|allowed| {
                    (value.value - allowed).abs() <= binding.bounds.allowed_tolerance
                })
            }) {
                return Err(format!(
                    "{label}: value {} {} is not implemented by this material binding.",
                    value.value / binding.scale,
                    binding.unit
                ));
            }
            available.insert(binding.target.clone(), value);
            output.insert(binding.target.clone(), value);
        }
        Ok(output)
    }

    pub(crate) fn evaluate_batch(
        &self,
        materials: &[HashMap<String, Scalar>],
        contexts: &[HashMap<String, Scalar>],
        path: &str,
    ) -> Result<Vec<HashMap<String, Scalar>>, String> {
        if materials.len() != contexts.len() {
            return Err(format!(
                "{path}: material and context batch lengths differ."
            ));
        }
        materials
            .iter()
            .zip(contexts)
            .enumerate()
            .map(|(index, (material, context))| {
                self.evaluate(material, context, &format!("{path}[{index}]"))
            })
            .collect()
    }
}

fn number(value: Option<&Value>, path: &str) -> Result<f64, String> {
    value
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{path} must be a finite number."))
}

fn read_bound(
    bounds: &Value,
    inclusive: &str,
    exclusive: &str,
    scale: f64,
    path: &str,
) -> Result<Option<(f64, bool)>, String> {
    match (bounds.get(inclusive), bounds.get(exclusive)) {
        (Some(_), Some(_)) => Err(format!(
            "{path}.bounds cannot declare both {inclusive} and {exclusive}."
        )),
        (Some(value), None) => Ok(Some((
            number(Some(value), &format!("{path}.bounds.{inclusive}"))? * scale,
            false,
        ))),
        (None, Some(value)) => Ok(Some((
            number(Some(value), &format!("{path}.bounds.{exclusive}"))? * scale,
            true,
        ))),
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn quantity(value: f64, unit: &str) -> Scalar {
        let (scale, dimension) = model_expression::unit(unit).unwrap();
        Scalar {
            value: value * scale,
            dimension,
        }
    }

    #[test]
    fn aliases_defaults_and_dimensioned_derived_expressions_follow_json_order() {
        let plan = MaterialBindings::from_json(&json!([
            {"target":"density","source":"rho","unit":"kg/m^3","bounds":{"exclusiveMinimum":0}},
            {"target":"speed","source":"c","unit":"m/s","nonzero":true},
            {"target":"loss","source":"eta","unit":"1","default":0,"implementedValues":[0],"implementedTolerance":1e-14},
            {"target":"mu","expression":"young/(2*(1+nu))","unit":"Pa"}
        ])).unwrap();
        let material = HashMap::from([
            ("rho".into(), quantity(2700., "kg/m^3")),
            ("c".into(), quantity(343., "m/s")),
            ("young".into(), quantity(12., "Pa")),
            ("nu".into(), quantity(0.5, "1")),
        ]);
        let values = plan
            .evaluate(&material, &HashMap::new(), "material")
            .unwrap();
        assert_eq!(values["density"].value, 2700.);
        assert_eq!(values["speed"].value, 343.);
        assert_eq!(values["loss"].value, 0.);
        assert!((values["mu"].value - 4.).abs() < 1e-12);
        assert!(!plan.uses_position());
    }

    #[test]
    fn constraints_dimension_mismatch_and_optional_inputs_are_checked() {
        let plan = MaterialBindings::from_json(&json!([
            {"target":"rho","source":"rho","unit":"kg/m^3","bounds":{"exclusiveMinimum":0}},
            {"target":"nu","source":"nu","unit":"1","bounds":{"exclusiveMinimum":-1,"exclusiveMaximum":0.5}},
            {"target":"eta","source":"eta","unit":"1","optional":true}
        ])).unwrap();
        let mut material = HashMap::from([
            ("rho".into(), quantity(1., "kg/m^3")),
            ("nu".into(), quantity(0.3, "1")),
        ]);
        assert!(
            !plan
                .evaluate(&material, &HashMap::new(), "material")
                .unwrap()
                .contains_key("eta")
        );
        material.insert("rho".into(), quantity(1., "Pa"));
        assert!(
            plan.evaluate(&material, &HashMap::new(), "material")
                .unwrap_err()
                .contains("units")
        );
        material.insert("rho".into(), quantity(0., "kg/m^3"));
        assert!(
            plan.evaluate(&material, &HashMap::new(), "material")
                .unwrap_err()
                .contains("bounds")
        );
        material.insert("rho".into(), quantity(1., "kg/m^3"));
        material.insert("nu".into(), quantity(0.5, "1"));
        assert!(
            plan.evaluate(&material, &HashMap::new(), "material")
                .unwrap_err()
                .contains("bounds")
        );
    }

    #[test]
    fn reports_dependency_and_invalid_declarations() {
        assert!(
            MaterialBindings::from_json(&json!([
            {"target":"x","source":"a","unit":"1"},{"target":"x","source":"b","unit":"1"}]))
            .is_err()
        );
        assert!(
            MaterialBindings::from_json(&json!([
            {"target":"x","source":"a","unit":"1","bounds":{"minimum":1,"maximum":0}}]))
            .is_err()
        );
        let plan = MaterialBindings::from_json(&json!([
            {"target":"missing","source":"none","unit":"1"}]))
        .unwrap();
        assert!(
            plan.evaluate(&HashMap::new(), &HashMap::new(), "material")
                .unwrap_err()
                .contains("missing material input")
        );
    }

    #[test]
    fn coordinate_aliases_report_position_use_and_evaluate_per_batch_row() {
        let plan = MaterialBindings::from_json(&json!([
            {"target":"location","source":"x","unit":"m"}
        ])).unwrap();
        assert!(plan.uses_position());
        let material = [HashMap::new(), HashMap::new()];
        let contexts = [HashMap::from([("x".into(), quantity(1., "m"))]),
                        HashMap::from([("x".into(), quantity(2., "m"))])];
        let values = plan.evaluate_batch(&material, &contexts, "material").unwrap();
        assert_eq!(values[0]["location"].value, 1.);
        assert_eq!(values[1]["location"].value, 2.);
        assert!(plan.evaluate_batch(&material[..1], &contexts, "material").is_err());
    }

    
}
