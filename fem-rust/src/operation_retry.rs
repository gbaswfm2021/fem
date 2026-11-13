//! Opt-in durable retry identity. Never start another worker for an existing operation ID.
use super::{checked_descendant, checked_run_id, model_schema, read_json};
use fullmeta_operation_contracts::{ContractIdentity, Diagnostic, digest};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};
const RECEIPT: &str = ".operation-completion.json";
fn issue(code: &str, message: String) -> Diagnostic {
    Diagnostic::new(code, message, None, "retry")
}

pub(super) fn fingerprint_for(
    model: &Value,
    study: &str,
    document: &str,
    schema_path: &str,
    schema_bytes: &[u8],
) -> Value {
    let schema_digest = digest(schema_bytes);
    let request = json!({"model":model,"studyId":study,"document":document,"schemaPath":schema_path,"schemaDigest":schema_digest,"contractDigest":fullmeta_operation_contracts::contract_digest()});
    json!({"version":1,"sha256":digest(&serde_json::to_vec(&request).expect("JSON request")),"schemaDigest":schema_digest,"contractDigest":fullmeta_operation_contracts::contract_digest()})
}
pub(super) fn fingerprint(model: &Value, study: &str, document: &str) -> Result<Value, String> {
    let schema = model_schema::runtime_model_schema_path()?;
    let bytes = fs::read(&schema)
        .map_err(|error| format!("Could not fingerprint the interpreting schema: {error}"))?;
    Ok(fingerprint_for(
        model,
        study,
        document,
        &schema.to_string_lossy(),
        &bytes,
    ))
}
fn run_directory(document: &Path, id: &str) -> Result<PathBuf, Diagnostic> {
    let runs = env::var("FULLMETA_RUNS_DIRECTORY").map_err(|_| {
        issue(
            "OPERATION_CONTEXT_INVALID",
            "The original run-storage context is missing.".into(),
        )
    })?;
    if !checked_run_id(&runs) || !checked_run_id(id) {
        return Err(issue(
            "OPERATION_CONTEXT_INVALID",
            "The operation or run-storage ID is invalid.".into(),
        ));
    }
    Ok(document
        .parent()
        .ok_or_else(|| {
            issue(
                "OPERATION_CONTEXT_INVALID",
                "The original document has no parent directory.".into(),
            )
        })?
        .join(runs)
        .join(id))
}
fn validate_outputs(root: &Path, directory: &Path, event: &Value) -> Result<(), Diagnostic> {
    let directory = fs::canonicalize(directory)
        .map_err(|error| issue("OPERATION_ARTIFACTS_INVALID", error.to_string()))?;
    fullmeta_operation_contracts::validate_event("fem.studyRun", event)
        .map_err(|issues| issues.into_iter().next().expect("schema issue"))?;
    let paths = event["outputFiles"].as_array().ok_or_else(|| {
        issue(
            "OPERATION_ARTIFACTS_INVALID",
            "The saved completion has no output files.".into(),
        )
    })?;
    let declared =
        fullmeta_operation_contracts::descriptor("fem.studyRun").expect("known contract");
    for required in declared["artifacts"]["requiredSuffixes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !paths
            .iter()
            .filter_map(Value::as_str)
            .any(|path| path.ends_with(required))
        {
            return Err(issue(
                "OPERATION_ARTIFACT_MISSING",
                format!("The saved completion omits required artifact {required}."),
            ));
        }
    }
    for value in paths {
        let path = value.as_str().ok_or_else(|| {
            issue(
                "OPERATION_ARTIFACTS_INVALID",
                "A saved output path is not text.".into(),
            )
        })?;
        let checked = checked_descendant(root, Path::new(path), false)
            .map_err(|message| issue("OPERATION_ARTIFACT_MISSING", message))?;
        let checked = fs::canonicalize(checked)
            .map_err(|error| issue("OPERATION_ARTIFACT_MISSING", error.to_string()))?;
        if !checked.starts_with(&directory) {
            return Err(issue(
                "OPERATION_ARTIFACTS_INVALID",
                "A saved output belongs to a different run.".into(),
            ));
        }
        let empty_allowed = declared["artifacts"]["allowEmptySuffixes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .any(|suffix| path.ends_with(suffix));
        if !empty_allowed
            && fs::metadata(&checked)
                .map_err(|error| issue("OPERATION_ARTIFACT_MISSING", error.to_string()))?
                .len()
                == 0
        {
            return Err(issue(
                "OPERATION_ARTIFACT_MISSING",
                format!("Saved output {path} is empty."),
            ));
        }
    }
    Ok(())
}
pub(super) fn reuse(
    model: &Value,
    study: &str,
    id: &str,
    document: &Path,
    document_name: &str,
    identity: &ContractIdentity,
) -> Result<Option<(Value, i32)>, Diagnostic> {
    let directory = run_directory(document, id)?;
    if !directory
        .try_exists()
        .map_err(|error| issue("OPERATION_RECORD_INVALID", error.to_string()))?
    {
        return Ok(None);
    }
    let project = document.parent().expect("checked document parent");
    let relative = directory
        .strip_prefix(project)
        .map_err(|error| issue("OPERATION_RECORD_INVALID", error.to_string()))?;
    let directory = checked_descendant(project, relative, true)
        .map_err(|message| issue("OPERATION_RECORD_INVALID", message))?;
    let record = read_json(
        &checked_descendant(&directory, Path::new("run.json"), false)
            .map_err(|message| issue("OPERATION_RECORD_INVALID", message))?,
    )
    .map_err(|message| issue("OPERATION_RECORD_INVALID", message))?;
    let expected = fingerprint(model, study, document_name)
        .map_err(|message| issue("OPERATION_RECORD_INVALID", message))?;
    if record["operationId"].as_str() != Some(id)
        || record["runId"].as_str() != Some(id)
        || record["model"]["file"].as_str() != Some(document_name)
        || record["study"]["id"].as_str() != Some(study)
        || record.get("operationContract") != Some(&identity.as_json())
        || record.get("requestFingerprint") != Some(&expected)
    {
        return Err(issue("OPERATION_ID_CONFLICT","This operation ID already belongs to different model, Study, document, interpreting schema or native contract bytes.".into()));
    }
    let mut event = match record["state"].as_str() {
        Some("completed") => {
            let receipt = checked_descendant(&directory, Path::new(RECEIPT), false)
                .map_err(|message| issue("OPERATION_ARTIFACT_MISSING", message))?;
            let event = read_json(&receipt)
                .map_err(|message| issue("OPERATION_RECORD_INVALID", message))?;
            let root = env::var_os("FULLMETA_ROOT")
                .map(PathBuf::from)
                .ok_or_else(|| {
                    issue(
                        "OPERATION_CONTEXT_INVALID",
                        "The original workspace root is missing.".into(),
                    )
                })?;
            validate_outputs(&root, &directory, &event)?;
            if event["runId"].as_str() != Some(id) {
                return Err(issue(
                    "OPERATION_RECORD_INVALID",
                    "The saved completion belongs to a different run.".into(),
                ));
            }
            event
        }
        Some("cancelled") => {
            json!({"type":"cancelled","state":"cancelled","message":"This operation was already cancelled.","runId":id,"operationId":id})
        }
        Some("failed" | "interrupted") => {
            let diagnostic = issue(
                "OPERATION_PREVIOUSLY_FAILED",
                record["error"]
                    .as_str()
                    .unwrap_or("This operation already terminated unsuccessfully.")
                    .into(),
            );
            fullmeta_operation_contracts::error_event(&[diagnostic])
        }
        _ => {
            let mut diagnostic=issue("OPERATION_ALREADY_RUNNING","This operation already has a run. Observe it through the existing run-status operation instead of launching another worker.".into());
            diagnostic.run_id = Some(id.into());
            return Err(diagnostic);
        }
    };
    event["runId"] = json!(id);
    event["operationId"] = json!(id);
    event["reused"] = json!(true);
    let code = if event["type"] == "complete" { 0 } else { 1 };
    Ok(Some((event, code)))
}
pub(super) fn save_completion(root: &Path, id: &str, event: &Value) -> Result<(), String> {
    let document = env::var_os("FULLMETA_DOCUMENT")
        .map(PathBuf::from)
        .ok_or("The original document is missing from the completion context.")?;
    let directory = run_directory(&document, id).map_err(|error| error.to_string())?;
    let directory = fs::canonicalize(directory).map_err(|error| error.to_string())?;
    validate_outputs(root, &directory, event).map_err(|error| error.to_string())?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join(RECEIPT))
        .map_err(|error| format!("Could not persist native completion receipt: {error}"))?;
    let bytes = serde_json::to_vec(event).map_err(|error| error.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Could not persist native completion receipt: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_retry_identity_includes_unknown_data_schema_and_study() {
        let model = json!({"x-unknown":{"order":[2,1],"flag":false}});
        let initial = fingerprint_for(&model, "study", "models/a.json", "schema.json", b"schema1");
        assert_eq!(
            initial,
            fingerprint_for(&model, "study", "models/a.json", "schema.json", b"schema1")
        );
        assert_ne!(
            initial,
            fingerprint_for(&model, "study", "models/a.json", "schema.json", b"schema2")
        );
        assert_ne!(
            initial,
            fingerprint_for(
                &json!({"x-unknown":{"order":[1,2],"flag":false}}),
                "study",
                "models/a.json",
                "schema.json",
                b"schema1"
            )
        );
        assert_ne!(
            initial,
            fingerprint_for(&model, "other", "models/a.json", "schema.json", b"schema1")
        );
    }
}
