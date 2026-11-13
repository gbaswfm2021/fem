//! Adapt preparation inputs to the application-neutral option resolver.
use serde_json::{Map, Value, json};

type Result<T> = std::result::Result<T, String>;

/// Preserve the preparation API while the supplied schema owns value-source
/// precedence, conditional option catalogs, and compatibility validation.
pub(crate) fn resolve(
    capability_study: &Value,
    profile_settings: &Value,
    solver: &Value,
    field_formulation: &str,
    element_order: u64,
) -> Result<Map<String, Value>> {
    let resolution = capability_study
        .get("solverResolution")
        .ok_or("The schema must declare study.solverResolution.")?;
    let keys = resolution
        .get("contextKeys")
        .ok_or("The schema must declare option-resolution context keys.")?;
    let mut context = Map::new();
    for (slot, value) in [
        ("formulation", json!(field_formulation)),
        ("order", json!(element_order)),
    ] {
        let key = keys
            .get(slot)
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                format!("The schema must declare option-resolution context key {slot:?}.")
            })?;
        context.insert(key.to_string(), value);
    }
    let options = capability_study
        .get("solverOptions")
        .ok_or("The schema must declare study.solverOptions.")?;
    let rules = capability_study
        .get("solverOptionRules")
        .ok_or("The schema must declare study.solverOptionRules.")?;
    let sources = json!({
        "explicit": solver,
        "defaults": profile_settings.get("defaults").unwrap_or(&Value::Null),
        "shared": capability_study.get("solverDefaults").unwrap_or(&Value::Null),
    });
    fullmeta_model::rules_options::resolve(
        resolution,
        options,
        rules,
        &sources,
        &Value::Object(context),
    )?
    .as_object()
    .cloned()
    .ok_or("Resolved options must be an object.".into())
}
