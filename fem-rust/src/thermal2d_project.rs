//! Project orchestration for one stationary thermal Study. Geometry generation
//! and meshing are delegated to the shared mesh executable; the thermal module
//! owns preparation and its normalized numerical request. Whole-model
//! validation is provided by the common FEM dispatcher.

#[path = "run_artifacts.rs"]
mod run_artifacts;
#[path = "run_record_guard.rs"]
mod run_record_guard;
#[path = "../../mesh-rust/src/sha256.rs"]
mod sha256;

use crate::thermal2d;
use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MODEL_FILE: &str = "fem_model.json";
const RUNS_DIRECTORY: &str = "fem_runs";

struct Events {
    ndjson: bool,
}
impl Events {
    fn emit(&self, event: Value) {
        if self.ndjson {
            println!("{event}");
        } else {
            if let Some(message) = event.get("message").and_then(Value::as_str) {
                println!("{message}");
            }
            if event.get("type").and_then(Value::as_str) == Some("complete") {
                if let Some(directory) = event.get("runDirectory").and_then(Value::as_str) {
                    println!("Run folder: {directory}");
                }
            }
        }
        let _ = io::stdout().flush();
    }
    fn progress(&self, stage: &str, message: &str, run_id: Option<&str>) {
        self.emit(json!({"type":"progress","event":"stage-start","state":"running","stage":stage,"message":message,
            "runId":run_id,"totalPoints":1,"completedPoints":0}));
    }
}

fn hidden_command(path: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn regular_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(&metadata) || !metadata.is_file() {
        return Err(format!(
            "Expected a regular file without symbolic links: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn real_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "Expected a real directory without symbolic links: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn child_directory(parent: &Path, name: &str) -> Result<PathBuf> {
    real_directory(parent)?;
    let path = parent.join(name);
    match fs::symlink_metadata(&path) {
        Ok(_) => real_directory(&path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir(&path)?,
        Err(error) => return Err(error.into()),
    }
    real_directory(&path)?;
    Ok(path)
}

fn safe_name(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value != "."
        && value != ".."
        && value.chars().enumerate().all(|(index, character)| {
            if index == 0 {
                character.is_ascii_alphanumeric()
            } else {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            }
        })
}

fn project_folder(input: &str) -> Result<String> {
    let input = input.trim().replace('\\', "/");
    let folder = input.strip_prefix("projects/").unwrap_or(&input);
    if !safe_name(folder, 64) {
        return Err("The project must be one folder directly inside projects/.".into());
    }
    Ok(folder.to_string())
}

fn application_root() -> Result<PathBuf> {
    Ok(fullmeta_model::runtime_paths::installation_root()?)
}

fn project_directory(root: &Path, folder: &str) -> Result<PathBuf> {
    let projects = root.join("projects");
    real_directory(&projects)?;
    let project = projects.join(project_folder(folder)?);
    real_directory(&project)?;
    Ok(project)
}

fn mesh_executable() -> Result<PathBuf> {
    Ok(crate::model_schema::external_program_path("mesh")?)
}

struct RunLock {
    path: PathBuf,
    file: Option<File>,
}
impl RunLock {
    fn acquire(project: &Path) -> Result<Self> {
        let path = project.join(".fullmeta-run.lock");
        let open = || OpenOptions::new().write(true).create_new(true).open(&path);
        let mut file = match open() {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                regular_file(&path)?;
                let previous = fs::read_to_string(&path)?;
                let pid = previous.lines().find_map(|line| {
                    line.strip_prefix("pid=")?
                        .split_whitespace()
                        .next()?
                        .parse::<u32>()
                        .ok()
                });
                if pid.is_none_or(process_is_alive) || fs::read_to_string(&path)? != previous {
                    return Err(
                        "Another active FullMeta operation holds the project run lock.".into(),
                    );
                }
                fs::remove_file(&path)?;
                open()?
            }
            Err(error) => return Err(error.into()),
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
            return Err(error.into());
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

fn process_is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        let output = hidden_command("tasklist.exe")
            .args(["/FO", "CSV", "/NH", "/FI", &format!("PID eq {pid}")])
            .output();
        return output
            .map(|output| {
                if !output.status.success() {
                    return true;
                }
                String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                    line.split(',')
                        .nth(1)
                        .is_some_and(|field| field.trim().trim_matches('"') == pid.to_string())
                })
            })
            .unwrap_or(true);
    }
    #[cfg(unix)]
    {
        hidden_command("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(true)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        true
    }
}

fn write_json(path: &Path, value: &Value, replace: bool) -> Result<()> {
    if replace {
        return Ok(run_record_guard::write_scenario_record(path, value)?);
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    writeln!(file, "{}", serde_json::to_string_pretty(value)?)?;
    file.sync_all()?;
    drop(file);
    Ok(())
}

fn command(
    executable: &Path,
    arguments: &[&str],
    cwd: &Path,
    input: Option<&Value>,
) -> Result<Output> {
    let mut process = crate::model_schema::command(executable)?;
    process
        .args(arguments)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(input) = input {
        let mut child = process.stdin(Stdio::piped()).spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or("Could not open subprocess input.")?;
        stdin.write_all(&serde_json::to_vec(input)?)?;
        drop(stdin);
        Ok(child.wait_with_output()?)
    } else {
        Ok(process.stdin(Stdio::null()).output()?)
    }
}

fn successful_output(output: &Output, description: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.as_ref()
    } else {
        stderr.as_ref()
    };
    let detail = detail
        .chars()
        .rev()
        .take(16_000)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    Err(format!("{description} failed: {}", detail.trim()).into())
}

fn artifact_metadata(path: &Path, reference: &str) -> Result<Value> {
    regular_file(path)?;
    let bytes = fs::read(path)?;
    Ok(json!({"path":reference,"bytes":bytes.len(),"sha256":sha256::hex_digest(&bytes)}))
}

/// Reuse only a complete, content-verified cache. Copying into the new run
/// keeps each Study self-contained and preserves the earlier mesh-only record.
fn reuse_mesh(runs: &Path, current: &Path, stem: &str, geometry_hash: &str) -> Result<bool> {
    let mut candidates = fs::read_dir(runs)?.collect::<std::result::Result<Vec<_>, _>>()?;
    candidates.sort_by_key(|entry| std::cmp::Reverse(entry.file_name()));
    for entry in candidates {
        let directory = entry.path();
        if directory == current || real_directory(&directory).is_err() {
            continue;
        }
        let record = directory.join("run.json");
        if regular_file(&record).is_err() {
            continue;
        }
        let record: Value = match serde_json::from_slice(&fs::read(record)?) {
            Ok(record) => record,
            Err(_) => continue,
        };
        if !matches!(
            record.get("state").and_then(Value::as_str),
            Some("mesh-only" | "completed")
        ) || record
            .pointer("/geometry/geometryHash")
            .and_then(Value::as_str)
            != Some(geometry_hash)
        {
            continue;
        }
        let mesh = directory.join("mesh");
        if real_directory(&mesh).is_err() {
            continue;
        }
        let cache_path = mesh.join(format!("{stem}-mesh-cache.json"));
        if regular_file(&cache_path).is_err() {
            continue;
        }
        let cache: Value = match serde_json::from_slice(&fs::read(&cache_path)?) {
            Ok(cache) => cache,
            Err(_) => continue,
        };
        if cache.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-cache/3")
            || cache.get("geometryHash").and_then(Value::as_str) != Some(geometry_hash)
        {
            continue;
        }
        let suffixes = [
            ("source", ".geo"),
            ("map", "-model-map.json"),
            ("msh", ".msh"),
            ("mesh", "-mesh.mesh"),
            ("report", "-mesh-report.json"),
            ("gmshLog", "-gmsh.log"),
        ];
        let mut valid = true;
        for (key, suffix) in suffixes {
            let filename = format!("{stem}{suffix}");
            let path = mesh.join(&filename);
            if regular_file(&path).is_err() {
                valid = false;
                break;
            }
            let bytes = fs::read(path)?;
            let expected = &cache["files"][key];
            if bytes.is_empty()
                || expected.get("path").and_then(Value::as_str) != Some(filename.as_str())
                || expected.get("bytes").and_then(Value::as_u64) != Some(bytes.len() as u64)
                || expected.get("sha256").and_then(Value::as_str)
                    != Some(sha256::hex_digest(&bytes).as_str())
            {
                valid = false;
                break;
            }
        }
        if !valid {
            continue;
        }
        for (_, suffix) in suffixes {
            if matches!(suffix, ".geo" | "-model-map.json") {
                continue;
            }
            let filename = format!("{stem}{suffix}");
            fs::copy(mesh.join(&filename), current.join("mesh").join(filename))?;
        }
        return Ok(true);
    }
    Ok(false)
}

fn record_mesh_artifacts(mesh: &Path, stem: &str, hash: &str, report: &Value) -> Result<Value> {
    let suffixes = [
        ("source", ".geo"),
        ("map", "-model-map.json"),
        ("msh", ".msh"),
        ("mesh", "-mesh.mesh"),
        ("report", "-mesh-report.json"),
        ("gmshLog", "-gmsh.log"),
    ];
    let mut cache_files = serde_json::Map::new();
    let mut files = serde_json::Map::new();
    for (key, suffix) in suffixes {
        let filename = format!("{stem}{suffix}");
        cache_files.insert(
            key.to_string(),
            artifact_metadata(&mesh.join(&filename), &filename)?,
        );
        files.insert(
            key.to_string(),
            artifact_metadata(&mesh.join(&filename), &format!("mesh/{filename}"))?,
        );
    }
    let filename = format!("{stem}-mesh-cache.json");
    let cache = json!({"schema":"fullmeta-mesh-cache/3","geometryHash":hash,"createdAt":timestamp(),
        "mesh":{"nodes":report["nodes"],"triangles":report["triangles"]},"files":cache_files});
    write_json(&mesh.join(&filename), &cache, false)?;
    files.insert(
        "cache".into(),
        artifact_metadata(&mesh.join(&filename), &format!("mesh/{filename}"))?,
    );
    Ok(Value::Object(files))
}

pub(crate) fn run_saved_project(
    folder: &str,
    study_id: Option<&str>,
    csv: Option<&str>,
) -> Result<()> {
    let root = application_root()?;
    let folder = project_folder(folder)?;
    let project = project_directory(&root, &folder)?;
    let path = project.join(MODEL_FILE);
    regular_file(&path)?;
    let model: Value = serde_json::from_slice(&fs::read(&path)?)?;
    crate::model_schema::select_schema_for_model_file(&model, &path)?;
    run_project(
        &root,
        &project,
        &format!("projects/{folder}"),
        MODEL_FILE,
        &model,
        study_id,
        csv,
        &Events { ndjson: false },
    )
}

/// The server submits the edited model directly, without saving or replacing
/// the project manifest. The run's model snapshot records exactly those inputs.
pub(crate) fn run_server(input: &Value) -> Result<()> {
    let events = Events { ndjson: true };
    let model = if input.get("modelFormat").and_then(Value::as_str) == Some("fem-model/2") {
        input
    } else {
        input
            .get("model")
            .ok_or("A server run requires a FullMeta model.")?
    };
    let requested_study = input
        .get("studyId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            env::var("FULLMETA_STUDY_ID")
                .ok()
                .filter(|id| !id.trim().is_empty())
        });
    let outcome = (|| {
        let root = application_root()?;
        let folder = env::var("FULLMETA_MODEL_FOLDER").unwrap_or_else(|_| {
            format!(
                "projects/{}",
                model
                    .pointer("/metadata/id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        });
        let file = env::var("FULLMETA_MODEL_FILE").unwrap_or_else(|_| MODEL_FILE.into());
        let local_component = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.len() <= 200
                && !part
                    .chars()
                    .any(|character| character.is_control() || "\\/:*?\"<>|".contains(character))
        };
        if !local_component(&file)
            || file.starts_with(['.', ' '])
            || !file.to_ascii_lowercase().ends_with(".json")
        {
            return Err("The thermal model filename is invalid.".into());
        }
        let mut directory = root.clone();
        if folder != "." {
            for part in folder.split('/') {
                if !local_component(part) {
                    return Err("The thermal model folder is invalid.".into());
                }
                directory.push(part);
                real_directory(&directory)?;
            }
        }
        run_project(
            &root,
            &directory,
            &folder,
            &file,
            model,
            requested_study.as_deref(),
            None,
            &events,
        )
    })();
    if let Err(error) = &outcome {
        events.emit(json!({"type":"error","state":"error","message":error.to_string(),"totalPoints":1,"completedPoints":0}));
    }
    outcome
}

fn run_project(
    root: &Path,
    project: &Path,
    folder: &str,
    model_filename: &str,
    model: &Value,
    study_id: Option<&str>,
    csv: Option<&str>,
    events: &Events,
) -> Result<()> {
    thermal2d::validate_model(model)?;
    regular_file(&project.join(model_filename))?;
    let model_id = model
        .pointer("/metadata/id")
        .and_then(Value::as_str)
        .ok_or("metadata.id is required.")?;
    let _lock = RunLock::acquire(project)?;
    let studies = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .ok_or("model.model.study must be an array.")?;
    let study = match study_id {
        Some(id) => studies
            .iter()
            .find(|study| study.get("id").and_then(Value::as_str) == Some(id))
            .ok_or("The requested Study is absent from the model.")?,
        None => studies
            .first()
            .ok_or("The model does not contain a Study.")?,
    };
    let study_id = study
        .get("id")
        .and_then(Value::as_str)
        .ok_or("The selected Study has no ID.")?;
    let mut envelope = json!({"model":model,"studyId":study_id,"meshFile":"mesh/model-mesh.mesh"});
    if let Some(csv) = csv {
        envelope["csvFilename"] = json!(csv);
    }
    let plan = thermal2d::prepare_study(&envelope)?;
    let csv = plan
        .get("csvFilename")
        .and_then(Value::as_str)
        .ok_or("The stationary plan has no summary CSV name.")?;
    let fields_enabled = plan
        .get("fieldFiles")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mesh_executable = mesh_executable()?;
    let runs_directory =
        env::var("FULLMETA_RUNS_DIRECTORY").unwrap_or_else(|_| RUNS_DIRECTORY.into());
    if !safe_name(&runs_directory, 64) {
        return Err("The thermal run storage name is invalid.".into());
    }
    let runs = child_directory(project, &runs_directory)?;
    let coordinated = run_artifacts::coordinated_run_id();
    let run_id = coordinated.clone().unwrap_or_else(|| {
        format!(
            "{}-{}-{study_id}",
            timestamp().replace(':', "-"),
            std::process::id()
        )
    });
    let directory = runs.join(&run_id);
    if coordinated.is_some() {
        real_directory(&directory)?;
    } else {
        fs::create_dir(&directory)?;
    }
    let mesh_directory = child_directory(&directory, "mesh")?;
    let output_directory = child_directory(&directory, "output")?;
    let logs = child_directory(&directory, "logs")?;
    let request_path = directory.join("request.json");
    let record_path = directory.join("run.json");
    let mut request = plan["request"].clone();
    let mut record = json!({
        "schema":"fullmeta-run/1","kind":"study","runId":run_id,"state":"running","startedAt":timestamp(),"completedAt":null,
        "model":{"file":if folder == "." {model_filename.to_string()} else {format!("{folder}/{model_filename}")},"id":model_id,"modelFormat":"fem-model/2",
            "sha256":sha256::hex_digest(&serde_json::to_vec(model)?),"snapshot":model},
        "study":{"id":study_id,"name":study.get("name"),"type":"stationary","snapshot":study},
        "solver":{"executable":"thermal2d","formulation":"T","studyType":"stationary","requestFile":"request.json",
            "backend":request["solverBackend"],"ordering":request["solverOrdering"],"threads":request["solverThreads"],
            "elementOrder":1,"constitutiveQuadrature":request["constitutiveQuadrature"],"fieldFiles":fields_enabled},
        "geometry":{},
        "artifacts":{"csv":format!("output/{csv}"),"fields":if fields_enabled {json!("output/temperature.json")} else {Value::Null},
            "fieldFiles":[],"stdout":"logs/solver.stdout.log","stderr":"logs/solver.stderr.log"},
        "progress":{"stage":"geometry","totalPoints":1,"completedPoints":0},"error":null
    });
    write_json(&directory.join("model.json"), model, false)?;
    let adopted = run_artifacts::adopt_server_record(&record_path, &mut record)?;
    write_json(&record_path, &record, adopted)?;
    events.progress(
        "geometry",
        "Model validated; preparing stationary thermal geometry and mesh…",
        Some(&run_id),
    );
    let execution = (|| -> Result<()> {
        let compiled = command(
            &mesh_executable,
            &[
                "compile",
                "-",
                "--component",
                "model",
                "--output-dir",
                &mesh_directory.to_string_lossy(),
            ],
            root,
            Some(model),
        )?;
        fs::write(logs.join("geometry.stdout.log"), &compiled.stdout)?;
        fs::write(logs.join("geometry.stderr.log"), &compiled.stderr)?;
        successful_output(&compiled, "Geometry compilation")?;
        let geometry: Value = serde_json::from_slice(&compiled.stdout)?;
        let geo_name = geometry
            .get("geoName")
            .and_then(Value::as_str)
            .ok_or("Geometry compiler omitted geoName.")?;
        let map_name = geometry
            .get("mapName")
            .and_then(Value::as_str)
            .ok_or("Geometry compiler omitted mapName.")?;
        let hash = geometry
            .get("geometryHash")
            .and_then(Value::as_str)
            .ok_or("Geometry compiler omitted its input hash.")?;
        if !safe_name(geo_name, 120)
            || !geo_name.ends_with(".geo")
            || !safe_name(map_name, 120)
            || hash.len() != 64
            || !hash.chars().all(|character| character.is_ascii_hexdigit())
        {
            return Err("The mesh tool returned invalid geometry artifact names or hash.".into());
        }
        let stem = geo_name.strip_suffix(".geo").unwrap();
        if map_name != format!("{stem}-model-map.json") {
            return Err("The mesh map filename does not match its geometry source.".into());
        }
        regular_file(&mesh_directory.join(geo_name))?;
        regular_file(&mesh_directory.join(map_name))?;
        request["meshFile"] = json!(format!("mesh/{stem}-mesh.mesh"));
        request["modelMapFile"] = json!(format!("mesh/{map_name}"));
        write_json(&request_path, &request, false)?;
        let reused = reuse_mesh(&runs, &directory, stem, hash)?;
        if !reused {
            record["progress"]["stage"] = json!("mesh");
            record["progress"]["stageStartedAt"] = json!(timestamp());
            write_json(&record_path, &record, true)?;
            events.progress("mesh", "Generating the 2D thermal mesh…", Some(&run_id));
            let generated = command(
                &mesh_executable,
                &["mesh", &mesh_directory.join(geo_name).to_string_lossy()],
                &mesh_directory,
                None,
            )?;
            fs::write(logs.join("mesh.stdout.log"), &generated.stdout)?;
            fs::write(logs.join("mesh.stderr.log"), &generated.stderr)?;
            successful_output(&generated, "Mesh generation")?;
        } else {
            fs::write(
                logs.join("mesh.stdout.log"),
                "Reused a verified mesh from an earlier run.\n",
            )?;
            fs::write(logs.join("mesh.stderr.log"), [])?;
        }
        let report_path = mesh_directory.join(format!("{stem}-mesh-report.json"));
        regular_file(&report_path)?;
        let mut report: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
        if reused {
            // Copied reports initially name the earlier run's absolute files.
            // Keep every artifact reference within this run's own mesh folder.
            report["geometry"] = json!(mesh_directory.join(geo_name));
            report["msh"] = json!(mesh_directory.join(format!("{stem}.msh")));
            report["mesh"] = json!(mesh_directory.join(format!("{stem}-mesh.mesh")));
            report["gmshLog"] = json!(mesh_directory.join(format!("{stem}-gmsh.log")));
            fs::write(&report_path, serde_json::to_vec_pretty(&report)?)?;
        }
        let nodes = report
            .get("nodes")
            .and_then(Value::as_u64)
            .ok_or("Mesh report omits the node count.")?;
        let triangles = report
            .get("triangles")
            .and_then(Value::as_u64)
            .ok_or("Mesh report omits the triangle count.")?;
        if nodes == 0 || triangles == 0 {
            return Err("The thermal runner requires a nonempty triangular mesh.".into());
        }
        let capabilities = crate::model_schema::engine_capabilities()?;
        if let Some(limits) =
            capabilities.pointer("/study/solverResourceLimitsByProfile/thermal-2d")
        {
            for (property, count) in [("maximumNodes", nodes), ("maximumCells", triangles)] {
                if let Some(value) = limits.get(property) {
                    let limit = value.as_u64().filter(|limit| *limit > 0).ok_or_else(|| {
                        format!("Thermal resource policy {property} must be a positive integer.")
                    })?;
                    if count > limit {
                        return Err(format!("The thermal mesh {property} resource limit is {limit}; this mesh requires {count}. Adjust the common schema resource policy or mesh settings." ).into());
                    }
                }
            }
        }
        let files = record_mesh_artifacts(&mesh_directory, stem, hash, &report)?;
        record["geometry"] = json!({"geometryHash":hash,"source":format!("mesh/{geo_name}"),
            "map":format!("mesh/{map_name}"),"mesh":format!("mesh/{stem}-mesh.mesh"),"msh":format!("mesh/{stem}.msh"),
            "report":format!("mesh/{stem}-mesh-report.json"),"cache":format!("mesh/{stem}-mesh-cache.json"),
            "files":files,"reused":reused});
        record["artifacts"]["meshLog"] = json!(format!("mesh/{stem}-gmsh.log"));
        write_json(&record_path, &record, true)?;
        events.emit(json!({"type":"mesh","state":"success","message":if reused {"Thermal mesh ready (reused)."} else {"Thermal mesh generated."},
            "runId":run_id,"mesh":{"dimension":2,"elementFamily":"triangle","nodes":nodes,"triangles":triangles,
                "boundaryEdges":report.get("boundaryEdges"),"materialDiagnostics":report.get("materialDiagnostics"),"reused":reused}}));
        events.progress(
            "solve",
            "Starting the stationary temperature solver…",
            Some(&run_id),
        );
        let mut timings =
            run_artifacts::SolverStageTimings::create(&logs.join("solver-stage-timings.csv"))?;
        record["solver"]["stageTimings"] = json!("logs/solver-stage-timings.csv");
        let stdout_bytes = run_solver(root, &request_path, &logs, |line| {
            if let Some((stage, message)) = run_artifacts::solver_stage(line)? {
                timings.start(0, None, stage)?;
                record["progress"]["stage"] = json!(stage);
                record["progress"]["stageStartedAt"] = json!(timestamp());
                write_json(&record_path, &record, true)?;
                events.emit(json!({"type":"progress","event":"stage-start","state":"running",
                    "stage":stage,"message":message,"runId":run_id,"totalPoints":1,"completedPoints":0}));
            }
            Ok(())
        })?;
        timings.finish()?;
        events.emit(json!({"type":"progress","state":"running","stage":"verification","message":"Checking thermal outputs…",
            "runId":run_id,"totalPoints":1,"completedPoints":1}));
        let field_path = output_directory.join("temperature.json");
        regular_file(&field_path)?;
        let field: Value = serde_json::from_slice(&fs::read(&field_path)?)?;
        if field.get("schema").and_then(Value::as_str) != Some("fullmeta-thermal-field/1") {
            return Err("The thermal solver returned an incompatible field artifact.".into());
        }
        let residual = field
            .pointer("/solver/residual")
            .and_then(Value::as_f64)
            .ok_or("Thermal field has no residual.")?;
        let unknowns = field
            .pointer("/solver/unknowns")
            .and_then(Value::as_u64)
            .filter(|&unknowns| unknowns > 0 && unknowns <= nodes)
            .ok_or("Thermal field has an invalid reduced unknown count.")?;
        let minimum = field
            .get("minimumTemperatureK")
            .and_then(Value::as_f64)
            .ok_or("Thermal field has no minimum temperature.")?;
        let maximum = field
            .get("maximumTemperatureK")
            .and_then(Value::as_f64)
            .ok_or("Thermal field has no maximum temperature.")?;
        let summary = format!(
            "case,unknowns,residual,minimum_temperature_K,maximum_temperature_K\n0,{unknowns},{residual:.12e},{minimum:.12e},{maximum:.12e}\n"
        );
        fs::write(output_directory.join(csv), summary)?;
        let fields = if fields_enabled {
            vec![run_artifacts::register_case_field_file(
                &output_directory,
                "temperature.json",
                0,
                &json!({"stationary":true,"label":"Stationary temperature"}),
            )?]
        } else {
            fs::remove_file(&field_path)?;
            Vec::new()
        };
        let field_names = run_artifacts::case_field_file_names(
            &output_directory,
            &fields,
            if fields_enabled { 1 } else { 0 },
        )?;
        record["state"] = json!("completed");
        record["completedAt"] = json!(timestamp());
        record["progress"]["completedPoints"] = json!(1);
        record["artifacts"]["fieldFiles"] = json!(fields);
        record["artifacts"]["fieldFileCount"] = json!(fields.len());
        record["artifacts"]["csvBytes"] = json!(fs::metadata(output_directory.join(csv))?.len());
        record["solver"]["stdoutBytes"] = json!(stdout_bytes);
        record["solver"]["residual"] = json!(residual);
        record["solver"]["unknowns"] = json!(unknowns);
        write_json(&record_path, &record, true)?;
        let run_directory = if folder == "." {
            format!("{runs_directory}/{run_id}")
        } else {
            format!("{folder}/{runs_directory}/{run_id}")
        };
        let output_files = ["run.json", "request.json"]
            .iter()
            .map(|name| format!("{run_directory}/{name}"))
            .chain(run_artifacts::completion_solver_log_files(&run_directory))
            .chain(
                record
                    .pointer("/artifacts/meshLog")
                    .and_then(Value::as_str)
                    .map(|path| format!("{run_directory}/{path}")),
            )
            .chain(std::iter::once(format!("{run_directory}/output/{csv}")))
            .chain(
                field_names
                    .iter()
                    .map(|name| format!("{run_directory}/output/{name}")),
            )
            .collect::<Vec<_>>();
        events.emit(json!({"type":"complete","state":"success","message":"Stationary thermal Study completed.",
            "runId":run_id,"runDirectory":run_directory,
            "totalPoints":1,"completedPoints":1,"dimension":2,"elementFamily":"triangle",
            "csv":format!("output/{csv}"),"fields":field_names,"outputFiles":output_files}));
        Ok(())
    })();
    if let Err(error) = &execution {
        record["state"] = json!("failed");
        record["completedAt"] = json!(timestamp());
        record["error"] = json!(error.to_string());
        let _ = write_json(&record_path, &record, true);
    }
    execution
}

fn run_solver(
    root: &Path,
    request: &Path,
    logs: &Path,
    mut progress: impl FnMut(&str) -> Result<()>,
) -> Result<u64> {
    let executable = env::current_exe()?;
    let mut child = crate::model_schema::command(&executable)?
        .arg("--solve")
        .arg(request)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or("Thermal solver stderr was not captured.")?;
    let stderr_reader = std::thread::spawn(move || -> io::Result<Vec<u8>> {
        let mut output = Vec::new();
        stderr.read_to_end(&mut output)?;
        Ok(output)
    });
    let stdout = child
        .stdout
        .take()
        .ok_or("Thermal solver stdout was not captured.")?;
    let mut log = File::create(logs.join("solver.stdout.log"))?;
    let mut progress_error = None;
    for line in BufReader::new(stdout).lines() {
        let line = line?;
        writeln!(log, "{line}")?;
        log.flush()?;
        if progress_error.is_none() {
            progress_error = progress(&line).err();
        }
    }
    let status = child.wait()?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "Thermal solver stderr reader stopped unexpectedly.")??;
    let stderr_path = logs.join("solver.stderr.log");
    if stderr.is_empty() {
        fs::write(
            &stderr_path,
            if status.success() {
                "Solver completed without standard error output.\n"
            } else {
                "Solver failed without standard error output.\n"
            },
        )?;
    } else {
        fs::write(&stderr_path, &stderr)?;
    }
    if let Some(error) = progress_error {
        return Err(error);
    }
    if !status.success() {
        return Err(format!(
            "Thermal solve failed ({status}): {}",
            String::from_utf8_lossy(&stderr).trim()
        )
        .into());
    }
    if log.metadata()?.len() == 0 {
        writeln!(log, "Thermal solver completed without standard output.")?;
        log.flush()?;
    }
    Ok(log.metadata()?.len())
}

fn timestamp() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let z = (elapsed.as_secs() / 86_400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = month_index + if month_index < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    let seconds = elapsed.as_secs() % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3_600,
        seconds / 60 % 60,
        seconds % 60,
        elapsed.subsec_millis()
    )
}
