//! Scenario entry points reuse the common FEM preflight executable. The model
//! hash receipt avoids repeating preflight for an unchanged dispatcher input;
//! it is an optimization between local runtime processes, not an authorization.
use serde_json::{Value, json};
use std::{env, io::Write, process::Stdio};

pub(crate) fn validation_report(model: &Value) -> Result<Value, String> {
    let input = serde_json::to_vec(model)
        .map_err(|error| format!("Could not encode the model for preflight: {error}"))?;
    if crate::validation_receipt::matches(model) {
        return Ok(
            json!({"schema":"fullmeta-model-validation/1", "valid":true, "errors":[], "structureErrors":[]}),
        );
    }
    let current = env::current_exe()
        .map_err(|error| format!("Could not locate the scenario executable: {error}"))?;
    let runtime = current
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("The scenario executable name is invalid.")?;
    let dispatcher = crate::model_schema::external_program_path("fem")?;
    let mut command = crate::model_schema::command(&dispatcher)?;
    command
        .args(["--preflight", "-", "--runtime", runtime])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| {
        format!(
            "Could not start common FEM preflight {}: {error}",
            dispatcher.display()
        )
    })?;
    let write_result = child
        .stdin
        .take()
        .ok_or("The common preflight process has no input pipe.")?
        .write_all(&input);
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Could not read common FEM preflight: {error}"))?;
    if let Err(error) = write_result {
        if output.status.success() {
            return Err(format!(
                "Could not send the model to common FEM preflight: {error}"
            ));
        }
    }
    let report: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "Common FEM preflight returned invalid JSON: {error}. {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    })?;
    let valid = report
        .get("valid")
        .and_then(Value::as_bool)
        .ok_or("Common FEM preflight omitted its validation status.")?;
    let strings = |key| {
        report
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().all(Value::is_string))
    };
    if report.get("schema").and_then(Value::as_str) != Some("fullmeta-model-validation/1")
        || !strings("errors")
        || !strings("structureErrors")
        || report["errors"]
            .as_array()
            .is_none_or(|errors| errors.is_empty() != valid)
        || output.status.code() != Some(if valid { 0 } else { 1 })
    {
        return Err("Common FEM preflight returned an inconsistent validation response.".into());
    }
    Ok(report)
}

pub(crate) fn validation_errors(model: &Value) -> Vec<String> {
    match validation_report(model) {
        Ok(report) => report["errors"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Err(error) => vec![error],
    }
}

pub(crate) fn validate(model: &Value) -> Result<(), String> {
    let errors = validation_errors(model);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// The dispatcher owns schema-driven Study preparation. The scenario supplies
/// the model snapshot and selected action and receives its normalized run data.
pub(crate) fn prepare_study(envelope: &Value) -> Result<Value, String> {
    let current = env::current_exe()
        .map_err(|error| format!("Could not locate the scenario executable: {error}"))?;
    let directory = current
        .parent()
        .ok_or("The scenario executable has no runtime directory.")?;
    let dispatcher = crate::model_schema::external_program_path("fem")?;
    let mut command = crate::model_schema::command(&dispatcher)?;
    command
        .arg("--prepare-study")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| {
        format!(
            "Could not start common Study preparation {}: {error}",
            dispatcher.display()
        )
    })?;
    let input = serde_json::to_vec(envelope)
        .map_err(|error| format!("Could not encode Study preparation input: {error}"))?;
    let write_result = child
        .stdin
        .take()
        .ok_or("Study preparation has no input pipe.")?
        .write_all(&input);
    let output = child
        .wait_with_output()
        .map_err(|error| format!("Could not read common Study preparation: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Common Study preparation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    write_result.map_err(|error| format!("Could not send Study preparation input: {error}"))?;
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Common Study preparation returned invalid JSON: {error}"))
}
