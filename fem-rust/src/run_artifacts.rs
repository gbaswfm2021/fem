//! Shared run field-artifact handling. Callers select their own output files
//! and attach metadata; this module does not interpret scenario filenames.

#[path = "study_fingerprint.rs"]
mod study_fingerprint;

use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

type Result<T> = std::result::Result<T, String>;

pub(super) fn completion_solver_log_files(run_directory: &str) -> [String; 3] {
    [
        "logs/solver.stdout.log",
        "logs/solver.stderr.log",
        "logs/solver-stage-timings.csv",
    ]
    .map(|path| format!("{run_directory}/{path}"))
}

fn field_filename(name: &str) -> Result<()> {
    if Path::new(name).file_name().and_then(|value| value.to_str()) != Some(name)
        || name.contains('/')
        || name.contains('\\')
        || !name.ends_with(".json")
    {
        return Err(
            "A field artifact must be a JSON filename inside its run output directory.".into(),
        );
    }
    Ok(())
}

fn regular_field_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    let is_link = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let is_link = metadata.file_type().is_symlink();
    if is_link || !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "Field artifact {} is not a nonempty regular file.",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod completion_output_tests {
    use super::completion_solver_log_files;

    #[test]
    fn completion_log_paths_are_bound_to_the_run_directory() {
        assert_eq!(
            completion_solver_log_files("projects/example/fem_runs/run-1"),
            [
                "projects/example/fem_runs/run-1/logs/solver.stdout.log",
                "projects/example/fem_runs/run-1/logs/solver.stderr.log",
                "projects/example/fem_runs/run-1/logs/solver-stage-timings.csv",
            ]
        );
    }
}

pub(super) fn register_case_field_file(
    output_directory: &Path,
    name: &str,
    case_index: usize,
    metadata: &Value,
) -> Result<Value> {
    field_filename(name)?;
    regular_field_file(&output_directory.join(name))?;
    let mut artifact = metadata
        .as_object()
        .ok_or("Field artifact metadata must be an object.")?
        .clone();
    artifact.insert("name".into(), json!(name));
    artifact.insert("caseIndex".into(), json!(case_index));
    Ok(Value::Object(artifact))
}

pub(super) fn collect_case_field_files(
    case_directory: &Path,
    output_directory: &Path,
    case_index: usize,
    metadata: &Value,
    include_filename: impl Fn(&str) -> bool,
) -> Result<Vec<Value>> {
    let mut entries = fs::read_dir(case_directory)
        .map_err(|error| error.to_string())?
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut artifacts = Vec::new();
    for entry in entries {
        let source_name = entry
            .file_name()
            .into_string()
            .map_err(|_| "A field output filename is not valid UTF-8.")?;
        if !include_filename(&source_name) {
            continue;
        }
        field_filename(&source_name)?;
        let source = entry.path();
        regular_field_file(&source)?;
        let source_path = Path::new(&source_name);
        let stem = source_path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("Invalid field output filename.")?;
        let extension = source_path
            .extension()
            .and_then(|value| value.to_str())
            .ok_or("Field output filename has no extension.")?;
        let mut destination_name = source_name.clone();
        let mut destination = output_directory.join(&destination_name);
        if fs::symlink_metadata(&destination).is_ok() {
            destination_name = format!("{stem}_case-{:04}.{extension}", case_index + 1);
            destination = output_directory.join(&destination_name);
            let mut duplicate = 2usize;
            while fs::symlink_metadata(&destination).is_ok() {
                destination_name =
                    format!("{stem}_case-{:04}-{duplicate}.{extension}", case_index + 1);
                destination = output_directory.join(&destination_name);
                duplicate += 1;
            }
        }
        fs::rename(&source, &destination).map_err(|error| error.to_string())?;
        artifacts.push(register_case_field_file(
            output_directory,
            &destination_name,
            case_index,
            metadata,
        )?);
    }
    Ok(artifacts)
}

pub(super) fn case_field_file_names(
    output_directory: &Path,
    artifacts: &[Value],
    expected_cases: usize,
) -> Result<Vec<String>> {
    if artifacts.len() != expected_cases {
        return Err(format!(
            "The solver wrote {} field files for {expected_cases} requested cases.",
            artifacts.len()
        ));
    }
    // Case indices distinguish parameter combinations sharing a frequency.
    // Completion and consumers use this index rather than rediscovering files
    // from any scenario's filename convention.
    let mut names_by_case = vec![None; expected_cases];
    let mut names = HashSet::new();
    for artifact in artifacts {
        let name = artifact
            .get("name")
            .and_then(Value::as_str)
            .ok_or("A field artifact is missing its filename.")?;
        field_filename(name)?;
        let case_index = artifact
            .get("caseIndex")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .filter(|value| *value < expected_cases)
            .ok_or("A field artifact has an invalid case index.")?;
        if names_by_case[case_index].is_some() || !names.insert(name.to_string()) {
            return Err("The solver wrote duplicate field artifacts for a requested case.".into());
        }
        regular_field_file(&output_directory.join(name))?;
        names_by_case[case_index] = Some(name.to_string());
    }
    names_by_case
        .into_iter()
        .map(|name| name.ok_or_else(|| "A requested case is missing its field artifact.".into()))
        .collect()
}
pub(crate) fn solver_stage(line: &str) -> Result<Option<(&'static str, &'static str)>> {
    let Some(payload) = line.strip_prefix("FULLMETA_STAGE ") else {
        return Ok(None);
    };
    let event: serde_json::Value = serde_json::from_str(payload)
        .map_err(|error| format!("Invalid solver stage event: {error}"))?;
    if event.get("event").and_then(serde_json::Value::as_str) != Some("stage-start") {
        return Err("Solver stage event has an unknown event type.".into());
    }
    let stage = match event.get("stage").and_then(serde_json::Value::as_str) {
        Some("assembly") => ("assembly", "Assembling the numerical system…"),
        Some("symbolic-analysis") => ("symbolic-analysis", "Analyzing the sparse system…"),
        Some("factorization") => ("factorization", "Factoring the sparse system…"),
        Some("solve") => ("solve", "Solving the numerical system…"),
        Some("output") => ("output", "Writing the numerical results…"),
        _ => return Err("Solver stage event has an unknown stage.".into()),
    };
    Ok(Some(stage))
}

pub(crate) fn coordinated_run_id() -> Option<String> {
    std::env::var("FULLMETA_RUN_ID").ok().filter(|id| {
        id.len() <= 160
            && id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    })
}

pub(crate) fn adopt_server_record(
    path: &std::path::Path,
    record: &mut serde_json::Value,
) -> Result<bool> {
    let Some(run_id) = coordinated_run_id() else {
        if let Some(fingerprint) = record.pointer("/model/snapshot").and_then(|snapshot| {
            study_fingerprint::fingerprint(snapshot, record.pointer("/study/id")?.as_str()?)
        }) {
            record["study"]["inputFingerprint"] = fingerprint;
        }
        return Ok(false);
    };
    let previous: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|error| {
            format!(
                "Could not read the dispatcher Study record {}: {error}",
                path.display()
            )
        })?)
        .map_err(|error| format!("Invalid dispatcher Study record: {error}"))?;
    if previous.get("runId").and_then(serde_json::Value::as_str) != Some(run_id.as_str())
        || previous.get("state").and_then(serde_json::Value::as_str) != Some("running")
        || previous.pointer("/study/id") != record.pointer("/study/id")
        || previous.pointer("/model/file") != record.pointer("/model/file")
    {
        return Err("The dispatcher Study record does not match the prepared run.".into());
    }
    for field in [
        "operationId",
        "startedAt",
        "worker",
        "operationContract",
        "requestFingerprint",
    ] {
        if let Some(value) = previous.get(field) {
            record[field] = value.clone();
        }
    }
    if let Some(fingerprint) = previous.pointer("/study/inputFingerprint") {
        record["study"]["inputFingerprint"] = fingerprint.clone();
    }
    Ok(true)
}

pub(crate) struct SolverStageTimings {
    file: std::fs::File,
    active: Option<(usize, Option<f64>, &'static str, std::time::Instant)>,
}

impl SolverStageTimings {
    pub(crate) fn create(path: &std::path::Path) -> std::io::Result<Self> {
        use std::io::Write;
        let mut file = std::fs::File::create(path)?;
        writeln!(file, "case,frequency_Hz,stage,duration_ms")?;
        Ok(Self { file, active: None })
    }

    pub(crate) fn start(
        &mut self,
        case: usize,
        frequency: Option<f64>,
        stage: &'static str,
    ) -> std::io::Result<()> {
        let now = std::time::Instant::now();
        self.write_active(now)?;
        self.active = Some((case, frequency, stage, now));
        Ok(())
    }

    pub(crate) fn finish(&mut self) -> std::io::Result<()> {
        self.write_active(std::time::Instant::now())
    }

    fn write_active(&mut self, ended: std::time::Instant) -> std::io::Result<()> {
        use std::io::Write;
        if let Some((case, frequency, stage, started)) = self.active.take() {
            let frequency = frequency.map(|hz| hz.to_string()).unwrap_or_default();
            let elapsed_ms = ended.duration_since(started).as_secs_f64() * 1000.0;
            writeln!(self.file, "{case},{frequency},{stage},{elapsed_ms:.6}")?;
            self.file.flush()?;
        }
        Ok(())
    }
}
