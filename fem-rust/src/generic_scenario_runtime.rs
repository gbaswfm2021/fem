//! Capability-based entry point for scenario packages without Rust modules.
//!
//! Numerical family and Study preparation are selected by package declarations.
use serde_json::Value;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

pub(crate) fn declared_kind(metadata: &Value) -> Result<Option<&str>, String> {
    let Some(execution) = metadata.get("execution") else {
        return Ok(None);
    };
    if metadata.get("solverModule").is_some() {
        return Err("Generic execution and a scenario solverModule are mutually exclusive.".into());
    }
    record_module_label(metadata)?;
    let kind = execution
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("Generic execution.kind must be a supported string.")?;
    if !matches!(kind, "scalarSimplexP1" | "vectorSimplexP1" | "orientedTetrahedronNedelec") {
        return Err(format!(
            "Unsupported generic scenario execution kind {kind:?}."
        ));
    }
    Ok(Some(kind))
}

/// A historical run-record label is a compatibility identifier, not a Rust
/// module to load or execute. Its characters follow the existing module-label
/// grammar so records remain safe for older consumers.
pub(crate) fn record_module_label(metadata: &Value) -> Result<Option<&str>, String> {
    let Some(value) = metadata
        .get("execution")
        .and_then(|execution| execution.get("recordModuleLabel"))
    else {
        return Ok(None);
    };
    let label = value
        .as_str()
        .filter(|label| {
            let mut bytes = label.bytes();
            matches!(bytes.next(), Some(b'a'..=b'z'))
                && label.len() <= 64
                && bytes
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        })
        .ok_or("execution.recordModuleLabel must be a safe lowercase compatibility label.")?;
    Ok(Some(label))
}

fn validate(package: &Value, kind: &str, profile: &str) -> Result<(), String> {
    if !matches!(kind, "scalarSimplexP1" | "vectorSimplexP1" | "orientedTetrahedronNedelec") {
        return Err(format!(
            "Unsupported generic scenario execution kind {kind:?}."
        ));
    }
    if package.get("id").and_then(Value::as_str) != Some(profile) {
        return Err(
            "Generic scenario package identity does not match the requested profile.".into(),
        );
    }
    let metadata = package
        .pointer("/schemaDocument/x-fem-scenario")
        .ok_or("Generic scenario execution metadata is missing.")?;
    if declared_kind(metadata)? != Some(kind) {
        return Err("Requested generic execution kind does not match the scenario package.".into());
    }
    Ok(())
}

pub(crate) fn run(kind: &str, profile: &str, arguments: &[String]) -> Result<i32, String> {
    if !matches!(kind, "scalarSimplexP1" | "vectorSimplexP1" | "orientedTetrahedronNedelec") {
        return Err(format!(
            "Unsupported generic scenario execution kind {kind:?}."
        ));
    }
    let package = crate::scenario_package::load_installed(profile)?;
    validate(&package, kind, profile)?;
    run_validated(&package, kind, profile, arguments)
}

fn run_validated(package: &Value, kind: &str, profile: &str, arguments: &[String]) -> Result<i32, String> {
    let preparation = || {
        package
            .pointer("/schemaDocument/x-fem-capabilities/study/preparationByProfile")
            .and_then(|v| v.get(profile))
            .and_then(|v| v.get("kind"))
            .and_then(Value::as_str)
            .ok_or("Scenario preparation kind is required for generic execution.")
    };
    match arguments {
        [command, request] if command == "--solve" => {
            match (kind, preparation()?) {
                ("scalarSimplexP1", "stationaryPlan") => crate::prepared_scalar_execution::run_request(
                    std::path::Path::new(request),
                    profile,
                )?,
                ("scalarSimplexP1", "frequencyPlan") => crate::prepared_frequency_execution::run_request(
                    std::path::Path::new(request),
                    profile,
                )?,
                ("vectorSimplexP1", "frequencyPlan") => crate::prepared_frequency_execution::run_vector_request(
                    std::path::Path::new(request),
                    profile,
                )?,
                ("orientedTetrahedronNedelec", "configuredFrequencyRequest") => crate::prepared_edge_publication::run_request(
                    std::path::Path::new(request),
                    profile,
                )?,
                (_, other) => {
                    return Err(format!(
                        "Generic {kind} execution does not support preparation kind {other:?}."
                    ))
                }
            }
            Ok(0)
        }
        [command, manifest] if command == "--sequence" => {
            match (kind, preparation()?) {
                ("scalarSimplexP1", "frequencyPlan") => crate::prepared_frequency_execution::run_sequence(
                    std::path::Path::new(manifest),
                    profile,
                )?,
                ("vectorSimplexP1", "frequencyPlan") => crate::prepared_frequency_execution::run_vector_sequence(
                    std::path::Path::new(manifest),
                    profile,
                )?,
                ("orientedTetrahedronNedelec", "configuredFrequencyRequest") => crate::prepared_edge_publication::run_sequence(
                    std::path::Path::new(manifest),
                    profile,
                )?,
                (_, other) => {
                    return Err(format!(
                        "Generic {kind} sequence does not support preparation kind {other:?}."
                    ))
                }
            }
            Ok(0)
        }
        [command, ..] if command == "--sequence" => {
            Err(format!("Generic {kind} --sequence requires exactly one manifest path."))
        }
        [command, ..] if command == "--solve" => {
            Err(format!("Generic {kind} --solve requires exactly one request path."))
        }
        _ => crate::run_shared_operation(arguments),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_capability_and_package_identity_without_claiming_a_runner() {
        let package = json!({"id":"fresh", "schemaDocument":{"x-fem-scenario":{"execution":{"kind":"scalarSimplexP1"}}}});
        assert!(validate(&package, "scalarSimplexP1", "fresh").is_ok());
        let mut vector = package.clone();
        vector["schemaDocument"]["x-fem-scenario"]["execution"]["kind"] = json!("vectorSimplexP1");
        assert!(validate(&vector, "vectorSimplexP1", "fresh").is_ok());
        assert!(validate(&vector, "scalarSimplexP1", "fresh").is_err());
        let mut edge = package.clone();
        edge["schemaDocument"]["x-fem-scenario"]["execution"]["kind"] = json!("orientedTetrahedronNedelec");
        assert!(validate(&edge, "orientedTetrahedronNedelec", "fresh").is_ok());
        assert!(validate(&edge, "scalarSimplexP1", "fresh").is_err());
        assert!(validate(&package, "unknown", "fresh")
            .unwrap_err()
            .contains("Unsupported"));
        assert!(validate(&package, "scalarSimplexP1", "wrong").is_err());
        let mut mixed = package.clone();
        mixed["schemaDocument"]["x-fem-scenario"]["solverModule"] = json!("old");
        assert!(validate(&mixed, "scalarSimplexP1", "fresh")
            .unwrap_err()
            .contains("mutually exclusive"));
        assert_eq!(declared_kind(&json!({})).unwrap(), None);
        assert_eq!(
            declared_kind(&json!({"execution":{"kind":"scalarSimplexP1"}})).unwrap(),
            Some("scalarSimplexP1")
        );
        assert!(declared_kind(&json!({"execution":{"kind":"unknown"}}))
            .unwrap_err()
            .contains("Unsupported"));
        assert!(declared_kind(&json!({"execution":{}})).is_err());
    }

    #[test]
    fn solve_routes_to_file_runner_and_rejects_wrong_profile_before_io() {
        use std::{
            fs,
            time::{SystemTime, UNIX_EPOCH},
        };
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let path = base.join(format!(
            "fullmeta-generic-request-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, json!({"solver":"different-profile"}).to_string()).unwrap();
        let arguments = vec!["--solve".into(), path.to_string_lossy().into_owned()];
        let package = json!({"schemaDocument":{"x-fem-capabilities":{"study":{"preparationByProfile":{
            "selected-profile":{"kind":"stationaryPlan"}}}}}});
        let error = run_validated(&package, "scalarSimplexP1", "selected-profile", &arguments).unwrap_err();
        assert!(error.contains("does not match the selected scenario"));
        assert!(run_validated(
            &package,
            "scalarSimplexP1",
            "selected-profile",
            &["--sequence".into(), "cases.json".into()]
        )
        .unwrap_err()
        .contains("does not support"));
        assert!(run_validated(&package, "vectorSimplexP1", "selected-profile", &arguments)
            .unwrap_err().contains("does not support"));
        assert!(
            run_validated(&package, "scalarSimplexP1", "selected-profile", &["--solve".into()])
                .unwrap_err()
                .contains("exactly one")
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn build_script_accepts_json_only_generic_package_and_rejects_unknown_kind() {
        use std::{
            fs,
            time::{SystemTime, UNIX_EPOCH},
        };
        let base = fs::canonicalize(std::env::temp_dir()).unwrap();
        let path = base.join(format!(
            "fullmeta-generic-package-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let scenarios = path.join("src/scenarios");
        let output = path.join("out");
        fs::create_dir(&path).unwrap();
        fs::create_dir_all(&scenarios).unwrap();
        fs::create_dir_all(&output).unwrap();
        fs::write(path.join("native-operations.json"), b"{}").unwrap();
        let mut package = json!({"contractVersion":"fullmeta-fem-scenario/1", "id":"new-scalar", "discovery":{"default":true},
            "schemaDocument":{"x-fem-scenario":{"execution":{"kind":"scalarSimplexP1"}}}});
        let package_file = scenarios.join("newscalar.json");
        fs::write(&package_file, package.to_string()).unwrap();
        fs::write(
            scenarios.join("registry.json"),
            json!({"format":"fullmeta-fem-scenario-registry/1",
            "packages":[{"id":"new-scalar", "file":"newscalar.json", "default":true}]})
            .to_string(),
        )
        .unwrap();
        build_script::generate_scenario_runners(&path, &output);
        let generated = fs::read_to_string(output.join("scenario_runners.rs")).unwrap();
        assert!(!generated.contains("newscalar.rs"));
        assert!(!generated.contains("scenario_runner_newscalar"));
        package["schemaDocument"]["x-fem-scenario"]["execution"]["kind"] = json!("unsupported");
        fs::write(&package_file, package.to_string()).unwrap();
        assert!(
            std::panic::catch_unwind(|| build_script::generate_scenario_runners(&path, &output))
                .is_err()
        );
        let resolved = fs::canonicalize(&path).unwrap();
        assert_eq!(resolved.parent(), Some(base.as_path()));
        fs::remove_dir_all(resolved).unwrap();
    }
}
