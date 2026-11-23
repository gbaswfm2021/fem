//! Bridge declared parameter-only boundary quantities to an existing
//! dimensioned expression context without selecting a physics profile.
use std::collections::HashMap;

use crate::boundary_form::BoundaryFormPlan;
use crate::boundary_input::{BoundaryInput, BoundaryValues};
use crate::model_expression::Scalar;

/// Compile each selected model row from its formulation/type declaration.
/// The caller supplies already validated selection-to-tag bindings.
pub(crate) fn compile_rows<'a>(
    package: &Value,
    formulation: &str,
    rows: impl IntoIterator<Item = (usize, &'a Value)>,
) -> Result<HashMap<usize, BoundaryInput>, String> {
    let inherited = package.get("formulationInheritance")
        .and_then(|aliases| aliases.get(formulation)).and_then(serde_json::Value::as_str);
    let declarations = package
        .get("boundaryInputs")
        .and_then(|v| v.get(formulation).or_else(|| inherited.and_then(|base| v.get(base))))
        .and_then(Value::as_object)
        .ok_or("Missing declared boundary inputs for formulation.")?;
    let mut result = HashMap::new();
    for (tag, row) in rows {
        let kind = row
            .get("type")
            .and_then(Value::as_str)
            .ok_or("Boundary type is required.")?;
        let contract = declarations
            .get(kind)
            .ok_or_else(|| format!("Boundary type {kind:?} has no input declaration."))?;
        let input = BoundaryInput::compile(contract, row)
            .map_err(|error| format!("Boundary tag {tag}: {error}"))?;
        if result.insert(tag, input).is_some() {
            return Err(format!(
                "More than one boundary condition targets physical tag {tag}."
            ));
        }
    }
    Ok(result)
}
use serde_json::{Map, Value};

/// Validate the declared input quantities with the caller's established
/// expression policy, then evaluate the compiled generic boundary contract.
/// No coordinate sampling is performed by this adapter.
pub(crate) fn evaluate_parameters<F>(
    contract: &Value,
    row: &Value,
    path: &str,
    resolved: &Map<String, Value>,
    forms: &HashMap<String, BoundaryFormPlan>,
    validate: F,
) -> Result<BoundaryValues, String>
where
    F: Fn(&Value, &str, &str) -> Result<f64, String>,
{
    if contract.get("sampling").and_then(Value::as_str) != Some("parameters") {
        return Err("Declared boundary adapter requires parameters sampling.".into());
    }
    let compiled = BoundaryInput::compile(contract, row)?;
    if let Some(spec) = contract.get("prescribed") {
        let scalar = spec
            .get("scalar")
            .ok_or("Boundary scalar declaration is required.")?;
        let pointer = scalar
            .get("path")
            .and_then(Value::as_str)
            .ok_or("Boundary scalar path is required.")?;
        let unit = scalar
            .get("unit")
            .and_then(Value::as_str)
            .ok_or("Boundary scalar unit is required.")?;
        let value = row
            .pointer(pointer)
            .ok_or_else(|| format!("{path}: declared scalar quantity is missing."))?;
        validate(value, unit, &format!("{path}{pointer}"))?;
        if let Some(vector) = spec.get("vector") {
            let pointer = vector
                .get("path")
                .and_then(Value::as_str)
                .ok_or("Boundary vector path is required.")?;
            let unit = vector
                .get("unit")
                .and_then(Value::as_str)
                .ok_or("Boundary vector unit is required.")?;
            if let Some(values) = row.pointer(pointer) {
                let values = values
                    .as_array()
                    .ok_or("Boundary vector must be an array.")?;
                for (index, value) in values.iter().enumerate() {
                    validate(value, unit, &format!("{path}{pointer}[{index}]"))?;
                }
            }
        }
    }
    let symbols = resolved
        .iter()
        .map(|(name, value)| {
            let value: Scalar =
                serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
            Ok((name.clone(), value))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    let form = compiled
        .form_key()
        .map(|key| {
            forms
                .get(key)
                .ok_or_else(|| format!("Declared boundary form {key:?} is missing."))
        })
        .transpose()?;
    compiled.evaluate(&symbols, None, &HashMap::new(), form)
}
