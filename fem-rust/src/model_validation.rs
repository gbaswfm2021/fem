//! FEM metadata adapter for the application-neutral schema rule interpreter.
//!
//! The caller supplies installed schemas, extracted catalogs and external binding
//! data. Model validation policy and semantic check algorithms live in the schema
//! and model library; this module does not implement FEM validation rules.
use crate::model_schema as schema;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn structure_errors(model: &Value) -> Vec<String> {
    match schema::root() {
        Ok(root) => {
            let mut errors = schema::validate(model, root);
            if let Some(physics) = model.pointer("/model/physics").and_then(Value::as_object) {
                for (key, condition) in physics {
                    if let (Some(id), Some(_)) = (
                        condition.get("id").and_then(Value::as_str),
                        condition.get("type").and_then(Value::as_str),
                    ) {
                        if id != key {
                            errors.push(format!(
                                "model.physics.{key}.id must match its property key {key:?}."
                            ));
                        }
                    }
                }
            }
            errors
        }
        Err(error) => vec![error],
    }
}

pub(crate) fn validate(model: &Value) -> Vec<String> {
    let errors = structure_errors(model);
    if errors.is_empty() {
        semantic_errors(model, &json!({}))
    } else {
        errors
    }
}

pub(crate) fn validation_report(model: &Value) -> Value {
    validation_report_with_context(model, &json!({}))
}

/// External selector/installation data is isolated from the schema-derived
/// catalogs so a caller cannot accidentally replace validation source data.
pub(crate) fn validation_report_with_context(model: &Value, external: &Value) -> Value {
    let structure_errors = structure_errors(model);
    let errors = if structure_errors.is_empty() {
        semantic_errors(model, external)
    } else {
        structure_errors.clone()
    };
    json!({
        "schema": "fullmeta-model-validation/1",
        "validationKind": "model",
        "valid": errors.is_empty(),
        "errors": errors,
        "structureErrors": structure_errors,
    })
}

pub(crate) fn declared_profile(model: &Value) -> Result<&str, String> {
    let pointer = schema::root()?
        .pointer("/x-fem-modelValidation/binding/profilePath")
        .and_then(Value::as_str)
        .ok_or("The model schema must declare its profile binding path.")?;
    model
        .pointer(pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("The profile binding at {pointer} must be a string."))
}

fn semantic_errors(model: &Value, external: &Value) -> Vec<String> {
    let result = (|| {
        let context = validation_context(model, external)?;
        let schemas = schema::context()?;
        fullmeta_model::rules::validate(model, schemas.root(), schemas.documents(), &context)
    })();
    match result {
        Ok(errors) => errors,
        Err(error) => vec![error],
    }
}

/// Validate operation arguments after the submitted model has passed its own
/// rule program. The schema owns the request shape and contextual constraints;
/// preparation consumes the resulting arguments without repeating model rules.
pub(crate) fn validate_preparation_request(
    envelope: &Value,
    external: &Value,
) -> Result<(), String> {
    let model = envelope
        .get("model")
        .ok_or("Study preparation input requires model.")?;
    let schemas = schema::context()?;
    let declaration = schemas
        .root()
        .get("x-fullmeta-preparationValidation")
        .ok_or("The model schema must declare its preparation request validation.")?;
    let context = validation_context(model, external)?;
    let errors = fullmeta_model::rules::validate_declaration(
        envelope,
        schemas.root(),
        schemas.documents(),
        &context,
        declaration,
    )?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn normalized_fields(fields: Vec<schema::FieldVariable>) -> Value {
    Value::Array(
        fields
            .into_iter()
            .map(|field| {
                json!({
                    "symbol": field.symbol,
                    "unit": field.unit,
                    "role": field.role,
                    "identicallyZero": field.identically_zero,
                    "solverFamily": field.solver_family,
                    "axis": field.axis,
                })
            })
            .collect(),
    )
}

fn has_role(declaration: &Value, role: &str) -> bool {
    match declaration.get("x-fullmeta-role") {
        Some(Value::String(value)) => value == role,
        Some(Value::Array(values)) => values.iter().any(|value| value.as_str() == Some(role)),
        _ => false,
    }
}

fn geometry_domain_ids<'a>(model: &'a Value, schema: &Value) -> Result<Vec<&'a str>, String> {
    let geometry = schema
        .pointer("/x-fem-modelValidation/geometry")
        .ok_or("The model schema must declare its geometry validation policy.")?;
    let domain_types = geometry
        .get("domainFeatureTypes")
        .and_then(Value::as_array)
        .ok_or("The geometry validation policy must declare domainFeatureTypes.")?;
    let group_types = geometry
        .get("groupFeatureTypes")
        .and_then(Value::as_array)
        .ok_or("The geometry validation policy must declare groupFeatureTypes.")?;
    let mut pending: Vec<&Value> = model
        .pointer("/model/geometry/features")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .rev()
        .collect();
    let mut ids = Vec::new();
    while let Some(feature) = pending.pop() {
        if domain_types.contains(&feature["type"]) {
            if let Some(id) = feature.get("id").and_then(Value::as_str) {
                ids.push(id);
            }
        }
        if group_types.contains(&feature["type"]) {
            pending.extend(
                feature
                    .get("features")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .rev(),
            );
        }
    }
    Ok(ids)
}

fn parameter_dependencies(model: &Value) -> BTreeMap<String, BTreeSet<String>> {
    let mut direct = BTreeMap::new();
    for row in model
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(symbol), Some(expression)) = (row["symbol"].as_str(), row["expression"].as_str())
        else {
            continue;
        };
        // Parameter validation owns malformed expressions and dependency cycles.
        if let Ok(inspected) = fullmeta_core::expression::inspect_expression(expression) {
            direct.insert(symbol, inspected.symbols);
        }
    }
    let mut closure = BTreeMap::new();
    for (&symbol, dependencies) in &direct {
        let mut pending: Vec<&str> = dependencies.iter().map(String::as_str).collect();
        let mut seen = BTreeSet::new();
        while let Some(dependency) = pending.pop() {
            if direct.contains_key(dependency) && seen.insert(dependency) {
                pending.extend(direct[dependency].iter().map(String::as_str));
            }
        }
        closure.insert(
            symbol.to_owned(),
            seen.into_iter().map(str::to_owned).collect(),
        );
    }
    closure
}

fn validation_context(model: &Value, external: &Value) -> Result<Value, String> {
    let schemas = schema::context()?;
    let profile_id = declared_profile(model)?;
    let binding = schemas
        .root()
        .pointer("/x-fem-modelValidation/binding")
        .ok_or("The schema must declare its metadata binding.")?;
    let scenario_annotation = binding
        .get("scenarioAnnotation")
        .and_then(Value::as_str)
        .ok_or("The schema must declare scenarioAnnotation.")?;
    let validation_annotation = binding
        .get("validationAnnotation")
        .and_then(Value::as_str)
        .ok_or("The schema must declare validationAnnotation.")?;
    let dimension_path = binding
        .get("dimensionMetadataPath")
        .and_then(Value::as_str)
        .ok_or("The schema must declare dimensionMetadataPath.")?;
    let profile_path = schemas
        .root()
        .pointer("/x-fem-modelValidation/expressions/scenarioProfilePointer")
        .and_then(Value::as_str)
        .ok_or("The schema must declare its scenario metadata identity pointer.")?;
    // Unknown model identities reach the generic schema rule program with empty
    // active catalogs, rather than failing a separate FEM-side identity check.
    let document = schemas.documents().iter().find(|document| {
        document.pointer(profile_path).and_then(Value::as_str) == Some(profile_id)
    });
    let scenario_document = document.cloned().unwrap_or_else(|| json!({}));
    let scenario = scenario_document
        .get(scenario_annotation)
        .cloned()
        .unwrap_or_else(|| json!({}));
    let validation = scenario_document
        .get(validation_annotation)
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut fields = json!([]);
    let mut all_fields = json!([]);
    let mut fields_by_formulation = Map::new();
    let mut material_properties = json!([]);
    let mut material_symbols = json!([]);
    let mut field_plot_symbols = json!([]);
    let mut reserved_symbols = json!([]);
    let mut sweep_result_quantities = json!([]);
    if document.is_some() {
        fields = normalized_fields(schema::field_variables_for_model(model, false)?);
        all_fields = normalized_fields(schema::field_variables_for_model(model, true)?);
        for formulation in scenario
            .pointer("/profile/formulations")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            fields_by_formulation.insert(
                formulation.to_string(),
                normalized_fields(schema::field_variables_for_profile(
                    profile_id,
                    Some(formulation),
                    false,
                )?),
            );
        }
        material_properties = json!(schema::material_properties_for_profile(profile_id)?);
        material_symbols = json!(schema::material_symbols_for_model(model)?);
        field_plot_symbols = json!(schema::field_plot_symbols_for_model(model)?);
        reserved_symbols = json!(schema::reserved_symbols_for_model(model)?);
        sweep_result_quantities = json!(schema::sweep_result_quantities_for_profile(profile_id)?);
    }
    let mut integration_contracts = Vec::new();
    for declaration in schemas.schema_annotations("x-fullmeta-role")? {
        if has_role(&declaration, "integration.selection") {
            for contract in declaration
                .pointer("/x-fullmeta-modelReference/targetSchema/oneOf")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if !integration_contracts.contains(contract) {
                    integration_contracts.push(contract.clone());
                }
            }
        }
    }
    let mut geometry_dimensions = Map::new();
    for annotation in [
        "x-fullmeta-geometryDimensions",
        "x-fullmeta-geometryDimension",
    ] {
        for declaration in schemas.schema_annotations(annotation)? {
            if let Some(kind) = declaration
                .pointer("/properties/type/const")
                .and_then(Value::as_str)
            {
                if let Some(dimensions) = schema::geometry_feature_dimensions(kind) {
                    geometry_dimensions.insert(kind.to_string(), json!(dimensions));
                }
            }
        }
    }
    Ok(json!({
        "capabilities": schema::engine_capabilities()?,
        "scenario": scenario,
        "scenarioDocument": scenario_document,
        "validation": validation,
        "profileId": profile_id,
        "dimension": scenario.pointer(dimension_path).cloned().unwrap_or(Value::Null),
        "fields": fields,
        "allFields": all_fields,
        "fieldsByFormulation": fields_by_formulation,
        "materialProperties": material_properties,
        "materialSymbols": material_symbols,
        "fieldPlotSymbols": field_plot_symbols,
        "reservedSymbols": reserved_symbols,
        "sweepResultQuantities": sweep_result_quantities,
        "integrationContracts": integration_contracts,
        "geometryDimensions": geometry_dimensions,
        "domainFeatureIds": geometry_domain_ids(model, schemas.root())?,
        "parameterDependencies": parameter_dependencies(model),
        "external": external,
    }))
}

/// Prepared native values use the same authored bound declaration as model
/// quantities and the same application-neutral numerical validator.
pub(crate) fn validate_prepared_frequency(value: f64, path: &str) -> Result<(), String> {
    let bounds = schema::root()?
        .pointer("/x-fem-modelValidation/expressions/frequency/bounds")
        .ok_or("The model schema must declare prepared-frequency bounds.")?;
    let errors =
        fullmeta_model::semantic_quantities::validate_resolved_number(value, bounds, path, "Hz")?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} The evaluated frequency was {value} Hz.",
            errors.join("\n")
        ))
    }
}
