//! Document-scoped removal of generated, terminal Study runs.

use serde_json::{Value, json};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::{
    Result, checked_descendant, checked_open_document, checked_run_id, is_link,
    model_document_filename_is_valid, read_json, utc_timestamp, verify_document_schema,
    workspace_document_path,
};

const PROJECT_RUN_LOCK: &str = ".fullmeta-run.lock";

pub(super) fn clear_from_stdin() -> Result<i32> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|error| error.to_string())?;
    let request: Value = serde_json::from_slice(&input)
        .map_err(|error| format!("Invalid Clear Results request: {error}"))?;
    let required = |key: &str| {
        request
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("Clear Results requires {key}."))
    };
    let result = clear_results(required("folder")?, required("file")?)?;
    println!("{result}");
    Ok(0)
}

fn clear_results(folder: &str, file: &str) -> Result<Value> {
    let document = checked_open_document(folder, file)?;
    let document_key = workspace_document_path(folder, file);
    let project = document
        .parent()
        .ok_or("The opened document has no project folder.")?;
    let storage = checked_run_storage(&document)?;
    let mut report = ClearReport::default();
    let Some(_lock) = ProjectRunLock::acquire(project)? else {
        report.blocked = Some(
            "The project run lock is busy. Finish or reconcile the active operation before clearing results.",
        );
        return Ok(report.response(folder, file, &document_key));
    };
    let runs = match fs::symlink_metadata(project.join(&storage)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(report.response(folder, file, &document_key));
        }
        Err(error) => return Err(format!("Could not inspect the Study run storage: {error}")),
        Ok(_) => checked_descendant(project, Path::new(&storage), true)?,
    };

    let mut eligible = Vec::new();
    let entries = fs::read_dir(&runs)
        .map_err(|error| format!("Could not list the Study run storage: {error}"))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Could not enumerate the Study run storage: {error}"))?;
        let id = entry.file_name();
        let kind = entry
            .file_type()
            .map_err(|error| format!("Could not inspect run {}: {error}", id.to_string_lossy()))?;
        if kind.is_file() {
            continue;
        }
        let id = id.to_string_lossy().into_owned();
        match checked_record(&runs, project, folder, &document_key, &id) {
            Ok(Some(record)) if terminal(&record) => eligible.push(id),
            Ok(Some(_)) => report.blocking_run_ids.push(id),
            Ok(None) => {}
            Err(reason) => report
                .skipped_unverified
                .push(json!({"runId":id,"reason":reason})),
        }
    }
    report
        .skipped_unverified
        .sort_by(|left, right| left["runId"].as_str().cmp(&right["runId"].as_str()));
    if !report.blocking_run_ids.is_empty() {
        report.blocking_run_ids.sort();
        report.blocked = Some(
            "The opened document has a nonterminal Study. Finish or reconcile it before clearing results.",
        );
        return Ok(report.response(folder, file, &document_key));
    }

    eligible.sort();
    for id in eligible {
        let outcome = checked_record(&runs, project, folder, &document_key, &id).and_then(|record| {
            if !record.as_ref().is_some_and(terminal) {
                return Err("The checked Study ownership or terminal state changed while clearing results.".into());
            }
            remove_run(&runs, &id, &mut report.removed_artifacts)
        });
        match outcome {
            Ok(()) => report.removed_runs += 1,
            Err(error) => report.failures.push(json!({"runId":id,"error":error})),
        }
    }
    Ok(report.response(folder, file, &document_key))
}

fn checked_run_storage(document: &Path) -> Result<String> {
    verify_document_schema(document, &read_json(document)?)?;
    let schema = crate::model_schema::command_schema()?;
    let storage = env::var("FULLMETA_STORAGE_RUNS_DIRECTORY")
        .map_err(|_| "Clear Results has no declared run storage.")?;
    if !checked_run_id(&storage)
        || schema
            .pointer("/x-fullmeta-runtime/storage/runsDirectory")
            .and_then(Value::as_str)
            != Some(storage.as_str())
    {
        return Err("Clear Results identifies invalid run storage.".into());
    }
    Ok(storage)
}

fn terminal(record: &Value) -> bool {
    matches!(
        record["state"].as_str(),
        Some("completed" | "failed" | "cancelled" | "interrupted")
    )
}

fn checked_record(
    runs: &Path,
    project: &Path,
    folder: &str,
    document: &str,
    id: &str,
) -> Result<Option<Value>> {
    if !checked_run_id(id) {
        return Err("The directory name is not a valid run ID.".into());
    }
    let manifest = checked_descendant(runs, &Path::new(id).join("run.json"), false)?;
    let record = read_json(&manifest)?;
    if record["schema"].as_str() != Some("fullmeta-run/1") || record["runId"].as_str() != Some(id) {
        return Err("The manifest does not identify this run directory.".into());
    }
    if record["state"].as_str() == Some("mesh-only") || record["kind"].as_str() == Some("mesh") {
        return Ok(None);
    }
    if record["kind"].as_str() != Some("study")
        || !record
            .pointer("/study/id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.is_empty())
    {
        return Err("The manifest does not identify a Study run.".into());
    }
    let owner = record
        .pointer("/model/file")
        .and_then(Value::as_str)
        .ok_or("The Study manifest does not identify its model document.")?;
    if owner == document {
        return Ok(Some(record));
    }
    let sibling = owner
        .rsplit('/')
        .next()
        .filter(|name| model_document_filename_is_valid(name))
        .ok_or("The Study manifest has an invalid model document path.")?;
    if workspace_document_path(folder, sibling) != owner
        || checked_descendant(project, Path::new(sibling), false).is_err()
    {
        return Err(
            "The Study manifest does not identify a checked model document in this project.".into(),
        );
    }
    Ok(None)
}

/// Preflight the full subtree before touching a run. A nested link makes the
/// run unsafe to clear, even if the owner manifest itself was checked.
fn removal_plan(runs: &Path, run: &Path, manifest: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut pending = vec![run.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let checked = checked_path(runs, &directory, true)?;
        let entries = fs::read_dir(&checked)
            .map_err(|error| format!("Could not list {}: {error}", checked.display()))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| format!("Could not enumerate {}: {error}", checked.display()))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?;
            if is_link(&metadata) {
                return Err(format!(
                    "The run contains a symbolic link or reparse point at {}.",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                pending.push(path.clone());
                directories.push(path);
            } else if metadata.is_file() {
                if path != manifest {
                    files.push(path);
                }
            } else {
                return Err(format!("Unsupported run artifact at {}.", path.display()));
            }
        }
    }
    Ok((files, directories))
}

fn checked_path(runs: &Path, path: &Path, directory: bool) -> Result<PathBuf> {
    let relative = path
        .strip_prefix(runs)
        .map_err(|_| "A run artifact escaped the checked run storage.")?;
    checked_descendant(runs, relative, directory)
}

fn remove_run(runs: &Path, id: &str, removed_artifacts: &mut usize) -> Result<()> {
    let run = checked_descendant(runs, Path::new(id), true)?;
    let manifest = checked_path(runs, &run.join("run.json"), false)?;
    let original_manifest = fs::read(&manifest)
        .map_err(|error| format!("Could not read {}: {error}", manifest.display()))?;
    let (files, directories) = removal_plan(runs, &run, &manifest)?;
    for file in files {
        let file = checked_path(runs, &file, false)?;
        fs::remove_file(&file)
            .map_err(|error| format!("Could not remove {}: {error}", file.display()))?;
        *removed_artifacts += 1;
    }
    for directory in directories.into_iter().rev() {
        let directory = checked_path(runs, &directory, true)?;
        fs::remove_dir(&directory)
            .map_err(|error| format!("Could not remove {}: {error}", directory.display()))?;
    }
    let manifest = checked_path(runs, &manifest, false)?;
    fs::remove_file(&manifest)
        .map_err(|error| format!("Could not remove {}: {error}", manifest.display()))?;
    if let Err(error) = fs::remove_dir(&run) {
        // Preserve the ownership record for retry if directory removal fails.
        let restored = checked_path(runs, &run, true).and_then(|_| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&manifest)
                .map_err(|restore| restore.to_string())?;
            file.write_all(&original_manifest)
                .and_then(|()| file.sync_all())
                .map_err(|restore| restore.to_string())
        });
        if let Err(restore) = restored {
            *removed_artifacts += 1;
            return Err(format!(
                "Could not remove {}: {error}. Could not restore its manifest: {restore}.",
                run.display()
            ));
        }
        return Err(format!("Could not remove {}: {error}.", run.display()));
    }
    *removed_artifacts += 1;
    Ok(())
}

#[derive(Default)]
struct ClearReport {
    removed_runs: usize,
    removed_artifacts: usize,
    skipped_unverified: Vec<Value>,
    failures: Vec<Value>,
    blocked: Option<&'static str>,
    blocking_run_ids: Vec<String>,
}

impl ClearReport {
    fn response(self, folder: &str, file: &str, document: &str) -> Value {
        json!({"folder":folder,"file":file,"document":document,"removedRuns":self.removed_runs,
            "removedArtifacts":self.removed_artifacts,"skippedUnverified":self.skipped_unverified,
            "failures":self.failures,"blocked":self.blocked,"blockingRunIds":self.blocking_run_ids})
    }
}

struct ProjectRunLock {
    path: PathBuf,
    file: Option<File>,
}

impl ProjectRunLock {
    fn acquire(project: &Path) -> Result<Option<Self>> {
        let path = project.join(PROJECT_RUN_LOCK);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = match options.open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "Could not acquire the project run lock {}: {error}",
                    path.display()
                ));
            }
        };
        let mut lock = Self {
            path,
            file: Some(file),
        };
        let file = lock.file.as_mut().expect("newly acquired project run lock");
        writeln!(
            file,
            "pid={} started={}",
            std::process::id(),
            utc_timestamp()
        )
        .and_then(|()| file.sync_all())
        .map_err(|error| format!("Could not write the project run lock: {error}"))?;
        Ok(Some(lock))
    }
}

impl Drop for ProjectRunLock {
    fn drop(&mut self) {
        self.file.take();
        let _ = fs::remove_file(&self.path);
    }
}
