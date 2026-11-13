#[path = "run_artifacts.rs"]
mod run_artifacts;
#[path = "run_record_guard.rs"]
mod run_record_guard;
#[path = "../../mesh-rust/src/sha256.rs"]
mod sha256;

use serde_json::{Value, json};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const MODEL_FILE: &str = "fem_model.json";
const RUNS_DIR: &str = "fem_runs";
const LOCK_FILE: &str = ".fullmeta-run.lock";
const MODEL_FORMAT_V2: &str = "fem-model/2";

fn maximum_sweep_points() -> Result<usize> {
    crate::json_schema::engine_capabilities()?
        .pointer("/study/maximumSweepPoints")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            Failure(
                "The model schema must declare study.maximumSweepPoints as a positive integer."
                    .into(),
            )
        })
}

fn mesh_resource_limits(profile_id: &str) -> Result<(Option<u64>, Option<u64>)> {
    let capabilities = crate::json_schema::engine_capabilities()?;
    let Some(profiles) = capabilities.pointer("/study/solverResourceLimitsByProfile") else {
        return Ok((None, None));
    };
    let profiles = profiles
        .as_object()
        .ok_or("The schema study.solverResourceLimitsByProfile must be an object.")?;
    let Some(policy) = profiles.get(profile_id) else {
        return Ok((None, None));
    };
    if !policy.is_object() {
        return Err(
            format!("The schema resource policy for {profile_id} must be an object.").into(),
        );
    }
    let maximum = |key: &str| -> Result<Option<u64>> {
        policy
            .get(key)
            .map(|value| {
                value.as_u64().filter(|limit| *limit > 0).ok_or_else(|| {
                    Failure(format!(
                        "The schema resource policy {profile_id}.{key} must be a positive integer."
                    ))
                })
            })
            .transpose()
    };
    Ok((maximum("maximumNodes")?, maximum("maximumCells")?))
}

fn check_mesh_resources(
    profile_id: &str,
    limits: (Option<u64>, Option<u64>),
    nodes: u64,
    cells: u64,
    cell_key: &str,
    case_index: usize,
) -> Result<()> {
    if nodes == 0 || cells == 0 {
        return Err(format!(
            "The {profile_id} mesh for case {} is empty: {nodes} nodes and {cells} {cell_key}.",
            case_index + 1
        )
        .into());
    }
    if let Some(maximum) = limits.0 {
        if nodes > maximum {
            return Err(format!("The configured {profile_id} resource policy allows at most {maximum} mesh nodes; case {} has {nodes}.", case_index + 1).into());
        }
    }
    if let Some(maximum) = limits.1 {
        if cells > maximum {
            return Err(format!("The configured {profile_id} resource policy allows at most {maximum} mesh cells; case {} has {cells} {cell_key}.", case_index + 1).into());
        }
    }
    Ok(())
}

#[derive(Debug)]
struct Failure(String);
type Result<T> = std::result::Result<T, Failure>;

impl From<String> for Failure {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl From<&str> for Failure {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}
impl From<io::Error> for Failure {
    fn from(value: io::Error) -> Self {
        Self(value.to_string())
    }
}
impl From<serde_json::Error> for Failure {
    fn from(value: serde_json::Error) -> Self {
        Self(value.to_string())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputMode {
    Terminal,
    Ndjson,
}

struct EventSink {
    mode: OutputMode,
}
impl EventSink {
    fn emit(&self, event: Value) {
        if self.mode == OutputMode::Ndjson {
            println!("{}", event);
            let _ = io::stdout().flush();
        } else {
            if let Some(message) = event.get("message").and_then(Value::as_str) {
                println!("{message}");
            }
            if event.get("type").and_then(Value::as_str) == Some("complete") {
                if let Some(directory) = event.get("runDirectory").and_then(Value::as_str) {
                    println!("Run folder: {directory}");
                }
                for (label, key) in [("Global-variable CSV", "csv"), ("Debug CSV", "debugCsv")] {
                    if let Some(filename) = event.get(key).and_then(Value::as_str) {
                        println!("{label}: {filename}");
                    }
                }
            }
        }
    }
    fn error(&self, message: &str, total: usize, completed: usize, run_dir: Option<&str>) {
        let detail = run_dir
            .map(|path| format!("{message} Run data: {path}/"))
            .unwrap_or_else(|| message.to_string());
        self.emit(json!({"type":"error","state":"error","message":detail,"totalPoints":total,"completedPoints":completed}));
    }
}

#[derive(Clone)]
struct ProjectInput {
    folder: String,
    file: String,
    study_id: Option<String>,
    csv_filename: Option<String>,
    data_folder: Option<String>,
    model_hash: Option<String>,
    model: Option<Value>,
    validate_only: bool,
}

struct RunLock {
    path: PathBuf,
    file: Option<File>,
}
impl RunLock {
    fn acquire(project: &Path) -> Result<Self> {
        let path = project.join(LOCK_FILE);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !remove_stale_run_lock(&path) {
                    return Err(Failure(format!(
                        "Another active FullMeta run or mesh operation holds {}.",
                        path.display()
                    )));
                }
                options.open(&path).map_err(|error| {
                    if error.kind() == io::ErrorKind::AlreadyExists {
                        Failure(format!(
                            "Another active FullMeta run or mesh operation holds {}.",
                            path.display()
                        ))
                    } else {
                        Failure(format!(
                            "Could not create the project run lock {}: {error}",
                            path.display()
                        ))
                    }
                })?
            }
            Err(error) => {
                return Err(Failure(format!(
                    "Could not create the project run lock {}: {error}",
                    path.display()
                )));
            }
        };
        let write_identity = (|| -> io::Result<()> {
            writeln!(file, "pid={} started={}", std::process::id(), timestamp())?;
            if let Some(run_id) = run_artifacts::coordinated_run_id() {
                writeln!(file, "runId={run_id}")?;
            }
            file.sync_all()
        })();
        if let Err(error) = write_identity {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(Failure(format!(
                "Could not write run lock {}: {error}",
                path.display()
            )));
        }
        Ok(Self {
            path,
            file: Some(file),
        })
    }
}
impl Drop for RunLock {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}

fn remove_stale_run_lock(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return !path.exists();
    };
    let Some(pid) = contents.lines().find_map(|line| {
        line.strip_prefix("pid=")?
            .split_whitespace()
            .next()?
            .parse::<u32>()
            .ok()
    }) else {
        return false;
    };
    if process_is_alive(pid) {
        return false;
    }
    let Ok(current_contents) = fs::read_to_string(path) else {
        return !path.exists();
    };
    if current_contents != contents {
        return false;
    }
    fs::remove_file(path).is_ok() || !path.exists()
}

fn process_is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let Ok(output) = Command::new("tasklist.exe")
            .args(["/FO", "CSV", "/NH", "/FI", &format!("PID eq {pid}")])
            .output()
        else {
            return true;
        };
        if !output.status.success() {
            return true;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        return text.lines().any(|line| {
            line.split(',')
                .nth(1)
                .map(|field| field.trim().trim_matches('"') == pid.to_string())
                .unwrap_or(false)
        });
    }
    #[cfg(unix)]
    {
        return Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(true);
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        true
    }
}

pub(crate) fn dispatch() -> ExitCode {
    #[cfg(any(
        feature = "solver-native-acoustic2d",
        feature = "solver-native-acoustic3d",
        feature = "solver-native-em2d",
        feature = "solver-native-em3d"
    ))]
    let result = entry();
    #[cfg(not(any(
        feature = "solver-native-acoustic2d",
        feature = "solver-native-acoustic3d",
        feature = "solver-native-em2d",
        feature = "solver-native-em3d"
    )))]
    let result = dispatch_to_scenario_executable();
    match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("fullmeta-fem: {}", error.0);
            ExitCode::FAILURE
        }
    }
}

/// The public coordinator validates the project and selects a scenario binary
/// from the capability catalog. Each scenario binary then owns its full run.
fn dispatch_to_scenario_executable() -> Result<ExitCode> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let root = repository_root()?;
    if arguments
        .first()
        .is_some_and(|argument| argument == "--server")
    {
        let model = read_stdin_json()?;
        validate_model(&model)?;
        let executable = scenario_executable(&root, &model, None)?;
        let mut child = crate::model_schema::command(&executable)?
            .arg("--server")
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| {
                Failure(format!(
                    "Could not start scenario executable {}: {error}",
                    executable.display()
                ))
            })?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or("Could not provide model JSON to the scenario executable")?;
        stdin.write_all(&serde_json::to_vec(&model)?)?;
        drop(stdin);
        return Ok(if child.wait()?.success() {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    if arguments.is_empty()
        || arguments
            .iter()
            .any(|argument| argument == "--help" || argument == "-h")
    {
        println!(
            "Usage: fullmeta-fem <project-folder-name> [--validate] [--study <study-id>] [--csv <filename>]\\nValidates the saved model and dispatches complete Study runs to the scenario executable."
        );
        return Ok(if arguments.is_empty() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        });
    }

    let folder = project_folder_name(&arguments[0])?;
    let mut validate_only = false;
    let mut study_id = None;
    let mut csv_filename = None;
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--validate" => validate_only = true,
            "--study" => {
                index += 1;
                study_id = Some(
                    arguments
                        .get(index)
                        .ok_or("--study requires an ID")?
                        .clone(),
                );
            }
            "--csv" => {
                index += 1;
                csv_filename = Some(
                    arguments
                        .get(index)
                        .ok_or("--csv requires a filename")?
                        .clone(),
                );
            }
            option => return Err(format!("Unknown option {option}. Use --help for usage.").into()),
        }
        index += 1;
    }
    if validate_only && (study_id.is_some() || csv_filename.is_some()) {
        return Err(
            "--validate cannot be combined with --study or --csv; it validates the complete model."
                .into(),
        );
    }
    let project = safe_project_directory(&root, &folder)?;
    let model_filename = MODEL_FILE;
    let model_path = safe_project_file(&project, model_filename)?;
    let model: Value = serde_json::from_slice(&fs::read(&model_path)?)
        .map_err(|error| Failure(format!("Invalid {} JSON: {error}", model_path.display())))?;
    validate_model(&model)?;
    if validate_only {
        println!("Model validation successful.");
        return Ok(ExitCode::SUCCESS);
    }

    let executable = scenario_executable(&root, &model, study_id.as_deref())?;
    let status = crate::model_schema::command(&executable)?
        .args(&arguments)
        .current_dir(&root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| {
            Failure(format!(
                "Could not start scenario executable {}: {error}",
                executable.display()
            ))
        })?;
    Ok(if status.success() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn read_stdin_json() -> Result<Value> {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    serde_json::from_str(&text)
        .map_err(|error| Failure(format!("Invalid service request JSON: {error}")))
}

fn option_value<'a>(arguments: &'a [String], option: &str) -> Option<&'a str> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == option)
        .map(|pair| pair[1].as_str())
}

fn scenario_executable(
    root: &Path,
    model: &Value,
    requested_study: Option<&str>,
) -> Result<PathBuf> {
    let profile = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .ok_or("The model does not declare scenario.profileId.")?;
    let study_items = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .ok_or("model.model.study must be an array")?;
    let study = match requested_study {
        Some(id) => study_items
            .iter()
            .find(|study| study.get("id").and_then(Value::as_str) == Some(id))
            .ok_or_else(|| Failure(format!("Unknown Study ID {id:?}.")))?,
        None => study_items
            .first()
            .ok_or("The model does not contain a Study to run")?,
    };
    let capabilities = crate::json_schema::engine_capabilities()?;
    let backend = study
        .pointer("/solver/backend")
        .and_then(Value::as_str)
        .or_else(|| {
            capabilities
                .pointer("/study/solverDefaults/backend")
                .and_then(Value::as_str)
        })
        .ok_or_else(|| Failure("No shared default solver backend is configured.".into()))?;
    let runtime = capabilities.pointer("/study/solverRuntimesByProfile")
        .and_then(|profiles| profiles.get(profile))
        .and_then(|backends| backends.get(backend))
        .ok_or_else(|| Failure(format!("No scenario executable is registered for profile {profile:?} and backend {backend:?}.")))?;
    if runtime.get("kind").and_then(Value::as_str) != Some("executable") {
        return Err(Failure(format!(
            "The configured runtime for profile {profile:?} must be an executable."
        )));
    }
    let program_id = runtime
        .get("programId")
        .and_then(Value::as_str)
        .ok_or("Capability runtime is missing its registered program ID")?;
    let configured_name = crate::json_schema::external_program_executable(program_id)?;
    let file_name = Path::new(configured_name)
        .file_name()
        .and_then(|value| value.to_str());
    if file_name != Some(configured_name) || configured_name.contains(['/', '\\', ':']) {
        return Err(Failure(format!(
            "Unsafe scenario executable name {configured_name:?} in capability catalog."
        )));
    }
    let stem = configured_name
        .strip_suffix(".exe")
        .unwrap_or(configured_name);
    executable(root, stem)
}

fn entry() -> Result<ExitCode> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments
        .first()
        .is_some_and(|argument| argument == "--validate")
    {
        if arguments.len() != 2 || arguments[1] != "-" {
            return Err("Submitted-model validation requires --validate -.".into());
        }
        let model = read_stdin_json()?;
        let report = crate::preflight_client::validation_report(&model).unwrap_or_else(|error| {
            json!({
                "schema": "fullmeta-model-validation/1",
                "valid": false,
                "errors": [error],
                "structureErrors": [],
            })
        });
        let mut errors = Vec::<String>::new();
        let mut structure_errors = Vec::<String>::new();
        for (key, output) in [
            ("errors", &mut errors),
            ("structureErrors", &mut structure_errors),
        ] {
            for message in report[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                if !output.iter().any(|existing| existing == message) {
                    output.push(message.to_string());
                }
            }
        }
        let valid = errors.is_empty();
        println!(
            "{}",
            json!({
                "schema": "fullmeta-model-validation/1",
                "valid": valid,
                "errors": errors,
                "structureErrors": structure_errors,
            })
        );
        return Ok(if valid {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        });
    }
    if arguments.first().is_some_and(|arg| arg == "--server") {
        let sink = EventSink {
            mode: OutputMode::Ndjson,
        };
        let model = read_stdin_json()?;
        let folder = env::var("FULLMETA_MODEL_FOLDER").unwrap_or_else(|_| {
            format!(
                "projects/{}",
                model
                    .pointer("/metadata/id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        });
        let request = ProjectInput {
            folder,
            file: env::var("FULLMETA_MODEL_FILE").unwrap_or_else(|_| MODEL_FILE.into()),
            study_id: env::var("FULLMETA_STUDY_ID")
                .ok()
                .filter(|id| !id.trim().is_empty()),
            csv_filename: None,
            data_folder: None,
            model_hash: None,
            model: Some(model),
            validate_only: false,
        };
        if let Err(error) = run_request(request, &sink, false) {
            sink.error(&error.0, 0, 0, None);
            return Ok(ExitCode::FAILURE);
        }
        return Ok(ExitCode::SUCCESS);
    }
    if arguments.is_empty() || arguments.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "Usage: fullmeta-fem <project-folder-name> [--validate] [--study <study-id>] [--csv <filename>]\n       fullmeta-fem --validate -\nRuns the saved projects/<project-folder-name>/fem_model.json sequentially.\nWith --validate, checks the model schema and semantic rules, then exits without geometry checks, meshing, or FEM.\nWith --validate -, reads the submitted model from stdin and returns a fullmeta-model-validation/1 JSON response."
        );
        return Ok(if arguments.is_empty() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        });
    }
    let folder = arguments[0].clone();
    let mut study_id = None;
    let mut csv_filename = None;
    let mut validate_only = false;
    let mut index = 1;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--validate" => validate_only = true,
            "--study" => {
                index += 1;
                study_id = Some(
                    arguments
                        .get(index)
                        .ok_or("--study requires an ID")?
                        .clone(),
                );
            }
            "--csv" => {
                index += 1;
                csv_filename = Some(
                    arguments
                        .get(index)
                        .ok_or("--csv requires a filename")?
                        .clone(),
                );
            }
            option => return Err(format!("Unknown option {option}. Use --help for usage.").into()),
        }
        index += 1;
    }
    if validate_only && (study_id.is_some() || csv_filename.is_some()) {
        return Err(
            "--validate cannot be combined with --study or --csv; it validates the complete model."
                .into(),
        );
    }
    let sink = EventSink {
        mode: OutputMode::Terminal,
    };
    let folder = project_folder_name(&folder)?;
    let model_filename = MODEL_FILE.to_string();
    run_request(
        ProjectInput {
            folder: format!("projects/{folder}"),
            file: model_filename,
            study_id,
            csv_filename,
            data_folder: None,
            model_hash: None,
            model: None,
            validate_only,
        },
        &sink,
        true,
    )?;
    Ok(ExitCode::SUCCESS)
}

fn repository_root() -> Result<PathBuf> {
    fullmeta_model::runtime_paths::installation_root().map_err(Failure)
}

fn project_folder_name(input: &str) -> Result<String> {
    let raw = input.trim().replace('\\', "/");
    let name = raw.strip_prefix("projects/").unwrap_or(&raw);
    if name.is_empty()
        || name.contains('/')
        || name == "."
        || name == ".."
        || name.len() > 64
        || !name.chars().enumerate().all(|(i, c)| {
            if i == 0 {
                c.is_ascii_alphanumeric()
            } else {
                c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
            }
        })
    {
        return Err("Project argument must be one folder name directly inside projects/.".into());
    }
    Ok(name.to_string())
}

fn safe_project_directory(root: &Path, folder: &str) -> Result<PathBuf> {
    let name = project_folder_name(folder)?;
    let projects = root.join("projects");
    let projects_meta = fs::symlink_metadata(&projects)
        .map_err(|error| Failure(format!("Could not open projects/: {error}")))?;
    if projects_meta.file_type().is_symlink() || !projects_meta.is_dir() {
        return Err("projects/ must be a real directory beside the application.".into());
    }
    let candidate = projects.join(&name);
    let meta = fs::symlink_metadata(&candidate).map_err(|error| {
        Failure(format!(
            "Project folder projects/{name} is unavailable: {error}"
        ))
    })?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err("Project folders must be real directories, not symbolic links.".into());
    }
    Ok(candidate)
}

fn safe_model_directory(root: &Path, folder: &str) -> Result<(String, PathBuf)> {
    let relative = folder.trim();
    if relative.is_empty()
        || relative.len() > 2048
        || relative.contains('\\')
        || relative.starts_with('/')
    {
        return Err("The model folder must be relative to the FullMeta folder.".into());
    }
    if relative == "." {
        return Ok((".".into(), root.to_path_buf()));
    }
    let mut directory = root.to_path_buf();
    for (index, part) in relative.split('/').enumerate() {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.chars().any(|character| {
                character.is_control()
                    || matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|')
            })
            || (index == 0
                && matches!(
                    part.to_ascii_lowercase().as_str(),
                    ".git" | ".server-control"
                ))
        {
            return Err("The model folder must be a real directory inside FullMeta.".into());
        }
        directory.push(part);
        let metadata = fs::symlink_metadata(&directory)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(format!(
                "Model folder {} must be a real directory.",
                directory.display()
            )
            .into());
        }
    }
    Ok((relative.to_string(), directory))
}

fn safe_model_file(directory: &Path, filename: &str) -> Result<PathBuf> {
    if filename.is_empty()
        || filename.len() > 200
        || filename.starts_with(' ')
        || filename.starts_with('.')
        || !filename.to_ascii_lowercase().ends_with(".json")
        || filename.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        return Err("Choose a JSON model filename directly inside its folder.".into());
    }
    let path = directory.join(filename);
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("The model JSON must be a regular file directly inside its folder.".into());
    }
    Ok(path)
}

fn relative_model_path(folder: &str, name: &str) -> String {
    if folder == "." {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

fn configured_runs_directory() -> String {
    if let Ok(value) = env::var("FULLMETA_RUNS_DIRECTORY") {
        if !value.is_empty()
            && value.len() <= 64
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            })
        {
            return value;
        }
    }
    RUNS_DIR.into()
}

fn safe_project_file(project: &Path, filename: &str) -> Result<PathBuf> {
    if filename != MODEL_FILE {
        return Err(format!("The model file must be named {MODEL_FILE}.").into());
    }
    let path = project.join(filename);
    let meta = fs::symlink_metadata(&path)
        .map_err(|error| Failure(format!("Could not read {filename}: {error}")))?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(
            "The model manifest must be a regular file directly in its project folder.".into(),
        );
    }
    Ok(path)
}

fn run_request(input: ProjectInput, sink: &EventSink, cli: bool) -> Result<()> {
    let root = repository_root()?;
    let (model_folder, project) = safe_model_directory(&root, &input.folder)?;
    let model_path = safe_model_file(&project, &input.file)?;
    let model_filename = input.file.clone();
    let runs_directory_name = configured_runs_directory();
    let model = match input.model {
        Some(model) => model,
        None => serde_json::from_slice(&fs::read(&model_path)?)
            .map_err(|error| Failure(format!("Invalid {} JSON: {error}", model_path.display())))?,
    };
    let _lock = if input.validate_only {
        None
    } else {
        Some(RunLock::acquire(&project)?)
    };

    if let Err(error) = validate_model(&model) {
        return Err(Failure(format!("Model validation failed: {}", error.0)));
    }
    sink.emit(
        json!({"type":"progress","state":"running","message":"Model validation successful."}),
    );
    if input.validate_only {
        return Ok(());
    }

    let expected_data_folder = model_folder.clone();
    if let Some(data_folder) = input.data_folder.as_deref() {
        if data_folder.trim_end_matches('/').to_lowercase() != expected_data_folder.to_lowercase() {
            return Err(
                format!("This model's run data folder must be {expected_data_folder}.").into(),
            );
        }
    }
    ensure_unique_model_id(&project, &model, &model_path).map_err(|error| {
        Failure(format!(
            "Could not check model IDs in this folder: {}",
            error.0
        ))
    })?;
    let studies = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .ok_or("model.model.study must be an array")?;
    let study = if let Some(id) = input.study_id.as_deref() {
        studies
            .iter()
            .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
            .ok_or_else(|| Failure(format!("Unknown Study ID {id:?}.")))?
    } else {
        studies
            .first()
            .ok_or("The model does not contain a Study to run")?
    };
    let study_id = study
        .get("id")
        .and_then(Value::as_str)
        .ok_or("Study id is required")?
        .to_string();
    let model_format = model
        .get("modelFormat")
        .and_then(Value::as_str)
        .unwrap_or("");
    let profile_id = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let is_model2_native_profile = model_format == MODEL_FORMAT_V2
        && matches!(profile_id, "acoustic-2d" | "acoustic-3d" | "em-2d");
    let is_model2_em3d = model_format == MODEL_FORMAT_V2 && profile_id == "em-3d";
    if model_format != MODEL_FORMAT_V2 {
        return Err(format!("fullmeta-fem only supports {MODEL_FORMAT_V2} projects.").into());
    }
    if !is_model2_native_profile && !is_model2_em3d {
        return Err(format!(
            "fullmeta-fem does not yet have an execution path for Model 2 profile {profile_id:?}."
        )
        .into());
    }
    let component_id = "model";

    let mesh_name = crate::json_schema::external_program_executable("mesh")?;
    let mesh_exe = executable(&root, mesh_name.strip_suffix(".exe").unwrap_or(mesh_name))?;
    let normalized =
        prepare_study(&model, &study_id, input.csv_filename.as_deref()).map_err(|error| {
            Failure(format!(
                "Profile {profile_id}, Study {study_id}, preparation: {}",
                error.0
            ))
        })?;
    if is_model2_native_profile {
        return run_model2(
            &root,
            &model_folder,
            &project,
            &model,
            &model_filename,
            &runs_directory_name,
            input.model_hash.as_deref(),
            study,
            &study_id,
            component_id,
            profile_id,
            &normalized,
            &mesh_exe,
            sink,
            cli,
        );
    }
    // EM 3D uses its specialized FEM backend request, prepared from the same
    // Model 2 project contract as the other scenario profiles.
    if !is_model2_em3d {
        return Err("The Model 2 Study runner selected an invalid execution route.".into());
    }
    run_model_em3d(
        &root,
        &model_folder,
        &project,
        &model,
        &model_filename,
        &runs_directory_name,
        input.model_hash.as_deref(),
        &study_id,
        component_id,
        &normalized,
        &mesh_exe,
        sink,
        cli,
    )
}

fn model_for_sweep_case(model: &Value, study_id: &str, case: &Value) -> Result<Value> {
    let parameters = case
        .get("parameterOverrides")
        .and_then(Value::as_array)
        .ok_or("Prepared sweep case is missing parameterOverrides")?;
    case.get("frequencyHz")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("Prepared sweep case is missing a valid frequencyHz")?;
    let mut case_model = model.clone();
    case_model["model"]["parameters"] = json!(parameters);
    let studies = case_model
        .pointer_mut("/model/study")
        .and_then(Value::as_array_mut)
        .ok_or("model.model.study must be an array")?;
    let study = studies
        .iter_mut()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(study_id))
        .ok_or_else(|| {
            Failure(format!(
                "Study {study_id:?} is not present in the case model"
            ))
        })?;
    // The case parameters determine the linked Physics frequency. Expanded
    // solver frequencies belong in the request, not in the model Study.
    if let Some(object) = study.as_object_mut() {
        object.remove("parameterSweep");
    }
    Ok(case_model)
}

fn run_model2(
    root: &Path,
    model_folder: &str,
    project: &Path,
    model: &Value,
    model_filename: &str,
    runs_directory_name: &str,
    supplied_model_hash: Option<&str>,
    study: &Value,
    study_id: &str,
    component_id: &str,
    profile_id: &str,
    study_plan: &Value,
    mesh_exe: &Path,
    sink: &EventSink,
    cli: bool,
) -> Result<()> {
    let (dimension, element_family, cell_key, boundary_key, field_prefix) = match profile_id {
        "acoustic-2d" => (
            2usize,
            "triangle",
            "triangles",
            "boundaryEdges",
            "pressure_",
        ),
        "acoustic-3d" => (
            3usize,
            "tetrahedron",
            "tetrahedra",
            "boundaryTriangles",
            "pressure_",
        ),
        "em-2d" => (2usize, "triangle", "triangles", "boundaryEdges", "emfield_"),
        _ => return Err(format!("Unsupported Model 2 solver profile {profile_id:?}.").into()),
    };
    if study.get("type").and_then(Value::as_str) != Some("frequencyDomain") {
        return Err(
            format!("The {profile_id} runner executes frequency-domain studies only.").into(),
        );
    }
    let component = model.pointer("/model").ok_or("The model is missing")?;
    let physics_id = study
        .get("physicsId")
        .and_then(Value::as_str)
        .ok_or("Study physicsId is required")?;
    let physics = crate::model_schema::physics(component)
        .filter(|physics| physics.get("id").and_then(Value::as_str) == Some(physics_id))
        .ok_or("Study physicsId is unknown")?;
    let formulation = physics
        .get("formulation")
        .and_then(Value::as_str)
        .ok_or("The selected Physics interface requires a formulation")?;
    let _frequency = physics
        .get("frequency")
        .ok_or("The selected Physics interface requires a frequency quantity")?;
    if study_plan.get("schema").and_then(Value::as_str) != Some("fullmeta-study-plan/1")
        || study_plan.get("profileId").and_then(Value::as_str) != Some(profile_id)
    {
        return Err(format!(
            "The shared Study preparer returned an incompatible plan for {profile_id}."
        )
        .into());
    }
    let csv_name = study_plan
        .get("csvFilename")
        .and_then(Value::as_str)
        .filter(|name| safe_filename(name, ".csv"))
        .ok_or("Prepared Study has no valid CSV filename.")?;
    let debug_csv = format!(
        "{}-debug.csv",
        csv_name.strip_suffix(".csv").unwrap_or(csv_name)
    );
    let fields_enabled = study_plan
        .get("fieldFiles")
        .and_then(Value::as_bool)
        .ok_or("Prepared Study has no field output policy.")?;
    let frequencies = study_plan
        .get("frequenciesHz")
        .and_then(Value::as_array)
        .ok_or("The shared Study preparer did not return frequenciesHz")?;
    let maximum_points = maximum_sweep_points()?;
    if frequencies.is_empty() || frequencies.len() > maximum_points {
        return Err(format!(
            "The {profile_id} Study must contain between 1 and {maximum_points} sweep cases."
        )
        .into());
    }
    let frequency_cases = study_plan
        .get("frequencyCases")
        .and_then(Value::as_array)
        .ok_or("The shared Study preparer did not return frequencyCases")?;
    if frequency_cases.len() != frequencies.len() {
        return Err("The prepared sweep frequencies and cases have different lengths.".into());
    }
    let solver_settings = study_plan
        .get("solverSettings")
        .ok_or("The shared Study preparer did not return solverSettings")?;
    let field_formulation = solver_settings
        .get("fieldFormulation")
        .and_then(Value::as_str)
        .ok_or("Prepared solver settings omit fieldFormulation")?;
    let element_order = solver_settings
        .get("elementOrder")
        .and_then(Value::as_u64)
        .ok_or("Prepared solver settings omit elementOrder")?;
    let constitutive_quadrature = solver_settings
        .get("constitutiveQuadrature")
        .and_then(Value::as_str)
        .ok_or("Prepared solver settings omit constitutiveQuadrature")?;
    let solver_backend = solver_settings
        .get("solverBackend")
        .and_then(Value::as_str)
        .ok_or("Prepared solver settings omit solverBackend")?;
    let solver_ordering = solver_settings
        .get("solverOrdering")
        .and_then(Value::as_str)
        .ok_or("Prepared solver settings omit solverOrdering")?;
    let solver_threads = solver_settings
        .get("solverThreads")
        .and_then(Value::as_u64)
        .ok_or("Prepared solver settings omit solverThreads")?;
    let total_points = frequencies.len();
    let announced_run_id = run_artifacts::coordinated_run_id();
    if let Some(id) = &announced_run_id {
        update_preparation_stage(
            &project.join(runs_directory_name).join(id),
            "geometry",
            total_points,
        )?;
    }
    sink.emit(json!({"type":"progress","state":"running","stage":"geometry","message":"Preparing Study geometry…",
        "totalPoints":total_points,"completedPoints":0,"runId":announced_run_id}));
    let model_hash = match supplied_model_hash {
        Some(value) if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) => {
            value.to_string()
        }
        Some(_) => return Err("The editor supplied an invalid model content hash.".into()),
        None => canonical_hash(model)?,
    };
    let model_id = model
        .pointer("/metadata/id")
        .and_then(Value::as_str)
        .ok_or("metadata.id is required")?;
    let runs_dir = ensure_real_child_directory(project, runs_directory_name)?;
    let case_models = frequency_cases
        .iter()
        .map(|case| model_for_sweep_case(model, study_id, case))
        .collect::<Result<Vec<_>>>()?;
    // The initial model was preflighted before Study preparation. Each sweep
    // geometry is checked by compile_geometry below, so a separate mesh check
    // process for every case would repeat that work.
    let (compiled, prepared_mesh) = compile_and_prepare_run_mesh(
        mesh_exe,
        root,
        project,
        &runs_dir,
        &case_models[0],
        component_id,
        study_id,
    )?;
    let run_id = prepared_mesh.run_id.clone();
    let run_relative =
        relative_model_path(model_folder, &format!("{runs_directory_name}/{run_id}"));
    let run_dir = prepared_mesh.run_directory.clone();
    let output_dir = run_dir.join("output");
    let logs_dir = run_dir.join("logs");
    ensure_real_child_directory(&run_dir, "output")?;
    ensure_real_child_directory(&run_dir, "logs")?;
    let case_work_root = ensure_real_child_directory(&run_dir, "case-work")?;
    let run_path = run_dir.join("run.json");
    let request_path = run_dir.join("request.json");
    let model_snapshot_path = run_dir.join("model.json");
    write_json_new(&model_snapshot_path, &case_models[0])?;
    let case_meshes = prepare_case_meshes(
        mesh_exe,
        root,
        &runs_dir,
        &run_dir,
        &logs_dir,
        &compiled,
        &prepared_mesh,
        &case_models,
        component_id,
        sink,
        total_points,
    )?;
    let mut frequency_cases_for_request = Vec::<Value>::with_capacity(total_points);
    for (index, ((case, case_model), case_mesh)) in frequency_cases
        .iter()
        .zip(&case_models)
        .zip(&case_meshes)
        .enumerate()
    {
        let case_directory_name = format!("case-{:04}", index + 1);
        let case_directory = ensure_real_child_directory(&case_work_root, &case_directory_name)?;
        write_json_new(&case_directory.join("model.json"), case_model)?;
        let case_output_directory = format!("case-work/{case_directory_name}");
        let case_overrides = json!({
            "modelFile":format!("{case_output_directory}/model.json"),
            "meshFile":run_relative_reference(&run_dir, &case_mesh.paths.mesh)?,
            "modelMapFile":run_relative_reference(&run_dir, &case_mesh.paths.map)?,
            "outputDirectory":case_output_directory
        });
        let mut case_row = case.clone();
        case_row["caseOverrides"] = case_overrides;
        frequency_cases_for_request.push(case_row);
    }
    let primary_case = case_meshes
        .first()
        .ok_or("The prepared Study has no cases")?;
    let mesh_name = run_relative_reference(&run_dir, &primary_case.paths.mesh)?;
    let geo_stem = primary_case
        .paths
        .source
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("Invalid geometry artifact filename")?;
    let primary_map = run_relative_reference(&run_dir, &primary_case.paths.map)?;
    let solver_request = json!({
        "schema":"fem-solver-request/1",
        "solver":profile_id,
        "studyId":study_id,
        "frequencyCases":frequency_cases_for_request,
        "sweepCsv":csv_name,
        "fieldFormulation":field_formulation,
        "elementOrder":element_order,
        "constitutiveQuadrature":constitutive_quadrature,
        "solverBackend":solver_backend,
        "solverOrdering":solver_ordering,
        "solverThreads":solver_threads,
        "modelFile":"model.json",
        "meshFile":mesh_name,
        "modelMapFile":primary_map,
        "outputDirectory":"output"
    });
    write_json_new(&request_path, &solver_request)?;
    let study_snapshot = study.clone();
    let geometry_case_records = case_meshes.iter().enumerate().map(|(index, case_mesh)| -> Result<Value> {
        Ok(json!({
            "caseIndex":index,
            "geometryHash":case_mesh.compiled.geometry_hash,
            "mesh":run_relative_reference(&run_dir,&case_mesh.paths.mesh)?,
            "map":run_relative_reference(&run_dir,&case_mesh.paths.map)?,
            "reused":case_mesh.reused,
            "meshSummary":{"nodes":case_mesh.report.get("nodes"),"cells":case_mesh.report.get(cell_key)}
        }))
    }).collect::<Result<Vec<_>>>()?;
    let mut record = json!({
        "schema":"fullmeta-run/1", "kind":"study", "runId":run_id, "state":"running", "startedAt":timestamp(), "completedAt":null,
        "model":{"file":relative_model_path(model_folder, model_filename),"id":model_id,"modelFormat":MODEL_FORMAT_V2,"sha256":model_hash,"snapshot":model},
        "study":{"id":study_id,"name":study_snapshot.get("name").and_then(Value::as_str).unwrap_or("Study"),"snapshot":study_snapshot},
        "solver":{"executable":profile_id,"formulation":formulation,"backend":solver_backend,"ordering":solver_ordering,"threads":solver_threads,"elementOrder":element_order,"constitutiveQuadrature":constitutive_quadrature,"requestFile":"request.json","fieldFiles":fields_enabled},
        "geometry":{"geometryHash":primary_case.compiled.geometry_hash,"source":run_relative_reference(&run_dir,&primary_case.paths.source)? ,"msh":run_relative_reference(&run_dir,&primary_case.paths.msh)? ,"mesh":run_relative_reference(&run_dir,&primary_case.paths.mesh)? ,"report":run_relative_reference(&run_dir,&primary_case.paths.report)? ,"map":run_relative_reference(&run_dir,&primary_case.paths.map)? ,"cache":run_relative_reference(&run_dir,&primary_case.paths.cache)? ,"files":null,"cases":geometry_case_records},
        "artifacts":{"csv":format!("output/{csv_name}"),"debugCsv":format!("output/{debug_csv}"),"fields":format!("output/{field_prefix}<frequency-Hz-to-6-decimals>Hz.json"),"stdout":"logs/solver.stdout.log","stderr":"logs/solver.stderr.log","meshLog":run_geometry_reference(&format!("{geo_stem}-gmsh.log"))},
        "progress":{"totalPoints":total_points,"completedPoints":0,"currentFrequencyHz":null},"error":null
    });
    if run_artifacts::adopt_server_record(&run_path, &mut record)?
        || prepared_mesh.promoted_mesh_run
    {
        write_json_replace(&run_path, &record)?;
    } else {
        write_json_new(&run_path, &record)?;
    }

    let mut completed = 0usize;
    let execute = (|| -> Result<()> {
        sink.emit(json!({"type":"progress","state":"running","message":format!("Model validated; preparing {dimension}D {} geometry and mesh…", if profile_id.starts_with("acoustic-") { "acoustic" } else { "electromagnetic" }),"totalPoints":total_points,"completedPoints":0,"runId":run_id}));
        let resource_limits = mesh_resource_limits(profile_id)?;
        for (index, case_mesh) in case_meshes.iter().enumerate() {
            let node_count = case_mesh
                .report
                .get("nodes")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let cell_count = case_mesh
                .report
                .get(cell_key)
                .and_then(Value::as_u64)
                .unwrap_or(0);
            check_mesh_resources(
                profile_id,
                resource_limits,
                node_count,
                cell_count,
                cell_key,
                index,
            )?;
            let mut mesh_summary = json!({"dimension":dimension,"elementFamily":element_family,"nodes":node_count,"materialDiagnostics":case_mesh.report.get("materialDiagnostics").cloned().unwrap_or_else(||json!([])),"caseIndex":index,"geometryHash":case_mesh.compiled.geometry_hash,"reused":case_mesh.reused});
            mesh_summary[cell_key] = json!(cell_count);
            mesh_summary[boundary_key] = case_mesh
                .report
                .get(boundary_key)
                .cloned()
                .unwrap_or_else(|| json!(0));
            sink.emit(json!({"type":"mesh","state":"success","message":if case_mesh.reused {format!("Sweep case {} mesh ready (reused)",index+1)} else {format!("Sweep case {} mesh generated",index+1)},"runId":run_id,"mesh":mesh_summary}));
        }
        record["geometry"]["files"] = describe_artifacts(&primary_case.paths)?;
        write_json_replace(&run_path, &record)?;
        sink.emit(json!({"type":"progress","state":"running","message":format!("{dimension}D {} solver is starting…", if profile_id.starts_with("acoustic-") { "acoustic pressure" } else { "electromagnetic scalar field" }),"totalPoints":total_points,"completedPoints":0,"runId":run_id}));
        let solver_runtime = solver_runtime(root, profile_id, solver_backend)?;
        let stdout_path = logs_dir.join("solver.stdout.log");
        let stderr_path = logs_dir.join("solver.stderr.log");
        let solver = run_solver(
            solver_runtime,
            &request_path,
            total_points,
            &run_id,
            &mut completed,
            &mut record,
            &run_path,
            &stdout_path,
            &stderr_path,
            sink,
        )?;
        if solver != 0 {
            return Err(solver_failure(&stdout_path, &stderr_path, solver)?);
        }
        if completed != total_points {
            return Err("The solver exited before reporting success for every case.".into());
        }
        let debug_bytes = fs::copy(
            logs_dir.join("solver-stage-timings.csv"),
            output_dir.join(&debug_csv),
        )?;
        sink.emit(json!({"type":"progress","state":"running","stage":"verification","message":"Checking Study outputs…",
            "totalPoints":total_points,"completedPoints":completed,"runId":run_id}));
        let mut field_artifacts = Vec::new();
        if fields_enabled {
            for (index, case) in frequency_cases.iter().enumerate() {
                let frequency_hz = case
                    .get("frequencyHz")
                    .and_then(Value::as_f64)
                    .ok_or("Prepared Study case is missing frequencyHz")?;
                let case_directory = case_work_root.join(format!("case-{:04}", index + 1));
                field_artifacts.extend(run_artifacts::collect_case_field_files(
                    &case_directory,
                    &output_dir,
                    index,
                    &json!({"frequencyHz":frequency_hz}),
                    |name| name.starts_with(field_prefix) && name.ends_with("Hz.json"),
                )?);
            }
        }
        let field_files = run_artifacts::case_field_file_names(
            &output_dir,
            &field_artifacts,
            if fields_enabled { total_points } else { 0 },
        )?;
        fs::remove_dir_all(&case_work_root)?;
        let csv_path = output_dir.join(csv_name);
        let csv_meta = fs::metadata(&csv_path).map_err(|error| {
            Failure(format!(
                "The {profile_id} solver completed without creating its sweep CSV: {error}"
            ))
        })?;
        if !csv_meta.is_file() || csv_meta.len() == 0 {
            return Err(format!(
                "The {profile_id} solver sweep CSV must be a nonempty regular file."
            )
            .into());
        }
        completed = total_points;
        record["state"] = json!("completed");
        record["completedAt"] = json!(timestamp());
        record["progress"]["completedPoints"] = json!(completed);
        record["artifacts"]["csvBytes"] = json!(csv_meta.len());
        record["artifacts"]["debugCsvBytes"] = json!(debug_bytes);
        record["artifacts"]["fieldFileCount"] = json!(field_files.len());
        record["artifacts"]["fieldFiles"] = json!(field_artifacts);
        record["solver"]["stdoutBytes"] = json!(fs::metadata(&stdout_path)?.len());
        write_json_replace(&run_path, &record)?;
        let output_files = ["run.json", "request.json"]
            .iter()
            .map(|name| format!("{run_relative}/{name}"))
            .chain(run_artifacts::completion_solver_log_files(&run_relative))
            .chain(completion_mesh_log_files(
                &run_relative,
                &run_dir,
                &case_meshes,
            )?)
            .chain(std::iter::once(format!("{run_relative}/output/{csv_name}")))
            .chain(std::iter::once(format!(
                "{run_relative}/output/{debug_csv}"
            )))
            .chain(
                field_files
                    .iter()
                    .map(|name| format!("{run_relative}/output/{name}")),
            )
            .collect::<Vec<_>>();
        sink.emit(json!({"type":"complete","state":"success","message":format!("Completed {total_points} of {total_points} {profile_id} frequency points"),"totalPoints":total_points,"completedPoints":total_points,"runId":run_id,"runDirectory":run_relative,"csv":format!("output/{csv_name}"),"debugCsv":format!("output/{debug_csv}"),"fields":field_files,"outputFiles":output_files,"dimension":dimension,"elementFamily":element_family}));
        Ok(())
    })();
    if let Err(error) = execute {
        record["state"] = json!("failed");
        record["completedAt"] = json!(timestamp());
        record["error"] = json!(error.0);
        record["progress"]["completedPoints"] = json!(completed);
        let _ = write_json_replace(&run_path, &record);
        if cli {
            return Err(error);
        }
        sink.error(&error.0, total_points, completed, Some(&run_relative));
    }
    Ok(())
}

fn run_model_em3d(
    root: &Path,
    model_folder: &str,
    project: &Path,
    model: &Value,
    model_filename: &str,
    runs_directory_name: &str,
    supplied_model_hash: Option<&str>,
    study_id: &str,
    component_id: &str,
    normalized: &Value,
    mesh_exe: &Path,
    sink: &EventSink,
    cli: bool,
) -> Result<()> {
    let base_request = normalized
        .get("request")
        .cloned()
        .ok_or("Rust Study preparation did not return a solver request")?;
    let solver_backend = base_request
        .get("solverBackend")
        .and_then(Value::as_str)
        .ok_or("Prepared EM3D request omits its solver backend.")?
        .to_owned();
    let base_cases = base_request
        .get("frequencyCases")
        .and_then(Value::as_array)
        .ok_or("Prepared EM3D request has no frequencyCases")?;
    let total_points = base_cases.len();
    let announced_run_id = run_artifacts::coordinated_run_id();
    if let Some(id) = &announced_run_id {
        update_preparation_stage(
            &project.join(runs_directory_name).join(id),
            "geometry",
            total_points,
        )?;
    }
    sink.emit(json!({"type":"progress","state":"running","stage":"geometry","message":"Preparing Study geometry…",
        "totalPoints":total_points,"completedPoints":0,"runId":announced_run_id}));
    let maximum_points = maximum_sweep_points()?;
    if total_points == 0 || total_points > maximum_points {
        return Err(format!(
            "The EM3D Study must contain between 1 and {maximum_points} sweep cases."
        )
        .into());
    }
    let case_model_configurations = normalized
        .get("caseModelConfigurations")
        .and_then(Value::as_array)
        .ok_or("Shared Study preparation did not return caseModelConfigurations")?;
    if case_model_configurations.len() != total_points {
        return Err("Prepared EM3D configurations and sweep cases have different lengths.".into());
    }
    let study_snapshot = normalized
        .get("study")
        .cloned()
        .ok_or("Rust Study preparation did not return a Study snapshot")?;
    let model_hash = match supplied_model_hash {
        Some(value) if value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()) => {
            value.to_string()
        }
        Some(_) => return Err("The editor supplied an invalid model content hash.".into()),
        None => canonical_hash(model)?,
    };
    let model_id = model
        .pointer("/metadata/id")
        .and_then(Value::as_str)
        .ok_or("metadata.id is required")?;
    let field_files = base_request
        .pointer("/modelConfiguration/fieldFiles")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let field_prefix = base_request
        .get("fieldFilePrefix")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let sweep_csv = base_request
        .get("sweepCsv")
        .and_then(Value::as_str)
        .ok_or("Prepared Study is missing sweepCsv")?
        .to_string();
    let debug_csv = base_request
        .get("debugCsv")
        .and_then(Value::as_str)
        .ok_or("Prepared Study is missing debugCsv")?
        .to_string();
    let cases_ghz = base_cases
        .iter()
        .map(|case| {
            case.get("frequencyHz")
                .and_then(Value::as_f64)
                .map(|value| value / 1e9)
                .ok_or_else(|| Failure("Prepared EM3D case is missing frequencyHz".into()))
        })
        .collect::<Result<Vec<_>>>()?;
    let case_models = base_cases
        .iter()
        .map(|case| model_for_sweep_case(model, study_id, case))
        .collect::<Result<Vec<_>>>()?;

    let runs_dir = ensure_real_child_directory(project, runs_directory_name)?;
    let (compiled, prepared_mesh) = compile_and_prepare_run_mesh(
        mesh_exe,
        root,
        project,
        &runs_dir,
        &case_models[0],
        component_id,
        study_id,
    )?;
    let run_id = prepared_mesh.run_id.clone();
    let run_relative =
        relative_model_path(model_folder, &format!("{runs_directory_name}/{run_id}"));
    let run_dir = prepared_mesh.run_directory.clone();
    let output_dir = ensure_real_child_directory(&run_dir, "output")?;
    let logs_dir = ensure_real_child_directory(&run_dir, "logs")?;
    let case_work_root = ensure_real_child_directory(&run_dir, "case-work")?;
    let run_path = run_dir.join("run.json");
    let request_path = run_dir.join("request.json");
    write_json_new(&run_dir.join("model.json"), &case_models[0])?;
    let case_meshes = prepare_case_meshes(
        mesh_exe,
        root,
        &runs_dir,
        &run_dir,
        &logs_dir,
        &compiled,
        &prepared_mesh,
        &case_models,
        component_id,
        sink,
        total_points,
    )?;
    let primary_case = case_meshes
        .first()
        .ok_or("The prepared EM3D Study has no cases")?;
    let primary_mesh = run_relative_reference(&run_dir, &primary_case.paths.mesh)?;
    let primary_map = run_relative_reference(&run_dir, &primary_case.paths.map)?;
    let mut frequency_cases = Vec::with_capacity(total_points);
    for (index, (((raw_case, case_model), case_mesh), model_configuration)) in base_cases
        .iter()
        .zip(&case_models)
        .zip(&case_meshes)
        .zip(case_model_configurations)
        .enumerate()
    {
        let output_name = format!("case-{:04}", index + 1);
        let output_path = ensure_real_child_directory(&case_work_root, &output_name)?;
        write_json_new(&output_path.join("model.json"), case_model)?;
        let mut row = raw_case.clone();
        if let Some(object) = row.as_object_mut() {
            object.remove("modelConfiguration");
        }
        row["caseOverrides"] = json!({
            "meshFile":run_relative_reference(&run_dir,&case_mesh.paths.mesh)?,
            "meshMap":run_relative_reference(&run_dir,&case_mesh.paths.map)?,
            "outputDirectory":format!("case-work/{output_name}"),
            "fieldFilePrefix":field_prefix,
            "modelConfiguration":model_configuration
        });
        frequency_cases.push(row);
    }
    let mut request = base_request;
    request["meshFile"] = json!(primary_mesh);
    request["meshMap"] = json!(primary_map);
    request["outputDirectory"] = json!("output");
    request["frequencyCases"] = json!(frequency_cases);
    write_json_new(&request_path, &request)?;
    let formulation = study_snapshot
        .pointer("/discretization/fieldFormulation")
        .and_then(Value::as_str)
        .unwrap_or("");
    let geometry_case_records = case_meshes.iter().enumerate().map(|(index, case_mesh)| -> Result<Value> {
        Ok(json!({"caseIndex":index,"geometryHash":case_mesh.compiled.geometry_hash,"mesh":run_relative_reference(&run_dir,&case_mesh.paths.mesh)?,"map":run_relative_reference(&run_dir,&case_mesh.paths.map)?,"reused":case_mesh.reused,"meshSummary":{"nodes":case_mesh.report.get("nodes"),"tetrahedra":case_mesh.report.get("tetrahedra")}}))
    }).collect::<Result<Vec<_>>>()?;
    let mut record = json!({
        "schema":"fullmeta-run/1","kind":"study","runId":run_id,"state":"running","startedAt":timestamp(),"completedAt":null,
        "model":{"file":relative_model_path(model_folder, model_filename),"id":model_id,"modelFormat":model.get("modelFormat").and_then(Value::as_str).unwrap_or(MODEL_FORMAT_V2),"sha256":model_hash,"snapshot":model},
        "study":{"id":study_id,"name":study_snapshot.get("name").and_then(Value::as_str).unwrap_or("Study"),"snapshot":study_snapshot},
        "solver":{"executable":"em3d","formulation":formulation,"requestFile":"request.json","fieldFilePrefix":field_prefix,"fieldFiles":field_files,
            "backend":solver_backend,"ordering":request["solverOrdering"],"threads":request["solverThreads"],
            "elementOrder":request["elementOrder"],"constitutiveQuadrature":request["constitutiveQuadrature"]},
        "geometry":{"geometryHash":primary_case.compiled.geometry_hash,"source":run_relative_reference(&run_dir,&primary_case.paths.source)?,"msh":run_relative_reference(&run_dir,&primary_case.paths.msh)?,"mesh":run_relative_reference(&run_dir,&primary_case.paths.mesh)?,"report":run_relative_reference(&run_dir,&primary_case.paths.report)?,"map":run_relative_reference(&run_dir,&primary_case.paths.map)?,"cache":run_relative_reference(&run_dir,&primary_case.paths.cache)?,"files":null,"cases":geometry_case_records},
        "artifacts":{"csv":format!("output/{sweep_csv}"),"debugCsv":format!("output/{debug_csv}"),"stdout":"logs/solver.stdout.log","stderr":"logs/solver.stderr.log","meshLog":run_geometry_reference(&format!("{}-gmsh.log",primary_case.paths.source.file_stem().and_then(|value|value.to_str()).unwrap_or("model")) )},
        "progress":{"totalPoints":total_points,"completedPoints":0,"currentFrequencyGHz":null},"error":null
    });
    if field_files {
        record["artifacts"]["fields"] =
            json!("output/<field-prefix>_<frequency-GHz-to-12-decimals>GHz.json");
    }
    if run_artifacts::adopt_server_record(&run_path, &mut record)?
        || prepared_mesh.promoted_mesh_run
    {
        write_json_replace(&run_path, &record)?;
    } else {
        write_json_new(&run_path, &record)?;
    }

    let mut completed = 0usize;
    let execute = (|| -> Result<()> {
        sink.emit(json!({"type":"progress","state":"running","message":"Model validated; preparing geometry and mesh…","totalPoints":total_points,"completedPoints":0,"runId":run_id}));
        let resource_limits = mesh_resource_limits("em-3d")?;
        for (index, case_mesh) in case_meshes.iter().enumerate() {
            let nodes = case_mesh
                .report
                .get("nodes")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let cells = case_mesh
                .report
                .get("tetrahedra")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            check_mesh_resources("em-3d", resource_limits, nodes, cells, "tetrahedra", index)?;
            sink.emit(json!({"type":"mesh","state":"success","message":if case_mesh.reused {format!("Sweep case {} mesh ready (reused)",index+1)} else {format!("Sweep case {} mesh generated",index+1)},"runId":run_id,"mesh":{"nodes":nodes,"tetrahedra":cells,"caseIndex":index,"geometryHash":case_mesh.compiled.geometry_hash,"reused":case_mesh.reused,"materialDiagnostics":case_mesh.report.get("materialDiagnostics").cloned().unwrap_or_else(||json!([]))}}));
        }
        record["geometry"]["files"] = describe_artifacts(&primary_case.paths)?;
        write_json_replace(&run_path, &record)?;
        sink.emit(json!({"type":"progress","state":"running","message":"Rust solver is starting…","totalPoints":total_points,"completedPoints":0,"runId":run_id}));
        let stdout_log = logs_dir.join("solver.stdout.log");
        let stderr_log = logs_dir.join("solver.stderr.log");
        let solver_runtime = solver_runtime(root, "em-3d", &solver_backend)?;
        let solver = run_solver(
            solver_runtime,
            &request_path,
            total_points,
            &run_id,
            &mut completed,
            &mut record,
            &run_path,
            &stdout_log,
            &stderr_log,
            sink,
        )?;
        if solver != 0 {
            return Err(solver_failure(&stdout_log, &stderr_log, solver)?);
        }
        if completed != total_points {
            return Err("The solver exited before reporting success for every case.".into());
        }
        sink.emit(json!({"type":"progress","state":"running","stage":"verification","message":"Checking Study outputs…",
            "totalPoints":total_points,"completedPoints":completed,"runId":run_id}));
        let mut field_artifacts = Vec::new();
        if field_files {
            for (index, frequency_ghz) in cases_ghz.iter().copied().enumerate() {
                let case_directory = case_work_root.join(format!("case-{:04}", index + 1));
                field_artifacts.extend(run_artifacts::collect_case_field_files(
                    &case_directory,
                    &output_dir,
                    index,
                    &json!({"frequencyHz":frequency_ghz * 1e9}),
                    |name| name.starts_with(field_prefix.as_str()) && name.ends_with("GHz.json"),
                )?);
            }
        }
        let output_fields = run_artifacts::case_field_file_names(
            &output_dir,
            &field_artifacts,
            if field_files { total_points } else { 0 },
        )?;
        fs::remove_dir_all(&case_work_root)?;
        let csv_path = output_dir.join(&sweep_csv);
        let debug_path = output_dir.join(&debug_csv);
        let csv_meta = fs::metadata(&csv_path).map_err(|error| {
            Failure(format!(
                "The solver completed without creating its requested CSV output: {error}"
            ))
        })?;
        let debug_meta = fs::metadata(&debug_path).map_err(|error| {
            Failure(format!(
                "The solver completed without creating its requested debug CSV output: {error}"
            ))
        })?;
        if !csv_meta.is_file()
            || csv_meta.len() == 0
            || !debug_meta.is_file()
            || debug_meta.len() == 0
        {
            return Err(
                "The solver completed without creating both requested nonempty CSV outputs.".into(),
            );
        }
        record["state"] = json!("completed");
        record["completedAt"] = json!(timestamp());
        record["progress"]["completedPoints"] = json!(total_points);
        record["progress"]["currentFrequencyGHz"] = Value::Null;
        record["artifacts"]["csvBytes"] = json!(csv_meta.len());
        record["artifacts"]["debugCsvBytes"] = json!(debug_meta.len());
        record["artifacts"]["fieldFileCount"] = json!(output_fields.len());
        if field_files {
            record["artifacts"]["fieldFiles"] = json!(field_artifacts);
        }
        write_json_replace(&run_path, &record)?;
        let output_files = ["run.json", "request.json"]
            .iter()
            .map(|name| format!("{run_relative}/{name}"))
            .chain(run_artifacts::completion_solver_log_files(&run_relative))
            .chain(completion_mesh_log_files(
                &run_relative,
                &run_dir,
                &case_meshes,
            )?)
            .chain(std::iter::once(format!(
                "{run_relative}/output/{sweep_csv}"
            )))
            .chain(std::iter::once(format!(
                "{run_relative}/output/{debug_csv}"
            )))
            .chain(
                output_fields
                    .iter()
                    .map(|name| format!("{run_relative}/output/{name}")),
            )
            .collect::<Vec<_>>();
        sink.emit(json!({"type":"complete","state":"success","message":format!("Completed {total_points} of {total_points} frequencies"),"totalPoints":total_points,"completedPoints":total_points,"runId":run_id,"runDirectory":run_relative,"csv":format!("output/{sweep_csv}"),"debugCsv":format!("output/{debug_csv}"),"fields":output_fields,"outputFiles":output_files,"frequenciesGHz":cases_ghz}));
        Ok(())
    })();
    if let Err(error) = execute {
        record["state"] = json!("failed");
        record["completedAt"] = json!(timestamp());
        record["error"] = json!(error.0);
        record["progress"]["completedPoints"] = json!(completed);
        record["progress"]["currentFrequencyGHz"] = Value::Null;
        let _ = write_json_replace(&run_path, &record);
        if cli {
            return Err(error);
        }
        sink.error(&error.0, total_points, completed, Some(&run_relative));
    }
    Ok(())
}

fn validate_model(model: &Value) -> Result<()> {
    let errors = super::validate_native_model(model);
    if !errors.is_empty() {
        return Err(format!("Model JSON is invalid:\n{}", errors.join("\n")).into());
    }
    Ok(())
}

fn ensure_unique_model_id(model_directory: &Path, model: &Value, source_path: &Path) -> Result<()> {
    let id = model
        .pointer("/metadata/id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    for entry in fs::read_dir(model_directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || path == source_path
            || !entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".json")
        {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(candidate) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if candidate.get("modelFormat").and_then(Value::as_str) == Some(MODEL_FORMAT_V2)
            && candidate
                .pointer("/metadata/id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase()
                == id
        {
            return Err(format!(
                "Model ID {id:?} is also used by {} in this folder.",
                entry.file_name().to_string_lossy()
            )
            .into());
        }
    }
    Ok(())
}

fn executable(_root: &Path, stem: &str) -> Result<PathBuf> {
    for document in crate::json_schema::context()?.documents() {
        if let Some(programs) = document
            .pointer("/x-fullmeta-runtime/externalPrograms")
            .and_then(Value::as_object)
        {
            for (id, program) in programs {
                let name = program
                    .get("executable")
                    .and_then(Value::as_str)
                    .and_then(|value| value.rsplit('/').next())
                    .unwrap_or("");
                if name.strip_suffix(".exe").unwrap_or(name) == stem {
                    return Ok(crate::json_schema::external_program_path(id)?);
                }
            }
        }
    }
    Err(Failure(format!(
        "No external program is registered for {stem:?}."
    )))
}

fn solver_runtime(root: &Path, profile_id: &str, backend: &str) -> Result<super::SolverRuntime> {
    #[cfg(feature = "solver-native-acoustic2d")]
    if profile_id == "acoustic-2d" {
        return Ok(super::SolverRuntime::NativeAcoustic2d);
    }
    #[cfg(feature = "solver-native-acoustic3d")]
    if profile_id == "acoustic-3d" {
        return Ok(super::SolverRuntime::NativeAcoustic3d);
    }
    #[cfg(feature = "solver-native-em2d")]
    if profile_id == "em-2d" {
        return Ok(super::SolverRuntime::NativeEm2d);
    }
    #[cfg(feature = "solver-native-em3d")]
    if profile_id == "em-3d" {
        return Ok(super::SolverRuntime::NativeEm3d);
    }

    let capabilities = crate::json_schema::engine_capabilities()?;
    let runtime = capabilities.pointer("/study/solverRuntimesByProfile")
        .and_then(|profiles| profiles.get(profile_id))
        .and_then(|backends| backends.get(backend))
        .ok_or_else(|| Failure(format!("No executable solver runtime is registered for profile {profile_id:?} and backend {backend:?}.")))?;
    if runtime.get("kind").and_then(Value::as_str) != Some("executable") {
        return Err(Failure(format!(
            "The configured solver runtime for {profile_id:?} must be an executable."
        )));
    }
    let program_id = runtime
        .get("programId")
        .and_then(Value::as_str)
        .ok_or("Solver runtime is missing its registered program ID")?;
    let name = crate::json_schema::external_program_executable(program_id)?;
    if Path::new(name).file_name().and_then(|value| value.to_str()) != Some(name)
        || name.contains(['/', '\\', ':'])
    {
        return Err(Failure(format!(
            "Unsafe scenario executable name {name:?} in capability catalog."
        )));
    }
    let stem = name.strip_suffix(".exe").unwrap_or(name);
    let executable = executable(root, stem)?;
    let solve_argument = runtime
        .get("solveArgument")
        .and_then(Value::as_str)
        .unwrap_or("--solve")
        .to_string();
    let sequence_argument = runtime
        .get("sequenceArgument")
        .and_then(Value::as_str)
        .unwrap_or("--sequence")
        .to_string();
    Ok(super::SolverRuntime::External {
        executable,
        solve_argument,
        sequence_argument,
    })
}

fn prepare_study(model: &Value, study_id: &str, csv: Option<&str>) -> Result<Value> {
    let mut input = json!({"model":model,"studyId":study_id,"meshFile":"mesh-placeholder.mesh"});
    if let Some(filename) = csv.filter(|value| !value.trim().is_empty()) {
        input["csvFilename"] = json!(filename);
    }
    crate::preflight_client::prepare_study(&input)
        .map_err(|error| Failure(format!("Study preparation: {error}")))
}

struct ProcessResult {
    code: i32,
    stdout: String,
    stderr: String,
}
fn command(executable: &Path, args: &[&str], cwd: &Path) -> Result<ProcessResult> {
    let output = crate::model_schema::command(executable)?
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| Failure(format!("Could not start {}: {error}", executable.display())))?;
    Ok(ProcessResult {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
fn command_with_stdin(
    executable: &Path,
    args: &[&str],
    cwd: &Path,
    stdin: &[u8],
) -> Result<ProcessResult> {
    let mut child = crate::model_schema::command(executable)?
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| Failure(format!("Could not start {}: {error}", executable.display())))?;
    child
        .stdin
        .take()
        .ok_or("Could not open subprocess input")?
        .write_all(stdin)
        .map_err(|error| {
            Failure(format!(
                "Could not send a request to {}: {error}",
                executable.display()
            ))
        })?;
    let output = child.wait_with_output()?;
    Ok(ProcessResult {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[derive(Clone)]
struct CompiledGeometry {
    geometry_hash: String,
    geo_name: String,
    map_name: String,
}

struct CachedRunMesh {
    run_id: String,
    run_directory: PathBuf,
    time: String,
}

struct PreparedRunMesh {
    run_id: String,
    run_directory: PathBuf,
    paths: MeshPaths,
    promoted_mesh_run: bool,
    reused_mesh: bool,
}

fn find_reusable_run_mesh(project: &Path, geometry_hash: &str) -> Result<Option<CachedRunMesh>> {
    let runs_directory = project.join(configured_runs_directory());
    let entries = match fs::read_dir(&runs_directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry?;
        let run_meta = fs::symlink_metadata(entry.path())?;
        if run_meta.file_type().is_symlink() || !run_meta.is_dir() {
            continue;
        }
        let run_id = entry.file_name().to_string_lossy().to_string();
        if !run_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        {
            continue;
        }
        let run_directory = entry.path();
        let record: Value = match fs::read(run_directory.join("run.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            Some(record) => record,
            None => continue,
        };
        let state = record.get("state").and_then(Value::as_str).unwrap_or("");
        if state != "mesh-only"
            || record
                .pointer("/geometry/geometryHash")
                .and_then(Value::as_str)
                != Some(geometry_hash)
        {
            continue;
        }
        let source = record
            .pointer("/geometry/source")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(source_name) = source.strip_prefix("mesh/") else {
            continue;
        };
        if !safe_filename(source_name, ".geo") {
            continue;
        }
        let mesh_directory = run_directory.join("mesh");
        match fs::symlink_metadata(&mesh_directory) {
            Ok(meta) if !meta.file_type().is_symlink() && meta.is_dir() => (),
            _ => continue,
        }
        let source_path = mesh_directory.join(source_name);
        let paths = match MeshPaths::new(&source_path) {
            Ok(paths) => paths,
            Err(_) => continue,
        };
        if check_generated_paths(&paths).is_err()
            || read_valid_cache(&paths, geometry_hash)?.is_none()
        {
            continue;
        }
        let time = record
            .get("completedAt")
            .or_else(|| record.get("startedAt"))
            .and_then(Value::as_str)
            .unwrap_or(&run_id)
            .to_string();
        candidates.push(CachedRunMesh {
            run_id,
            run_directory,
            time,
        });
    }
    candidates.sort_by(|left, right| right.time.cmp(&left.time));
    Ok(candidates.into_iter().next())
}

fn prepare_run_mesh(
    runs_directory: &Path,
    study_id: &str,
    geometry_directory: &Path,
    compiled: &CompiledGeometry,
    cached: Option<CachedRunMesh>,
) -> Result<PreparedRunMesh> {
    // A mesh-only folder is a prepared run awaiting Study output. Completed and
    // failed Study runs remain self-contained and are never used as a new run's cache.
    let coordinated = run_artifacts::coordinated_run_id();
    let promote = coordinated.is_none() && cached.is_some();
    let run_id = coordinated.clone().unwrap_or_else(|| {
        cached
            .as_ref()
            .map(|mesh| mesh.run_id.clone())
            .unwrap_or_else(|| make_run_id(study_id))
    });
    let run_directory = runs_directory.join(&run_id);
    if coordinated.is_some() {
        let manifest = fs::symlink_metadata(run_directory.join("run.json"))?;
        if !manifest.is_file() || manifest.file_type().is_symlink() {
            return Err("The coordinated Study run manifest must be a regular file.".into());
        }
    } else if !promote {
        fs::create_dir(&run_directory).map_err(|error| {
            Failure(format!(
                "Could not create run folder {}: {error}",
                run_directory.display()
            ))
        })?;
    } else {
        let cached_directory = &cached.as_ref().unwrap().run_directory;
        if cached_directory != &run_directory {
            return Err("A reusable mesh-only run resolved to an unexpected folder.".into());
        }
    }
    let mesh_directory = ensure_real_child_directory(&run_directory, "mesh")?;
    let geometry_source = geometry_directory.join(&compiled.geo_name);
    if !promote {
        fs::copy(&geometry_source, mesh_directory.join(&compiled.geo_name))?;
        fs::copy(
            geometry_directory.join(&compiled.map_name),
            mesh_directory.join(&compiled.map_name),
        )?;
    }
    let source_path = mesh_directory.join(&compiled.geo_name);
    let paths = MeshPaths::new(&source_path)?;
    check_generated_paths(&paths)?;
    if let (Some(_), Some(cache)) = (coordinated, cached.as_ref()) {
        let previous = MeshPaths::new(&cache.run_directory.join("mesh").join(&compiled.geo_name))?;
        if let Some(mut report) = read_valid_cache(&previous, &compiled.geometry_hash)? {
            for (from, to) in [
                (&previous.msh, &paths.msh),
                (&previous.mesh, &paths.mesh),
                (&previous.gmsh_log, &paths.gmsh_log),
                (&previous.map, &paths.map),
            ] {
                fs::copy(from, to)?;
            }
            report["geometry"] = json!(paths.source);
            report["msh"] = json!(paths.msh);
            report["mesh"] = json!(paths.mesh);
            report["gmshLog"] = json!(paths.gmsh_log);
            write_json_new(&paths.report, &report)?;
            write_mesh_cache(&paths, &compiled.geometry_hash, &report)?;
        }
    }
    Ok(PreparedRunMesh {
        run_id,
        run_directory,
        paths,
        promoted_mesh_run: promote,
        reused_mesh: cached.is_some(),
    })
}

fn compile_geometry(
    mesh_exe: &Path,
    root: &Path,
    model: &Value,
    component: &str,
    output_dir: &Path,
) -> Result<CompiledGeometry> {
    let result = command_with_stdin(
        mesh_exe,
        &[
            "compile",
            "-",
            "--component",
            component,
            "--output-dir",
            &output_dir.to_string_lossy(),
        ],
        root,
        &serde_json::to_vec(model)?,
    )?;
    if result.code != 0 {
        return Err(format!(
            "Geometry compilation failed: {}",
            tail(&result.stderr).trim()
        )
        .into());
    }
    let value: Value = serde_json::from_str(&result.stdout).map_err(|error| {
        Failure(format!(
            "The Rust mesh tool returned invalid geometry metadata: {error}"
        ))
    })?;
    let hash = value
        .get("geometryHash")
        .and_then(Value::as_str)
        .ok_or("Geometry compiler omitted geometryHash")?;
    let geo = value
        .get("geoName")
        .and_then(Value::as_str)
        .ok_or("Geometry compiler omitted geoName")?;
    let map = value
        .get("mapName")
        .and_then(Value::as_str)
        .ok_or("Geometry compiler omitted mapName")?;
    if hash.len() != 64
        || !hash.chars().all(|ch| ch.is_ascii_hexdigit())
        || !safe_filename(geo, ".geo")
        || !safe_filename(map, "-model-map.json")
    {
        return Err("The geometry compiler returned invalid artifact metadata.".into());
    }
    for name in [geo, map] {
        let path = output_dir.join(name);
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(
                "The mesh tool did not safely create the expected geometry artifacts.".into(),
            );
        }
    }
    Ok(CompiledGeometry {
        geometry_hash: hash.to_string(),
        geo_name: geo.to_string(),
        map_name: map.to_string(),
    })
}

fn compile_and_prepare_run_mesh(
    mesh_exe: &Path,
    root: &Path,
    project: &Path,
    runs_directory: &Path,
    model: &Value,
    component: &str,
    study_id: &str,
) -> Result<(CompiledGeometry, PreparedRunMesh)> {
    let staging_name = format!(".geometry-{}", make_run_id(study_id));
    let staging_directory = ensure_real_child_directory(runs_directory, &staging_name)?;
    let result = (|| {
        let compiled = compile_geometry(mesh_exe, root, model, component, &staging_directory)?;
        let cached_mesh = find_reusable_run_mesh(project, &compiled.geometry_hash)?;
        let prepared = prepare_run_mesh(
            runs_directory,
            study_id,
            &staging_directory,
            &compiled,
            cached_mesh,
        )?;
        Ok((compiled, prepared))
    })();
    let cleanup_result = fs::remove_dir_all(&staging_directory);
    match (result, cleanup_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => {
            Err(format!("Could not remove temporary run geometry: {error}").into())
        }
    }
}

fn safe_filename(value: &str, suffix: &str) -> bool {
    value.ends_with(suffix)
        && value.len() <= 120
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
        && !value.starts_with('.')
}
fn file_name(path: &Path) -> Result<&str> {
    path.file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| Failure("Invalid generated artifact filename".into()))
}

fn ensure_real_child_directory(parent: &Path, name: &str) -> Result<PathBuf> {
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
            return Err(format!("Generated {name} path must be a real directory.").into());
        }
        _ => {}
    }
    fs::create_dir_all(&path)?;
    // `name` is a fixed internal child name and the symlink check above prevents
    // the directory from redirecting outside the project. Keep the absolute path
    // as supplied so Windows installs do not depend on final-path resolution.
    Ok(path)
}

#[derive(Clone)]
struct MeshPaths {
    source: PathBuf,
    msh: PathBuf,
    mesh: PathBuf,
    gmsh_log: PathBuf,
    report: PathBuf,
    map: PathBuf,
    cache: PathBuf,
}
impl MeshPaths {
    fn new(source: &Path) -> Result<Self> {
        let stem = source
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("Invalid geometry source filename")?;
        let parent = source
            .parent()
            .ok_or("Geometry source has no parent folder")?;
        Ok(Self {
            source: source.to_path_buf(),
            msh: parent.join(format!("{stem}.msh")),
            mesh: parent.join(format!("{stem}-mesh.mesh")),
            gmsh_log: parent.join(format!("{stem}-gmsh.log")),
            report: parent.join(format!("{stem}-mesh-report.json")),
            map: parent.join(format!("{stem}-model-map.json")),
            cache: parent.join(format!("{stem}-mesh-cache.json")),
        })
    }
    fn cache_files(&self) -> [(&'static str, &Path); 6] {
        [
            ("source", &self.source),
            ("msh", &self.msh),
            ("mesh", &self.mesh),
            ("report", &self.report),
            ("map", &self.map),
            ("gmshLog", &self.gmsh_log),
        ]
    }
}

#[derive(Clone)]
struct PreparedCaseMesh {
    compiled: CompiledGeometry,
    paths: MeshPaths,
    report: Value,
    reused: bool,
}

fn run_relative_reference(run_directory: &Path, path: &Path) -> Result<String> {
    let relative = path.strip_prefix(run_directory).map_err(|_| {
        Failure(format!(
            "Generated solver input {} is outside its run directory.",
            path.display()
        ))
    })?;
    let reference = relative.to_string_lossy().replace('\\', "/");
    if reference.is_empty() || reference.starts_with("../") || reference.contains("/../") {
        return Err("Generated solver input path is not a safe run-relative reference.".into());
    }
    Ok(reference)
}

fn completion_mesh_log_files(
    run_relative: &str,
    run_directory: &Path,
    case_meshes: &[PreparedCaseMesh],
) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for case_mesh in case_meshes {
        let path = run_relative_reference(run_directory, &case_mesh.paths.gmsh_log)?;
        let path = format!("{run_relative}/{path}");
        if !files.contains(&path) {
            files.push(path);
        }
    }
    Ok(files)
}

fn prepare_case_meshes(
    mesh_exe: &Path,
    root: &Path,
    runs_directory: &Path,
    run_directory: &Path,
    logs_directory: &Path,
    initial_geometry: &CompiledGeometry,
    initial_mesh: &PreparedRunMesh,
    case_models: &[Value],
    component_id: &str,
    sink: &EventSink,
    total_points: usize,
) -> Result<Vec<PreparedCaseMesh>> {
    if case_models.is_empty() {
        return Err("The prepared Study has no cases to mesh.".into());
    }
    let case_mesh_root = ensure_real_child_directory(&run_directory.join("mesh"), "cases")?;
    let mut geometry_by_hash = HashMap::<String, PreparedCaseMesh>::new();
    let mut geometry_by_model = HashMap::<String, PreparedCaseMesh>::new();
    let mut case_meshes = Vec::with_capacity(case_models.len());
    for (index, model) in case_models.iter().enumerate() {
        let model_hash = canonical_hash(model)?;
        let prepared = if let Some(existing) = geometry_by_model.get(&model_hash) {
            let mut reused = existing.clone();
            reused.reused = true;
            reused
        } else if index == 0 {
            let (report, cache_reused) = get_or_generate_mesh(
                mesh_exe,
                root,
                &initial_mesh.paths,
                &initial_geometry.geometry_hash,
                sink,
                total_points,
                &initial_mesh.run_id,
                run_directory,
                logs_directory,
            )?;
            PreparedCaseMesh {
                compiled: initial_geometry.clone(),
                paths: initial_mesh.paths.clone(),
                report,
                reused: initial_mesh.reused_mesh || cache_reused,
            }
        } else {
            let staging_name = format!(".case-geometry-{}-{index:04}", initial_mesh.run_id);
            let staging = ensure_real_child_directory(runs_directory, &staging_name)?;
            let attempt = (|| -> Result<PreparedCaseMesh> {
                let compiled = compile_geometry(mesh_exe, root, model, component_id, &staging)?;
                if let Some(existing) = geometry_by_hash.get(&compiled.geometry_hash) {
                    let mut reused = existing.clone();
                    reused.reused = true;
                    return Ok(reused);
                }
                let geometry_directory = ensure_real_child_directory(
                    &case_mesh_root,
                    &format!("geometry-{}", compiled.geometry_hash),
                )?;
                fs::copy(
                    staging.join(&compiled.geo_name),
                    geometry_directory.join(&compiled.geo_name),
                )?;
                fs::copy(
                    staging.join(&compiled.map_name),
                    geometry_directory.join(&compiled.map_name),
                )?;
                let paths = MeshPaths::new(&geometry_directory.join(&compiled.geo_name))?;
                check_generated_paths(&paths)?;
                let (report, cache_reused) = get_or_generate_mesh(
                    mesh_exe,
                    root,
                    &paths,
                    &compiled.geometry_hash,
                    sink,
                    total_points,
                    &initial_mesh.run_id,
                    run_directory,
                    logs_directory,
                )?;
                Ok(PreparedCaseMesh {
                    compiled,
                    paths,
                    report,
                    reused: cache_reused,
                })
            })();
            let cleanup = fs::remove_dir_all(&staging);
            match (attempt, cleanup) {
                (Ok(value), Ok(())) => value,
                (Err(error), _) => return Err(error),
                (Ok(_), Err(error)) => {
                    return Err(
                        format!("Could not remove temporary sweep geometry: {error}").into(),
                    );
                }
            }
        };
        geometry_by_hash
            .entry(prepared.compiled.geometry_hash.clone())
            .or_insert_with(|| prepared.clone());
        geometry_by_model
            .entry(model_hash)
            .or_insert_with(|| prepared.clone());
        case_meshes.push(prepared);
    }
    Ok(case_meshes)
}

fn check_generated_paths(paths: &MeshPaths) -> Result<()> {
    for path in [
        &paths.source,
        &paths.msh,
        &paths.mesh,
        &paths.gmsh_log,
        &paths.report,
        &paths.map,
        &paths.cache,
    ] {
        if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            return Err(format!(
                "Generated artifact path is a symbolic link: {}.",
                path.display()
            )
            .into());
        }
    }
    Ok(())
}

fn update_preparation_stage(run_directory: &Path, stage: &str, total_points: usize) -> Result<()> {
    let path = run_directory.join("run.json");
    let mut record: Value = serde_json::from_slice(&fs::read(&path)?)?;
    record["progress"]["stage"] = json!(stage);
    record["progress"]["totalPoints"] = json!(total_points);
    record["progress"]["stageStartedAt"] = json!(timestamp());
    write_json_replace(&path, &record)
}

fn get_or_generate_mesh(
    mesh_exe: &Path,
    root: &Path,
    paths: &MeshPaths,
    geometry_hash: &str,
    sink: &EventSink,
    total: usize,
    run_id: &str,
    run_directory: &Path,
    logs_dir: &Path,
) -> Result<(Value, bool)> {
    if let Some(report) = read_valid_cache(paths, geometry_hash)? {
        return Ok((report, true));
    }
    if run_artifacts::coordinated_run_id().is_some() {
        update_preparation_stage(run_directory, "mesh", total)?;
    }
    sink.emit(
        json!({"type":"progress","event":"stage-start","state":"running","stage":"mesh",
        "message":"Generating Gmsh mesh…","totalPoints":total,"completedPoints":0,"runId":run_id}),
    );
    let output = command(
        mesh_exe,
        &["mesh", &paths.source.to_string_lossy()],
        paths.source.parent().unwrap_or(root),
    )?;
    fs::write(logs_dir.join("mesh.stdout.log"), &output.stdout)?;
    fs::write(logs_dir.join("mesh.stderr.log"), &output.stderr)?;
    if output.code != 0 {
        return Err(format!("Mesh generation failed: {}", tail(&output.stderr).trim()).into());
    }
    let report = read_complete_report(paths)?
        .ok_or("Mesh tool succeeded without a complete .msh, .mesh, and non-empty mesh report")?;
    write_mesh_cache(paths, geometry_hash, &report)?;
    Ok((report, false))
}

fn read_valid_cache(paths: &MeshPaths, geometry_hash: &str) -> Result<Option<Value>> {
    let cache = match fs::read(&paths.cache) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).ok(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(_) => None,
    };
    let Some(cache) = cache else {
        return Ok(None);
    };
    if cache.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-cache/3")
        || cache.get("geometryHash").and_then(Value::as_str) != Some(geometry_hash)
    {
        return Ok(None);
    }
    let Some(files) = cache.get("files").and_then(Value::as_object) else {
        return Ok(None);
    };
    for (name, path) in paths.cache_files() {
        let Some(expected) = files.get(name) else {
            return Ok(None);
        };
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(_) => return Ok(None),
        };
        if bytes.is_empty()
            || expected.get("path").and_then(Value::as_str)
                != path.file_name().and_then(|v| v.to_str())
            || expected.get("bytes").and_then(Value::as_u64) != Some(bytes.len() as u64)
            || expected.get("sha256").and_then(Value::as_str)
                != Some(sha256::hex_digest(&bytes).as_str())
        {
            return Ok(None);
        }
    }
    let Some(report) = read_complete_report(paths)? else {
        return Ok(None);
    };
    if report.get("nodes") != cache.pointer("/mesh/nodes")
        || report.get("triangles") != cache.pointer("/mesh/triangles")
        || report.get("tetrahedra") != cache.pointer("/mesh/tetrahedra")
    {
        return Ok(None);
    }
    Ok(Some(report))
}

fn read_complete_report(paths: &MeshPaths) -> Result<Option<Value>> {
    for path in [&paths.msh, &paths.mesh, &paths.report] {
        match fs::metadata(path) {
            Ok(meta) if meta.is_file() && meta.len() > 0 => {}
            _ => return Ok(None),
        }
    }
    let report: Value = match fs::read(&paths.report)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(value) => value,
        None => return Ok(None),
    };
    let has_cells = ["triangles", "tetrahedra"]
        .iter()
        .any(|key| report.get(*key).and_then(Value::as_u64).unwrap_or(0) > 0);
    if report.get("nodes").and_then(Value::as_u64).unwrap_or(0) == 0 || !has_cells {
        return Ok(None);
    }
    Ok(Some(report))
}

fn write_mesh_cache(paths: &MeshPaths, geometry_hash: &str, report: &Value) -> Result<()> {
    let mut files = serde_json::Map::new();
    for (name, path) in paths.cache_files() {
        let bytes = fs::read(path).map_err(|error| {
            Failure(format!(
                "Could not hash mesh cache input {}: {error}",
                path.display()
            ))
        })?;
        files.insert(name.into(), json!({"path":file_name(path)?,"bytes":bytes.len(),"sha256":sha256::hex_digest(&bytes)}));
    }
    let mut mesh = json!({"nodes":report["nodes"]});
    if let Some(triangles) = report.get("triangles") {
        mesh["triangles"] = triangles.clone();
    }
    if let Some(tetrahedra) = report.get("tetrahedra") {
        mesh["tetrahedra"] = tetrahedra.clone();
    }
    let cache = json!({"schema":"fullmeta-mesh-cache/3","geometryHash":geometry_hash,"createdAt":timestamp(),"mesh":mesh,"files":files});
    write_json_replace(&paths.cache, &cache)
}

fn describe_artifacts(paths: &MeshPaths) -> Result<Value> {
    let mut files = serde_json::Map::new();
    for (name, source) in [
        ("source", &paths.source),
        ("msh", &paths.msh),
        ("mesh", &paths.mesh),
        ("report", &paths.report),
        ("map", &paths.map),
        ("gmshLog", &paths.gmsh_log),
        ("cache", &paths.cache),
    ] {
        let before = fs::symlink_metadata(source)
            .map_err(|error| Failure(format!("Could not snapshot {name}: {error}")))?;
        if before.file_type().is_symlink() || !before.is_file() {
            return Err(format!(
                "Study mesh artifact {name} must be a regular file: {}.",
                source.display()
            )
            .into());
        }
        let bytes = fs::read(source)?;
        let after = fs::symlink_metadata(source)?;
        if after.file_type().is_symlink()
            || !after.is_file()
            || after.len() != before.len()
            || after.modified().ok() != before.modified().ok()
            || bytes.len() as u64 != before.len()
        {
            return Err(format!(
                "Study mesh artifact {name} changed while its run record was being created."
            )
            .into());
        }
        let artifact_name = file_name(source)?;
        files.insert(name.into(), json!({"path":run_geometry_reference(&artifact_name),"bytes":bytes.len(),"sha256":sha256::hex_digest(&bytes)}));
    }
    Ok(Value::Object(files))
}

fn run_geometry_reference(filename: &str) -> String {
    format!("mesh/{filename}")
}

fn run_solver(
    solver_runtime: super::SolverRuntime,
    request_path: &Path,
    total: usize,
    run_id: &str,
    completed: &mut usize,
    record: &mut Value,
    run_path: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
    sink: &EventSink,
) -> Result<i32> {
    let mut stdout_log = File::create(stdout_path)?;
    let mut timings = run_artifacts::SolverStageTimings::create(
        &stdout_path.with_file_name("solver-stage-timings.csv"),
    )?;
    record["solver"]["stageTimings"] = json!("logs/solver-stage-timings.csv");
    let mut callback_error = None;
    let result = super::coordinate_solver_sweep_with(solver_runtime, request_path, &mut |line| {
        if callback_error.is_none() {
            let line_result = (|| -> Result<()> {
                writeln!(stdout_log, "{line}")?;
                stdout_log.flush()?;
                report_solver_line(
                    line,
                    total,
                    run_id,
                    completed,
                    record,
                    run_path,
                    sink,
                    &mut timings,
                )
            })();
            if let Err(error) = line_result {
                callback_error = Some(error);
            }
        }
    });
    let exit_code = match result {
        Ok(()) => {
            if stdout_log.metadata()?.len() == 0 {
                writeln!(stdout_log, "Solver completed without standard output.")?;
            }
            fs::write(
                stderr_path,
                "Solver completed without standard error output.\n",
            )?;
            0
        }
        Err(error) => {
            fs::write(stderr_path, error.to_string())?;
            1
        }
    };
    timings.finish()?;
    if let Some(error) = callback_error {
        return Err(error);
    }
    Ok(exit_code)
}

fn report_solver_line(
    line: &str,
    total: usize,
    run_id: &str,
    completed: &mut usize,
    record: &mut Value,
    run_path: &Path,
    sink: &EventSink,
    timings: &mut run_artifacts::SolverStageTimings,
) -> Result<()> {
    if let Some((stage, message)) = run_artifacts::solver_stage(line)? {
        let frequency = record
            .pointer("/progress/currentFrequencyHz")
            .and_then(Value::as_f64)
            .ok_or("Solver stage event arrived outside an active frequency case.")?;
        timings.start(*completed, Some(frequency), stage)?;
        record["progress"]["stage"] = json!(stage);
        record["progress"]["stageStartedAt"] = json!(timestamp());
        write_json_replace(run_path, record)?;
        sink.emit(
            json!({"type":"progress","event":"stage-start","state":"running","stage":stage,
            "message":message,"currentFrequencyHz":frequency,"totalPoints":total,
            "completedPoints":*completed,"runId":run_id}),
        );
        return Ok(());
    }
    let Some(payload) = line.strip_prefix(super::SOLVER_PROGRESS_PREFIX) else {
        return Ok(());
    };
    let progress: Value = serde_json::from_str(payload)
        .map_err(|error| Failure(format!("Invalid solver progress event: {error}")))?;
    let case_index = progress
        .get("caseIndex")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|index| *index < total)
        .ok_or("Solver progress event has an invalid caseIndex.")?;
    if progress.get("totalCases").and_then(Value::as_u64) != Some(total as u64) {
        return Err("Solver progress event totalCases does not match the requested sweep.".into());
    }
    let frequency_hz = progress
        .get("frequencyHz")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or("Solver progress event has an invalid frequencyHz.")?;
    if case_index != *completed {
        return Err(format!(
            "Solver progress event for case {} is out of order; expected case {}.",
            case_index + 1,
            *completed + 1
        )
        .into());
    }
    let message = match progress.get("event").and_then(Value::as_str) {
        Some("case-start") => {
            record["progress"]["currentFrequencyHz"] = json!(frequency_hz);
            record["progress"]["currentFrequencyGHz"] = json!(frequency_hz / 1e9);
            format!(
                "Solving case {} of {total}: {frequency_hz} Hz",
                case_index + 1
            )
        }
        Some("case-complete") => {
            timings.finish()?;
            *completed += 1;
            record["progress"]["completedPoints"] = json!(*completed);
            record["progress"]["currentFrequencyHz"] = Value::Null;
            record["progress"]["currentFrequencyGHz"] = Value::Null;
            format!("Completed {} of {total} cases", *completed)
        }
        _ => return Err("Solver progress event has an unknown event type.".into()),
    };
    write_json_replace(run_path, record)?;
    sink.emit(
        json!({"type":"progress","event":progress["event"],"state":"running","message":message,
        "currentFrequencyHz":record["progress"]["currentFrequencyHz"],
        "totalPoints":total,"completedPoints":*completed,"runId":run_id}),
    );
    Ok(())
}

fn solver_failure(stdout: &Path, stderr: &Path, code: i32) -> Result<Failure> {
    let stderr_text = fs::read_to_string(stderr).unwrap_or_default();
    let stdout_text = fs::read_to_string(stdout).unwrap_or_default();
    let details = if stderr_text.trim().is_empty() {
        stdout_text
    } else {
        stderr_text
    };
    let details = tail(&details).trim().to_string();
    Ok(Failure(if details.is_empty() {
        format!("Rust solver exited with code {code}.")
    } else {
        details
    }))
}

fn write_json_new(path: &Path, value: &Value) -> Result<()> {
    let text = format!("{}\n", serde_json::to_string_pretty(value)?);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| Failure(format!("Could not create {}: {error}", path.display())))?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn write_json_replace(path: &Path, value: &Value) -> Result<()> {
    if path.file_name().and_then(|name| name.to_str()) == Some("run.json") {
        return run_record_guard::write_scenario_record(path, value).map_err(Failure);
    }
    let text = format!("{}\n", serde_json::to_string_pretty(value)?);
    let temp = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("run.json"),
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::rename(&temp, path).map_err(|error| {
        let _ = fs::remove_file(&temp);
        Failure(format!("Could not update {}: {error}", path.display()))
    })?;
    Ok(())
}

fn tail(value: &str) -> &str {
    value
        .get(value.len().saturating_sub(64 * 1024)..)
        .unwrap_or(value)
}

fn canonical_hash(value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(sha256::hex_digest(&bytes))
}

fn timestamp() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (elapsed.as_secs() / 86400) as i64;
    let seconds = elapsed.as_secs() % 86400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        elapsed.subsec_millis()
    )
}
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    (y + i64::from(m <= 2), m, d)
}
fn make_run_id(study: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let stamp = timestamp().replace([':', '.'], "-");
    let nonce = sha256::hex_digest(
        format!("{}-{}-{}", now.as_nanos(), std::process::id(), study).as_bytes(),
    );
    format!("{stamp}-{}-{study}", &nonce[..12])
}

#[cfg(test)]
mod mesh_cache_log_tests {
    use super::*;

    #[test]
    fn matching_cache_hashes_do_not_make_an_empty_gmsh_log_reusable() {
        let directory = std::env::temp_dir().join(make_run_id("mesh-log-regression"));
        fs::create_dir(&directory).unwrap();
        let result = (|| -> Result<()> {
            let paths = MeshPaths::new(&directory.join("case.geo"))?;
            for (_, path) in paths.cache_files() {
                fs::write(path, b"fixture")?;
            }
            let report = json!({"nodes": 3, "triangles": 1});
            fs::write(&paths.report, serde_json::to_vec(&report).unwrap())?;
            write_mesh_cache(&paths, "geometry", &report)?;
            assert!(read_valid_cache(&paths, "geometry")?.is_some());
            fs::write(&paths.gmsh_log, b"")?;
            // Recompute the cache receipt too: a matching zero-byte receipt
            // still must not reintroduce the completion failure on the next run.
            write_mesh_cache(&paths, "geometry", &report)?;
            assert!(read_valid_cache(&paths, "geometry")?.is_none());
            fs::write(&paths.gmsh_log, b"Gmsh invocation: exit code 0\n")?;
            write_mesh_cache(&paths, "geometry", &report)?;
            assert!(read_valid_cache(&paths, "geometry")?.is_some());
            Ok(())
        })();
        fs::remove_dir_all(directory).unwrap();
        result.unwrap();
    }
}
