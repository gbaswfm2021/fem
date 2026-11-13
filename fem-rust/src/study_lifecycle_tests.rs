use super::*;
use std::sync::Barrier;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

const DOCUMENT: &str = "disposable/fem_model.json";
const STUDY: &str = "testStudy";
const RUN: &str = "test-run";
const OPERATION: &str = "test-operation";
const RUNS: &str = "fem_runs";

struct Fixture(PathBuf);

impl Fixture {
    fn create(record: &Value) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let executable = env::current_exe().unwrap();
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = executable
            .parent()
            .unwrap()
            .join(format!("study-lifecycle-{}-{serial}", std::process::id()));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        fs::create_dir(fixture.0.join(RUNS)).unwrap();
        fs::create_dir(fixture.run_directory()).unwrap();
        fs::write(fixture.record(), serde_json::to_vec(record).unwrap()).unwrap();
        fixture
    }

    fn run_directory(&self) -> PathBuf {
        self.0.join(RUNS).join(RUN)
    }
    fn record(&self) -> PathBuf {
        self.run_directory().join("run.json")
    }
    fn project_lock(&self) -> PathBuf {
        self.0.join(".fullmeta-run.lock")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for name in ["run.json", ".run-record.lock"] {
            let _ = fs::remove_file(self.run_directory().join(name));
        }
        let _ = fs::remove_file(self.project_lock());
        let _ = fs::remove_dir(self.run_directory());
        let _ = fs::remove_dir(self.0.join(RUNS));
        let _ = fs::remove_dir(&self.0);
    }
}

fn fixture_record(pid: u32, marker: u64) -> Value {
    json!({"schema":"fullmeta-run/1","kind":"study","runId":RUN,"operationId":OPERATION,
        "state":"running","startedAt":"2026-01-01T00:00:00.000Z","completedAt":null,
        "model":{"file":DOCUMENT},"study":{"id":STUDY},"worker":{"pid":pid,"started":marker},
        "progress":{"stage":"solve","completedPoints":0},"artifacts":{},"error":null})
}

fn pinned_query() -> RunStatusQuery<'static> {
    RunStatusQuery {
        document: DOCUMENT,
        run_id: Some(RUN),
        study_id: Some(STUDY),
        operation_id: Some(OPERATION),
        terminal_only: false,
    }
}

#[test]
fn native_completion_verification_rejects_only_completion_and_preserves_cancelled_winners() {
    for state in ["completed", "cancelled", "failed", "interrupted"] {
        let mut record = fixture_record(1, 1);
        record["state"] = json!(state);
        record["error"] = json!("Original terminal diagnostic");
        let fixture = Fixture::create(&record);
        let changed = run_record_guard::reject_completed_verification(
            &fixture.record(),
            "Required artifact is missing.",
        )
        .unwrap();
        let after = read_json(&fixture.record()).unwrap();
        assert_eq!(changed, state == "completed");
        if state == "completed" {
            assert_eq!(after["state"], "failed");
            assert_eq!(after["error"], "Required artifact is missing.");
        } else {
            assert_eq!(after, record);
        }
    }
}

#[test]
fn saved_snapshot_equality_ignores_keys_and_numeric_spelling_but_preserves_order() {
    let saved: Value = serde_json::from_str(r#"{"list":[1.0,2],"type":"study"}"#).unwrap();
    let submitted: Value = serde_json::from_str(r#"{"type":"study","list":[1,2]}"#).unwrap();
    assert!(same_json_value(&saved, &submitted));
    assert!(!same_json_value(
        &saved,
        &json!({"list":[2,1],"type":"study"})
    ));
    assert!(!same_json_value(
        &saved,
        &json!({"list":[1,2],"type":"study","missing":null})
    ));
    let integer: Value = serde_json::from_str("9007199254740993").unwrap();
    let rounded: Value = serde_json::from_str("9007199254740992.0").unwrap();
    assert!(!same_json_value(&integer, &rounded));
}

#[test]
fn terminal_status_filters_before_ranking_and_uses_start_and_id() {
    let query = RunStatusQuery {
        document: DOCUMENT,
        run_id: None,
        study_id: Some(STUDY),
        operation_id: None,
        terminal_only: true,
    };
    let mut current = fixture_record(1, 1);
    assert!(!query.matches(RUN, &current));
    current["state"] = json!("completed");
    assert!(query.matches(RUN, &current));
    let mut newer = current.clone();
    newer["startedAt"] = json!("2026-01-01T00:00:00.1Z");
    current["startedAt"] = json!("2026-01-01T00:00:00Z");
    assert!(status_rank(&newer) > status_rank(&current));
    newer["study"]["id"] = json!("anotherStudy");
    assert!(!query.matches(RUN, &newer));
    newer["study"]["id"] = json!(STUDY);
    newer["model"]["file"] = json!("disposable/sibling.json");
    assert!(!query.matches(RUN, &newer));
    newer["model"]["file"] = json!(DOCUMENT);
    newer["startedAt"] = current["startedAt"].clone();
    newer["runId"] = json!("test-run-z");
    assert!(query.matches("test-run-z", &newer));
    assert!(status_rank(&newer) > status_rank(&current));
}

#[test]
#[ignore = "benign worker invoked by the controlled identity test"]
fn benign_worker() {
    if env::var_os("FEM3_LIFECYCLE_TEST_WORKER").is_some() {
        thread::sleep(Duration::from_secs(30));
    }
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn observation_preserves_files_and_pid_reuse_only_transitions_on_explicit_reconciliation() {
    struct Worker(std::process::Child);
    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut command = Command::new(env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "study_lifecycle_tests::benign_worker",
            "--ignored",
        ])
        .env("FEM3_LIFECYCLE_TEST_WORKER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let worker = Worker(command.spawn().unwrap());
    let pid = worker.0.id();
    let marker = process_identity(pid).unwrap().unwrap();
    let mut record = fixture_record(pid, marker);
    assert_eq!(run_liveness(&record), "active");
    record["worker"]["started"] = Value::Null;
    assert_eq!(run_liveness(&record), "unknown");
    record["worker"]["started"] = json!(marker.checked_add(1).unwrap());
    let fixture = Fixture::create(&record);
    fs::write(fixture.project_lock(), format!("pid={pid}\nrunId={RUN}\n")).unwrap();
    let original = fs::read(fixture.record()).unwrap();
    let lock = fs::read(fixture.project_lock()).unwrap();
    let observed = status_reply(&fixture.0, RUNS, &pinned_query(), false).unwrap();
    assert_eq!(observed["state"], "running");
    assert_eq!(observed["liveness"], "gone");
    assert_eq!(observed["operationId"], OPERATION);
    assert_eq!(fs::read(fixture.record()).unwrap(), original);
    assert_eq!(fs::read(fixture.project_lock()).unwrap(), lock);
    assert!(!fixture.run_directory().join(".run-record.lock").exists());
    let finished = status_reply(&fixture.0, RUNS, &pinned_query(), true).unwrap();
    assert_eq!(finished["state"], "interrupted");
    assert!(finished["completedAt"].is_string());
    let again = status_reply(&fixture.0, RUNS, &pinned_query(), true).unwrap();
    assert_eq!(again, finished);
    assert_eq!(process_identity(pid), Ok(Some(marker)));
    println!(
        "Checked run {RUN}, operation {OPERATION}, reused PID probe and explicit interruption."
    );
}

#[test]
#[cfg(any(windows, target_os = "linux"))]
fn reconciliation_rereads_terminal_winner_and_serializes_duplicate_transitions() {
    let pid = std::process::id();
    let marker = process_identity(pid).unwrap().unwrap();
    let running = fixture_record(pid, marker.checked_add(1).unwrap());
    let fixture = Fixture::create(&running);
    let path = fixture.record();
    let observed = Barrier::new(2);
    let published = Barrier::new(2);
    let mut completed = running.clone();
    completed["state"] = json!("completed");
    completed["completedAt"] = json!("2026-01-01T00:02:00.000Z");
    completed["artifacts"]["csv"] = json!("output/checked.csv");
    thread::scope(|scope| {
        let attempt = scope.spawn(|| {
            assert_eq!(run_liveness(&read_json(&path).unwrap()), "gone");
            observed.wait();
            published.wait();
            reconcile_gone_run(&path, &pinned_query()).unwrap()
        });
        observed.wait();
        run_record_guard::write_scenario_record(&path, &completed).unwrap();
        published.wait();
        let (record, changed) = attempt.join().unwrap();
        assert!(!changed);
        assert_eq!(record, completed);
    });
    assert_eq!(read_json(&path).unwrap(), completed);

    let fixture = Fixture::create(&running);
    let path = fixture.record();
    let ready = Barrier::new(2);
    let attempt = || {
        assert_eq!(run_liveness(&read_json(&path).unwrap()), "gone");
        ready.wait();
        reconcile_gone_run(&path, &pinned_query()).unwrap()
    };
    thread::scope(|scope| {
        let left = scope.spawn(&attempt);
        let right = scope.spawn(&attempt);
        let (first, first_changed) = left.join().unwrap();
        let (second, second_changed) = right.join().unwrap();
        assert_ne!(first_changed, second_changed);
        assert_eq!(first, second);
        assert_eq!(first["state"], "interrupted");
        assert!(first["completedAt"].is_string());
        assert_eq!(read_json(&path).unwrap(), first);
    });
    println!(
        "Checked run {RUN}, operation {OPERATION}, completed winner and one persisted interruption."
    );
}
