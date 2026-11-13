use crate::SolverCache;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn run<F>(solver: &str, solve: F) -> Result<(), String>
where
    F: Fn(&Path, &mut SolverCache) -> Result<(), Box<dyn std::error::Error>>,
{
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, request] if command == "--solve" => {
            let request_path = Path::new(request);
            validate_request(request_path, solver)?;
            solve(request_path, &mut SolverCache::default()).map_err(|error| error.to_string())
        }
        [command, manifest] if command == "--sequence" => {
            run_sequence(Path::new(manifest), solver, solve)
        }
        _ => Err(format!("Usage: {solver}.exe --solve <request.json> | --sequence <manifest.json>")),
    }
}

fn run_sequence<F>(
    manifest_path: &Path,
    solver: &str,
    solve: F,
) -> Result<(), String>
where
    F: Fn(&Path, &mut SolverCache) -> Result<(), Box<dyn std::error::Error>>,
{
    let manifest: Value = serde_json::from_slice(
        &fs::read(manifest_path).map_err(|error| format!("Could not read sequence manifest: {error}"))?,
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
    let mut solver_cache = SolverCache::default();
    for (index, item) in request_files.iter().enumerate() {
        let name = item
            .as_str()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("Solver sequence manifest requestFiles[{index}] must be a non-empty path."))?;
        let request_path = Path::new(name);
        if request_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(format!("Solver sequence manifest requestFiles[{index}] must not traverse parent directories."));
        }
        let request_path: PathBuf = if request_path.is_absolute() {
            request_path.to_path_buf()
        } else {
            manifest_directory.join(request_path)
        };
        validate_request(&request_path, solver)?;
        println!("Solving {solver} case {} of {}.", index + 1, request_files.len());
        solve(&request_path, &mut solver_cache).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn validate_request(request_path: &Path, solver: &str) -> Result<(), String> {
    let request: Value = serde_json::from_slice(
        &fs::read(request_path).map_err(|error| format!("Could not read solver request: {error}"))?,
    )
    .map_err(|error| format!("Invalid solver request JSON: {error}"))?;
    crate::solver_request::validate_single_case_request(&request, solver)
}
