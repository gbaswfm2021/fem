//! Runtime discovery and composition for FEM-owned scenario contracts.
//!
//! This module is shared by the FEM solver and mesher adapters. Keep it within
//! the application so both native consumers use the same registry and rules.

use serde_json::{Map, Value, json};
use std::{collections::HashSet, fs, path::Path};

const CAPABILITY_PATHS: &[(&str, &str)] = &[
    ("mesh", "elementOrdersByProfile"),
    ("physics", "featureTypesByProfile"),
    ("physics", "periodicPolicyByProfile"),
    ("physics", "solverBoundaryTypesByProfile"),
    ("study", "discretizationChoicesByProfile"),
    ("study", "preparationByProfile"),
    ("study", "solverRuntimesByProfile"),
    ("study", "solverSettingsByProfile"),
];

fn safe_id(value: &Value) -> Option<&str> {
    let id = value.as_str()?;
    let mut bytes = id.bytes();
    if !matches!(bytes.next(), Some(b'a'..=b'z'))
        || !bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || id.len() > 64
    {
        return None;
    }
    Some(id)
}

fn merge_capability(
    root: &mut Value,
    contract: &Value,
    section: &str,
    field: &str,
    id: &str,
) -> Result<(), String> {
    let Some(value) = contract.pointer(&format!("/capabilities/{section}/{field}/{id}")) else {
        return Ok(());
    };
    let target = root
        .pointer_mut(&format!("/x-fem-capabilities/{section}/{field}"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            format!("FEM schema must declare x-fem-capabilities.{section}.{field} as an object.")
        })?;
    // Per-profile capabilities may be arrays or scalar values (for example,
    // supported element orders). Keep the contract value intact; the profile
    // map itself is the object that must be merged.
    let contribution = value.clone();
    if let Some(existing) = target.get(id) {
        if existing != &contribution {
            return Err(format!(
                "Scenario {id:?} conflicts with x-fem-capabilities.{section}.{field}.{id}."
            ));
        }
    } else {
        target.insert(id.to_string(), contribution);
    }
    Ok(())
}

fn append_unique(root: &mut Value, pointer: &str, additions: &[Value]) -> Result<(), String> {
    let values = root
        .pointer_mut(pointer)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("FEM schema contribution target {pointer} must be an array."))?;
    for addition in additions {
        if !values.contains(addition) {
            values.push(addition.clone());
        }
    }
    Ok(())
}

fn merge_schema_contributions(root: &mut Value, package: &Value, id: &str) -> Result<(), String> {
    let Some(contributions) = package.get("schemaContributions") else {
        return Ok(());
    };
    let contributions = contributions
        .as_object()
        .ok_or_else(|| format!("Scenario {id:?} schemaContributions must be an object."))?;
    for section in [
        "x-fullmeta-metadataValidation",
        "x-fullmeta-preparationValidation",
        "x-fullmeta-rules",
    ] {
        let Some(source) = contributions.get(section) else {
            continue;
        };
        if !source.is_object() {
            return Err(format!(
                "Scenario {id:?} contribution {section} must be an object."
            ));
        }
        for key in ["obligations", "rules"] {
            let pointer = if key == "obligations" {
                format!("/schemaContributions/{section}/declarative/{key}")
            } else {
                format!("/schemaContributions/{section}/{key}")
            };
            let Some(additions) = package.pointer(&pointer).and_then(Value::as_array) else {
                continue;
            };
            let target_pointer = if key == "obligations" {
                format!("/{section}/declarative/{key}")
            } else {
                format!("/{section}/{key}")
            };
            if root.pointer(&target_pointer).is_none() {
                if key == "obligations" {
                    root.pointer_mut(&format!("/{section}/declarative"))
                        .and_then(Value::as_object_mut)
                        .ok_or_else(|| {
                            format!("FEM schema {section}.declarative must be an object.")
                        })?
                        .insert(key.into(), json!([]));
                } else {
                    root.as_object_mut()
                        .and_then(|object| object.get_mut(section))
                        .and_then(Value::as_object_mut)
                        .ok_or_else(|| format!("FEM schema {section} must be an object."))?
                        .insert(key.into(), json!([]));
                }
            }
            append_unique(root, &target_pointer, additions)?;
        }
    }
    for (key, pointer) in [
        ("modelAllOf", "/properties/model/allOf"),
        ("physicsAllOf", "/properties/model/properties/physics/allOf"),
        (
            "studyItemAllOf",
            "/properties/model/properties/study/items/allOf",
        ),
    ] {
        let Some(additions) = contributions.get(key).and_then(Value::as_array) else {
            continue;
        };
        append_unique(root, pointer, additions)?;
    }
    if let Some(ui) = contributions.get("x-fullmeta-ui-profiles") {
        let profile = ui
            .get(id)
            .filter(|value| value.is_object())
            .ok_or_else(|| format!("Scenario {id:?} must declare its UI profile contribution."))?;
        let root_object = root
            .as_object_mut()
            .ok_or("The FEM schema must be an object.")?;
        let profiles = root_object
            .entry("x-fullmeta-ui-profiles")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("FEM schema x-fullmeta-ui-profiles must be an object.")?;
        profiles.insert(id.to_string(), profile.clone());
    }
    if let Some(runtime) = contributions.get("x-fullmeta-runtime") {
        let runtime = runtime.as_object().ok_or_else(|| {
            format!("Scenario {id:?} contribution x-fullmeta-runtime must be an object.")
        })?;
        if let Some(programs_value) = runtime.get("externalPrograms") {
            let programs = programs_value.as_object().ok_or_else(|| {
                format!("Scenario {id:?} runtime externalPrograms must be an object.")
            })?;
            let root_programs = root
                .pointer_mut("/x-fullmeta-runtime/externalPrograms")
                .and_then(Value::as_object_mut)
                .ok_or(
                    "FEM schema must declare x-fullmeta-runtime.externalPrograms as an object.",
                )?;
            for (program_id, program) in programs {
                if let Some(existing) = root_programs.get(program_id) {
                    if existing != program {
                        return Err(format!(
                            "Scenario {id:?} conflicts with runtime program {program_id:?}."
                        ));
                    }
                } else {
                    root_programs.insert(program_id.clone(), program.clone());
                }
            }
        }
    }
    Ok(())
}

fn validate_scenario_programs(package: &Value, id: &str) -> Result<(), String> {
    let study_type = package
        .pointer("/schemaDocument/x-fem-scenario/studyType")
        .and_then(Value::as_str)
        .filter(|study_type| matches!(*study_type, "stationary" | "frequencyDomain"))
        .ok_or_else(|| {
            format!(
                "Scenario {id:?} must declare x-fem-scenario.studyType as stationary or frequencyDomain."
            )
        })?;
    if package
        .pointer("/schemaDocument/x-fem-scenario/starterDefaults/study/type")
        .and_then(Value::as_str)
        .is_some_and(|starter_type| starter_type != study_type)
    {
        return Err(format!(
            "Scenario {id:?} starter Study type must match x-fem-scenario.studyType."
        ));
    }
    let declared = package
        .pointer("/schemaContributions/x-fullmeta-runtime/externalPrograms")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("Scenario {id:?} must declare runtime programs in its contract."))?;
    let capabilities = package
        .pointer("/capabilities/study")
        .ok_or_else(|| format!("Scenario {id:?} must declare study capabilities."))?;
    let mut used = HashSet::new();
    if let Some(program_id) = capabilities
        .pointer(&format!("/preparationByProfile/{id}/programId"))
        .and_then(Value::as_str)
    {
        used.insert(program_id);
    }
    if let Some(runtimes) = capabilities
        .pointer(&format!("/solverRuntimesByProfile/{id}"))
        .and_then(Value::as_object)
    {
        for runtime in runtimes.values() {
            if let Some(program_id) = runtime.get("programId").and_then(Value::as_str) {
                used.insert(program_id);
            }
        }
    }
    for program_id in &used {
        if !declared.contains_key(*program_id) {
            return Err(format!(
                "Scenario {id:?} references runtime {program_id:?} without registering it."
            ));
        }
    }
    for (program_id, program) in declared {
        let mut program_id_bytes = program_id.bytes();
        if !matches!(program_id_bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z'))
            || !program_id_bytes.all(|byte| byte.is_ascii_alphanumeric())
            || program_id.len() > 64
            || program.get("pathType").and_then(Value::as_str) != Some("relative")
        {
            return Err(format!(
                "Scenario {id:?} declares an invalid runtime program {program_id:?}."
            ));
        }
        let executable = program
            .get("executable")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("Scenario {id:?} runtime {program_id:?} must declare an executable.")
            })?;
        let parts = executable.split('/').collect::<Vec<_>>();
        if !executable.starts_with(".runtime/")
            || executable.contains('\\')
            || parts
                .iter()
                .any(|part| part.is_empty() || *part == "." || *part == "..")
        {
            return Err(format!(
                "Scenario {id:?} runtime {program_id:?} executable must stay under .runtime/."
            ));
        }
        if program
            .get("allowedEntries")
            .and_then(Value::as_array)
            .is_none_or(|entries| {
                entries.is_empty()
                    || entries
                        .iter()
                        .any(|entry| entry.as_str().is_none_or(|value| value.trim().is_empty()))
            })
        {
            return Err(format!(
                "Scenario {id:?} runtime {program_id:?} must declare allowedEntries."
            ));
        }
    }
    Ok(())
}

fn merge_scenario_capabilities(root: &mut Value, package: &Value, id: &str) -> Result<(), String> {
    if let Some(formulations) = package
        .pointer("/capabilities/mesh/fieldFormulations")
        .and_then(Value::as_object)
    {
        let target = root
            .pointer_mut("/x-fem-capabilities/mesh/fieldFormulations")
            .and_then(Value::as_object_mut)
            .ok_or(
                "FEM schema must declare x-fem-capabilities.mesh.fieldFormulations as an object.",
            )?;
        for (formulation, value) in formulations {
            if let Some(existing) = target.get(formulation) {
                if existing != value {
                    return Err(format!(
                        "Scenario {id:?} conflicts with mesh field formulation {formulation:?}."
                    ));
                }
            } else {
                target.insert(formulation.clone(), value.clone());
            }
        }
    }
    if let Some(rules) = package
        .pointer("/capabilities/study/solverOptionRules")
        .and_then(Value::as_array)
    {
        let target = root
            .pointer_mut("/x-fem-capabilities/study/solverOptionRules")
            .and_then(Value::as_array_mut)
            .ok_or(
                "FEM schema must declare x-fem-capabilities.study.solverOptionRules as an array.",
            )?;
        for rule in rules {
            if !target.contains(rule) {
                target.push(rule.clone());
            }
        }
    }
    Ok(())
}

fn merge_physics_condition_actions(
    root: &mut Value,
    document: &Value,
    id: &str,
) -> Result<(), String> {
    let physics = document
        .get("x-fem-physics")
        .ok_or_else(|| format!("Scenario {id:?} must declare x-fem-physics."))?;
    let supported_boundaries = physics
        .get("supportedBoundaryTypes")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Scenario {id:?} supportedBoundaryTypes must be an array."))?;
    let supported_features = physics
        .get("supportedFeatureTypes")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Scenario {id:?} supportedFeatureTypes must be an array."))?;
    let declarations = physics
        .get("conditionActions")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Scenario {id:?} must declare conditionActions."))?;
    let supported = supported_boundaries
        .iter()
        .chain(supported_features)
        .filter_map(Value::as_str)
        .collect::<HashSet<_>>();
    let mut declared = HashSet::new();
    for action in declarations {
        let condition_type = action
            .get("conditionType")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("Scenario {id:?} has a malformed condition action."))?;
        for key in ["action", "itemName", "label"] {
            if action
                .get(key)
                .and_then(Value::as_str)
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err(format!(
                    "Scenario {id:?} condition action {condition_type:?}.{key} must be a non-empty string."
                ));
            }
        }
        if !declared.insert(condition_type) {
            return Err(format!(
                "Scenario {id:?} declares condition action {condition_type:?} more than once."
            ));
        }
        let action_menus = root
            .pointer_mut("/x-fullmeta-ui-tree/actionMenus")
            .and_then(Value::as_array_mut)
            .ok_or("FEM schema must declare x-fullmeta-ui-tree.actionMenus as an array.")?;
        let menu = action_menus
            .iter_mut()
            .find(|menu| {
                menu.pointer("/when/scope").and_then(Value::as_str) == Some("item")
                    && menu.pointer("/when/path") == Some(&json!(["model", "physics"]))
            })
            .ok_or("FEM schema must declare the Physics condition action menu.")?;
        let entries = menu
            .get_mut("entries")
            .and_then(Value::as_array_mut)
            .ok_or("FEM Physics condition action menu entries must be an array.")?;
        if let Some(existing) = entries.iter().find(|entry| {
            entry.get("conditionType").and_then(Value::as_str) == Some(condition_type)
        }) {
            if existing != action {
                return Err(format!(
                    "FEM condition type {condition_type:?} has conflicting action declarations."
                ));
            }
        } else {
            entries.push(action.clone());
        }
    }
    if let Some(missing) = supported
        .iter()
        .find(|condition_type| !declared.contains(**condition_type))
    {
        return Err(format!(
            "Scenario {id:?} has no UI action for supported condition {missing:?}."
        ));
    }
    Ok(())
}

fn scenario_profile(document: &Value, id: &str) -> Result<(u64, String), String> {
    let profile = document
        .pointer("/x-fem-scenario/profile")
        .ok_or_else(|| format!("Scenario {id:?} must declare x-fem-scenario.profile."))?;
    if let Some(module) = document.pointer("/x-fem-scenario/solverModule") {
        let module = module
            .as_str()
            .filter(|module| {
                let mut bytes = module.bytes();
                matches!(bytes.next(), Some(b'a'..=b'z'))
                    && bytes.all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
                    && module.len() <= 64
            })
            .ok_or_else(|| {
                format!("Scenario {id:?} solverModule must be a safe lowercase module name.")
            })?;
        if module.is_empty() {
            return Err(format!(
                "Scenario {id:?} solverModule must be a safe lowercase module name."
            ));
        }
    }
    if profile.get("id").and_then(Value::as_str) != Some(id) {
        return Err(format!(
            "Scenario {id:?} profile ID does not match its registry ID."
        ));
    }
    let dimension = profile
        .get("dimension")
        .and_then(Value::as_u64)
        .filter(|dimension| *dimension > 0)
        .ok_or_else(|| format!("Scenario {id:?} dimension must be a positive integer."))?;
    let physics = profile
        .get("physics")
        .and_then(Value::as_str)
        .filter(|physics| !physics.trim().is_empty())
        .ok_or_else(|| format!("Scenario {id:?} physics must be a non-empty string."))?
        .to_string();
    let formulations = profile
        .get("formulations")
        .and_then(Value::as_array)
        .filter(|values| {
            !values.is_empty()
                && values
                    .iter()
                    .all(|value| value.as_str().is_some_and(|s| !s.trim().is_empty()))
        })
        .ok_or_else(|| format!("Scenario {id:?} formulations must be a non-empty string array."))?;
    let _ = formulations;
    Ok((dimension, physics))
}

fn profile_material_schema(
    document: &Value,
    id: &str,
) -> Result<(Map<String, Value>, Vec<String>), String> {
    let declarations = document
        .pointer("/x-fem-scenario/materials/properties")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Scenario {id:?} must declare material properties."))?;
    let mut properties = Map::new();
    let mut required = vec![
        "id".to_string(),
        "name".to_string(),
        "selectionId".to_string(),
    ];
    for declaration in declarations {
        let key = declaration
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| {
                let mut chars = key.chars();
                chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
                    && key.len() <= 64
                    && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            })
            .ok_or_else(|| {
                format!("Scenario {id:?} has a material property with an unsafe key.")
            })?;
        let schema = declaration
            .get("schema")
            .filter(|schema| schema.is_object())
            .ok_or_else(|| {
                format!("Scenario {id:?} material property {key:?} must declare a JSON Schema.")
            })?;
        if let Some(existing) = properties.get(key) {
            if existing != schema {
                return Err(format!(
                    "Scenario {id:?} declares conflicting schemas for material {key:?}."
                ));
            }
        } else {
            properties.insert(key.to_string(), schema.clone());
        }
        if declaration.get("required").and_then(Value::as_bool) == Some(true) {
            required.push(key.to_string());
        } else if declaration
            .get("required")
            .is_some_and(|value| !value.is_boolean())
        {
            return Err(format!(
                "Scenario {id:?} material property {key:?}.required must be a boolean."
            ));
        }
    }
    required.sort();
    required.dedup();
    Ok((properties, required))
}

fn configure_profile_schema(
    root: &mut Value,
    document: &mut Value,
    id: &str,
) -> Result<(), String> {
    let (material_properties, material_required) = profile_material_schema(document, id)?;
    let generic_material_properties = root
        .pointer("/properties/model/properties/materials/items/properties")
        .and_then(Value::as_object)
        .ok_or("FEM base model.materials.items.properties must be an object.")?
        .iter()
        .filter(|(key, _)| ["id", "name", "selectionId"].contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Map<_, _>>();
    if generic_material_properties.len() != 3 {
        return Err(
            "FEM base schema must declare generic material fields id, name, and selectionId."
                .into(),
        );
    }
    let mut profile_material_properties = generic_material_properties;
    profile_material_properties.extend(material_properties);
    let model_properties = document
        .pointer_mut("/then/properties/model/properties")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            format!("Scenario {id:?} schema must declare a model-specific then branch.")
        })?;
    model_properties.insert(
        "materials".into(),
        json!({"items": {"type":"object", "additionalProperties":false, "properties":profile_material_properties, "required":material_required}}),
    );

    let physics = document
        .get("x-fem-physics")
        .ok_or_else(|| format!("Scenario {id:?} must declare x-fem-physics."))?;
    let supported_boundaries = physics
        .get("supportedBoundaryTypes")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Scenario {id:?} supportedBoundaryTypes must be an array."))?;
    let supported_features = physics
        .get("supportedFeatureTypes")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Scenario {id:?} supportedFeatureTypes must be an array."))?;
    let fixed_schemas = root
        .pointer("/properties/model/properties/physics/additionalProperties/oneOf")
        .and_then(Value::as_array)
        .ok_or("FEM base schema must declare its fixed Physics condition variants.")?;
    let mut profile_branches = Vec::new();
    let mut seen_types = HashSet::new();
    let mut domain_source_units = Vec::<String>::new();
    for (values, kind) in [
        (supported_boundaries, "boundary"),
        (supported_features, "feature"),
    ] {
        for value in values {
            let condition_type = value
                .as_str()
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| {
                    format!(
                        "Scenario {id:?} supported {kind} types must contain non-empty strings."
                    )
                })?;
            if !seen_types.insert(condition_type) {
                return Err(format!(
                    "Scenario {id:?} declares condition type {condition_type:?} more than once."
                ));
            }
            let branch = fixed_schemas
                .iter()
                .find(|branch| {
                    branch
                        .pointer("/properties/type/const")
                        .and_then(Value::as_str)
                        == Some(condition_type)
                })
                .ok_or_else(|| {
                    format!("FEM base schema has no fixed Physics {kind} variant for {condition_type:?}.")
                })?;
            let mut branch = branch.clone();
            if physics.pointer(&format!("/featureHandlingByType/{condition_type}")).and_then(Value::as_str) == Some("domainSource") {
                let layout = physics.get("domainSourceLayout").ok_or_else(|| format!("Scenario {id:?} must declare a domain source layout."))?;
                let labels = layout.get("componentLabels").and_then(Value::as_array)
                    .filter(|items| !items.is_empty() && items.len() <= 3 && items.iter().all(|v| v.as_str().is_some_and(|s| !s.trim().is_empty())))
                    .ok_or_else(|| format!("Scenario {id:?} must label every domain source component."))?;
                let defaults = physics.get("boundaryDefaultsByFormulation").and_then(Value::as_object)
                    .ok_or("Domain source formulation defaults are required.")?;
                let mut units = Vec::<String>::new();
                for (formulation, defaults) in defaults {
                    let components = defaults.get(condition_type).and_then(|v| v.get("values")).and_then(Value::as_array)
                        .filter(|v| v.len() == labels.len()).ok_or_else(|| format!("Scenario {id:?} needs domain source defaults for {formulation}."))?;
                    if let Some(form_labels) = layout.get("labelsByFormulation").and_then(|v| v.get(formulation)) {
                        if form_labels.as_array().is_none_or(|v| v.len() != labels.len() || v.iter().any(|x| x.as_str().is_none_or(|s| s.trim().is_empty()))) {
                            return Err(format!("Scenario {id:?} has invalid domain source labels for {formulation}."));
                        }
                    }
                    for component in components {
                        let unit = component.get("unit").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).ok_or("Domain source default needs a unit.")?;
                        if !units.iter().any(|known| known == unit) { units.push(unit.to_string()); }
                    }
                }
                if units.is_empty() { return Err(format!("Scenario {id:?} has no domain source units.")); }
                for unit in &units { if !domain_source_units.contains(unit) { domain_source_units.push(unit.clone()); } }
                let vector = branch.pointer_mut("/properties/values").and_then(Value::as_object_mut).ok_or("FEM base schema must declare domain source values.")?;
                vector.insert("minItems".into(), json!(labels.len()));
                vector.insert("maxItems".into(), json!(labels.len()));
                if let Some(title) = layout.get("title").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) { vector.insert("title".into(), json!(title)); }
                let settings = vector.get_mut("x-fullmeta-ui-behavior").and_then(|ui| ui.pointer_mut("/payload/widget/settings")).and_then(Value::as_object_mut)
                    .ok_or("Domain source values must declare widget settings.")?;
                settings.insert("componentLabels".into(), Value::Array(labels.clone()));
                vector.get_mut("items").and_then(|v| v.pointer_mut("/properties/unit")).and_then(Value::as_object_mut)
                    .ok_or("Domain source component unit schema is required.")?.insert("enum".into(), json!(units));
            }
            let layout = physics.get("boundaryFieldLayouts").and_then(|layouts| layouts.get(condition_type));
            if layout.is_none() {
                if let Some(properties) = branch.get_mut("properties").and_then(Value::as_object_mut) {
                    properties.remove("fieldValues");
                }
            } else {
                let layout = layout.unwrap();
                if !matches!(layout.get("role").and_then(Value::as_str), Some("primary" | "derived")) {
                    return Err(format!("Scenario {id:?} must declare a field role for {condition_type}."));
                }
                let shape = layout.get("shape").and_then(Value::as_array).filter(|shape| !shape.is_empty())
                    .ok_or_else(|| format!("Scenario {id:?} must declare a field shape for {condition_type}."))?;
                let count = shape.iter().try_fold(1_u64, |total, size| {
                    size.as_u64().filter(|size| *size > 0).and_then(|size| total.checked_mul(size))
                }).filter(|count| *count <= 1024).ok_or_else(|| format!("Scenario {id:?} has an invalid field shape for {condition_type}."))?;
                let labels = layout.get("componentLabels").and_then(Value::as_array)
                    .filter(|labels| labels.len() == count as usize && labels.iter().all(|label| label.as_str().is_some_and(|label| !label.trim().is_empty())))
                    .ok_or_else(|| format!("Scenario {id:?} must label every field component for {condition_type}."))?;
                let vector = branch.pointer_mut("/properties/fieldValues").and_then(Value::as_object_mut)
                    .ok_or("FEM base boundary schema must declare fieldValues.")?;
                vector.insert("minItems".into(), json!(count));
                vector.insert("maxItems".into(), json!(count));
                let settings = vector.get_mut("x-fullmeta-ui-behavior").and_then(|ui| ui.pointer_mut("/payload/widget/settings"))
                    .and_then(Value::as_object_mut).ok_or("Boundary fieldValues must declare widget settings.")?;
                settings.insert("componentLabels".into(), Value::Array(labels.clone()));
                if layout.get("zeroScalarOnly").and_then(Value::as_bool) == Some(true) {
                    if let Some(value) = branch.pointer_mut("/properties/value").and_then(Value::as_object_mut) {
                        value.insert("title".into(), json!("scalar value (zero only)"));
                    }
                }
            }
            profile_branches.push(branch);
        }
    }
    let parameter_units = root.pointer_mut("/properties/model/properties/parameters/items/properties/unit/enum")
        .and_then(Value::as_array_mut).ok_or("FEM base schema must declare parameter units.")?;
    for unit in domain_source_units { if !parameter_units.iter().any(|v| v.as_str() == Some(&unit)) { parameter_units.push(json!(unit)); } }
    let profile_physics = document
        .pointer_mut("/then/properties/model/properties/physics")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("Scenario {id:?} schema must constrain model.physics."))?;
    let base_physics_properties = root
        .pointer("/properties/model/properties/physics/properties")
        .and_then(Value::as_object)
        .ok_or("FEM base model.physics.properties must be an object.")?
        .clone();
    let scenario_physics_properties = profile_physics
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut physics_properties = base_physics_properties;
    physics_properties.extend(scenario_physics_properties);
    profile_physics.insert("properties".into(), Value::Object(physics_properties));
    profile_physics.insert(
        "additionalProperties".into(),
        json!({"oneOf":profile_branches}),
    );
    Ok(())
}

fn compose_material_schema(root: &mut Value, documents: &[Value]) -> Result<(), String> {
    let mut item_schema = root
        .pointer("/properties/model/properties/materials/items")
        .cloned()
        .ok_or("FEM base schema must declare model.materials.items as an object schema.")?;
    let mut properties = item_schema
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .ok_or("FEM base model.materials.items.properties must be an object.")?;
    let generic_keys = ["id", "name", "selectionId"];
    properties.retain(|key, _| generic_keys.contains(&key.as_str()));
    let mut required = item_schema
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .ok_or("FEM base model.materials.items.required must be an array.")?;
    required.retain(|key| key.as_str().is_some_and(|key| generic_keys.contains(&key)));
    for document in documents {
        let id = document
            .pointer("/x-fem-scenario/profile/id")
            .and_then(Value::as_str)
            .ok_or("Scenario document must declare x-fem-scenario.profile.id.")?;
        let (new_properties, _) = profile_material_schema(document, id)?;
        for (key, schema) in new_properties {
            if let Some(existing) = properties.get(&key) {
                if existing != &schema {
                    return Err(format!(
                        "FEM material {key:?} has conflicting schema declarations across scenarios."
                    ));
                }
            } else {
                properties.insert(key.clone(), schema);
            }
        }
    }
    item_schema["properties"] = Value::Object(properties);
    item_schema["required"] = Value::Array(required);
    *root
        .pointer_mut("/properties/model/properties/materials/items")
        .ok_or("FEM base schema must declare model.materials.items.")? = item_schema;
    Ok(())
}

fn read_package(path: &Path, id: &str) -> Result<(Value, Value), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Could not read scenario package {}: {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "Scenario package {} must be a regular file.",
            path.display()
        ));
    }
    let package: Value = serde_json::from_slice(&fs::read(path).map_err(|error| {
        format!(
            "Could not read scenario package {}: {error}",
            path.display()
        )
    })?)
    .map_err(|error| {
        format!(
            "Could not parse scenario package {}: {error}",
            path.display()
        )
    })?;
    if package.get("contractVersion").and_then(Value::as_str) != Some("fullmeta-fem-scenario/1")
        || package.get("id").and_then(Value::as_str) != Some(id)
        || package
            .get("version")
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(format!(
            "Scenario package {id:?} has an unsupported contract or mismatched identity."
        ));
    }
    let mut document = package
        .get("schemaDocument")
        .cloned()
        .ok_or_else(|| format!("Scenario package {id:?} must contain schemaDocument."))?;
    let expected_id = format!("https://fullmeta.local/applications/fem/scenarios/{id}.schema.json");
    if document.get("$id").and_then(Value::as_str) != Some(expected_id.as_str()) {
        return Err(format!(
            "Scenario package {id:?} declares an unexpected schema $id."
        ));
    }
    scenario_profile(&document, id)?;
    let study_type = document
        .pointer("/x-fem-scenario/studyType")
        .and_then(Value::as_str)
        .filter(|value| matches!(*value, "stationary" | "frequencyDomain"))
        .ok_or_else(|| {
            format!("Scenario {id:?} must declare a supported default Study type.")
        })?;
    let supports_stationary = document
        .pointer("/x-fem-scenario/supportsStationary")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            format!("Scenario {id:?} must declare x-fem-scenario.supportsStationary as a boolean.")
        })?;
    if study_type == "stationary" && !supports_stationary {
        return Err(format!(
            "Scenario {id:?} must enable stationary Studies when its default Study type is stationary."
        ));
    }
    if let Some(root) = package.get("capabilities") {
        // The $defs schema has its own annotations for editor consumers; the
        // contract's explicit contribution remains authoritative for the root.
        if !root.is_object() {
            return Err(format!(
                "Scenario package {id:?} capabilities must be an object."
            ));
        }
    } else {
        return Err(format!(
            "Scenario package {id:?} must declare capabilities."
        ));
    }
    if let Some(dialect) = package.get("$schema") {
        document["$schema"] = dialect.clone();
    }
    document.as_object_mut().unwrap().remove("$id");
    validate_scenario_programs(&package, id)?;
    Ok((package, document))
}

/// Load solver-owned packages and compose them with the selected FEM base schema.
pub fn compose(root: Value, schema_path: &Path) -> Result<(Value, Vec<Value>), String> {
    let schema_dir = schema_path
        .parent()
        .ok_or("The selected model schema has no parent directory.")?;
    let registry_path = schema_dir
        .join("fem-rust")
        .join("src")
        .join("scenarios")
        .join("registry.json");
    let registry_metadata = fs::symlink_metadata(&registry_path).map_err(|error| {
        format!(
            "Could not read FEM scenario registry {}: {error}",
            registry_path.display()
        )
    })?;
    if registry_metadata.file_type().is_symlink() || !registry_metadata.is_file() {
        return Err(format!(
            "FEM scenario registry {} must be a regular file.",
            registry_path.display()
        ));
    }
    let registry: Value =
        serde_json::from_slice(&fs::read(&registry_path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("Could not parse FEM scenario registry: {error}"))?;
    if registry.get("format").and_then(Value::as_str) != Some("fullmeta-fem-scenario-registry/1") {
        return Err("The FEM scenario registry format is unsupported.".into());
    }
    let entries = registry
        .get("packages")
        .and_then(Value::as_array)
        .filter(|entries| !entries.is_empty())
        .ok_or("The FEM scenario registry must contain at least one package.")?;

    let mut root = root;
    let mut seen = HashSet::new();
    let mut profile_ids = Vec::new();
    let mut scenario_dimensions = Vec::new();
    let mut scenario_physics = Vec::new();
    let mut model_physics_types = Vec::new();
    let mut model_formulations = Vec::new();
    let mut documents = Vec::new();
    let mut default_id = None;
    let mut loaded_packages = Vec::new();
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(safe_id)
            .ok_or("FEM scenario registry contains an unsafe or missing ID.")?;
        if !seen.insert(id.to_string()) {
            return Err(format!("FEM scenario {id:?} is registered more than once."));
        }
        if entry.get("file").and_then(Value::as_str) != Some(format!("{}.json", id.replace('-', "")).as_str()) {
            return Err(format!(
                "FEM scenario registry entry {id:?} must use its matching JSON filename."
            ));
        }
        if entry
            .get("default")
            .is_some_and(|value| !value.is_boolean())
        {
            return Err(format!(
                "FEM scenario registry entry {id:?} default must be a boolean when declared."
            ));
        }
        if entry.get("default").and_then(Value::as_bool) == Some(true) {
            if default_id.replace(id.to_string()).is_some() {
                return Err(
                    "The FEM scenario registry may declare at most one default profile.".into(),
                );
            }
        }
        let package_path = schema_dir
            .join("fem-rust")
            .join("src")
            .join("scenarios")
            .join(format!("{}.json", id.replace('-', "")));
        let (package, mut document) = read_package(&package_path, id)?;
        let (dimension, physics) = scenario_profile(&document, id)?;
        let profile = document
            .pointer("/x-fem-scenario/profile")
            .ok_or_else(|| format!("Scenario {id:?} must declare profile metadata."))?;
        model_physics_types.push(
            profile
                .get("modelPhysicsType")
                .and_then(Value::as_str)
                .unwrap_or(&physics)
                .to_string(),
        );
        let formulations = profile
            .get("formulations")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("Scenario {id:?} formulations must be an array."))?;
        model_formulations.extend(
            formulations
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string),
        );
        configure_profile_schema(&mut root, &mut document, id)?;
        merge_physics_condition_actions(&mut root, &document, id)?;
        scenario_dimensions.push(dimension);
        scenario_physics.push(physics);
        profile_ids.push(id.to_string());
        documents.push(document);
        loaded_packages.push((id.to_string(), package));
    }

    {
        let root_object = root
            .as_object_mut()
            .ok_or("The FEM base schema must be an object.")?;
        let definitions = root_object
            .entry("$defs")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("FEM schema $defs must be an object.")?;
        definitions.insert("femProfiles".into(), Value::Object(Map::new()));
    }
    {
        let profile_definitions = root
            .pointer_mut("/$defs/femProfiles")
            .and_then(Value::as_object_mut)
            .ok_or("FEM schema $defs.femProfiles must be an object.")?;
        for ((id, _), document) in loaded_packages.iter().zip(&documents) {
            profile_definitions.insert(id.clone(), document.clone());
        }
    }
    {
        let root_object = root
            .as_object_mut()
            .ok_or("The FEM base schema must be an object.")?;
        let all_of = root_object
            .entry("allOf")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("FEM schema allOf must be an array.")?;
        all_of.retain(|item| {
            !item
                .get("$ref")
                .and_then(Value::as_str)
                .is_some_and(|reference| reference.starts_with("#/$defs/femProfiles/"))
        });
        for id in &profile_ids {
            all_of.push(json!({"$ref": format!("#/$defs/femProfiles/{id}")}));
        }
    }
    for (id, package) in &loaded_packages {
        for (section, field) in CAPABILITY_PATHS {
            merge_capability(&mut root, &package, section, field, id)?;
        }
        merge_schema_contributions(&mut root, package, id)?;
        merge_scenario_capabilities(&mut root, package, id)?;
    }
    compose_material_schema(&mut root, &documents)?;

    let physics_values = scenario_physics.iter().cloned().collect::<HashSet<_>>();
    let dimension_values = scenario_dimensions.iter().copied().collect::<HashSet<_>>();
    let scenario_branches = profile_ids.iter().enumerate().map(|(index, id)| {
        json!({"properties": {"profileId": {"const": id}, "dimension": {"const": scenario_dimensions[index]}, "physics": {"const": scenario_physics[index]}}})
    }).collect::<Vec<_>>();
    let mut dimensions = dimension_values.into_iter().collect::<Vec<_>>();
    dimensions.sort_unstable();
    let mut physics = physics_values.into_iter().collect::<Vec<_>>();
    physics.sort();
    {
        let scenario = root
            .pointer_mut("/properties/scenario")
            .and_then(Value::as_object_mut)
            .ok_or("FEM base schema must declare properties.scenario as an object.")?;
        scenario.insert("oneOf".into(), Value::Array(scenario_branches));
        let scenario_properties = scenario
            .get_mut("properties")
            .and_then(Value::as_object_mut)
            .ok_or("FEM base schema must declare properties.scenario.properties as an object.")?;
        scenario_properties
            .entry("profileId")
            .or_insert_with(|| json!({"type":"string"}))["enum"] = json!(profile_ids);
        scenario_properties
            .entry("dimension")
            .or_insert_with(|| json!({"type":"integer"}))["enum"] = json!(dimensions);
        scenario_properties
            .entry("physics")
            .or_insert_with(|| json!({"type":"string"}))["enum"] = json!(physics);

        model_physics_types.sort();
        model_physics_types.dedup();
        model_formulations.sort();
        model_formulations.dedup();
        let model_physics_properties = root
            .pointer_mut("/properties/model/properties/physics/properties")
            .and_then(Value::as_object_mut)
            .ok_or("FEM base model physics schema must declare properties.")?;
        model_physics_properties
            .entry("type")
            .or_insert_with(|| json!({"type":"string"}))["enum"] = json!(model_physics_types);
        model_physics_properties
            .entry("formulation")
            .or_insert_with(|| json!({"type":"string"}))["enum"] = json!(model_formulations);
        let field_formulations = root
            .pointer("/x-fem-capabilities/mesh/fieldFormulations")
            .and_then(Value::as_object)
            .ok_or("FEM schema x-fem-capabilities.mesh.fieldFormulations must be an object.")?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let discretization = root
        .pointer_mut(
            "/properties/model/properties/study/items/properties/discretization/properties/fieldFormulation",
        )
        .ok_or("FEM schema must declare study discretization.fieldFormulation.")?;
        discretization["enum"] = json!(field_formulations);
    }
    if let Some(model_editor_scenario) = root.pointer_mut("/x-fullmeta-ui-modelEditor/scenario") {
        let default = default_id.unwrap_or_else(|| profile_ids[0].clone());
        if !profile_ids.iter().any(|id| {
            Some(id.as_str())
                == model_editor_scenario
                    .get("defaultProfileId")
                    .and_then(Value::as_str)
        }) {
            model_editor_scenario["defaultProfileId"] = Value::String(default);
        }
    }
    if let Some(ui_profiles) = root
        .get_mut("x-fullmeta-ui-profiles")
        .and_then(Value::as_object_mut)
    {
        ui_profiles.retain(|id, _| seen.contains(id));
    }
    Ok((root, documents))
}
