//! Resolve scenario declarations from the selected application's registry.
//!
//! Consumers share the same identity and path checks when loading declarations.
use serde_json::Value;
use std::fs;
use std::path::Path;

fn read_regular_json(path: &Path) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!("{} must be a regular file.", path.display()));
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Could not parse {}: {error}", path.display()))
}

pub(crate) fn load_installed(profile_id: &str) -> Result<Value, String> {
    let schema = crate::model_schema::runtime_model_schema_path()?;
    let application = schema
        .parent()
        .ok_or("Selected schema has no application directory.")?;
    load_from_directory(
        &application.join("fem-rust").join("src").join("scenarios"),
        profile_id,
    )
}

pub(crate) fn load_from_directory(root: &Path, profile_id: &str) -> Result<Value, String> {
    let mut chars = profile_id.bytes();
    if !matches!(chars.next(), Some(b'a'..=b'z'))
        || profile_id.len() > 64
        || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err(format!("Invalid scenario profile ID {profile_id:?}."));
    }
    let registry = read_regular_json(&root.join("registry.json"))?;
    if registry.get("format").and_then(Value::as_str) != Some("fullmeta-fem-scenario-registry/1") {
        return Err("Unsupported scenario registry format.".into());
    }
    let entries = registry
        .get("packages")
        .and_then(Value::as_array)
        .ok_or("Scenario registry packages must be an array.")?;
    let matches = entries
        .iter()
        .filter(|entry| entry.get("id").and_then(Value::as_str) == Some(profile_id))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(format!(
            "Scenario registry must contain exactly one entry for {profile_id:?}."
        ));
    }
    let filename = format!("{}.json", profile_id.replace('-', ""));
    if matches[0].get("file").and_then(Value::as_str) != Some(filename.as_str()) {
        return Err(format!(
            "Scenario {profile_id:?} registry filename does not match its ID."
        ));
    }
    let package = read_regular_json(&root.join(filename))?;
    if package.get("contractVersion").and_then(Value::as_str) != Some("fullmeta-fem-scenario/1")
        || package.get("id").and_then(Value::as_str) != Some(profile_id)
        || package
            .get("version")
            .and_then(Value::as_str)
            .is_none_or(|v| v.trim().is_empty())
    {
        return Err(format!(
            "Scenario {profile_id:?} has an invalid contract, identity or version."
        ));
    }
    Ok(package)
}
