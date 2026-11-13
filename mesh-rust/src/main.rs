mod expression;
mod geometry_cli;
mod geometry_v2;
#[cfg(windows)]
mod gmsh_api;
mod mesh_items;
mod mesh_options;
mod mesher;
mod schema_metadata;
mod selection_address;
mod sha256;

use serde_json::{Value, json};
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--describe-operations") {
        println!("{}", fullmeta_operation_contracts::describe(Some("mesh")));
        return ExitCode::SUCCESS;
    }
    if matches!(
        args.first().map(String::as_str),
        Some("--operation-check" | "--operation-compile")
    ) {
        return match contracted_geometry(&args) {
            Ok(output) => {
                println!("{output}");
                ExitCode::SUCCESS
            }
            Err(issues) => {
                println!("{}", fullmeta_operation_contracts::error_response(&issues));
                ExitCode::FAILURE
            }
        };
    }
    match run(args) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn contracted_geometry(
    args: &[String],
) -> Result<Value, Vec<fullmeta_operation_contracts::Diagnostic>> {
    use fullmeta_operation_contracts::{
        Diagnostic, binding_from_environment, resolve_binding, validate_input, validate_result,
    };
    let fail = |message: String| {
        vec![Diagnostic::new(
            "GEOMETRY_OPERATION_FAILED",
            message,
            None,
            "geometry",
        )]
    };
    let entry = args[0].as_str();
    if args.len() != 1 {
        return Err(fail(
            "Operation entries accept one JSON request on standard input, without extra arguments."
                .into(),
        ));
    }
    let id = if entry == "--operation-check" {
        "mesh.check"
    } else {
        "mesh.compile"
    };
    let identity = binding_from_environment("mesh", entry)
        .map_err(|issue| vec![issue])?
        .unwrap_or(resolve_binding(&json!({"id":id,"version":1})).map_err(|issue| vec![issue])?);
    let mut raw = String::new();
    std::io::stdin()
        .read_to_string(&mut raw)
        .map_err(|error| fail(format!("Could not read operation input: {error}")))?;
    let input: Value = serde_json::from_str(&raw).map_err(|error| {
        vec![Diagnostic::new(
            "OPERATION_INPUT_INVALID",
            format!("Invalid operation JSON: {error}"),
            Some(String::new()),
            "input",
        )]
    })?;
    validate_input(&identity.id, &input)?;
    let mut compiler = vec![
        if id == "mesh.check" {
            "check".into()
        } else {
            "compile".into()
        },
        "-".into(),
    ];
    if let Some(component) = input.get("componentId").and_then(Value::as_str) {
        compiler.extend(["--component".into(), component.into()]);
    }
    if id == "mesh.compile" {
        let directory = input
            .get("outputDirectory")
            .and_then(Value::as_str)
            .unwrap_or(".");
        let path = Path::new(directory);
        if path.is_absolute()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
        {
            return Err(vec![Diagnostic::new(
                "OPERATION_OUTPUT_PATH_INVALID",
                "Choose an output directory inside the command working directory.",
                Some("/outputDirectory".into()),
                "input",
            )]);
        }
        let cwd = env::current_dir()
            .and_then(fs::canonicalize)
            .map_err(|error| fail(error.to_string()))?;
        let mut ancestor = cwd.join(path);
        while !ancestor.exists() {
            if !ancestor.pop() {
                return Err(fail(
                    "The output directory has no existing workspace parent.".into(),
                ));
            }
        }
        let actual = fs::canonicalize(ancestor).map_err(|error| fail(error.to_string()))?;
        if !actual.starts_with(&cwd) {
            return Err(vec![Diagnostic::new(
                "OPERATION_OUTPUT_PATH_INVALID",
                "The output directory resolves outside the command working directory.",
                Some("/outputDirectory".into()),
                "input",
            )]);
        }
        compiler.extend(["--output-dir".into(), directory.into()]);
    }
    let model = serde_json::to_string(&input["model"]).map_err(|error| fail(error.to_string()))?;
    let output = geometry_cli::execute_with_input(compiler, Some(&model)).map_err(fail)?;
    let value: Value = serde_json::from_str(&output).map_err(|error| {
        vec![Diagnostic::new(
            "OPERATION_RESULT_INVALID",
            error.to_string(),
            None,
            "result",
        )]
    })?;
    validate_result(id, &value)?;
    Ok(value)
}

fn run(args: Vec<String>) -> Result<String, String> {
    let command = args.first().cloned().ok_or_else(usage)?;
    match command.as_str() {
        "check" | "compile" => geometry_cli::execute(args).map_err(|error| error.to_string()),
        "preview" => preview_existing_mesh(args),
        "mesh" => {
            if args.len() < 2 || args.len() > 3 || args[1] == "--help" || args[1] == "-h" {
                return Err(usage());
            }
            let gmsh = args.get(2).map(String::as_str);
            let report = mesher::run(&args[1], gmsh).map_err(|error| error.to_string())?;
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())
        }
        "build" => {
            if args.len() < 2 || args[1] == "--help" || args[1] == "-h" {
                return Err(usage());
            }
            let model_path = args[1].clone();
            let (compile_args, run_metadata) = parse_build_args(args)?;
            let started_at = utc_timestamp();
            let compiled_text =
                geometry_cli::execute(compile_args).map_err(|error| error.to_string())?;
            let compiled: Value = serde_json::from_str(&compiled_text)
                .map_err(|error| format!("Geometry compiler returned invalid metadata: {error}"))?;
            let geo_path = compiled
                .get("geoPath")
                .and_then(Value::as_str)
                .ok_or("Geometry compiler did not return a geometry path")?;
            let paths = build_artifact_paths(Path::new(geo_path))?;
            let mut run_record = run_metadata.as_ref().map(|metadata| {
                mesh_run_record(
                    metadata,
                    compiled.get("modelId").cloned().unwrap_or(Value::Null),
                    compiled
                        .get("geometryHash")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    &paths,
                    &started_at,
                    "running",
                    Value::Null,
                    Value::Null,
                )
            });
            if let Some(record) = run_record.as_ref() {
                write_json_file(&paths.run, record)?;
            }
            let report = match mesher::run(geo_path, None) {
                Ok(report) => report,
                Err(error) => {
                    if let (Some(metadata), Some(record)) =
                        (run_metadata.as_ref(), run_record.as_mut())
                    {
                        *record = mesh_run_record(
                            metadata,
                            compiled.get("modelId").cloned().unwrap_or(Value::Null),
                            compiled
                                .get("geometryHash")
                                .and_then(Value::as_str)
                                .unwrap_or_default(),
                            &paths,
                            &started_at,
                            "failed",
                            json!(utc_timestamp()),
                            json!(error.to_string()),
                        );
                        let _ = write_json_file(&paths.run, record);
                    }
                    return Err(error.to_string());
                }
            };
            if let (Some(metadata), Some(record)) = (run_metadata.as_ref(), run_record.as_mut()) {
                if let Err(error) = write_mesh_cache(
                    &paths,
                    compiled
                        .get("geometryHash")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    &report,
                ) {
                    *record = mesh_run_record(
                        metadata,
                        compiled.get("modelId").cloned().unwrap_or(Value::Null),
                        compiled
                            .get("geometryHash")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        &paths,
                        &started_at,
                        "failed",
                        json!(utc_timestamp()),
                        json!(error),
                    );
                    let _ = write_json_file(&paths.run, record);
                    return Err(error);
                }
                *record = mesh_run_record(
                    metadata,
                    compiled.get("modelId").cloned().unwrap_or(Value::Null),
                    compiled
                        .get("geometryHash")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    &paths,
                    &started_at,
                    "mesh-only",
                    json!(utc_timestamp()),
                    report.clone(),
                );
                write_json_file(&paths.run, record)?;
            }
            serde_json::to_string_pretty(&json!({
                "model": Path::new(&model_path),
                "geometry": compiled,
                "mesh": report,
                "runId": run_metadata.as_ref().map(|metadata| metadata.run_id.as_str())
            }))
            .map_err(|error| error.to_string())
        }
        "--help" | "-h" => Ok(usage()),
        _ => Err(usage()),
    }
}

fn preview_existing_mesh(args: Vec<String>) -> Result<String, String> {
    let mut values = args.into_iter().skip(1);
    let input = values.next().ok_or_else(usage)?;
    if input != "-" {
        return Err("The preview command requires the model JSON on standard input.".into());
    }
    let mut component_id = None;
    let mut runs_directory = None;
    while let Some(argument) = values.next() {
        match argument.as_str() {
            "--component" => {
                component_id = Some(values.next().ok_or("--component requires an ID.")?)
            }
            "--runs-directory" => {
                runs_directory = Some(
                    values
                        .next()
                        .ok_or("--runs-directory requires a directory name.")?,
                )
            }
            _ => return Err(format!("Unknown preview option {argument:?}.\n{}", usage())),
        }
    }
    let component_id = component_id
        .filter(|value| !value.is_empty())
        .ok_or("The preview command requires --component.")?;
    let runs_directory = runs_directory
        .filter(|value| valid_path_component(value))
        .ok_or("The runs directory must be a simple directory name.")?;
    let mut source = String::new();
    std::io::stdin()
        .read_to_string(&mut source)
        .map_err(|error| format!("Could not read model JSON from stdin: {error}"))?;
    let geometry = geometry_cli::check_source(&source, None, Some(&component_id))?;
    let model_id = geometry
        .get("modelId")
        .and_then(Value::as_str)
        .ok_or("The geometry compiler returned no model ID.")?;
    let geometry_hash = geometry
        .get("geometryHash")
        .and_then(Value::as_str)
        .ok_or("The geometry compiler returned no geometry hash.")?;
    let preview = find_existing_mesh_for_hash(model_id, geometry_hash, &runs_directory)?
        .unwrap_or_else(|| json!({ "available": false }));
    serde_json::to_string_pretty(&preview)
        .map_err(|error| format!("Could not format mesh preview response: {error}"))
}

fn find_existing_mesh_for_hash(
    model_id: &str,
    geometry_hash: &str,
    runs_name: &str,
) -> Result<Option<Value>, String> {
    if !valid_sha256(geometry_hash) {
        return Ok(None);
    }
    let project_directory = env::current_dir().map_err(|error| error.to_string())?;
    let runs_directory = project_directory.join(runs_name);
    match fs::symlink_metadata(&runs_directory) {
        Ok(info) if info.file_type().is_symlink() || !info.is_dir() => return Ok(None),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect mesh run history: {error}")),
    }
    let entries = fs::read_dir(&runs_directory)
        .map_err(|error| format!("Could not list mesh run history: {error}"))?;
    let mut candidates = Vec::<(String, Value)>::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not inspect a mesh run entry: {error}"))?;
        let run_id = match entry.file_name().to_str() {
            Some(value) => value.to_string(),
            None => continue,
        };
        let run_directory = entry.path();
        match fs::symlink_metadata(&run_directory) {
            Ok(info) if info.file_type().is_symlink() || !info.is_dir() => continue,
            Ok(_) => {}
            Err(_) => continue,
        }
        let run_record_path = run_directory.join("run.json");
        match fs::symlink_metadata(&run_record_path) {
            Ok(info) if info.file_type().is_symlink() || !info.is_file() => continue,
            Ok(_) => {}
            Err(_) => continue,
        }
        let record: Value = match fs::read(&run_record_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        {
            Some(value) => value,
            None => continue,
        };
        if record.get("runId").and_then(Value::as_str) != Some(run_id.as_str())
            || record.pointer("/model/id").and_then(Value::as_str) != Some(model_id)
            || !matches!(
                record.get("state").and_then(Value::as_str),
                Some("mesh-only" | "completed")
            )
        {
            continue;
        }
        let Some(root_geometry) = record.get("geometry") else {
            continue;
        };
        let mut geometries = vec![root_geometry];
        if let Some(cases) = root_geometry.get("cases").and_then(Value::as_array) {
            geometries.extend(cases.iter());
        }
        let mut seen_sources = Vec::<String>::new();
        for geometry in geometries {
            if geometry.get("geometryHash").and_then(Value::as_str) != Some(geometry_hash) {
                continue;
            }
            let Some(source_relative) = run_geometry_source(geometry) else {
                continue;
            };
            if seen_sources.contains(&source_relative) {
                continue;
            }
            seen_sources.push(source_relative.clone());
            let Some(source_path) = safe_run_file(&run_directory, &source_relative) else {
                continue;
            };
            let Some(paths) = mesh_artifact_paths(&source_path) else {
                continue;
            };
            let Some(report) = valid_mesh_cache(geometry_hash, &paths) else {
                continue;
            };
            let mesh_file = match workspace_relative_file(&paths.mesh) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let model_map_file = match workspace_relative_file(&paths.map) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let time = record
                .get("completedAt")
                .and_then(Value::as_str)
                .or_else(|| record.get("startedAt").and_then(Value::as_str))
                .unwrap_or(&run_id)
                .to_string();
            candidates.push((time, json!({
                "available": true,
                "runId": run_id,
                "geometryFile": source_path.file_name().and_then(|name| name.to_str()).unwrap_or("model.geo"),
                "meshFile": mesh_file,
                "modelMapFile": model_map_file,
                "report": report
            })));
        }
    }
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    Ok(candidates.into_iter().next().map(|(_, preview)| preview))
}

fn run_geometry_source(geometry: &Value) -> Option<String> {
    let source = geometry
        .get("source")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let mesh = geometry.get("mesh").and_then(Value::as_str)?;
            let normalized = mesh.replace('\\', "/");
            let lower = normalized.to_ascii_lowercase();
            let suffix = "-mesh.mesh";
            let stem_end = lower.strip_suffix(suffix)?.len();
            Some(format!("{}.geo", &normalized[..stem_end]))
        })?;
    let parts: Vec<String> = source
        .replace('\\', "/")
        .split('/')
        .map(str::to_string)
        .collect();
    if parts.is_empty()
        || parts.iter().any(|part| !valid_path_component(part))
        || (parts.len() > 1 && parts[0] != "mesh")
        || !parts
            .last()
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".geo"))
    {
        return None;
    }
    Some(parts.join("/"))
}

struct MeshArtifactPaths {
    source: PathBuf,
    msh: PathBuf,
    mesh: PathBuf,
    report: PathBuf,
    map: PathBuf,
    gmsh_log: PathBuf,
    cache: PathBuf,
}

fn mesh_artifact_paths(source: &Path) -> Option<MeshArtifactPaths> {
    let directory = source.parent()?;
    let stem = source.file_stem()?.to_str()?;
    Some(MeshArtifactPaths {
        source: source.to_path_buf(),
        msh: directory.join(format!("{stem}.msh")),
        mesh: directory.join(format!("{stem}-mesh.mesh")),
        report: directory.join(format!("{stem}-mesh-report.json")),
        map: directory.join(format!("{stem}-model-map.json")),
        gmsh_log: directory.join(format!("{stem}-gmsh.log")),
        cache: directory.join(format!("{stem}-mesh-cache.json")),
    })
}

fn valid_mesh_cache(geometry_hash: &str, paths: &MeshArtifactPaths) -> Option<Value> {
    let cache_bytes = read_safe_regular_file(&paths.cache)?;
    let cache: Value = serde_json::from_slice(&cache_bytes).ok()?;
    if cache.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-cache/3")
        || cache.get("geometryHash").and_then(Value::as_str) != Some(geometry_hash)
    {
        return None;
    }
    let files = cache.get("files")?.as_object()?;
    let expected_paths = [
        ("source", &paths.source),
        ("msh", &paths.msh),
        ("mesh", &paths.mesh),
        ("report", &paths.report),
        ("map", &paths.map),
        ("gmshLog", &paths.gmsh_log),
    ];
    let mut contents = Vec::with_capacity(expected_paths.len());
    for (key, path) in expected_paths {
        let bytes = read_safe_regular_file(path)?;
        let expected = files.get(key)?;
        let file_name = path.file_name()?.to_str()?;
        if bytes.is_empty()
            || expected.get("path").and_then(Value::as_str) != Some(file_name)
            || expected.get("bytes").and_then(Value::as_u64)? != bytes.len() as u64
            || expected.get("sha256").and_then(Value::as_str)? != sha256::hex_digest(&bytes)
        {
            return None;
        }
        contents.push((key, bytes));
    }
    let mesh_bytes = contents
        .iter()
        .find(|(name, _)| *name == "mesh")?
        .1
        .as_slice();
    let report_bytes = contents
        .iter()
        .find(|(name, _)| *name == "report")?
        .1
        .as_slice();
    let report: Value = serde_json::from_slice(report_bytes).ok()?;
    if mesh_bytes.is_empty()
        || contents
            .iter()
            .find(|(name, _)| *name == "msh")?
            .1
            .is_empty()
    {
        return None;
    }
    // Mesh reports record the cell count but do not declare a separate
    // dimension field. Infer the cell kind from the same report fields used
    // when the cache was written.
    let cell_key = if report.get("triangles").and_then(Value::as_u64).is_some() {
        "triangles"
    } else if report.get("tetrahedra").and_then(Value::as_u64).is_some() {
        "tetrahedra"
    } else {
        return None;
    };
    let nodes = report.get("nodes").and_then(Value::as_u64)?;
    let cells = report.get(cell_key).and_then(Value::as_u64)?;
    if nodes == 0
        || cells == 0
        || cache.pointer("/mesh/nodes").and_then(Value::as_u64) != Some(nodes)
        || cache
            .pointer(&format!("/mesh/{cell_key}"))
            .and_then(Value::as_u64)
            != Some(cells)
    {
        return None;
    }
    Some(report)
}

fn safe_run_file(run_directory: &Path, relative: &str) -> Option<PathBuf> {
    let normalized = relative.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.is_empty() || parts.iter().any(|part| !valid_path_component(part)) {
        return None;
    }
    let mut path = run_directory.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        let info = fs::symlink_metadata(&path).ok()?;
        if info.file_type().is_symlink()
            || (index + 1 < parts.len() && !info.is_dir())
            || (index + 1 == parts.len() && !info.is_file())
        {
            return None;
        }
    }
    Some(path)
}

fn read_safe_regular_file(path: &Path) -> Option<Vec<u8>> {
    let info = fs::symlink_metadata(path).ok()?;
    if info.file_type().is_symlink() || !info.is_file() {
        return None;
    }
    fs::read(path).ok()
}

fn workspace_relative_file(path: &Path) -> Result<String, String> {
    let root =
        env::var_os("FULLMETA_ROOT").ok_or("The mesh preview has no FullMeta workspace root.")?;
    let root = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| format!("Could not resolve FullMeta workspace root: {error}"))?;
    let file = path
        .canonicalize()
        .map_err(|error| format!("Could not resolve preview mesh artifact: {error}"))?;
    let relative = file
        .strip_prefix(&root)
        .map_err(|_| "The preview mesh artifact is outside FullMeta.")?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn valid_path_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 119
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod mesh_preview_path_tests {
    use super::run_geometry_source;
    use serde_json::json;

    #[test]
    fn accepts_run_root_mesh_only_geometry_source() {
        assert_eq!(
            run_geometry_source(&json!({ "source": "geometry.geo" })),
            Some("geometry.geo".to_string())
        );
    }

    #[test]
    fn accepts_study_case_geometry_sources_under_mesh_directory() {
        assert_eq!(
            run_geometry_source(&json!({ "source": "mesh/case-001/geometry.geo" })),
            Some("mesh/case-001/geometry.geo".to_string())
        );
    }

    #[test]
    fn rejects_unexpected_paths_and_traversal() {
        assert_eq!(run_geometry_source(&json!({ "source": "other/geometry.geo" })), None);
        assert_eq!(run_geometry_source(&json!({ "source": "mesh/../geometry.geo" })), None);
        assert_eq!(run_geometry_source(&json!({ "source": "../geometry.geo" })), None);
    }
}

struct BuildRunMetadata {
    run_id: String,
    model_file: String,
    operation_id: String,
}

struct BuildArtifactPaths {
    run: PathBuf,
    source: PathBuf,
    msh: PathBuf,
    mesh: PathBuf,
    report: PathBuf,
    map: PathBuf,
    gmsh_log: PathBuf,
    cache: PathBuf,
}

fn parse_build_args(args: Vec<String>) -> Result<(Vec<String>, Option<BuildRunMetadata>), String> {
    let mut compile_args = vec!["compile".to_string(), args[1].clone()];
    let mut run_id = None;
    let mut model_file = None;
    let mut operation_id = None;
    let mut rest = args.into_iter().skip(2);
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--run-id" => run_id = Some(rest.next().ok_or("--run-id requires a value")?),
            "--model-file" => {
                model_file = Some(rest.next().ok_or("--model-file requires a value")?)
            }
            "--operation-id" => {
                operation_id = Some(rest.next().ok_or("--operation-id requires a value")?)
            }
            _ => compile_args.push(argument),
        }
    }
    let metadata = match (run_id, model_file, operation_id) {
        (None, None, None) => None,
        (Some(run_id), Some(model_file), Some(operation_id)) => {
            if !valid_run_id(&run_id) || !valid_operation_id(&operation_id) {
                return Err("The mesh run ID or operation ID is invalid.".into());
            }
            if model_file.is_empty()
                || model_file.contains('\0')
                || Path::new(&model_file)
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
            {
                return Err("The model file recorded for this run must be a relative path.".into());
            }
            Some(BuildRunMetadata {
                run_id,
                model_file,
                operation_id,
            })
        }
        _ => {
            return Err(
                "Run metadata requires --run-id, --model-file, and --operation-id together.".into(),
            );
        }
    };
    Ok((compile_args, metadata))
}

fn valid_run_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn valid_operation_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn build_artifact_paths(geometry: &Path) -> Result<BuildArtifactPaths, String> {
    let directory = geometry.parent().unwrap_or(Path::new(".")).to_path_buf();
    let stem = geometry
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or("Invalid generated geometry filename")?;
    Ok(BuildArtifactPaths {
        run: PathBuf::from("run.json"),
        source: geometry.to_path_buf(),
        msh: directory.join(format!("{stem}.msh")),
        mesh: directory.join(format!("{stem}-mesh.mesh")),
        report: directory.join(format!("{stem}-mesh-report.json")),
        map: directory.join(format!("{stem}-model-map.json")),
        gmsh_log: directory.join(format!("{stem}-gmsh.log")),
        cache: directory.join(format!("{stem}-mesh-cache.json")),
    })
}

fn relative_run_path(path: &Path) -> Result<String, String> {
    let root = env::current_dir().map_err(|error| error.to_string())?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let relative = absolute.strip_prefix(&root).map_err(|_| {
        format!(
            "Generated mesh artifact escaped its run directory: {}",
            path.display()
        )
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn mesh_run_record(
    metadata: &BuildRunMetadata,
    model_id: Value,
    geometry_hash: &str,
    paths: &BuildArtifactPaths,
    started_at: &str,
    state: &str,
    completed_at: Value,
    mesh: Value,
) -> Value {
    json!({
        "schema": "fullmeta-run/1",
        "runId": metadata.run_id,
        "state": state,
        "startedAt": started_at,
        "completedAt": completed_at,
        "model": { "id": model_id, "file": metadata.model_file },
        "geometry": {
            "geometryHash": geometry_hash,
            "source": relative_run_path(&paths.source).unwrap_or_default(),
            "msh": relative_run_path(&paths.msh).unwrap_or_default(),
            "mesh": relative_run_path(&paths.mesh).unwrap_or_default(),
            "report": relative_run_path(&paths.report).unwrap_or_default(),
            "map": relative_run_path(&paths.map).unwrap_or_default(),
            "cache": relative_run_path(&paths.cache).unwrap_or_default()
        },
        "operationId": metadata.operation_id,
        "error": if state == "failed" { mesh.clone() } else { Value::Null },
        "mesh": if state == "mesh-only" { mesh } else { Value::Null }
    })
}

fn write_json_file(path: &Path, value: &Value) -> Result<(), String> {
    let contents = format!(
        "{}\n",
        serde_json::to_string_pretty(value).map_err(|error| error.to_string())?
    );
    fs::write(path, contents)
        .map_err(|error| format!("Could not write run metadata {}: {error}", path.display()))
}

fn write_mesh_cache(
    paths: &BuildArtifactPaths,
    geometry_hash: &str,
    report: &Value,
) -> Result<(), String> {
    let sources = [
        ("source", &paths.source),
        ("msh", &paths.msh),
        ("mesh", &paths.mesh),
        ("report", &paths.report),
        ("map", &paths.map),
        ("gmshLog", &paths.gmsh_log),
    ];
    let mut files = serde_json::Map::new();
    for (name, path) in sources {
        let contents = fs::read(path).map_err(|error| {
            format!(
                "Could not read mesh cache source {}: {error}",
                path.display()
            )
        })?;
        let filename = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("Invalid mesh cache source filename")?;
        files.insert(
            name.to_string(),
            json!({
                "path": filename,
                "bytes": contents.len(),
                "sha256": sha256::hex_digest(&contents)
            }),
        );
    }
    let mut mesh = serde_json::Map::new();
    mesh.insert(
        "nodes".into(),
        report.get("nodes").cloned().unwrap_or(Value::Null),
    );
    let cell_name = if report.get("triangles").is_some() {
        "triangles"
    } else {
        "tetrahedra"
    };
    mesh.insert(
        cell_name.into(),
        report.get(cell_name).cloned().unwrap_or(Value::Null),
    );
    write_json_file(
        &paths.cache,
        &json!({
            "schema": "fullmeta-mesh-cache/3",
            "geometryHash": geometry_hash,
            "createdAt": utc_timestamp(),
            "mesh": mesh,
            "files": files
        }),
    )
}

fn utc_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = duration.as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        day_seconds / 3_600,
        (day_seconds % 3_600) / 60,
        day_seconds % 60,
        duration.subsec_millis()
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn usage() -> String {
    "Usage: fullmeta-mesh check <fem_model.json|-> [--component <id>]\n       fullmeta-mesh compile <fem_model.json|-> [output.geo] [--component <id>] [--output-dir <directory>] [--overwrite]\n       fullmeta-mesh preview <fem_model.json|-> --component <id> --runs-directory <name>\n       fullmeta-mesh mesh <geometry.geo> [gmsh.exe]\n       fullmeta-mesh build <fem_model.json|-> [--component <id>] [--output-dir <directory>] [--overwrite] [--run-id <id> --model-file <path> --operation-id <id>]".to_string()
}
