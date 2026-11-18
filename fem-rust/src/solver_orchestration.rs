use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;

// Shared sweep progress is independent of each solver's diagnostic wording.
const SOLVER_PROGRESS_PREFIX: &str = "FULLMETA_PROGRESS ";

pub(crate) fn solver_case_progress_line(
    event: &str,
    case_index: usize,
    total_cases: usize,
    frequency_hz: f64,
) -> String {
    format!(
        "{SOLVER_PROGRESS_PREFIX}{}",
        json!({
            "event":event,"caseIndex":case_index,"totalCases":total_cases,"frequencyHz":frequency_hz
        })
    )
}

pub(crate) mod coordinator;
use crate::model_schema as json_schema;
use crate::{model_schema, solver_request};

/// Raw-model checks are performed by the common FEM preflight service.
pub(crate) fn validate_native_model(model: &Value) -> Vec<String> {
    crate::preflight_client::validation_errors(model)
}

pub(crate) fn run_shared_operation(
    args: &[String],
) -> Result<std::process::ExitCode, Box<dyn Error>> {
    if let Some(project) = args.first().filter(|value| !value.starts_with('-')) {
        let root = fullmeta_model::runtime_paths::installation_root()?;
        let file = root
            .join("projects")
            .join(project.strip_prefix("projects/").unwrap_or(project))
            .join("fem_model.json");
        let model: Value = serde_json::from_slice(&fs::read(&file)?)?;
        model_schema::select_schema_for_model_file(&model, &file)?;
    }
    if !matches!(
        args.first().map(String::as_str),
        Some("--prepare-study" | "--capabilities")
    ) {
        return Ok(coordinator::dispatch_with_arguments(args));
    }
    crate::model_schema::ensure_scenario_metadata()?;
    let capabilities = json_schema::engine_capabilities()?;
    if args.first().map(String::as_str) == Some("--prepare-study") {
        let mut input = String::new();
        io::stdin().read_to_string(&mut input)?;
        let envelope: Value = serde_json::from_str(&input)?;
        let model = envelope
            .get("model")
            .ok_or("prepare request.model is required.")?;
        let errors = validate_native_model(model);
        if !errors.is_empty() {
            return Err(format!("Model validation:\n{}", errors.join("\n")).into());
        }
        let prepared = crate::preflight_client::prepare_study(&envelope)
            .map_err(|error| format!("Study preparation: {error}"))?;
        println!("{}", serde_json::to_string(&prepared)?);
        return Ok(std::process::ExitCode::SUCCESS);
    }
    if args.first().map(String::as_str) == Some("--capabilities") {
        let runtime = json!({"schema":"fullmeta-solver-runtime/1","solverConfigurationSchema":capabilities["study"]["requestConfigurationSchema"],"solverRequestSchema":capabilities["study"]["solverRequestSchema"],"engineCapabilities":capabilities});
        println!("{}", serde_json::to_string(&runtime)?);
        return Ok(std::process::ExitCode::SUCCESS);
    }
    Err("Usage: fullmeta-fem <project> [--validate] [--study <id>] [--csv <file>] | --prepare-study | --capabilities | --solve <request.json> | --sequence <manifest.json>".into())
}

#[derive(Clone)]
pub(crate) enum SolverRuntime {
    Native {
        module: String,
    },
    Generic {
        kind: String,
        profile: String,
    },
    External {
        executable: PathBuf,
        solve_argument: String,
        sequence_argument: String,
    },
}

fn application_root() -> Result<PathBuf, Box<dyn Error>> {
    let executable = env::current_exe()?;
    executable
        .ancestors()
        .find(|ancestor| ancestor.join("projects").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            "Could not locate the FullMeta application root beside the scenario executable.".into()
        })
}

fn coordinate_solver_sweep_with(
    runtime: SolverRuntime,
    request_path: &Path,
    on_stdout_line: &mut dyn FnMut(&str),
) -> Result<(), Box<dyn Error>> {
    let request: Value = serde_json::from_slice(&fs::read(request_path)?)?;
    let solver = request
        .get("solver")
        .and_then(Value::as_str)
        .ok_or("request.solver is required.")?;
    solver_request::validate_request(&request, solver)
        .map_err(|error| format!("Solver request: {error}"))?;
    let frequency_cases = request
        .get("frequencyCases")
        .and_then(Value::as_array)
        .ok_or("request.frequencyCases must be an array.")?
        .clone();
    let capabilities = json_schema::engine_capabilities()?;
    let max_points = capabilities
        .pointer("/study/maximumSweepPoints")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("The model schema must declare study.maximumSweepPoints as a positive integer.")?;
    if frequency_cases.is_empty() || frequency_cases.len() > max_points {
        return Err(format!(
            "The solver request must contain between 1 and {max_points} frequency points."
        )
        .into());
    }
    if frequency_cases.iter().any(|case| {
        case.get("frequencyHz")
            .and_then(Value::as_f64)
            .is_none_or(|value| !value.is_finite() || value <= 0.0)
    }) {
        return Err(
            "Every request.frequencyCases[].frequencyHz value must be positive and finite.".into(),
        );
    }
    let request_directory = request_path.parent().unwrap_or(Path::new("."));
    let output_directory = request
        .get("outputDirectory")
        .and_then(Value::as_str)
        .ok_or("request.outputDirectory is required.")?;
    let output_directory = Path::new(output_directory);
    let output_directory = if output_directory.is_absolute() {
        output_directory.to_path_buf()
    } else {
        request_directory.join(output_directory)
    };
    fs::create_dir_all(&output_directory)?;
    let sweep_csv = request
        .get("sweepCsv")
        .and_then(Value::as_str)
        .ok_or("request.sweepCsv is required.")?;
    let debug_csv = request.get("debugCsv").and_then(Value::as_str);
    let mut merged_sweep = String::new();
    let mut merged_debug = String::new();
    let request_stem = request_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("request");
    let run_token = format!("{}-{}", std::process::id(), request_stem);
    let mut case_jobs = Vec::with_capacity(frequency_cases.len());
    for (index, case) in frequency_cases.iter().enumerate() {
        let case_number = index + 1;
        let mut single_case_request = request.clone();
        let mut solver_case = case.clone();
        if let Some(overrides) = case.get("caseOverrides").and_then(Value::as_object) {
            for (key, value) in overrides {
                single_case_request[key] = value.clone();
            }
        }
        if let Some(case_object) = solver_case.as_object_mut() {
            case_object.remove("caseOverrides");
            case_object.remove("parameterOverrides");
        }
        single_case_request["frequencyCases"] = json!([solver_case]);
        let case_output_directory = single_case_request
            .get("outputDirectory")
            .and_then(Value::as_str)
            .ok_or("request.outputDirectory must be a string after applying case overrides.")?;
        let case_output_directory = Path::new(case_output_directory);
        let case_output_directory = if case_output_directory.is_absolute() {
            case_output_directory.to_path_buf()
        } else {
            request_directory.join(case_output_directory)
        };
        fs::create_dir_all(&case_output_directory)?;
        let case_csv_name = format!(".fullmeta-case-{run_token}-{case_number:04}.csv");
        single_case_request["sweepCsv"] = json!(case_csv_name);
        let case_debug_name =
            debug_csv.map(|_| format!(".fullmeta-case-{run_token}-{case_number:04}-debug.csv"));
        if let Some(name) = &case_debug_name {
            single_case_request["debugCsv"] = json!(name);
        }

        solver_request::validate_single_case_request(&single_case_request, solver)
            .map_err(|error| format!("Prepared solver request for case {case_number}: {error}"))?;

        let case_request_path =
            request_directory.join(format!(".fullmeta-case-{run_token}-{case_number:04}.json"));
        fs::write(
            &case_request_path,
            serde_json::to_vec(&single_case_request)?,
        )?;
        case_jobs.push((
            case_request_path,
            case_output_directory,
            case_csv_name,
            case_debug_name,
        ));
    }

    let mut request_files = Vec::with_capacity(case_jobs.len());
    for (case_request_path, _, _, _) in &case_jobs {
        let name = case_request_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Solver case request filename is not valid UTF-8.")?;
        request_files.push(name.to_string());
    }
    let sequence_manifest_path =
        request_directory.join(format!(".fullmeta-sequence-{run_token}.json"));
    fs::write(
        &sequence_manifest_path,
        serde_json::to_vec(&json!({"requestFiles":request_files}))?,
    )?;
    let solver_result = run_solver_sequence(&runtime, &sequence_manifest_path, on_stdout_line);
    let _ = fs::remove_file(&sequence_manifest_path);
    for (case_request_path, _, _, _) in &case_jobs {
        let _ = fs::remove_file(case_request_path);
    }
    let (exit_code, stderr) = solver_result?;
    if exit_code != 0 {
        return Err(format!(
            "Scenario solver exited with code {exit_code}. {}",
            stderr.trim()
        )
        .into());
    }

    for (case_request_path, case_output_directory, case_csv_name, case_debug_name) in &case_jobs {
        let _ = fs::remove_file(case_request_path);
        append_case_csv(
            &mut merged_sweep,
            &case_output_directory.join(case_csv_name),
        )?;
        fs::remove_file(case_output_directory.join(case_csv_name))?;
        if let Some(name) = case_debug_name {
            append_case_csv(&mut merged_debug, &case_output_directory.join(name))?;
            fs::remove_file(case_output_directory.join(name))?;
        }
    }

    fs::write(output_directory.join(sweep_csv), merged_sweep)?;
    if let Some(debug_csv) = debug_csv {
        fs::write(output_directory.join(debug_csv), merged_debug)?;
    }
    let runtime_name = match runtime {
        SolverRuntime::Native { .. } | SolverRuntime::Generic { .. } => "native",
        SolverRuntime::External { .. } => "external executable",
    };
    let execution_summary = format!(
        "Completed one batched {runtime_name} solver invocation for the {} sweep ({} cases).",
        solver,
        frequency_cases.len(),
    );
    on_stdout_line(&execution_summary);
    Ok(())
}

fn run_solver_case(
    runtime: &SolverRuntime,
    request_path: &Path,
    on_stdout_line: &mut dyn FnMut(&str),
) -> Result<(i32, String), Box<dyn Error>> {
    let root = application_root()?;
    let (executable, arguments) = match runtime {
        SolverRuntime::Native { module } => (
            env::current_exe()?,
            vec![
                "--scenario-module".to_string(),
                module.clone(),
                "--solve".to_string(),
                request_path.to_string_lossy().into_owned(),
            ],
        ),
        SolverRuntime::Generic { kind, profile } => (
            env::current_exe()?,
            vec!["--scenario-runtime".into(), kind.clone(), profile.clone(), "--solve".into(),
                request_path.to_string_lossy().into_owned()],
        ),
        SolverRuntime::External {
            executable,
            solve_argument,
            ..
        } => (
            executable.clone(),
            vec![
                solve_argument.clone(),
                request_path.to_string_lossy().into_owned(),
            ],
        ),
    };
    run_child(&executable, &arguments, request_path, &root, on_stdout_line)
}

fn run_solver_sequence(
    runtime: &SolverRuntime,
    manifest_path: &Path,
    on_stdout_line: &mut dyn FnMut(&str),
) -> Result<(i32, String), Box<dyn Error>> {
    let root = application_root()?;
    let manifest = manifest_path.to_string_lossy().into_owned();
    let (executable, arguments) = match runtime {
        SolverRuntime::Native { module } => (
            env::current_exe()?,
            vec![
                "--scenario-module".to_string(),
                module.clone(),
                "--sequence".to_string(),
                manifest,
            ],
        ),
        SolverRuntime::Generic { kind, profile } => (
            env::current_exe()?,
            vec!["--scenario-runtime".into(), kind.clone(), profile.clone(), "--sequence".into(), manifest],
        ),
        SolverRuntime::External {
            executable,
            sequence_argument,
            ..
        } => (
            executable.clone(),
            vec![sequence_argument.clone(), manifest],
        ),
    };
    run_child(
        &executable,
        &arguments,
        manifest_path,
        &root,
        on_stdout_line,
    )
}

fn run_child(
    executable: &Path,
    arguments: &[String],
    input_path: &Path,
    root: &Path,
    on_stdout_line: &mut dyn FnMut(&str),
) -> Result<(i32, String), Box<dyn Error>> {
    if !input_path.is_file() {
        return Err(format!("Solver input is missing: {}", input_path.display()).into());
    }
    let mut child = crate::model_schema::command(executable)?
        .args(arguments)
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "Could not start scenario solver {}: {error}",
                executable.display()
            )
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Could not capture scenario solver stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Could not capture scenario solver stderr")?;
    let stderr_thread = std::thread::spawn(move || -> std::io::Result<String> {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::BufReader::new(stderr), &mut text)?;
        Ok(text)
    });
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        line.clear();
        if std::io::BufRead::read_until(&mut reader, b'\n', &mut line)? == 0 {
            break;
        }
        on_stdout_line(String::from_utf8_lossy(&line).trim_end());
    }
    let status = child.wait()?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| "Scenario solver stderr reader stopped unexpectedly")??;
    Ok((status.code().unwrap_or(-1), stderr))
}

fn append_case_csv(merged: &mut String, path: &Path) -> Result<(), Box<dyn Error>> {
    let csv = fs::read_to_string(path)?;
    let mut lines = csv.lines();
    let header = lines
        .next()
        .ok_or_else(|| format!("Solver output {} is empty.", path.display()))?;
    if merged.is_empty() {
        merged.push_str(header);
        merged.push('\n');
    } else {
        let expected = merged.lines().next().unwrap_or("");
        if expected != header {
            return Err(format!(
                "Solver output {} has a different CSV header from the earlier cases.",
                path.display()
            )
            .into());
        }
    }
    for line in lines {
        if !line.is_empty() {
            merged.push_str(line);
            merged.push('\n');
        }
    }
    Ok(())
}
