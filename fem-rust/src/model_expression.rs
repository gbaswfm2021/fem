//! FEM adapters for schema-bound generic expression runtimes.
//! Parsing, units, dependency resolution and numerical expression algorithms
//! live in the application-neutral model/kernel libraries.
use fullmeta_core::pointwise::ComplexValue;
use fullmeta_core::quantity::Dimension;
pub use fullmeta_core::quantity::QuantityValue as Scalar;
use fullmeta_model::runtime_expressions as runtime;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

pub type Result<T> = std::result::Result<T, String>;

fn bind_policy(kind: &str, model: &Value, context: &Value) -> Result<Value> {
    let root = crate::model_schema::root()?;
    let declaration = root
        .get("x-fullmeta-expressionRuntime")
        .and_then(|rules| rules.get(kind))
        .ok_or_else(|| {
            format!("The model schema must declare x-fullmeta-expressionRuntime.{kind}.")
        })?;
    runtime::bind_policy(model, root, context, declaration)
}

fn runtime_policy(kind: &str) -> Result<&'static Value> {
    static REAL: OnceLock<Result<Value>> = OnceLock::new();
    static COMPLEX: OnceLock<Result<Value>> = OnceLock::new();
    let slot = match kind {
        "real" => &REAL,
        "complex" => &COMPLEX,
        _ => return Err(format!("Unknown compiled expression policy {kind:?}.")),
    };
    match slot.get_or_init(|| {
        let capabilities = crate::model_schema::engine_capabilities()?;
        bind_policy(kind, &Value::Null, &json!({"capabilities":capabilities}))
    }) {
        Ok(value) => Ok(value),
        Err(error) => Err(error.clone()),
    }
}

pub fn unit(name: &str) -> Result<(f64, Dimension)> {
    let unit = fullmeta_core::quantity::get_unit(name)?;
    Ok((unit.scale, unit.dimension))
}

pub fn inspect(source: &str) -> Result<(Value, BTreeSet<String>, BTreeSet<String>)> {
    let inspected = fullmeta_core::expression::inspect_expression(source)?;
    Ok((
        serde_json::to_value(inspected.tree).map_err(|error| error.to_string())?,
        inspected.symbols.into_iter().collect(),
        inspected.calls.into_iter().collect(),
    ))
}

#[derive(Clone, Debug)]
pub struct CompiledExpression {
    compiled: runtime::CompiledExpression,
}

impl CompiledExpression {
    pub fn compile(source: &str) -> Result<Self> {
        Ok(Self {
            compiled: runtime::CompiledExpression::compile(
                source,
                runtime_policy("real")?,
                "expression",
            )?,
        })
    }
    pub fn uses_symbol(&self, name: &str) -> bool {
        self.compiled.uses_symbol(name)
    }
    pub fn evaluate(&self, symbols: &HashMap<String, Scalar>, path: &str) -> Result<Scalar> {
        self.compiled
            .evaluate_with(|name| symbols.get(name).copied(), path)
    }
}

#[derive(Clone, Debug)]
pub struct CompiledQuantity {
    compiled: runtime::CompiledQuantity,
}

impl CompiledQuantity {
    pub fn compile(quantity: &Value, path: &str) -> Result<Self> {
        Ok(Self {
            compiled: runtime::CompiledQuantity::compile(quantity, runtime_policy("real")?, path)?,
        })
    }
    pub fn uses_symbol(&self, name: &str) -> bool {
        self.compiled.uses_symbol(name)
    }
    pub fn evaluate(&self, symbols: &HashMap<String, Scalar>, path: &str) -> Result<f64> {
        self.compiled
            .evaluate_with(|name| symbols.get(name).copied(), path)
    }
}

pub fn evaluate_expression(
    source: &str,
    symbols: &HashMap<String, Scalar>,
    path: &str,
) -> Result<Scalar> {
    CompiledExpression::compile(source)?.evaluate(symbols, path)
}

pub fn resolve_parameters(parameters: &[Value], model: &Value) -> Result<HashMap<String, Scalar>> {
    let reserved = crate::model_schema::reserved_symbols_for_model(model)?;
    let policy = bind_policy(
        "parameters",
        model,
        &json!({"reservedSymbols":reserved,"realPolicy":runtime_policy("real")?}),
    )?;
    runtime::resolve_parameters(parameters, &policy)?
        .into_iter()
        .map(|(name, value)| {
            let scalar: Scalar =
                serde_json::from_value(value).map_err(|error| error.to_string())?;
            Ok((name, scalar))
        })
        .collect()
}

pub fn evaluate_quantity(
    quantity: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
) -> Result<f64> {
    CompiledQuantity::compile(quantity, path)?.evaluate(symbols, path)
}

pub fn evaluate_quantity_with_symbols(
    quantity: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
) -> Result<f64> {
    evaluate_quantity(quantity, symbols, path)
}

#[derive(Clone, Debug)]
pub struct CompiledComplexQuantity {
    compiled: fullmeta_model::semantic_quantities::CompiledComplexQuantity,
}

impl CompiledComplexQuantity {
    pub fn compile(quantity: &Value, path: &str) -> Result<Self> {
        Ok(Self {
            compiled: fullmeta_model::semantic_quantities::CompiledComplexQuantity::compile(
                quantity,
                runtime_policy("complex")?,
                path,
            )?,
        })
    }
    pub fn evaluate(&self, symbols: &HashMap<String, Scalar>, path: &str) -> Result<ComplexValue> {
        self.compiled.evaluate_with(
            |name| {
                symbols
                    .get(name)
                    .map(|value| (value.value, value.dimension))
            },
            path,
        )
    }
}

pub fn evaluate_complex_quantity_with_symbols(
    quantity: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
) -> Result<ComplexValue> {
    CompiledComplexQuantity::compile(quantity, path)?.evaluate(symbols, path)
}

/// Combine a dimensionless incident factor with quantities for the active
/// primary-field components. Each component keeps its own physical unit.
pub fn primary_field_units(model: &Value) -> Result<Vec<String>> {
    Ok(
        crate::model_schema::field_variables_for_model(model, false)?
            .into_iter()
            .filter(|field| field.role == "primary")
            .map(|field| field.unit)
            .collect(),
    )
}

pub fn incident_component_quantities(
    boundary: &Value,
    expected_units: &[String],
    path: &str,
) -> Result<Option<Vec<Value>>> {
    let Some(factor) = boundary.get("incidentField") else {
        return Ok(None);
    };
    let factor_expression = factor
        .get("expression")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.incidentField.expression must be a string."))?;
    let factor_unit = factor
        .get("unit")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.incidentField.unit must be a string."))?;
    if factor_unit != "1" {
        return Err(format!("{path}.incidentField must be dimensionless."));
    }
    let components = boundary
        .get("polarization")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!("{path}.polarization must contain the incident field components.")
        })?;
    if components.len() != expected_units.len() {
        return Err(format!(
            "{path}.polarization has {} components; the active primary field needs {}.",
            components.len(),
            expected_units.len()
        ));
    }
    components.iter().enumerate().map(|(index,component)| {
        let component_path=format!("{path}.polarization[{index}]");
        let expression=component.get("expression").and_then(Value::as_str)
            .ok_or_else(||format!("{component_path}.expression must be a string."))?;
        let component_unit=component.get("unit").and_then(Value::as_str)
            .ok_or_else(||format!("{component_path}.unit must be a string."))?;
        if unit(component_unit)?.1!=unit(&expected_units[index])?.1 {
            return Err(format!("{component_path} must use a unit compatible with {}.",expected_units[index]));
        }
        Ok(json!({"expression":format!("({factor_expression})*({expression})"),"unit":component_unit}))
    }).collect::<Result<Vec<_>>>().map(Some)
}

pub fn scalar_incident_quantity(
    boundary: &Value,
    model: &Value,
    path: &str,
) -> Result<Option<Value>> {
    let units = primary_field_units(model)?;
    if units.len() != 1 {
        return Err(format!(
            "{path} requires exactly one active primary-field component."
        ));
    }
    Ok(incident_component_quantities(boundary, &units, path)?
        .and_then(|mut components| components.pop()))
}
