//! Runtime-configured launcher for the FEM application.
//!
//! Common model validation and Study preparation read deployed schema declarations.
//! Validation checks declarations without preparing or running a Study. Meshing
//! and scenario programs own run-time checks on generated and numerical data.

mod model_expression;
mod model_schema;
mod model_validation;
mod operation_retry;
mod result_clear;
mod result_resolve;
mod result_save;
mod run_record_guard;
#[path = "../../mesh-rust/src/sha256.rs"]
mod sha256;
mod solver_options;
#[cfg(test)]
mod study_lifecycle_tests;
mod study_prepare;
mod validation_receipt;

use serde_json::{Value, json};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const MODEL_FILE: &str = "fem_model.json";
type Result<T> = std::result::Result<T, String>;

fn application_id_is_valid(value: &str) -> bool {
    let mut characters = value.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && value.len() <= 48
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn model_document_filename_is_valid(name: &str) -> bool {
    if name.len() < 5 || !name.to_ascii_lowercase().ends_with(".json") {
        return false;
    }
    let stem = &name[..name.len() - 5];
    if stem.is_empty()
        || stem.chars().count() > 195
        || stem.starts_with(' ')
        || stem.starts_with('.')
        || name
            .chars()
            .any(|character| character.is_control() || "<>:\"/\\|?*".contains(character))
    {
        return false;
    }
    let device = stem.split('.').next().unwrap_or(stem).to_ascii_uppercase();
    !matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !((device.starts_with("COM") || device.starts_with("LPT"))
            && device.chars().count() == 4
            && matches!(device.chars().last(), Some('1'..='9')))
}

/// Preserve the editor's sibling-document ID guard when native validation is
/// launched with an opened-document context by the generic Kernel command host.
pub(crate) fn sibling_model_id_conflict(
    model: &Value,
    document_path: &Path,
) -> Result<Option<String>> {
    let Some(model_id) = model
        .pointer("/metadata/id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
    else {
        return Ok(None);
    };
    let document_path = fs::canonicalize(document_path)
        .map_err(|error| format!("Could not resolve the opened model document: {error}"))?;
    let directory = document_path
        .parent()
        .ok_or("The opened model document has no parent directory.")?;
    let current_file = document_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("The opened model document has an invalid filename.")?;
    let root_schema = model_schema::root()?;
    let legacy_model_format = root_schema
        .pointer("/properties/modelFormat/const")
        .and_then(Value::as_str);
    let legacy_application = root_schema
        .pointer("/properties/application/const")
        .and_then(Value::as_str);
    let mut conflicts = Vec::new();

    for entry in fs::read_dir(directory)
        .map_err(|error| format!("Could not inspect sibling model documents: {error}"))?
    {
        let entry = entry
            .map_err(|error| format!("Could not inspect a sibling model document: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not inspect a sibling model document: {error}"))?;
        if !file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if !model_document_filename_is_valid(&name) || name.eq_ignore_ascii_case(current_file) {
            continue;
        }
        let sibling = match fs::read(entry.path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        {
            Some(value) => value,
            None => continue,
        };
        let Some(sibling_model_format) = sibling
            .get("modelFormat")
            .and_then(Value::as_str)
            .filter(|format| !format.trim().is_empty())
        else {
            continue;
        };
        let recognized_application = match sibling.get("application") {
            Some(Value::String(application)) => application_id_is_valid(application),
            Some(_) => false,
            None => {
                legacy_model_format == Some(sibling_model_format)
                    && legacy_application.is_some_and(application_id_is_valid)
            }
        };
        if !recognized_application {
            continue;
        }
        let Some(sibling_id) = sibling.pointer("/metadata/id").and_then(Value::as_str) else {
            continue;
        };
        if sibling_id.eq_ignore_ascii_case(model_id) {
            conflicts.push(name);
        }
    }

    if conflicts.is_empty() {
        return Ok(None);
    }
    conflicts.sort_by_key(|name| name.to_ascii_lowercase());
    Ok(Some(format!(
        "Project identity: Model ID \"{model_id}\" is also used by {} in this folder. Give the models separate IDs so their generated data remains distinguishable.",
        conflicts.join(", ")
    )))
}

fn append_report_error(report: &mut Value, error: String) {
    if let Some(errors) = report.get_mut("errors").and_then(Value::as_array_mut) {
        errors.push(json!(error));
    }
    report["valid"] = json!(false);
}

#[cfg(windows)]
fn windows_path_without_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn executable_relative_to_root(root: &Path, executable: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let root = windows_path_without_verbatim_prefix(root);
        let executable = windows_path_without_verbatim_prefix(executable);
        let mut executable_components = executable.components();
        for root_component in root.components() {
            let executable_component = executable_components.next()?;
            if !root_component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&executable_component.as_os_str().to_string_lossy())
            {
                return None;
            }
        }
        let mut relative = PathBuf::new();
        for component in executable_components {
            match component {
                Component::Normal(name) => relative.push(name),
                _ => return None,
            }
        }
        (!relative.as_os_str().is_empty()).then_some(relative)
    }
    #[cfg(not(windows))]
    {
        executable.strip_prefix(root).ok().map(Path::to_path_buf)
    }
}

struct Registry {
    root: PathBuf,
    executable: PathBuf,
    profiles: HashSet<String>,
    capabilities: Value,
    programs: Value,
}

#[cfg(test)]
#[test]
fn missing_preparation_route_names_the_stage_and_selection() {
    let registry = Registry {
        root: PathBuf::new(),
        executable: PathBuf::new(),
        profiles: HashSet::new(),
        capabilities: json!({"study":{"preparationByProfile":{}}}),
        programs: json!({}),
    };
    let model = json!({"scenario":{"profileId":"acoustic-2d"}});
    assert_eq!(registry.require_preparation_route(&model, Some("study")), Err(
        "Profile acoustic-2d, Study study, preparation: preparationByProfile registration is missing.".into()
    ));
}

fn main() {
    let entry = env::args().nth(1).unwrap_or_default();
    if entry == "--describe-operations" {
        println!("{}", fullmeta_operation_contracts::describe(Some("fem")));
        return;
    }
    let contract = match fullmeta_operation_contracts::binding_from_environment("fem", &entry) {
        Ok(value) => value,
        Err(issue) => {
            if entry == "--server" {
                println!("{}", fullmeta_operation_contracts::error_event(&[issue]));
            } else {
                println!("{}", fullmeta_operation_contracts::error_response(&[issue]));
            }
            std::process::exit(1);
        }
    };
    let code = match run(contract.as_ref()) {
        Ok(code) => code,
        Err(error) => {
            if contract.is_some() {
                let issues = [fullmeta_operation_contracts::Diagnostic::new(
                    "NATIVE_OPERATION_FAILED",
                    error.clone(),
                    None,
                    "native",
                )];
                if entry == "--server" {
                    println!("{}", fullmeta_operation_contracts::error_event(&issues));
                } else {
                    println!("{}", fullmeta_operation_contracts::error_response(&issues));
                }
            }
            eprintln!("fullmeta-fem: {error}");
            1
        }
    };
    std::process::exit(code);
}

fn run(operation_contract: Option<&fullmeta_operation_contracts::ContractIdentity>) -> Result<i32> {
    let raw_arguments = env::args().skip(1).collect::<Vec<_>>();
    if raw_arguments.first().map(String::as_str) == Some("--run-status") {
        return read_server_run_status();
    }
    if raw_arguments.first().map(String::as_str) == Some("--cancel-run") {
        return cancel_server_run();
    }
    if raw_arguments.first().map(String::as_str) == Some("--clear-results") {
        return result_clear::clear_from_stdin();
    }
    if raw_arguments.first().map(String::as_str) == Some("--resolve-result") {
        let mut input = Vec::new();
        io::stdin()
            .read_to_end(&mut input)
            .map_err(|error| format!("Could not read result-resolution request: {error}"))?;
        let request: Value = serde_json::from_slice(&input)
            .map_err(|error| format!("Invalid result-resolution request JSON: {error}"))?;
        let response = result_resolve::resolve(&request)?;
        println!(
            "{}",
            serde_json::to_string(&response).map_err(|error| error.to_string())?
        );
        return Ok(0);
    }
    if raw_arguments.first().map(String::as_str) == Some("--save-figure") {
        let mut input = Vec::new();
        io::stdin()
            .read_to_end(&mut input)
            .map_err(|error| format!("Could not read figure-save request: {error}"))?;
        let request: Value = serde_json::from_slice(&input)
            .map_err(|error| format!("Invalid figure-save request JSON: {error}"))?;
        let response = result_save::save(&request)?;
        println!(
            "{}",
            serde_json::to_string(&response).map_err(|error| error.to_string())?
        );
        return Ok(0);
    }
    let command_schema = model_schema::command_schema()?;
    let declaration = command_schema
        .get("x-fullmeta-cli")
        .ok_or("The model schema must declare x-fullmeta-cli.")?;
    let parsed = fullmeta_model::argv::decode(
        &env::args().skip(1).collect::<Vec<_>>(),
        declaration,
        command_schema,
    )?;
    if parsed.help {
        let help = declaration
            .get("help")
            .ok_or("The CLI declaration requires help settings.")?;
        println!(
            "{}",
            help.get("text")
                .and_then(Value::as_str)
                .ok_or("The CLI declaration requires help text.")?
        );
        return help
            .get(if parsed.empty {
                "emptyExitCode"
            } else {
                "exitCode"
            })
            .and_then(Value::as_i64)
            .and_then(|code| i32::try_from(code).ok())
            .ok_or("The CLI declaration requires an integer help exit status.".into());
    }
    let arguments = &parsed.arguments;
    let scenario = parsed.globals.get("scenario").and_then(Value::as_str);
    match parsed.mode.as_str() {
        "submittedValidation" | "submittedPreflight" => {
            let runtime = parsed.values.get("runtime").and_then(Value::as_str);
            let mut input = Vec::new();
            io::stdin()
                .read_to_end(&mut input)
                .map_err(|error| format!("Could not read submitted model input: {error}"))?;
            let model: Value = serde_json::from_slice(&input)
                .map_err(|error| format!("Invalid submitted model JSON: {error}"))?;
            let registry = Registry::load()?;
            if let Some(profile) = scenario {
                registry.require_registration(profile)?;
            }
            let report = registry.validation_report(&model, scenario, runtime);
            let valid = report.get("valid").and_then(Value::as_bool) == Some(true);
            println!("{report}");
            Ok(if valid { 0 } else { 1 })
        }
        "capabilities" => {
            let registry = Registry::load()?;
            let capabilities = registry.capabilities;
            println!(
                "{}",
                json!({
                    "schema": "fullmeta-solver-runtime/1",
                    "solverConfigurationSchema": capabilities["study"]["requestConfigurationSchema"],
                    "solverRequestSchema": capabilities["study"]["solverRequestSchema"],
                    "engineCapabilities": capabilities,
                })
            );
            Ok(0)
        }
        "preparation" | "server" => {
            let mut input = Vec::new();
            io::stdin()
                .read_to_end(&mut input)
                .map_err(|error| format!("Could not read {} input: {error}", arguments[0]))?;
            let value: Value = serde_json::from_slice(&input)
                .map_err(|error| format!("Invalid {} input JSON: {error}", arguments[0]))?;
            if let Some(contract) = operation_contract {
                if let Err(issues) =
                    fullmeta_operation_contracts::validate_input(&contract.id, &value)
                {
                    if parsed.mode == "server" {
                        println!("{}", fullmeta_operation_contracts::error_event(&issues));
                    } else {
                        println!("{}", fullmeta_operation_contracts::error_response(&issues));
                    }
                    return Ok(1);
                }
            }
            let model = if parsed.mode == "preparation" {
                value
                    .get("model")
                    .ok_or("Study preparation input requires model.")?
            } else {
                &value
            };
            let requested_study = if parsed.mode == "preparation" {
                Some(
                    value
                        .get("studyId")
                        .and_then(Value::as_str)
                        .ok_or("Could not read the Study preparation studyId.")?
                        .to_string(),
                )
            } else if parsed.mode == "server" {
                env::var("FULLMETA_STUDY_ID")
                    .ok()
                    .filter(|id| !id.trim().is_empty())
            } else {
                None
            };
            let study_id = requested_study.as_deref();
            let server_run = if parsed.mode == "server"
                && env::var_os("FULLMETA_OPERATION_ID").is_some()
            {
                let operation_id = env::var("FULLMETA_OPERATION_ID")
                    .map_err(|_| "The Study operation ID is invalid.")?;
                if !checked_run_id(&operation_id) {
                    return Err("The Study operation ID is invalid.".into());
                }
                let folder = env::var("FULLMETA_MODEL_FOLDER")
                    .map_err(|_| "The Study model folder is missing.")?;
                let file = env::var("FULLMETA_MODEL_FILE")
                    .map_err(|_| "The Study model filename is missing.")?;
                let document = checked_open_document(&folder, &file)?;
                let saved = read_json(&document)?;
                if !same_json_value(&saved, model) {
                    println!(
                        "{}",
                        json!({"type":"error", "state":"error", "code":"STUDY_SNAPSHOT_CONFLICT",
                        "stage":"preflight", "operationId":operation_id,
                        "message":"The saved model changed before Study dispatch. Reopen the model, review the changes, and run again."})
                    );
                    return Ok(1);
                }
                verify_document_schema(&document, &saved)?;
                if let Some(contract) = operation_contract {
                    match operation_retry::reuse(
                        model,
                        study_id.ok_or("The Study request has no selected Study ID.")?,
                        &operation_id,
                        &document,
                        &workspace_document_path(&folder, &file),
                        contract,
                    ) {
                        Ok(Some((event, code))) => {
                            println!("{event}");
                            return Ok(code);
                        }
                        Ok(None) => {}
                        Err(issue) => {
                            println!("{}", fullmeta_operation_contracts::error_event(&[issue]));
                            return Ok(1);
                        }
                    }
                }
                Some(ServerRun::begin(
                    model,
                    study_id.ok_or("The Study request has no selected Study ID.")?,
                    operation_id,
                    &document,
                    &workspace_document_path(&folder, &file),
                    operation_contract,
                )?)
            } else {
                None
            };
            let execution = (|| {
                let registry = Registry::load()?;
                if let Some(profile) = scenario {
                    registry.require_registration(profile)?;
                }
                registry.require_valid_model(model, scenario)?;
                registry.require_preparation_route(model, study_id)?;
                if parsed.mode == "preparation" {
                    model_validation::validate_preparation_request(
                        &value,
                        &json!({"selectedScenario":scenario}),
                    )?;
                }
                if parsed.mode == "preparation"
                    && study_prepare::uses_common_preparation(model, &registry.capabilities)?
                {
                    let prepared =
                        study_prepare::prepare_validated_study(&value, &registry.capabilities)?;
                    if let Some(contract) = operation_contract {
                        if let Err(issues) =
                            fullmeta_operation_contracts::validate_result(&contract.id, &prepared)
                        {
                            println!("{}", fullmeta_operation_contracts::error_response(&issues));
                            return Ok(1);
                        }
                    }
                    println!("{prepared}");
                    return Ok(0);
                }
                registry.require_valid_dispatch(model, scenario, study_id)?;
                let executable = registry.scenario_executable(model, study_id)?;
                launch_preflighted(
                    &registry.root,
                    &executable,
                    arguments,
                    Some(&input),
                    model,
                    server_run.as_ref().map(|run| run.id.as_str()),
                )
            })();
            let reported_failure =
                operation_contract.is_some() && server_run.is_some() && execution.is_err();
            if let Some(run) = server_run {
                match &execution {
                    Err(error) => {
                        run.fail(error)?;
                        if operation_contract.is_some() {
                            run_record_guard::reject_completed_verification(&run.path, error)?;
                        }
                        if operation_contract.is_some() {
                            let mut issue = fullmeta_operation_contracts::Diagnostic::new(
                                "STUDY_EXECUTION_FAILED",
                                error.clone(),
                                None,
                                "study",
                            );
                            issue.run_id = Some(run.id.clone());
                            let mut event = fullmeta_operation_contracts::error_event(&[issue]);
                            event["runId"] = json!(run.id);
                            println!("{event}");
                        } else {
                            println!(
                                "{}",
                                json!({"type":"error","state":"error","runId":run.id,"message":error})
                            );
                        }
                    }
                    Ok(code) if *code != 0 => {
                        run.fail("The scenario process exited before completing this Study.")?
                    }
                    _ => {}
                }
            }
            if reported_failure {
                return Ok(1);
            }
            execution
        }
        "project" => {
            let options = project_options(&parsed)?;
            let runtime_root = runtime_root()?;
            let project = safe_project_directory(&runtime_root, &options.folder)?;
            let model_path = project.join(MODEL_FILE);
            let metadata = fs::symlink_metadata(&model_path)
                .map_err(|error| format!("Could not read {}: {error}", model_path.display()))?;
            if is_link(&metadata) || !metadata.is_file() {
                return Err(
                    "The model manifest must be a regular file directly in its project folder."
                        .into(),
                );
            }
            let model = read_json(&model_path)?;
            select_schema_for_model_file(&model, &model_path)?;
            let registry = Registry::load()?;
            if let Some(profile) = scenario {
                registry.require_registration(profile)?;
            }
            let report = registry.validation_report_at(&model, scenario, None, Some(&model_path));
            let valid = report.get("valid").and_then(Value::as_bool) == Some(true);
            if options.validate_only {
                if valid {
                    println!("Model validation passed for {}.", options.folder);
                } else {
                    print_validation_errors(&report);
                }
                return Ok(if valid { 0 } else { 1 });
            }
            require_valid_report(&report)?;
            registry.require_preparation_route(&model, options.study_id.as_deref())?;
            registry.require_valid_dispatch(&model, scenario, options.study_id.as_deref())?;
            let executable = registry.scenario_executable(&model, options.study_id.as_deref())?;
            launch_preflighted(&registry.root, &executable, arguments, None, &model, None)
        }
        mode => Err(format!(
            "The CLI declaration selects an unknown dispatcher action {mode:?}."
        )),
    }
}

struct ServerRun {
    id: String,
    path: PathBuf,
}

impl ServerRun {
    fn begin(
        model: &Value,
        study_id: &str,
        id: String,
        document: &Path,
        document_name: &str,
        operation_contract: Option<&fullmeta_operation_contracts::ContractIdentity>,
    ) -> Result<Self> {
        let project = document
            .parent()
            .ok_or("The Study document has no containing folder.")?;
        let runs_name =
            env::var("FULLMETA_RUNS_DIRECTORY").map_err(|_| "The Study run storage is missing.")?;
        if !checked_run_id(&runs_name) {
            return Err("The Study run-storage name is invalid.".into());
        }
        match fs::create_dir(project.join(&runs_name)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("Could not create run storage: {error}")),
        }
        let runs = checked_descendant(project, Path::new(&runs_name), true)?;
        let directory = runs.join(&id);
        fs::create_dir(&directory).map_err(|error| {
            format!(
                "Could not create Study run {}: {error}",
                directory.display()
            )
        })?;
        let path = directory.join("run.json");
        let study = model
            .pointer("/model/study")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|study| study.get("id").and_then(Value::as_str) == Some(study_id));
        let pid = std::process::id();
        let worker_started = process_identity(pid).ok().flatten();
        let stage = "preparation";
        let snapshot_bytes = serde_json::to_vec(model).map_err(|error| error.to_string())?;
        let mut record = json!({
            "schema":"fullmeta-run/1", "kind":"study", "runId":id, "operationId":id,
            "state":"running", "startedAt":utc_timestamp(), "completedAt":null,
            "model":{"file":document_name,"id":model.pointer("/metadata/id"),"snapshot":model,
                "sha256":sha256::hex_digest(&snapshot_bytes)},
            "study":{"id":study_id,"name":study.and_then(|item| item.get("name")),"snapshot":study},
            "progress":{"stage":stage,"completedPoints":0,"totalPoints":null},"artifacts":{}, "error":null,
        });
        if let Some(contract) = operation_contract {
            record["operationContract"] = contract.as_json();
            record["requestFingerprint"] =
                operation_retry::fingerprint(model, study_id, document_name)?;
        }
        if let Some(fingerprint) = result_resolve::study_input_fingerprint(model, study_id) {
            record["study"]["inputFingerprint"] = fingerprint;
        }
        if let Some(started) = worker_started {
            record["worker"] = json!({"pid":pid,"started":started});
        }
        let mut manifest = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| format!("Could not create the Study run manifest: {error}"))?;
        manifest
            .write_all(
                serde_json::to_string_pretty(&record)
                    .map_err(|error| error.to_string())?
                    .as_bytes(),
            )
            .and_then(|_| manifest.sync_all())
            .map_err(|error| format!("Could not write the Study run manifest: {error}"))?;
        println!(
            "{}",
            json!({"type":"start","state":"running","stage":stage,"runId":id,"operationId":id,
            "message":"Preparing the Study…","completedPoints":0,"totalPoints":null})
        );
        io::stdout().flush().map_err(|error| error.to_string())?;
        Ok(Self { id, path })
    }

    fn fail(&self, diagnostic: &str) -> Result<()> {
        run_record_guard::update_running(&self.path, |record| {
            record["state"] = json!("failed");
            record["completedAt"] = json!(utc_timestamp());
            record["error"] = json!(diagnostic);
            Ok(true)
        })?;
        Ok(())
    }
}

fn same_json_value(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right
                        .get(key)
                        .is_some_and(|other| same_json_value(value, other))
                })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(value, other)| same_json_value(value, other))
        }
        (Value::Number(left), Value::Number(right)) if left.is_f64() != right.is_f64() => {
            let (fractional, integer) = if left.is_f64() {
                (left, right)
            } else {
                (right, left)
            };
            let value = fractional.as_f64().unwrap();
            let exact = integer
                .as_i64()
                .map(i128::from)
                .or_else(|| integer.as_u64().map(i128::from));
            value.fract() == 0.0 && exact == Some(value as i128)
        }
        _ => left == right,
    }
}

fn checked_run_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn workspace_document_path(folder: &str, file: &str) -> String {
    if folder == "." {
        file.to_string()
    } else {
        format!("{folder}/{file}")
    }
}

fn checked_document_path(folder: &str, file: &str) -> Result<PathBuf> {
    if !model_document_filename_is_valid(file) {
        return Err("The Study model filename is invalid.".into());
    }
    let root = runtime_root()?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the application root: {error}"))?;
    let requested = workspace_document_path(folder, file);
    let document = checked_descendant(&root, Path::new(&requested), false)?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the Study model file: {error}"))?;
    let actual = document
        .strip_prefix(&root)
        .map_err(|_| "The Study model is outside the application root.")?;
    if actual.to_string_lossy().replace('\\', "/") != requested {
        return Err(
            "The Study model path must use its canonical workspace-relative folder and filename."
                .into(),
        );
    }
    Ok(document)
}

fn checked_open_document(folder: &str, file: &str) -> Result<PathBuf> {
    let expected = checked_document_path(folder, file)?;
    let opened = env::var_os("FULLMETA_DOCUMENT")
        .map(PathBuf::from)
        .ok_or("The Kernel did not provide the opened document.")?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the opened document: {error}"))?;
    if expected != opened {
        return Err("The Study context differs from the opened document.".into());
    }
    Ok(opened)
}

fn checked_status_document(folder: &str, file: &str) -> Result<PathBuf> {
    // Status uses the explicitly checked document in its request so it can
    // observe a Study while the Kernel's document execution queue is held.
    let document = if env::var_os("FULLMETA_DOCUMENT").is_some() {
        checked_open_document(folder, file)?
    } else {
        checked_document_path(folder, file)?
    };
    verify_document_schema(&document, &read_json(&document)?)?;
    Ok(document)
}

fn verify_document_schema(document: &Path, saved_model: &Value) -> Result<()> {
    let selected = model_schema::runtime_model_schema_path()?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the registered model schema: {error}"))?;
    let schema = read_json(&selected)?;
    if saved_model.get("application") != schema.pointer("/properties/application/const") {
        return Err("The Study document does not belong to the selected application.".into());
    }
    let actual = declared_schema_for_model_file(saved_model, document)?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the document's model schema: {error}"))?;
    if actual != selected {
        return Err("The Study document selects a different schema.".into());
    }
    Ok(())
}

struct RunStatusQuery<'a> {
    document: &'a str,
    run_id: Option<&'a str>,
    study_id: Option<&'a str>,
    operation_id: Option<&'a str>,
    terminal_only: bool,
}

impl RunStatusQuery<'_> {
    fn matches(&self, run_id: &str, record: &Value) -> bool {
        let state = record.get("state").and_then(Value::as_str);
        let terminal = run_record_guard::is_terminal(state);
        let operation = record.get("operationId").and_then(Value::as_str);
        record.get("schema").and_then(Value::as_str) == Some("fullmeta-run/1")
            && record.get("kind").and_then(Value::as_str) == Some("study")
            && record.get("runId").and_then(Value::as_str) == Some(run_id)
            && record.pointer("/model/file").and_then(Value::as_str) == Some(self.document)
            && record
                .pointer("/study/id")
                .and_then(Value::as_str)
                .is_some_and(checked_run_id)
            && self.run_id.is_none_or(|id| id == run_id)
            && self
                .study_id
                .is_none_or(|id| record.pointer("/study/id").and_then(Value::as_str) == Some(id))
            && self.operation_id.is_none_or(|id| operation == Some(id))
            && if self.terminal_only {
                terminal
            } else {
                operation.is_some_and(checked_run_id) && (terminal || state == Some("running"))
            }
    }
}

fn utc_sort_key(value: &Value) -> Option<(&str, &str)> {
    let text = value.as_str()?.strip_suffix('Z')?;
    let (base, fraction) = text.split_once('.').unwrap_or((text, ""));
    if base.len() != 19
        || !base.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            10 => byte == b'T',
            13 | 16 => byte == b':',
            _ => byte.is_ascii_digit(),
        })
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some((base, fraction.trim_end_matches('0')))
}

fn status_rank(record: &Value) -> (bool, Option<(&str, &str)>, &str) {
    (
        record["state"].as_str() == Some("running"),
        utc_sort_key(&record["startedAt"]),
        record["runId"].as_str().unwrap_or(""),
    )
}

fn find_status_record(
    project: &Path,
    runs: &str,
    query: &RunStatusQuery<'_>,
) -> Result<Option<(PathBuf, Value)>> {
    match fs::symlink_metadata(project.join(runs)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect the Study run storage: {error}")),
        Ok(_) => {}
    }
    let directory = checked_descendant(project, Path::new(runs), true)?;
    if let Some(id) = query.run_id {
        let relative = Path::new(id).join("run.json");
        match fs::symlink_metadata(directory.join(&relative)) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Could not inspect the Study run record: {error}")),
        }
        let path = checked_descendant(&directory, &relative, false)?;
        let record = read_json(&path)?;
        return Ok(query.matches(id, &record).then_some((path, record)));
    }
    let mut newest: Option<(PathBuf, Value)> = None;
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("Could not list the Study run storage: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not inspect a Study run: {error}"))?;
        let filename = entry.file_name();
        let Some(id) = filename.to_str().filter(|id| checked_run_id(id)) else {
            continue;
        };
        let Ok(path) = checked_descendant(&directory, &Path::new(id).join("run.json"), false)
        else {
            continue;
        };
        let Ok(record) = read_json(&path) else {
            continue;
        };
        if query.matches(id, &record)
            && newest
                .as_ref()
                .is_none_or(|(_, old)| status_rank(&record) > status_rank(old))
        {
            newest = Some((path, record));
        }
    }
    Ok(newest)
}

fn read_server_run_status() -> Result<i32> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| error.to_string())?;
    let request: Value = serde_json::from_slice(&input)
        .map_err(|error| format!("Invalid Study status request: {error}"))?;
    let request = request
        .as_object()
        .ok_or("Study status requires a JSON object.")?;
    let required = |key: &str| {
        request
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("Study status requires {key}."))
    };
    let identifier = |key: &str| {
        request
            .get(key)
            .map(|value| {
                value
                    .as_str()
                    .filter(|value| checked_run_id(value))
                    .ok_or_else(|| format!("Study status identifies an invalid {key}."))
            })
            .transpose()
    };
    let flag = |key: &str| {
        request
            .get(key)
            .map(|value| {
                value
                    .as_bool()
                    .ok_or_else(|| format!("Study status {key} must be a boolean."))
            })
            .transpose()
            .map(|value| value.unwrap_or(false))
    };
    let document = checked_status_document(required("folder")?, required("file")?)?;
    let runs = env::var("FULLMETA_STORAGE_RUNS_DIRECTORY")
        .map_err(|_| "Study status has no run storage.")?;
    if !checked_run_id(&runs)
        || model_schema::root()?
            .pointer("/x-fullmeta-runtime/storage/runsDirectory")
            .and_then(Value::as_str)
            != Some(runs.as_str())
    {
        return Err("Study status identifies invalid run storage.".into());
    }
    let project = document
        .parent()
        .ok_or("The document has no parent folder.")?;
    let document_path = workspace_document_path(required("folder")?, required("file")?);
    let query = RunStatusQuery {
        document: &document_path,
        run_id: identifier("runId")?,
        study_id: identifier("studyId")?,
        operation_id: identifier("operationId")?,
        terminal_only: flag("terminalOnly")?,
    };
    let reconcile = flag("reconcile")?;
    if query.terminal_only && (reconcile || query.study_id.is_none()) {
        return Err(
            "Terminal-only Study status requires a Study ID and read-only observation.".into(),
        );
    }
    if reconcile
        && (query.run_id.is_none() || query.study_id.is_none() || query.operation_id.is_none())
    {
        return Err(
            "Study reconciliation requires the run, Study, and operation IDs from observation."
                .into(),
        );
    }
    println!("{}", status_reply(project, &runs, &query, reconcile)?);
    Ok(0)
}

fn status_reply(
    project: &Path,
    runs: &str,
    query: &RunStatusQuery<'_>,
    reconcile: bool,
) -> Result<Value> {
    let Some((path, mut record)) = find_status_record(project, runs, query)? else {
        return Ok(json!({"available":false}));
    };
    if reconcile && run_liveness(&record) == "gone" {
        let (updated, transitioned) = reconcile_gone_run(&path, query)?;
        if transitioned {
            release_stale_project_run_lock(project, updated["runId"].as_str().unwrap_or(""));
        }
        record = updated;
    }
    Ok(
        json!({"available":true,"runId":record["runId"],"studyId":record["study"]["id"],
        "studyName":record["study"]["name"],"operationId":record["operationId"],
        "state":record["state"],"error":record["error"],"progress":record["progress"],
        "startedAt":record["startedAt"],"completedAt":record["completedAt"],"liveness":run_liveness(&record)}),
    )
}

fn run_liveness(record: &Value) -> &'static str {
    if run_record_guard::is_terminal(record.get("state").and_then(Value::as_str)) {
        return "finished";
    }
    let pid = record
        .pointer("/worker/pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid != 0);
    let started = record
        .pointer("/worker/started")
        .and_then(Value::as_u64)
        .filter(|started| *started != 0);
    match (pid, started) {
        (Some(pid), Some(started)) => match process_identity(pid) {
            Ok(Some(observed)) if observed == started => "active",
            Ok(_) => "gone",
            Err(()) => "unknown",
        },
        _ => "unknown",
    }
}

fn reconcile_gone_run(path: &Path, query: &RunStatusQuery<'_>) -> Result<(Value, bool)> {
    let id = query
        .run_id
        .ok_or("Study reconciliation requires a pinned run ID.")?;
    let (record, updated) = run_record_guard::update_running(path, |record| {
        if !query.matches(id, record) || run_liveness(record) != "gone" {
            return Ok(false);
        }
        record["state"] = json!("interrupted");
        record["completedAt"] = json!(utc_timestamp());
        record["error"] = json!("The Study worker exited before a terminal result was recorded.");
        Ok(true)
    })?;
    if !query.matches(id, &record) {
        return Err("The Study identity changed during reconciliation.".into());
    }
    Ok((record, updated))
}

#[cfg(windows)]
fn process_identity(pid: u32) -> std::result::Result<Option<u64>, ()> {
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit_handle: i32, pid: u32) -> *mut c_void;
        fn GetProcessTimes(
            handle: *mut c_void,
            created: *mut FileTime,
            exited: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    // Request only synchronization and basic process metadata.
    let handle = unsafe { OpenProcess(0x0010_1000, 0, pid) };
    if handle.is_null() {
        return if io::Error::last_os_error().raw_os_error() == Some(87) {
            Ok(None)
        } else {
            Err(())
        };
    }
    let (mut created, mut exited, mut kernel, mut user) = (
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
        FileTime::default(),
    );
    let state = unsafe { WaitForSingleObject(handle, 0) };
    let got_times =
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    unsafe {
        CloseHandle(handle);
    }
    match state {
        0 => Ok(None),
        258 if got_times != 0 => Ok(Some(
            (u64::from(created.high) << 32) | u64::from(created.low),
        )),
        _ => Err(()),
    }
}

#[cfg(target_os = "linux")]
fn process_identity(pid: u32) -> std::result::Result<Option<u64>, ()> {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => {
            let mut fields = stat.rsplit_once(") ").ok_or(())?.1.split_ascii_whitespace();
            match fields.next() {
                Some("Z" | "X") => Ok(None),
                Some(_) => fields
                    .nth(18)
                    .and_then(|field| field.parse().ok())
                    .map(Some)
                    .ok_or(()),
                None => Err(()),
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
fn process_identity(_pid: u32) -> std::result::Result<Option<u64>, ()> {
    Err(())
}

fn cancel_server_run() -> Result<i32> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| format!("Could not read Study cancellation request: {error}"))?;
    let request: Value = serde_json::from_slice(&input)
        .map_err(|error| format!("Invalid Study cancellation request JSON: {error}"))?;
    let run_id = request
        .get("runId")
        .and_then(Value::as_str)
        .filter(|value| checked_run_id(value))
        .ok_or("Study cancellation requires a valid run ID.")?;
    let folder = env::var("FULLMETA_MODEL_FOLDER")
        .map_err(|_| "The cancellation request has no model folder context.")?;
    let file = env::var("FULLMETA_MODEL_FILE")
        .map_err(|_| "The cancellation request has no model filename context.")?;
    let operation_id = env::var("FULLMETA_OPERATION_ID")
        .map_err(|_| "The cancellation request has no operation context.")?;
    let study_id = env::var("FULLMETA_STUDY_ID")
        .map_err(|_| "The cancellation request has no Study context.")?;
    let document_name = workspace_document_path(&folder, &file);
    let document = checked_open_document(&folder, &file)?;
    verify_document_schema(&document, &read_json(&document)?)?;
    let runs_directory = env::var("FULLMETA_RUNS_DIRECTORY")
        .map_err(|_| "The cancellation request has no run-storage context.")?;
    if runs_directory.is_empty()
        || runs_directory.len() > 64
        || !runs_directory
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err("The run-storage directory name is invalid.".into());
    }
    let root = runtime_root()?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the application root: {error}"))?;
    let configured_root = env::var_os("FULLMETA_ROOT")
        .map(PathBuf::from)
        .ok_or("The Kernel did not provide the application root.")?;
    if configured_root
        .canonicalize()
        .map_err(|error| format!("Could not resolve the application root: {error}"))?
        != root
    {
        return Err("The cancellation request identifies a different application root.".into());
    }
    let project = document
        .parent()
        .ok_or("The cancellation document has no containing folder.")?;
    let relative_run_directory = Path::new(&runs_directory).join(run_id);
    match fs::symlink_metadata(project.join(&relative_run_directory)) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("Could not inspect the Study run folder: {error}")),
    }
    let run_directory = checked_descendant(project, &relative_run_directory, true)
        .map_err(|error| format!("The Study run folder is unavailable: {error}"))?;
    let record_path = match fs::symlink_metadata(run_directory.join("run.json")) {
        Ok(_) => checked_descendant(&run_directory, Path::new("run.json"), false)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(format!("Could not inspect the Study run record: {error}")),
    };
    let query = RunStatusQuery {
        document: &document_name,
        run_id: Some(run_id),
        study_id: Some(&study_id),
        operation_id: Some(&operation_id),
        terminal_only: false,
    };
    let (record, updated) = run_record_guard::update_running(&record_path, |record| {
        if !query.matches(run_id, record) {
            return Ok(false);
        }
        record["state"] = json!("cancelled");
        record["completedAt"] = json!(utc_timestamp());
        record["error"] = json!("Study stopped by user.");
        if let Some(progress) = record.get_mut("progress").and_then(Value::as_object_mut) {
            for key in ["currentFrequencyGHz", "currentFrequencyHz"] {
                if let Some(value) = progress.get_mut(key) {
                    *value = Value::Null;
                }
            }
        }
        Ok(true)
    })?;
    if !query.matches(run_id, &record) {
        return Err("The selected run record does not match the cancellation request.".into());
    }
    if updated {
        release_stale_project_run_lock(project, run_id);
    }
    println!(
        "{}",
        json!({"runId":run_id,"operationId":operation_id,"state":record["state"],"changed":updated})
    );
    Ok(0)
}

fn release_stale_project_run_lock(project: &Path, run_id: &str) {
    let path = project.join(".fullmeta-run.lock");
    if !fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file() && !is_link(&metadata))
    {
        return;
    }
    let Ok(previous) = fs::read_to_string(&path) else {
        return;
    };
    if !previous
        .lines()
        .any(|line| line.strip_prefix("runId=") == Some(run_id))
    {
        return;
    }
    let Some(pid) = previous.lines().find_map(|line| {
        line.strip_prefix("pid=")?
            .split_whitespace()
            .next()?
            .parse::<u32>()
            .ok()
    }) else {
        return;
    };
    if matches!(process_identity(pid), Ok(None))
        && fs::read_to_string(&path)
            .as_ref()
            .is_ok_and(|current| current == &previous)
    {
        let _ = fs::remove_file(path);
    }
}

fn utc_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (elapsed.as_secs() / 86_400) as i64;
    let seconds = elapsed.as_secs() % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        elapsed.subsec_millis()
    )
}

impl Registry {
    fn require_preparation_route(&self, model: &Value, study_id: Option<&str>) -> Result<()> {
        let profile = model_validation::declared_profile(model)?;
        let context = format!(
            "Profile {profile}, Study {}, preparation",
            study_id.unwrap_or("(default)")
        );
        let preparation = self
            .capabilities
            .pointer("/study/preparationByProfile")
            .and_then(|profiles| profiles.get(profile))
            .ok_or_else(|| format!("{context}: preparationByProfile registration is missing."))?;
        match preparation.get("kind").and_then(Value::as_str) {
            Some("frequencyPlan" | "configuredFrequencyRequest") => Ok(()),
            Some("scenario") => {
                let id = preparation
                    .get("programId")
                    .and_then(Value::as_str)
                    .filter(|id| *id != "fem")
                    .ok_or_else(|| {
                        format!(
                            "{context}: scenario preparation must name a non-dispatcher programId."
                        )
                    })?;
                let entry = preparation
                    .get("entry")
                    .and_then(Value::as_str)
                    .filter(|entry| *entry == "--prepare-study")
                    .ok_or_else(|| format!("{context}: missing --prepare-study entry."))?;
                if !self
                    .programs
                    .get(id)
                    .and_then(|program| program.get("allowedEntries"))
                    .and_then(Value::as_array)
                    .is_some_and(|entries| {
                        entries.iter().any(|value| value.as_str() == Some(entry))
                    })
                {
                    return Err(format!("{context}: program {id} does not allow {entry}."));
                }
                let program = model_schema::external_program_path(id)?;
                if program != self.scenario_executable(model, study_id)? {
                    return Err(format!(
                        "{context}: {id} must resolve to the selected scenario executable."
                    ));
                }
                Ok(())
            }
            other => Err(format!(
                "{context}: unsupported preparation kind {other:?}."
            )),
        }
    }

    fn validation_report(
        &self,
        model: &Value,
        selected: Option<&str>,
        runtime: Option<&str>,
    ) -> Value {
        let document_path = env::var_os("FULLMETA_DOCUMENT").map(PathBuf::from);
        self.validation_report_at(model, selected, runtime, document_path.as_deref())
    }

    fn validation_report_at(
        &self,
        model: &Value,
        selected: Option<&str>,
        runtime: Option<&str>,
        document_path: Option<&Path>,
    ) -> Value {
        let normalize = |name: &str| {
            executable_name(name.rsplit('/').next().unwrap_or(name)).map(|name| {
                if cfg!(windows) {
                    name.to_ascii_lowercase()
                } else {
                    name
                }
            })
        };
        let runtimes = self
            .capabilities
            .pointer("/study/solverRuntimesByProfile")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(profile, backends)| {
                let names = backends
                    .as_object()
                    .into_iter()
                    .flatten()
                    .filter_map(|(_, backend)| {
                        backend
                            .get("programId")
                            .and_then(Value::as_str)
                            .and_then(|id| self.programs.get(id))
                            .and_then(|program| program.get("executable"))
                            .and_then(Value::as_str)
                            .and_then(|name| normalize(name).ok())
                    })
                    .collect::<Vec<_>>();
                (profile.clone(), json!(names))
            })
            .collect::<serde_json::Map<_, _>>();
        let mut report = model_validation::validation_report_with_context(
            model,
            &json!({
                "selectedScenario":selected,
                "runtimeName":runtime.map(|name|normalize(name).unwrap_or_else(|_|name.to_string())),
                "registeredProfiles":self.profiles.iter().collect::<Vec<_>>(),
                "runtimeExecutablesByProfile":runtimes,
            }),
        );
        if let Some(document_path) = document_path {
            match sibling_model_id_conflict(model, document_path) {
                Ok(Some(error)) => append_report_error(&mut report, error),
                Ok(None) => {}
                Err(error) => {
                    append_report_error(&mut report, format!("Project identity: {error}"))
                }
            }
        }
        report
    }

    fn require_valid_model(&self, model: &Value, selected: Option<&str>) -> Result<()> {
        // The parent dispatcher passes this exact-model receipt only after its
        // model validation succeeds. The Study-preparation child still checks
        // its operation envelope below, while avoiding a second full model
        // validation for the same unchanged JSON.
        if validation_receipt::matches(model) {
            return Ok(());
        }
        require_valid_report(&self.validation_report(model, selected, None))
    }

    fn require_valid_dispatch(
        &self,
        model: &Value,
        selected: Option<&str>,
        study_id: Option<&str>,
    ) -> Result<()> {
        let profile = selected
            .map(Ok)
            .unwrap_or_else(|| model_validation::declared_profile(model))?;
        let mut input = json!({
            "profileId":profile,
            "registeredProfiles":self.profiles.iter().collect::<Vec<_>>(),
            "runtimes":self.capabilities.pointer("/study/solverRuntimesByProfile"),
            "model":model,
            "requireStudy":true,
        });
        if let Some(study_id) = study_id {
            input["studyId"] = json!(study_id);
        }
        model_schema::validate_declaration("x-fullmeta-dispatchValidation", &input)
    }

    fn load() -> Result<Self> {
        let executable = env::current_exe()
            .map_err(|error| format!("Could not locate the dispatcher executable: {error}"))?;
        let root = runtime_root()?;
        // current_exe supplies an absolute installation path. Checking each
        // descendant avoids final-path resolution, which protected Windows
        // installations may deny even when normal file access is permitted.
        let root = root.to_path_buf();
        let executable_relative = executable_relative_to_root(&root, &executable)
            .ok_or("The dispatcher executable must remain inside the FullMeta application root.")?;
        checked_descendant(&root, &executable_relative, false)?;
        // Embedded profile schemas and the root's optional $ref graph are the
        // complete registry, even when the model lives outside the installation.
        // The server supplies the resolved x-fullmeta-schema path.
        let documents = model_schema::context()?.documents().to_vec();
        let mut capabilities = json!({"schema": "fem-engine-capabilities/2"});
        for document in &documents {
            if let Some(declared) = document.get("x-fem-capabilities") {
                merge_capabilities(&mut capabilities, declared.clone());
            }
        }
        let programs = fullmeta_model::runtime_paths::program_registry(&documents)?;
        model_schema::validate_declaration(
            "x-fullmeta-registrationValidation",
            &json!({
                "documents":documents,"capabilities":capabilities,"externalPrograms":programs,
            }),
        )?;
        let profiles = documents
            .iter()
            .filter_map(|document| {
                document
                    .pointer("/x-fem-scenario/profile/id")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect::<HashSet<_>>();
        Ok(Self {
            root,
            executable,
            profiles,
            capabilities,
            programs,
        })
    }

    fn require_registration(&self, profile: &str) -> Result<()> {
        model_schema::validate_declaration(
            "x-fullmeta-dispatchValidation",
            &json!({
                "profileId":profile,
                "registeredProfiles":self.profiles.iter().collect::<Vec<_>>(),
                "runtimes":self.capabilities.pointer("/study/solverRuntimesByProfile"),
            }),
        )
    }

    fn scenario_executable(&self, model: &Value, requested_study: Option<&str>) -> Result<PathBuf> {
        let profile = model_validation::declared_profile(model)?;
        let studies = model.pointer("/model/study").and_then(Value::as_array);
        let study = if let Some(id) = requested_study {
            Some(
                studies
                    .and_then(|studies| {
                        studies
                            .iter()
                            .find(|study| study.get("id").and_then(Value::as_str) == Some(id))
                    })
                    .ok_or_else(|| format!("Could not read the validated Study {id:?}."))?,
            )
        } else {
            studies.and_then(|studies| studies.first())
        };
        let profile_settings = self
            .capabilities
            .pointer("/study/solverSettingsByProfile")
            .and_then(|profiles| profiles.get(profile));
        let backend = study.and_then(|study| study.pointer("/solver/backend")).and_then(Value::as_str)
            .or_else(|| profile_settings.and_then(|settings| settings.pointer("/defaults/backend")).and_then(Value::as_str))
            .or_else(|| self.capabilities.pointer("/study/solverDefaults/backend").and_then(Value::as_str))
            .ok_or("No solver backend is declared by the Study, scenario defaults, or shared defaults.")?;
        let runtime = self.capabilities.pointer("/study/solverRuntimesByProfile")
            .and_then(|profiles| profiles.get(profile)).and_then(|backends| backends.get(backend))
            .ok_or_else(|| format!("No scenario executable is registered for profile {profile:?} and backend {backend:?}."))?;
        let program_id = runtime
            .get("programId")
            .and_then(Value::as_str)
            .ok_or("Scenario runtime is missing its registered program ID.")?;
        let configured = self
            .programs
            .get(program_id)
            .and_then(|program| program.get("executable"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("Scenario runtime refers to unregistered program {program_id:?}.")
            })?;
        let filename = configured
            .rsplit('/')
            .next()
            .ok_or("The executable path has no filename.")?;
        let name = executable_name(filename)?;
        let candidate = model_schema::external_program_path(program_id)?;
        if !candidate.is_file() {
            return Err(format!(
                "The scenario executable {name:?} for profile {profile:?} and backend {backend:?} is missing at {}. Install the matching scenario executable beside fullmeta-fem.",
                candidate.display()
            ));
        }
        let dispatcher_name = self
            .executable
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Could not read the dispatcher executable filename.")?;
        let normalize_name = |name: &str| {
            if cfg!(windows) {
                name.to_ascii_lowercase()
            } else {
                name.to_string()
            }
        };
        model_schema::validate_declaration(
            "x-fullmeta-launchValidation",
            &json!({
                "destinationName":normalize_name(&name), "launcherName":normalize_name(dispatcher_name),
            }),
        )?;
        Ok(candidate)
    }
}

/// Walk only local path components and reject every link or Windows reparse
/// point, including directory junctions, before reading or launching a file.
fn checked_descendant(
    directory: &Path,
    relative: &Path,
    expect_directory: bool,
) -> Result<PathBuf> {
    let mut components = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(name) => components.push(name),
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "Unsafe relative application path {}.",
                    relative.display()
                ));
            }
        }
    }
    if components.is_empty() {
        return Err("An application path must name a descendant file or directory.".into());
    }
    let mut candidate = directory.to_path_buf();
    for (index, name) in components.iter().enumerate() {
        candidate.push(name);
        let metadata = fs::symlink_metadata(&candidate)
            .map_err(|error| format!("Could not inspect {}: {error}", candidate.display()))?;
        if is_link(&metadata) {
            return Err(format!(
                "Application paths must not contain symbolic links or reparse points: {}.",
                candidate.display()
            ));
        }
        let require_directory = index + 1 < components.len() || expect_directory;
        if require_directory && !metadata.is_dir() {
            return Err(format!(
                "Application path {} must be a real directory.",
                candidate.display()
            ));
        }
        if !require_directory && !metadata.is_file() {
            return Err(format!(
                "Application path {} must be a regular file.",
                candidate.display()
            ));
        }
    }
    Ok(candidate)
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn runtime_root() -> Result<PathBuf> {
    fullmeta_model::runtime_paths::installation_root()
}

fn declared_schema_for_model_file(model: &Value, model_file: &Path) -> Result<PathBuf> {
    let configured = model
        .get("x-fullmeta-schema")
        .and_then(Value::as_str)
        .ok_or(
            "The model JSON must declare x-fullmeta-schema as a relative path to its schema file.",
        )?;
    let relative = Path::new(configured);
    if configured.trim().is_empty()
        || configured.contains(['\\', ':', '?', '#'])
        || relative.is_absolute()
    {
        return Err(
            "The model JSON x-fullmeta-schema value must be a relative JSON file path.".into(),
        );
    }
    let directory = model_file
        .parent()
        .ok_or("Could not locate the model JSON's containing folder.")?;
    Ok(directory.join(relative))
}

fn select_schema_for_model_file(model: &Value, model_file: &Path) -> Result<()> {
    model_schema::select_model_schema_path(declared_schema_for_model_file(model, model_file)?)
}

fn read_json(path: &Path) -> Result<Value> {
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid JSON in {}: {error}", path.display()))
}

fn merge_capabilities(target: &mut Value, source: Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            if let Some(existing) = target.get_mut(key) {
                merge_capabilities(existing, value.clone());
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = source;
    }
}

fn executable_name(configured: &str) -> Result<String> {
    let schema = model_schema::command_schema()?;
    let declaration = schema
        .pointer("/x-fullmeta-cli/executableName")
        .ok_or("The model schema must declare CLI executable-name criteria.")?;
    let configured = fullmeta_model::argv::string_value(configured, declaration, schema)?;
    let stem = configured
        .get(configured.len().saturating_sub(4)..)
        .filter(|suffix| suffix.eq_ignore_ascii_case(".exe"))
        .map(|_| &configured[..configured.len() - 4])
        .unwrap_or(&configured);
    Ok(if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    })
}

struct ProjectOptions {
    folder: String,
    study_id: Option<String>,
    validate_only: bool,
}

fn project_options(parsed: &fullmeta_model::argv::DecodedArgs) -> Result<ProjectOptions> {
    let folder = parsed
        .values
        .get("folder")
        .and_then(Value::as_str)
        .ok_or("Could not read the decoded project folder.")?
        .to_string();
    let study_id = parsed
        .values
        .get("study")
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or("Could not read the decoded Study selector.")
        })
        .transpose()?;
    let validate_only = parsed
        .values
        .get("validate")
        .map(|value| {
            value
                .as_bool()
                .ok_or("Could not read the decoded validation flag.")
        })
        .transpose()?
        .unwrap_or(false);
    Ok(ProjectOptions {
        folder,
        study_id,
        validate_only,
    })
}

fn safe_project_directory(root: &Path, folder: &str) -> Result<PathBuf> {
    let projects = root.join("projects");
    let metadata = fs::symlink_metadata(&projects)
        .map_err(|error| format!("Could not open projects/: {error}"))?;
    if is_link(&metadata) || !metadata.is_dir() {
        return Err("projects/ must be a real directory beside the application.".into());
    }
    checked_descendant(&projects, Path::new(folder), true)
        .map_err(|error| format!("Project folder projects/{folder} is unavailable: {error}"))
}

fn print_validation_errors(report: &Value) {
    for error in report
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        eprintln!("{error}");
    }
}

fn require_valid_report(report: &Value) -> Result<()> {
    if report.get("valid").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    let errors = report
        .get("errors")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!("Model preflight failed:\n{errors}"))
}

fn launch_preflighted(
    root: &Path,
    executable: &Path,
    arguments: &[String],
    input: Option<&[u8]>,
    model: &Value,
    run_id: Option<&str>,
) -> Result<i32> {
    let capture_completion =
        run_id.is_some() && env::var_os("FULLMETA_OPERATION_CONTRACT").is_some();
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(root)
        .env_remove("FULLMETA_OPERATION_CONTRACT")
        .stdout(if capture_completion {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .stderr(Stdio::inherit());
    let model_bytes = serde_json::to_vec(model)
        .map_err(|error| format!("Could not prepare the validated model receipt: {error}"))?;
    command.env(
        "FULLMETA_MODEL_SCHEMA_FILE",
        model_schema::runtime_model_schema_path()?,
    );
    command.env("FULLMETA_ROOT", root);
    command.env(
        "FULLMETA_PREFLIGHT_MODEL_SHA256",
        sha256::hex_digest(&model_bytes),
    );
    if let Some(run_id) = run_id {
        command.env("FULLMETA_RUN_ID", run_id);
    }
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::inherit()
    });
    let mut child = command.spawn().map_err(|error| {
        format!(
            "Could not start scenario executable {}: {error}",
            executable.display()
        )
    })?;
    let stream = if capture_completion {
        let stdout = child
            .stdout
            .take()
            .ok_or("The Study worker has no event pipe.")?;
        Some(std::thread::spawn(move || -> Result<Option<Value>> {
            let mut complete = None;
            for line in BufReader::new(stdout).lines() {
                let line = line.map_err(|error| error.to_string())?;
                let event: Value = serde_json::from_str(&line)
                    .map_err(|error| format!("The Study worker emitted invalid NDJSON: {error}"))?;
                if event["type"] == "complete" {
                    complete = Some(event);
                } else {
                    println!("{line}");
                    io::stdout().flush().map_err(|error| error.to_string())?;
                }
            }
            Ok(complete)
        }))
    } else {
        None
    };
    let input_result = if let Some(input) = input {
        match child.stdin.take() {
            Some(mut stdin) => stdin.write_all(input),
            None => Err(io::Error::other("The scenario process has no input pipe.")),
        }
    } else {
        Ok(())
    };
    let status = child.wait().map_err(|error| {
        format!(
            "Could not wait for scenario executable {}: {error}",
            executable.display()
        )
    })?;
    if let Some(stream) = stream {
        let completion = stream
            .join()
            .map_err(|_| "The Study event reader stopped unexpectedly.")??;
        if status.success() {
            let event = completion.ok_or("The Study worker exited without a completion event.")?;
            operation_retry::save_completion(root, run_id.expect("captured run"), &event)?;
            println!("{event}");
            io::stdout().flush().map_err(|error| error.to_string())?;
        }
    }
    if let Err(error) = input_result {
        // A child that rejects input may close its pipe early. Preserve its own
        // failure status and diagnostics instead of replacing them with BrokenPipe.
        if status.success() {
            return Err(format!(
                "Could not provide input to scenario executable {}: {error}",
                executable.display()
            ));
        }
    }
    Ok(status.code().unwrap_or(1))
}
