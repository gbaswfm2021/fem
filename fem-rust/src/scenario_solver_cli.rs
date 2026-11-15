use crate::SolverCache;
use serde_json::Value;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(crate) fn run<F>(arguments: &[String], solver: &str, solve: F) -> Result<i32, String>
where
    F: Fn(&Path, &mut SolverCache) -> Result<(), Box<dyn std::error::Error>>,
{
    match arguments {
        [command, request] if command == "--solve" => {
            let request_path = Path::new(request);
            validate_request(request_path, solver)?;
            solve(request_path, &mut SolverCache::default()).map_err(|error| error.to_string())?;
            Ok(0)
        }
        [command, manifest] if command == "--sequence" => {
            run_sequence(Path::new(manifest), solver, solve)?;
            Ok(0)
        }
        _ => Err(format!(
            "Usage: {solver}.exe --solve <request.json> | --sequence <manifest.json>"
        )),
    }
}

pub(crate) fn run_with_fallback<F, R>(
    arguments: &[String],
    solver: &str,
    solve: F,
    fallback: R,
) -> Result<i32, String>
where
    F: Fn(&Path, &mut SolverCache) -> Result<(), Box<dyn std::error::Error>>,
    R: FnOnce(&[String]) -> Result<i32, String>,
{
    if matches!(
        arguments.first().map(String::as_str),
        Some("--solve" | "--sequence")
    ) {
        run(arguments, solver, solve)
    } else {
        fallback(arguments)
    }
}

fn run_sequence<F>(manifest_path: &Path, solver: &str, solve: F) -> Result<(), String>
where
    F: Fn(&Path, &mut SolverCache) -> Result<(), Box<dyn std::error::Error>>,
{
    let manifest: Value = serde_json::from_slice(
        &fs::read(manifest_path)
            .map_err(|error| format!("Could not read sequence manifest: {error}"))?,
    )
    .map_err(|error| format!("Invalid sequence manifest JSON: {error}"))?;
    let request_files = manifest
        .get("requestFiles")
        .and_then(Value::as_array)
        .ok_or("Solver sequence manifest requestFiles must be an array.")?;
    if request_files.is_empty() {
        return Err("Solver sequence manifest must contain at least one request file.".into());
    }

    let manifest_directory = manifest_path.parent().unwrap_or(Path::new("."));
    let total_cases = request_files.len();
    let mut solver_cache = SolverCache::default();
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
            return Err(format!(
                "Solver sequence manifest requestFiles[{index}] must not traverse parent directories."
            ));
        }
        let request_path: PathBuf = if request_path.is_absolute() {
            request_path.to_path_buf()
        } else {
            manifest_directory.join(request_path)
        };
        let request = validate_request(&request_path, solver)?;
        let frequency_hz = request
            .pointer("/frequencyCases/0/frequencyHz")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| {
                format!(
                    "Solver sequence requestFiles[{index}] must contain one positive finite frequency case."
                )
            })?;
        emit_case_progress("case-start", index, total_cases, frequency_hz)?;
        println!("Solving {solver} case {} of {total_cases}.", index + 1);
        solve(&request_path, &mut solver_cache).map_err(|error| error.to_string())?;
        emit_case_progress("case-complete", index, total_cases, frequency_hz)?;
    }
    Ok(())
}

fn emit_case_progress(
    event: &str,
    case_index: usize,
    total_cases: usize,
    frequency_hz: f64,
) -> Result<(), String> {
    let line = crate::solver_orchestration::solver_case_progress_line(
        event,
        case_index,
        total_cases,
        frequency_hz,
    );
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    writeln!(stdout, "{line}")
        .and_then(|_| stdout.flush())
        .map_err(|error| format!("Could not report solver case progress: {error}"))
}

fn validate_request(request_path: &Path, solver: &str) -> Result<Value, String> {
    let request: Value = serde_json::from_slice(
        &fs::read(request_path)
            .map_err(|error| format!("Could not read solver request: {error}"))?,
    )
    .map_err(|error| format!("Invalid solver request JSON: {error}"))?;
    crate::solver_request::validate_request(&request, solver)?;
    if request.get("frequencyCases").is_some() {
        crate::solver_request::validate_single_case_request(&request, solver)?;
    }
    Ok(request)
}
