//! Serialize Study manifest updates across the dispatcher and scenario workers.

use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

type Result<T> = std::result::Result<T, String>;

pub(crate) fn is_terminal(state: Option<&str>) -> bool {
    matches!(
        state,
        Some("completed" | "cancelled" | "failed" | "interrupted")
    )
}

fn regular_file(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return false;
        }
    }
    metadata.is_file() && !metadata.file_type().is_symlink()
}

fn lock_record(record: &Path) -> Result<File> {
    let path = record.with_file_name(".run-record.lock");
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !regular_file(&metadata) => {
            return Err("The Study record lock is not a regular file.".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect the Study record lock: {error}")),
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(OPEN_REPARSE_POINT);
    }
    let lock = options
        .open(&path)
        .map_err(|error| format!("Could not open the Study record lock: {error}"))?;
    if !regular_file(&lock.metadata().map_err(|error| error.to_string())?) {
        return Err("The Study record lock is not a regular file.".into());
    }
    lock.lock()
        .map_err(|error| format!("Could not acquire the Study record lock: {error}"))?;
    Ok(lock)
}

fn read_record(path: &Path) -> Result<Value> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect the Study record: {error}"))?;
    if !regular_file(&metadata) {
        return Err("The Study record is not a regular file.".into());
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read the Study record: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("Invalid Study record: {error}"))
}

fn replace_record(path: &Path, record: &Value) -> Result<()> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".run-record-{}-{serial}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Could not create the Study record update: {error}"))?;
    let result = (|| -> Result<()> {
        serde_json::to_writer_pretty(&mut file, record).map_err(|error| error.to_string())?;
        file.write_all(b"\n")
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Could not write the Study record update: {error}"))?;
        Ok(())
    })();
    drop(file);
    let result = result.and_then(|()| {
        fs::rename(&temporary, path)
            .map_err(|error| format!("Could not publish the Study record update: {error}"))
    });
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

/// Read again under the writer lock. A transition from a previously observed
/// running record may only commit while the checked record is still running.
#[allow(dead_code)]
pub(crate) fn update_running(
    path: &Path,
    update: impl FnOnce(&mut Value) -> Result<bool>,
) -> Result<(Value, bool)> {
    let _lock = lock_record(path)?;
    let mut record = read_record(path)?;
    let changed = record["state"].as_str() == Some("running") && update(&mut record)?;
    if changed {
        replace_record(path, &record)?;
    }
    Ok((record, changed))
}

/// The contract-owning dispatcher verifies completion after its child exits.
/// A rejected completion is failed; cancellation and every earlier failure win.
pub(crate) fn reject_completed_verification(path: &Path, message: &str) -> Result<bool> {
    let _lock = lock_record(path)?;
    let mut record = read_record(path)?;
    if record["state"].as_str() != Some("completed") {
        return Ok(false);
    }
    record["state"] = serde_json::json!("failed");
    record["error"] = serde_json::json!(message);
    replace_record(path, &record)?;
    Ok(true)
}

/// A scenario worker may advance a running manifest or promote its selected
/// mesh-only run. A terminal manifest published by any writer wins the race.
#[allow(dead_code)]
pub(crate) fn write_scenario_record(path: &Path, next: &Value) -> Result<()> {
    let _lock = lock_record(path)?;
    let current = read_record(path)?;
    let state = current["state"].as_str();
    if state != Some("running")
        && !(state == Some("mesh-only") && next["state"].as_str() == Some("running"))
    {
        return Err(format!(
            "The Study record already has state {}.",
            state.unwrap_or("(missing)")
        ));
    }
    if current["runId"] != next["runId"] {
        return Err("The Study record update identifies a different run.".into());
    }
    replace_record(path, next)
}
