//! Read the mesher's application metadata directly from registered model schemas.
//!
//! Schema files own the declarations. This module only loads and checks those
//! declarations; it does not embed scenario names, fields or material symbols.

use serde_json::Value;
use std::{
    collections::{BTreeSet, HashSet},
    env,
    path::{Path, PathBuf},
    sync::OnceLock,
};

fn runtime_documents() -> Result<&'static [Value], String> {
    static DOCUMENTS: OnceLock<Result<Vec<Value>, String>> = OnceLock::new();
    match DOCUMENTS.get_or_init(load_runtime_documents) {
        Ok(documents) => Ok(documents),
        Err(error) => Err(error.clone()),
    }
}

static SELECTED_SCHEMA: OnceLock<PathBuf> = OnceLock::new();
pub fn model_schema_selected() -> bool {
    SELECTED_SCHEMA.get().is_some()
}
fn schema_path_from_environment() -> Result<Option<PathBuf>, String> {
    let Some(value) = env::var_os("FULLMETA_MODEL_SCHEMA_FILE") else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err("FULLMETA_MODEL_SCHEMA_FILE must be an absolute path.".into());
    }
    Ok(Some(path))
}
pub fn select_model_schema_from_environment() -> Result<(), String> {
    let path = schema_path_from_environment()?.ok_or("FULLMETA_MODEL_SCHEMA_FILE is not set.")?;
    SELECTED_SCHEMA
        .set(path)
        .map_err(|_| "The mesh process already selected a schema.".to_string())
}
pub fn select_model_schema(model: &Value, file: Option<&Path>) -> Result<(), String> {
    let path = if let Some(path) = schema_path_from_environment()? {
        path
    } else {
        let file = file.ok_or("Pass the selected schema through FULLMETA_MODEL_SCHEMA_FILE when reading a model from stdin.")?;
        let reference = model
            .get("x-fullmeta-schema")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or("The model must declare x-fullmeta-schema.")?;
        if reference.contains(['\\', ':', '?', '#']) || Path::new(reference).is_absolute() {
            return Err("x-fullmeta-schema must be a relative path.".into());
        }
        let file = if file.is_absolute() {
            file.to_path_buf()
        } else {
            env::current_dir().map_err(|e| e.to_string())?.join(file)
        };
        file.parent()
            .ok_or("The model has no directory.")?
            .join(reference)
    };
    SELECTED_SCHEMA
        .set(path)
        .map_err(|_| "The mesh process already selected a schema.".to_string())
}
fn load_runtime_documents() -> Result<Vec<Value>, String> {
    let path = SELECTED_SCHEMA
        .get()
        .ok_or("Select the model schema before compiling geometry.")?;
    let mut documents = fullmeta_model::schema_documents::load_schema_documents(path)?;
    if documents.is_empty() {
        return Err("The model schema document is not loaded.".into());
    }
    let root = documents.remove(0);
    documents.extend(embedded_scenario_documents(&root)?);
    documents.insert(0, root);
    Ok(documents)
}

/// Bind an expression declaration from the selected application schema.
pub fn expression_runtime_policy(
    kind: &str,
    model: &Value,
    context: &Value,
) -> Result<Value, String> {
    let root = runtime_documents()?
        .first()
        .ok_or("The model schema document is not loaded.")?;
    let declaration = root
        .get("x-fullmeta-expressionRuntime")
        .and_then(|runtime| runtime.get(kind))
        .ok_or_else(|| {
            format!("The model schema must declare x-fullmeta-expressionRuntime.{kind}.")
        })?;
    fullmeta_model::runtime_expressions::bind_policy(model, root, context, declaration)
}

pub fn runtime_tool_path(id: &str) -> Result<PathBuf, String> {
    let documents = runtime_documents()?;
    let schema_file = SELECTED_SCHEMA
        .get()
        .ok_or("Select the model schema before resolving runtime tools.")?;
    let root = fullmeta_model::runtime_paths::installation_root()?;
    fullmeta_model::runtime_paths::runtime_tool_path(documents, schema_file, &root, id)
}

fn embedded_scenario_documents(root: &Value) -> Result<Vec<Value>, String> {
    let Some(profiles) = root
        .pointer("/$defs/femProfiles")
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };
    let root_id = root
        .get("$id")
        .and_then(Value::as_str)
        .ok_or("The FEM model schema must declare a $id before embedding profile schemas.")?;
    let base = root_id
        .rsplit_once('/')
        .map(|(base, _)| format!("{base}/"))
        .ok_or("The FEM model schema $id must include a document path.")?;
    profiles
        .iter()
        .map(|(key, profile_schema)| {
            let profile_id = profile_schema
                .pointer("/x-fem-scenario/profile/id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("Embedded FEM profile {key:?} must declare x-fem-scenario.profile.id.")
                })?;
            if key != profile_id {
                return Err(format!(
                    "Embedded FEM profile key {key:?} must match its profile ID {profile_id:?}."
                ));
            }
            let mut document = profile_schema.clone();
            let object = document
                .as_object_mut()
                .ok_or_else(|| format!("Embedded FEM profile {key:?} must be a schema object."))?;
            object.insert(
                "$id".into(),
                Value::String(format!("{base}{profile_id}.schema.json")),
            );
            if let Some(dialect) = root.get("$schema") {
                object.insert("$schema".into(), dialect.clone());
            }
            Ok(document)
        })
        .collect()
}

fn metadata_string<'a>(value: &'a Value, key: &str, context: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{context}.{key} must be a non-empty string."))
}

fn metadata_strings<'a>(value: Option<&'a Value>, context: &str) -> Result<Vec<&'a str>, String> {
    let values = value
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
        .ok_or_else(|| format!("{context} must be a non-empty string array."))?;
    let mut strings = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let string = value
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("{context}[{index}] must be a non-empty string."))?;
        if strings.contains(&string) {
            return Err(format!("{context} declares {string:?} more than once."));
        }
        strings.push(string);
    }
    Ok(strings)
}

fn scenario_metadata(profile_id: &str) -> Result<&'static Value, String> {
    let mut selected = None;
    // The manifest always registers the model schema first.
    for document in runtime_documents()?.iter().skip(1) {
        let metadata = document
            .get("x-fem-scenario")
            .ok_or("Registered scenario schemas require x-fem-scenario metadata.")?;
        let profile = metadata
            .get("profile")
            .filter(|profile| profile.is_object())
            .ok_or("Schema x-fem-scenario.profile must be an object.")?;
        let id = metadata_string(profile, "id", "Schema x-fem-scenario.profile")?;
        if id != profile_id {
            continue;
        }
        if selected.is_some() {
            return Err(format!(
                "Registered scenario schemas declare profile {profile_id:?} more than once."
            ));
        }
        let context = format!("Scenario {profile_id:?} metadata");
        metadata_string(profile, "physics", &format!("{context}.profile"))?;
        if profile.get("fieldSetId").is_some() || profile.get("materialSetId").is_some() {
            return Err(format!(
                "{context} must declare fields and materials directly in its own schema."
            ));
        }
        if profile
            .get("dimension")
            .and_then(Value::as_u64)
            .filter(|dimension| *dimension > 0)
            .is_none()
        {
            return Err(format!(
                "{context}.profile.dimension must be a positive integer."
            ));
        }
        for (schema_key, profile_key) in [
            ("profileId", "id"),
            ("physics", "physics"),
            ("dimension", "dimension"),
        ] {
            if document.pointer(&format!("/$defs/scenario/properties/{schema_key}/const"))
                != profile.get(profile_key)
            {
                return Err(format!(
                    "{context}.profile.{profile_key} must match its scenario schema constraint."
                ));
            }
        }
        let allowed = metadata_strings(
            profile.get("formulations"),
            &format!("{context}.profile.formulations"),
        )?;
        let declared = if let Some(formulation) = document
            .pointer("/$defs/physicsFormulation/const")
            .and_then(Value::as_str)
        {
            vec![formulation]
        } else {
            metadata_strings(
                document.pointer("/$defs/physicsFormulation/enum"),
                &format!("Scenario {profile_id:?} physicsFormulation.enum"),
            )?
        };
        if declared.len() != allowed.len()
            || declared
                .iter()
                .any(|formulation| !allowed.contains(formulation))
        {
            return Err(format!(
                "{context}.profile.formulations must match its physicsFormulation schema."
            ));
        }
        selected = Some(metadata);
    }
    selected
        .ok_or_else(|| format!("No registered scenario schema declares profile {profile_id:?}."))
}

fn merge_capabilities(target: &mut Value, source: &Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            if let Some(existing) = target.get_mut(key) {
                merge_capabilities(existing, value);
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = source.clone();
    }
}

/// Compose capabilities from the root schema, imported extensions and embedded profiles.
pub fn engine_capabilities() -> Result<Value, String> {
    let mut capabilities = serde_json::json!({ "schema": "fem-engine-capabilities/2" });
    for document in runtime_documents()? {
        if let Some(declared) = document.get("x-fem-capabilities") {
            if !declared.is_object() {
                return Err("Mesh schema x-fem-capabilities declarations must be objects.".into());
            }
            merge_capabilities(&mut capabilities, declared);
        }
    }
    Ok(capabilities)
}

/// Reserve fields from every supported formulation plus material parameters.
/// Changing a model's formulation therefore cannot invalidate parameter names.
pub fn reserved_symbols_for_model(model: &Value) -> Result<Vec<String>, String> {
    let profile_id = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or("model.scenario.profileId must be a non-empty string.")?;
    let metadata = scenario_metadata(profile_id)?;
    let allowed = metadata_strings(
        metadata.pointer("/profile/formulations"),
        &format!("Scenario {profile_id:?} metadata.profile.formulations"),
    )?;
    let fields = metadata
        .pointer("/fields/formulations")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!("Scenario {profile_id:?} metadata.fields.formulations must be an object.")
        })?;
    if fields.len() != allowed.len()
        || allowed
            .iter()
            .any(|formulation| !fields.get(*formulation).is_some_and(Value::is_object))
    {
        return Err(format!(
            "Scenario {profile_id:?} must declare fields for exactly its supported formulations."
        ));
    }
    let mut symbols = BTreeSet::new();
    for formulation in allowed {
        for key in [
            "fieldVariables",
            "derivedFieldVariables",
            "zeroFieldVariables",
        ] {
            let context = format!("Scenario {profile_id:?} formulation {formulation:?} {key}");
            let variables = fields[formulation]
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{context} must be an array."))?;
            if key == "fieldVariables" && variables.is_empty() {
                return Err(format!("{context} must declare primary field variables."));
            }
            for (index, variable) in variables.iter().enumerate() {
                let context = format!("{context}[{index}]");
                let symbol = metadata_string(variable, "symbol", &context)?;
                metadata_string(variable, "unit", &context)?;
                if let Some(dimensions) = variable.get("zeroInDimensions") {
                    let dimensions = dimensions.as_array().ok_or_else(|| {
                        format!("{context}.zeroInDimensions must be an integer array.")
                    })?;
                    for dimension in dimensions {
                        if dimension
                            .as_u64()
                            .filter(|dimension| *dimension > 0)
                            .is_none()
                        {
                            return Err(format!(
                                "{context}.zeroInDimensions must contain positive integers."
                            ));
                        }
                    }
                }
                if let Some(component) = variable.get("solverComponent") {
                    if !component.is_object() {
                        return Err(format!("{context}.solverComponent must be an object."));
                    }
                    for key in ["family", "axis"] {
                        if component.get(key).is_some() {
                            metadata_string(component, key, &format!("{context}.solverComponent"))?;
                        }
                    }
                }
                symbols.insert(symbol.to_string());
            }
        }
    }
    let materials = metadata
        .get("materials")
        .ok_or_else(|| format!("Scenario {profile_id:?} metadata.materials must be an object."))?;
    metadata_string(
        materials,
        "name",
        &format!("Scenario {profile_id:?} metadata.materials"),
    )?;
    let properties = materials
        .get("properties")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            format!("Scenario {profile_id:?} metadata.materials.properties must be an array.")
        })?;
    let mut material_keys = HashSet::new();
    let mut material_symbols = HashSet::new();
    for (index, property) in properties.iter().enumerate() {
        let context = format!("Scenario {profile_id:?} metadata.materials.properties[{index}]");
        let key = metadata_string(property, "key", &context)?;
        let symbol = metadata_string(property, "symbol", &context)?;
        metadata_string(property, "unit", &context)?;
        if !material_keys.insert(key) {
            return Err(format!(
                "Scenario {profile_id:?} declares material key {key:?} more than once."
            ));
        }
        if !material_symbols.insert(symbol) {
            return Err(format!(
                "Scenario {profile_id:?} declares material symbol {symbol:?} more than once."
            ));
        }
        symbols.insert(symbol.to_string());
    }
    Ok(symbols.into_iter().collect())
}
