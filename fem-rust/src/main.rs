use serde_json::{Value, json};
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::path::PathBuf;

// Shared sweep progress is independent of each solver's diagnostic wording.
const SOLVER_PROGRESS_PREFIX: &str = "FULLMETA_PROGRESS ";

fn solver_case_progress_line(
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

#[cfg(any(
    feature = "solver-native-acoustic2d",
    feature = "solver-native-acoustic3d"
))]
pub(crate) mod acoustic2d;
#[cfg(feature = "solver-native-acoustic3d")]
pub(crate) mod acoustic3d;
pub(crate) mod coordinator;
#[cfg(feature = "solver-native-em2d")]
pub(crate) mod em2d;
#[cfg(feature = "solver-native-em3d")]
pub(crate) mod em3d;
#[cfg(any(
    feature = "solver-native-acoustic2d",
    feature = "solver-native-acoustic3d",
    feature = "solver-native-em2d",
    feature = "solver-native-em3d"
))]
pub(crate) mod field_output;
#[cfg(any(
    feature = "solver-native-acoustic2d",
    feature = "solver-native-acoustic3d",
    feature = "solver-native-em2d",
    feature = "solver-native-em3d"
))]
pub(crate) mod integration;
#[cfg(any(
    feature = "solver-native-acoustic2d",
    feature = "solver-native-acoustic3d",
    feature = "solver-native-em2d",
    feature = "solver-native-em3d"
))]
pub(crate) mod material;
#[cfg(any(
    feature = "solver-native-acoustic2d",
    feature = "solver-native-acoustic3d",
    feature = "solver-native-em2d"
))]
pub(crate) mod mesh2d;
pub(crate) mod model_expression;
pub(crate) mod model_schema;
pub(crate) mod preflight_client;
#[cfg(any(
    feature = "solver-native-acoustic2d",
    feature = "solver-native-acoustic3d",
    feature = "solver-native-em2d"
))]
pub(crate) mod scalar2d;
#[cfg(any(
    feature = "solver-native-acoustic2d",
    feature = "solver-native-acoustic3d",
    feature = "solver-native-em2d",
    feature = "solver-native-em3d"
))]
pub(crate) mod solver_common;
pub(crate) mod solver_request;
pub(crate) mod validation_receipt;
pub(crate) use model_schema as json_schema;

#[cfg(any(
    feature = "solver-native-acoustic2d",
    feature = "solver-native-acoustic3d",
    feature = "solver-native-em2d",
    feature = "solver-native-em3d"
))]
pub(crate) use solver_common::{
    C, MU0, SolverCache, TET_Q, TRI_Q, c_add, c_mul, c_scale, c_sqrt, solve_linear_system,
};

/// Raw-model checks are performed by the common FEM preflight service.
pub(crate) fn validate_native_model(model: &Value) -> Vec<String> {
    crate::preflight_client::validation_errors(model)
}

fn main() -> std::process::ExitCode {
    match run_native() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("fullmeta-fem: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

pub(crate) fn run_native() -> Result<std::process::ExitCode, Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if let Some(project) = args.get(1).filter(|value| !value.starts_with('-')) {
        let root = fullmeta_model::runtime_paths::installation_root()?;
        let file = root
            .join("projects")
            .join(project.strip_prefix("projects/").unwrap_or(project))
            .join("fem_model.json");
        let model: Value = serde_json::from_slice(&fs::read(&file)?)?;
        model_schema::select_schema_for_model_file(&model, &file)?;
    }
    #[cfg(any(
        feature = "solver-native-acoustic2d",
        feature = "solver-native-acoustic3d",
        feature = "solver-native-em2d",
        feature = "solver-native-em3d"
    ))]
    {
        if args.get(1).map(String::as_str) == Some("--solve") {
            crate::model_schema::ensure_scenario_metadata()?;
            let request = args
                .get(2)
                .ok_or("--solve requires one solver request JSON path.")?;
            run_one_case(Path::new(request))?;
            return Ok(std::process::ExitCode::SUCCESS);
        }
        if args.get(1).map(String::as_str) == Some("--sequence") {
            crate::model_schema::ensure_scenario_metadata()?;
            let manifest = args
                .get(2)
                .ok_or("--sequence requires a solver request sequence manifest path.")?;
            run_scenario_sequence(Path::new(manifest))?;
            return Ok(std::process::ExitCode::SUCCESS);
        }
    }
    if !matches!(
        args.get(1).map(String::as_str),
        Some("--prepare-study" | "--capabilities")
    ) {
        return Ok(coordinator::dispatch());
    }
    crate::model_schema::ensure_scenario_metadata()?;
    let capabilities = json_schema::engine_capabilities()?;
    if args.get(1).map(String::as_str) == Some("--prepare-study") {
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
    if args.get(1).map(String::as_str) == Some("--capabilities") {
        let runtime = json!({"schema":"fullmeta-solver-runtime/1","solverConfigurationSchema":capabilities["study"]["requestConfigurationSchema"],"solverRequestSchema":capabilities["study"]["solverRequestSchema"],"engineCapabilities":capabilities});
        println!("{}", serde_json::to_string(&runtime)?);
        return Ok(std::process::ExitCode::SUCCESS);
    }
    Err("Usage: fullmeta-fem <project> [--validate] [--study <id>] [--csv <file>] | --prepare-study | --capabilities | --solve <request.json> | --sequence <manifest.json>".into())
}

#[derive(Clone)]
pub(crate) enum SolverRuntime {
    #[cfg(feature = "solver-native-acoustic2d")]
    NativeAcoustic2d,
    #[cfg(feature = "solver-native-acoustic3d")]
    NativeAcoustic3d,
    #[cfg(feature = "solver-native-em2d")]
    NativeEm2d,
    #[cfg(feature = "solver-native-em3d")]
    NativeEm3d,
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
    #[cfg(feature = "solver-native-acoustic2d")]
    let native_acoustic2d_runtime = matches!(&runtime, SolverRuntime::NativeAcoustic2d);
    #[cfg(not(feature = "solver-native-acoustic2d"))]
    let native_acoustic2d_runtime = false;
    #[cfg(feature = "solver-native-em3d")]
    let native_em3d_runtime = matches!(&runtime, SolverRuntime::NativeEm3d);
    #[cfg(not(feature = "solver-native-em3d"))]
    let native_em3d_runtime = false;
    #[cfg(feature = "solver-native-acoustic3d")]
    let native_acoustic3d_runtime = matches!(&runtime, SolverRuntime::NativeAcoustic3d);
    #[cfg(not(feature = "solver-native-acoustic3d"))]
    let native_acoustic3d_runtime = false;
    #[cfg(feature = "solver-native-em2d")]
    let native_em2d_runtime = matches!(&runtime, SolverRuntime::NativeEm2d);
    #[cfg(not(feature = "solver-native-em2d"))]
    let native_em2d_runtime = false;
    let native_runtime = native_acoustic2d_runtime
        || native_acoustic3d_runtime
        || native_em2d_runtime
        || native_em3d_runtime;

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

    #[cfg(feature = "solver-native-em3d")]
    let native_em3d_sequence_result = if native_em3d_runtime {
        for (index, case) in frequency_cases.iter().enumerate() {
            on_stdout_line(&format!(
                "Prepared em-3d case {} of {} at {} Hz.",
                index + 1,
                frequency_cases.len(),
                case["frequencyHz"]
            ));
        }
        let manifest_name = format!(".fullmeta-em3d-sequence-{run_token}.json");
        let manifest_path = request_directory.join(&manifest_name);
        let request_names = case_jobs
            .iter()
            .map(|(path, _, _, _)| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        format!("Invalid generated case request path {}.", path.display())
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        fs::write(
            &manifest_path,
            serde_json::to_vec(&json!({"requestFiles":request_names}))?,
        )?;
        let result = run_native_em3d_sequence(&manifest_path, on_stdout_line);
        let _ = fs::remove_file(&manifest_path);
        Some(result)
    } else {
        None
    };
    #[cfg(feature = "solver-native-em3d")]
    if let Some(result) = native_em3d_sequence_result {
        for (case_request_path, _, _, _) in &case_jobs {
            let _ = fs::remove_file(case_request_path);
        }
        let (exit_code, stderr) = result?;
        if exit_code != 0 {
            return Err(format!(
                "Scenario solver exited with code {exit_code}. {}",
                stderr.trim()
            )
            .into());
        }
    }

    #[cfg(feature = "solver-native-acoustic3d")]
    let native_acoustic3d_sequence_result = if native_acoustic3d_runtime {
        for (index, case) in frequency_cases.iter().enumerate() {
            on_stdout_line(&format!(
                "Prepared acoustic-3d RIVRS case {} of {} at {} Hz.",
                index + 1,
                frequency_cases.len(),
                case["frequencyHz"]
            ));
        }
        let manifest_name = format!(".fullmeta-acoustic3d-sequence-{run_token}.json");
        let manifest_path = request_directory.join(&manifest_name);
        let request_names = case_jobs
            .iter()
            .map(|(path, _, _, _)| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        format!("Invalid generated case request path {}.", path.display())
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        fs::write(
            &manifest_path,
            serde_json::to_vec(&json!({"requestFiles":request_names}))?,
        )?;
        let result = run_native_acoustic3d_sequence(&manifest_path, on_stdout_line);
        let _ = fs::remove_file(&manifest_path);
        Some(result)
    } else {
        None
    };
    #[cfg(feature = "solver-native-acoustic3d")]
    if let Some(result) = native_acoustic3d_sequence_result {
        for (case_request_path, _, _, _) in &case_jobs {
            let _ = fs::remove_file(case_request_path);
        }
        let (exit_code, stderr) = result?;
        if exit_code != 0 {
            return Err(format!(
                "Scenario solver exited with code {exit_code}. {}",
                stderr.trim()
            )
            .into());
        }
    }

    if !native_em3d_runtime && !native_acoustic3d_runtime {
        for (index, (case_request_path, _, _, _)) in case_jobs.iter().enumerate() {
            let case = &frequency_cases[index];
            let frequency_hz = case["frequencyHz"]
                .as_f64()
                .ok_or("Prepared case frequency is missing.")?;
            on_stdout_line(&solver_case_progress_line(
                "case-start",
                index,
                frequency_cases.len(),
                frequency_hz,
            ));
            on_stdout_line(&format!(
                "Running {} case {} of {} at {} Hz.",
                solver,
                index + 1,
                frequency_cases.len(),
                case["frequencyHz"]
            ));
            let solver_result = run_solver_case(&runtime, case_request_path, on_stdout_line);
            let _ = fs::remove_file(case_request_path);
            let (exit_code, stderr) = solver_result?;
            if exit_code != 0 {
                return Err(format!(
                    "Scenario solver exited with code {exit_code}. {}",
                    stderr.trim()
                )
                .into());
            }
            on_stdout_line(&solver_case_progress_line(
                "case-complete",
                index,
                frequency_cases.len(),
                frequency_hz,
            ));
        }
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
        #[cfg(feature = "solver-native-acoustic2d")]
        SolverRuntime::NativeAcoustic2d => "native",
        #[cfg(feature = "solver-native-em3d")]
        SolverRuntime::NativeEm3d => "native",
        #[cfg(feature = "solver-native-acoustic3d")]
        SolverRuntime::NativeAcoustic3d => "native",
        #[cfg(feature = "solver-native-em2d")]
        SolverRuntime::NativeEm2d => "native",
        SolverRuntime::External { .. } => "external executable",
    };
    #[cfg(feature = "solver-native-em3d")]
    let execution_summary = if native_runtime {
        format!(
            "Completed {} one-case native solver case(s) in one process; symbolic analysis was reused whenever the matrix pattern matched.",
            frequency_cases.len()
        )
    } else {
        format!(
            "Completed {} one-case {runtime_name} solver invocation(s) for the {} sweep.",
            frequency_cases.len(),
            solver
        )
    };
    #[cfg(not(feature = "solver-native-em3d"))]
    let execution_summary = format!(
        "Completed {} one-case {runtime_name} solver invocation(s) for the {} sweep.",
        frequency_cases.len(),
        solver
    );
    on_stdout_line(&execution_summary);
    Ok(())
}

#[cfg(feature = "solver-native-em3d")]
fn run_em3d_sequence(manifest_path: &Path) -> Result<(), Box<dyn Error>> {
    let mut solver_cache = SolverCache::default();
    run_request_sequence(manifest_path, &mut |request_path| {
        em3d::run_request_with_cache(request_path, &mut solver_cache)
    })
}

#[cfg(feature = "solver-native-acoustic3d")]
fn run_acoustic3d_sequence(manifest_path: &Path) -> Result<(), Box<dyn Error>> {
    let mut solver_cache = SolverCache::default();
    run_request_sequence(manifest_path, &mut |request_path| {
        acoustic3d::run_request_with_cache(request_path, &mut solver_cache)
    })
}

fn run_one_case(request_path: &Path) -> Result<(), Box<dyn Error>> {
    #[cfg(feature = "solver-native-acoustic3d")]
    {
        return acoustic3d::run_request(request_path);
    }
    #[cfg(feature = "solver-native-acoustic2d")]
    {
        return acoustic2d::run_request(request_path);
    }
    #[cfg(feature = "solver-native-em2d")]
    {
        return em2d::run_request(request_path);
    }
    #[cfg(feature = "solver-native-em3d")]
    {
        return em3d::run_request(request_path);
    }
    #[allow(unreachable_code)]
    Err("This fullmeta-fem executable has no scenario solver selected.".into())
}

fn run_scenario_sequence(manifest_path: &Path) -> Result<(), Box<dyn Error>> {
    #[cfg(feature = "solver-native-em3d")]
    {
        return run_em3d_sequence(manifest_path);
    }
    #[cfg(feature = "solver-native-acoustic3d")]
    {
        return run_acoustic3d_sequence(manifest_path);
    }
    #[cfg(any(feature = "solver-native-acoustic2d", feature = "solver-native-em2d"))]
    {
        return run_2d_sequence(manifest_path);
    }
    #[allow(unreachable_code)]
    Err("This fullmeta-fem executable has no scenario solver selected.".into())
}

#[cfg(any(feature = "solver-native-acoustic2d", feature = "solver-native-em2d"))]
fn run_2d_sequence(manifest_path: &Path) -> Result<(), Box<dyn Error>> {
    run_request_sequence(manifest_path, &mut run_one_case)
}

fn run_request_sequence(
    manifest_path: &Path,
    run_case: &mut dyn FnMut(&Path) -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let manifest: Value = serde_json::from_slice(&fs::read(manifest_path)?)?;
    let request_files = manifest
        .get("requestFiles")
        .and_then(Value::as_array)
        .ok_or("Solver sequence manifest requestFiles must be an array.")?;
    if request_files.is_empty() {
        return Err("Solver sequence manifest must contain at least one request file.".into());
    }
    let manifest_directory = manifest_path.parent().unwrap_or(Path::new("."));
    for (index, item) in request_files.iter().enumerate() {
        let name = item
            .as_str()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                format!("Solver sequence manifest requestFiles[{index}] must be a non-empty path.")
            })?;
        let request_path = Path::new(name);
        if request_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!("Solver sequence manifest requestFiles[{index}] must not traverse parent directories.").into());
        }
        let request_path = if request_path.is_absolute() {
            request_path.to_path_buf()
        } else {
            manifest_directory.join(request_path)
        };
        let request: Value = serde_json::from_slice(&fs::read(&request_path)?)?;
        let frequency_hz = solver_request::single_case_frequency(&request)?;
        println!(
            "{}",
            solver_case_progress_line("case-start", index, request_files.len(), frequency_hz)
        );
        std::io::Write::flush(&mut io::stdout())?;
        run_case(&request_path)?;
        println!(
            "{}",
            solver_case_progress_line("case-complete", index, request_files.len(), frequency_hz)
        );
        std::io::Write::flush(&mut io::stdout())?;
    }
    Ok(())
}

fn run_solver_case(
    runtime: &SolverRuntime,
    request_path: &Path,
    on_stdout_line: &mut dyn FnMut(&str),
) -> Result<(i32, String), Box<dyn Error>> {
    let root = application_root()?;
    let (executable, argument) = match runtime {
        #[cfg(feature = "solver-native-acoustic2d")]
        SolverRuntime::NativeAcoustic2d => (env::current_exe()?, "--solve".to_string()),
        #[cfg(feature = "solver-native-acoustic3d")]
        SolverRuntime::NativeAcoustic3d => (env::current_exe()?, "--solve".to_string()),
        #[cfg(feature = "solver-native-em2d")]
        SolverRuntime::NativeEm2d => (env::current_exe()?, "--solve".to_string()),
        #[cfg(feature = "solver-native-em3d")]
        SolverRuntime::NativeEm3d => (env::current_exe()?, "--solve".to_string()),
        SolverRuntime::External {
            executable,
            solve_argument,
            ..
        } => (executable.clone(), solve_argument.clone()),
    };
    run_child(&executable, &argument, request_path, &root, on_stdout_line)
}

fn run_child(
    executable: &Path,
    argument: &str,
    input_path: &Path,
    root: &Path,
    on_stdout_line: &mut dyn FnMut(&str),
) -> Result<(i32, String), Box<dyn Error>> {
    if !input_path.is_file() {
        return Err(format!("Solver input is missing: {}", input_path.display()).into());
    }
    let mut child = crate::model_schema::command(executable)?
        .arg(argument)
        .arg(input_path)
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

#[cfg(feature = "solver-native-acoustic3d")]
fn run_native_acoustic3d_sequence(
    manifest_path: &Path,
    on_stdout_line: &mut dyn FnMut(&str),
) -> Result<(i32, String), Box<dyn Error>> {
    let root = application_root()?;
    if !manifest_path.is_file() {
        return Err(format!(
            "Acoustic 3D sequence manifest is missing: {}",
            manifest_path.display()
        )
        .into());
    }
    let executable = env::current_exe()?;
    let mut child = crate::model_schema::command(&executable)?
        .arg("--sequence")
        .arg(manifest_path)
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "Could not start native acoustic-3d sweep solver {}: {error}",
                executable.display()
            )
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Could not capture native acoustic-3d sweep stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Could not capture native acoustic-3d sweep stderr")?;
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
        .map_err(|_| "Native acoustic-3d sweep stderr stopped unexpectedly")??;
    Ok((status.code().unwrap_or(-1), stderr))
}

#[cfg(feature = "solver-native-em3d")]
fn run_native_em3d_sequence(
    manifest_path: &Path,
    on_stdout_line: &mut dyn FnMut(&str),
) -> Result<(i32, String), Box<dyn Error>> {
    let root = application_root()?;
    if !manifest_path.is_file() {
        return Err(format!(
            "EM3D sequence manifest is missing: {}",
            manifest_path.display()
        )
        .into());
    }
    let executable = env::current_exe()?;
    let mut child = crate::model_schema::command(&executable)?
        .arg("--sequence")
        .arg(manifest_path)
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            format!(
                "Could not start native EM3D sweep solver {}: {error}",
                executable.display()
            )
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Could not capture native EM3D sweep stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Could not capture native EM3D sweep stderr")?;
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
        .map_err(|_| "Native EM3D sweep stderr reader stopped unexpectedly")??;
    Ok((status.code().unwrap_or(-1), stderr))
}

#[cfg(not(target_arch = "wasm32"))]
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
