mod common;

#[cfg(feature = "test-adapters")]
use std::fs;
#[cfg(feature = "test-adapters")]
use std::process::Command as StdCommand;
#[cfg(feature = "test-adapters")]
use std::sync::{Arc, Barrier};
#[cfg(feature = "test-adapters")]
use std::thread;
#[cfg(feature = "test-adapters")]
use std::time::{Duration, Instant};

#[cfg(feature = "test-adapters")]
use assert_cmd::Command;
#[cfg(feature = "test-adapters")]
use quinte::model::{Policy, RunStatus, SandboxMode, TEXT_MODEL};
#[cfg(feature = "test-adapters")]
use quinte::run;
#[cfg(feature = "test-adapters")]
use quinte::store::Store;
#[cfg(feature = "test-adapters")]
use quinte::util::{read_json, sha256_file, write_json};
use serde_json::{Value, json};

const SCHEMA: &str = include_str!("../schemas/host-invocation.schema.json");
const RUN_ID: &str = "019fd896-7769-7c62-a3c3-e4f34fbc09f2";
const INVOCATION_ID: &str = "019fd896-7769-7c62-a3c3-e4f34fbc09f3";

fn digest(character: char) -> String {
    format!("sha256:{}", character.to_string().repeat(64))
}

fn validator() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    jsonschema::options().build(&schema).unwrap()
}

fn base(operation: &str, code: &str) -> Value {
    json!({
        "host_receipt_version": "1.0",
        "invocation_id": INVOCATION_ID,
        "receipt_path": "/tmp/quinte-home/host/receipts/019fd896-7769-7c62-a3c3-e4f34fbc09f3.json",
        "operation": operation,
        "observed_at": "2026-08-07T00:00:00.000Z",
        "state_root": "/tmp/quinte-home",
        "state": {
            "code": code,
            "active_run_ids": []
        }
    })
}

fn manifest(status: &str) -> Value {
    json!({
        "status": status,
        "manifest_version": "2.0",
        "brief_sha256": digest('a'),
        "policy_sha256": digest('b'),
        "snapshot_sha256": digest('c'),
        "runtime_sha256": digest('d'),
        "error": null,
        "result_sha256": null
    })
}

fn assert_valid(value: &Value) {
    let validator = validator();
    let errors = validator
        .iter_errors(value)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "receipt was invalid: {errors:?}\n{value:#}");
}

#[cfg(feature = "test-adapters")]
struct HostFixture {
    _temporary: tempfile::TempDir,
    home: std::path::PathBuf,
    brief: std::path::PathBuf,
    executable: std::path::PathBuf,
}

#[cfg(feature = "test-adapters")]
fn fake_policy(executable: &std::path::Path) -> Policy {
    let seat = quinte::model::SeatBinding::default();
    let route = |party_id: &str, route_id: &str| quinte::model::RoutePolicy {
        party_id: party_id.into(),
        route_id: route_id.into(),
        adapter: "fake".into(),
        executable: executable.display().to_string(),
        required: true,
        family: seat.family.clone(),
        provider: seat.provider.clone(),
        text_model: seat.text_model.clone(),
        multimodal_model: seat.multimodal_model.clone(),
        perspective: String::new(),
    };
    Policy {
        legacy_v1_source: false,
        policy_version: "2.0".into(),
        seat: seat.clone(),
        roster: ["A", "B", "C", "D", "E"]
            .into_iter()
            .map(|party| {
                route(
                    &format!("Party {party}"),
                    &format!("fake-{}", party.to_ascii_lowercase()),
                )
            })
            .collect(),
        counterpart_arbiter: route("Counterpart Arbiter", "fake-counterpart"),
        primary_arbiter: route("Primary Arbiter", "fake-primary"),
        auto_primary_arbiter: true,
        text_model: TEXT_MODEL.into(),
        multimodal_model: "mimo-v2.5".into(),
        max_parallel_r1: 5,
        max_parallel_r2: 1,
        r2_parallel: false,
        max_attempts: 1,
        timeout_seconds: 30,
        retry_backoff_seconds: 0,
        retry_backoff_max_seconds: 0,
        r2_min_interval_seconds: 0,
        max_output_bytes: 1_048_576,
        max_snapshot_files: 100,
        max_snapshot_bytes: 1_048_576,
        max_attachment_bytes: 1_048_576,
        sandbox_mode: SandboxMode::Process,
    }
}

#[cfg(feature = "test-adapters")]
fn host_fixture(controlled_worker: bool) -> HostFixture {
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    if controlled_worker {
        fs::write(temporary.path().join("fake-agent-controlled"), b"controlled\n").unwrap();
    }
    let home = temporary.path().join("home");
    fs::create_dir_all(&home).unwrap();
    write_json(&home.join("policy.json"), &fake_policy(&executable)).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, b"bounded evidence\n").unwrap();
    let brief = temporary.path().join("brief.json");
    write_json(
        &brief,
        &json!({
            "brief_version": "1.1",
            "question": "Can the host preserve launch identity?",
            "evidence_roots": [evidence],
            "attachments": [],
            "action_scope": "test only"
        }),
    )
    .unwrap();
    HostFixture {
        _temporary: temporary,
        home,
        brief,
        executable,
    }
}

#[cfg(feature = "test-adapters")]
fn host_command(fixture: &HostFixture) -> Command {
    let mut command = Command::cargo_bin("quinte").unwrap();
    command
        .env("QUINTE_HOME", &fixture.home)
        .env("QUINTE_ALLOW_FAKE_ADAPTERS", "1");
    command
}

#[cfg(feature = "test-adapters")]
fn host_command_std(fixture: &HostFixture) -> StdCommand {
    let mut command = StdCommand::new(assert_cmd::cargo::cargo_bin!("quinte"));
    command
        .env("QUINTE_HOME", &fixture.home)
        .env("QUINTE_ALLOW_FAKE_ADAPTERS", "1");
    command
}

#[cfg(feature = "test-adapters")]
fn envelope_data(output: &[u8]) -> Value {
    let envelope: Value = serde_json::from_slice(output).unwrap();
    assert_eq!(envelope["ok"], true);
    envelope["data"].clone()
}

#[cfg(feature = "test-adapters")]
fn wait_for_path(path: &std::path::Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(feature = "test-adapters")]
fn start_controlled(fixture: &HostFixture) -> Value {
    let output = host_command(fixture)
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .args(["--json"])
        .timeout(Duration::from_secs(30))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "host start failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = envelope_data(&output.stdout);
    wait_for_path(
        &fixture
            .executable
            .parent()
            .unwrap()
            .join("fake-agent-started"),
        Duration::from_secs(30),
    );
    receipt
}

#[cfg(feature = "test-adapters")]
fn release_and_wait(fixture: &HostFixture, run_id: &str) {
    fs::write(
        fixture
            .executable
            .parent()
            .unwrap()
            .join("fake-agent-release"),
        b"release\n",
    )
    .unwrap();
    let output = host_command(fixture)
        .args(["wait", run_id, "--json"])
        .timeout(Duration::from_secs(60))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "detached worker did not finish: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(feature = "test-adapters")]
struct ControlledRunCleanup {
    home: std::path::PathBuf,
    run_id: String,
    release: std::path::PathBuf,
    disarmed: bool,
}

#[cfg(feature = "test-adapters")]
impl ControlledRunCleanup {
    fn new(fixture: &HostFixture, run_id: &str) -> Self {
        Self {
            home: fixture.home.clone(),
            run_id: run_id.to_string(),
            release: fixture
                .executable
                .parent()
                .unwrap()
                .join("fake-agent-release"),
            disarmed: false,
        }
    }

    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

#[cfg(feature = "test-adapters")]
impl Drop for ControlledRunCleanup {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        let _ = quinte::run::cancel(&Store::new(self.home.clone()), &self.run_id);
        let _ = fs::write(&self.release, b"release\n");
    }
}

#[cfg(feature = "test-adapters")]
fn wait_for_terminal(store: &Store, run_id: &str, timeout: Duration) -> quinte::model::RunManifest {
    let deadline = Instant::now() + timeout;
    loop {
        let manifest = store.load_manifest(run_id).unwrap();
        if manifest.status.terminal() {
            return manifest;
        }
        assert!(
            Instant::now() < deadline,
            "run {run_id} did not reach a terminal state; current={:?}",
            manifest.status
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn host_receipt_schema_has_stable_identity_and_closed_top_level() {
    let schema: Value = serde_json::from_str(SCHEMA).unwrap();
    assert_eq!(
        schema["$id"],
        "https://github.com/eric-stone-plus/QUINTE/contracts/host-receipt/1.0/schema.json"
    );
    assert_eq!(schema["properties"]["host_receipt_version"]["const"], "1.0");
    assert!(
        schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "receipt_path")
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["properties"]["invocation_id"]["$ref"],
        "#/$defs/uuidv7"
    );
    assert_eq!(
        schema["properties"]["run_id"]["$ref"],
        "#/$defs/uuidv7"
    );
}

#[test]
fn host_receipt_schema_rejects_noncanonical_or_non_v7_ids() {
    let mut receipt = base("preflight", "ready");
    receipt["preflight"] = json!({
        "doctor_version": "1.0",
        "ok": true,
        "platform": "linux",
        "checks": []
    });
    assert_valid(&receipt);

    for invocation_id in [
        "019fd896-7769-1c62-a3c3-e4f34fbc09f3", // UUIDv1
        "019FD896-7769-7C62-A3C3-E4F34FBC09F3", // uppercase
        "019fd89677697c62a3c3e4f34fbc09f3",     // compact spelling
    ] {
        let mut invalid = receipt.clone();
        invalid["invocation_id"] = json!(invocation_id);
        assert!(
            !validator().is_valid(&invalid),
            "accepted invalid invocation id {invocation_id}"
        );
    }

    let mut invalid_run = base("start", "started");
    invalid_run["run_id"] = json!("019fd896-7769-1c62-a3c3-e4f34fbc09f2");
    invalid_run["state"]["active_run_ids"] = json!([]);
    invalid_run["brief"] = json!({
        "source": "/tmp/brief.json",
        "source_sha256": digest('e'),
        "canonical_sha256": digest('a')
    });
    invalid_run["manifest"] = manifest("queued");
    assert!(!validator().is_valid(&invalid_run));
}

#[test]
fn host_receipt_schema_removed_unreachable_run_not_found_outcome() {
    let mut receipt = base("reconcile", "run_not_found");
    receipt["recovery"] = json!({
        "outcome": "run_not_found",
        "launch_safe": false,
        "receipt_path": "/tmp/quinte-home/host/receipts/invocation.json"
    });
    assert!(!validator().is_valid(&receipt));
}

#[test]
fn fixtures_cover_every_host_operation() {
    let mut preflight = base("preflight", "ready");
    preflight["preflight"] = json!({
        "doctor_version": "1.0",
        "ok": true,
        "platform": "linux",
        "checks": [{"name": "git", "ok": true}]
    });
    assert_valid(&preflight);

    let mut start = base("start", "started");
    start["state"]["active_run_ids"] = json!([RUN_ID]);
    start["run_id"] = json!(RUN_ID);
    start["brief"] = json!({
        "source": "/tmp/brief.json",
        "source_sha256": digest('e'),
        "canonical_sha256": digest('a')
    });
    start["manifest"] = manifest("queued");
    start["manifest"]["worker_pid"] = json!(4242);
    assert_valid(&start);

    let mut status = base("status", "observed");
    status["state"]["active_run_ids"] = json!([RUN_ID]);
    status["state"]["worker"] = json!({
        "state": "running",
        "pid": 4242,
        "heartbeat_at": "2026-08-07T00:00:02.000Z",
        "heartbeat_age_seconds": 1,
        "recovery_needed": false
    });
    status["state"]["attempts"] = json!([{
        "phase": "R1",
        "party_id": "Party A",
        "route_id": "mimo-a",
        "attempt": 2,
        "state": "retry_wait",
        "duration_ms": 300000,
        "timeout_seconds": 300,
        "timed_out": true,
        "retryable": true,
        "failure_class": "timeout",
        "retry_due_at": "2026-08-07T00:00:15.000Z"
    }]);
    status["run_id"] = json!(RUN_ID);
    status["manifest"] = manifest("r1_running");
    assert_valid(&status);

    let mut inspect = base("inspect", "terminal");
    inspect["run_id"] = json!(RUN_ID);
    inspect["manifest"] = manifest("completed");
    inspect["manifest"]["result_sha256"] = json!(digest('f'));
    inspect["result"] = json!({
        "verified": true,
        "actionable": true,
        "contract_version": "2.1",
        "sha256": digest('f'),
        "path": "/tmp/quinte-home/runs/run/result.json"
    });
    assert_valid(&inspect);

    let mut reconcile = base("reconcile", "no_active_run");
    reconcile["recovery"] = json!({
        "outcome": "no_active_run",
        "launch_safe": true,
        "receipt_path": "/tmp/quinte-home/host/receipts/invocation.json"
    });
    assert_valid(&reconcile);
}

#[test]
fn operation_specific_bindings_fail_closed() {
    let start_without_bindings = base("start", "started");
    assert!(!validator().is_valid(&start_without_bindings));

    let mut terminal_without_result = base("inspect", "terminal");
    terminal_without_result["run_id"] = json!(RUN_ID);
    terminal_without_result["manifest"] = manifest("completed");
    terminal_without_result["manifest"]["result_sha256"] = json!(digest('f'));
    assert!(!validator().is_valid(&terminal_without_result));

    let reconcile_without_recovery = base("reconcile", "no_active_run");
    assert!(!validator().is_valid(&reconcile_without_recovery));

    let mut unknown = base("status", "observed");
    unknown["run_id"] = json!(RUN_ID);
    unknown["manifest"] = manifest("r1_running");
    unknown["surprise"] = json!(true);
    assert!(!validator().is_valid(&unknown));
}

#[test]
fn host_spec_preserves_scheduler_and_one_active_boundaries() {
    let spec = include_str!("../specs/HOST.md");
    for required in [
        "quinte host start --brief FILE",
        "quinte host reconcile",
        "One-active is a host resource rule",
        "Enumeration is fail-closed",
        "lane.retry_scheduled",
        "result.actionable=true",
        "QUINTE_HOME",
        "manifest.runtime_sha256",
        "Terminal handoff gate",
        "result.verified=true",
        "not a permanent seat",
        "does not authorize",
    ] {
        assert!(spec.contains(required), "HOST.md omitted {required:?}");
    }
}

#[cfg(feature = "test-adapters")]
#[test]
fn preflight_persists_a_schema_valid_receipt_and_latest_projection() {
    let fixture = host_fixture(false);

    let output = host_command(&fixture)
        .args(["host", "preflight", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "preflight failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = envelope_data(&output.stdout);
    assert_valid(&receipt);
    assert_eq!(receipt["operation"], "preflight");
    assert_eq!(receipt["state"]["code"], "ready");
    assert_eq!(receipt["state"]["active_run_ids"], json!([]));
    let invocation_id = receipt["invocation_id"].as_str().unwrap();
    let durable_path = fixture
        .home
        .join("host/receipts")
        .join(format!("{invocation_id}.json"));
    assert_eq!(receipt["receipt_path"], durable_path.display().to_string());
    let durable: Value = read_json(&durable_path).unwrap();
    let latest: Value = read_json(&fixture.home.join("host/latest.json")).unwrap();
    assert_eq!(durable, receipt);
    assert_eq!(latest, receipt);
}

#[cfg(feature = "test-adapters")]
#[test]
fn latest_projection_failure_does_not_invalidate_the_durable_receipt() {
    let fixture = host_fixture(false);
    fs::create_dir_all(fixture.home.join("host/latest.json")).unwrap();

    let output = host_command(&fixture)
        .args(["host", "preflight", "--json"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "preflight failed after only its latest projection failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("durable host receipt"),
        "projection failure was not diagnosed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = envelope_data(&output.stdout);
    assert_valid(&receipt);
    let durable_path = std::path::PathBuf::from(receipt["receipt_path"].as_str().unwrap());
    assert_eq!(read_json::<Value>(&durable_path).unwrap(), receipt);
    assert!(fixture.home.join("host/latest.json").is_dir());
}

#[cfg(feature = "test-adapters")]
#[test]
fn host_start_returns_detached_and_persists_final_provenance_receipt() {
    let fixture = host_fixture(true);
    let started_at = Instant::now();
    let receipt = start_controlled(&fixture);
    let run_id = receipt["run_id"].as_str().unwrap().to_string();
    let mut cleanup = ControlledRunCleanup::new(&fixture, &run_id);

    assert!(started_at.elapsed() < Duration::from_secs(30));
    assert_valid(&receipt);
    assert_eq!(receipt["operation"], "start");
    assert_eq!(receipt["state"]["code"], "started");
    assert_eq!(receipt["state"]["active_run_ids"], json!([run_id]));
    assert!(receipt["manifest"]["worker_pid"].as_u64().unwrap() > 0);
    assert_eq!(
        receipt["brief"]["source_sha256"],
        sha256_file(&fixture.brief).unwrap()
    );
    let store = Store::new(fixture.home.clone());
    let manifest = store.load_manifest(&run_id).unwrap();
    assert_eq!(receipt["brief"]["canonical_sha256"], manifest.brief_sha256);
    assert_eq!(receipt["manifest"]["brief_sha256"], manifest.brief_sha256);
    let invocation_id = receipt["invocation_id"].as_str().unwrap();
    let durable_path = fixture
        .home
        .join("host/receipts")
        .join(format!("{invocation_id}.json"));
    assert_eq!(receipt["receipt_path"], durable_path.display().to_string());
    let durable: Value = read_json(&durable_path).unwrap();
    assert_eq!(durable, receipt, "the durable receipt must contain worker_pid");

    let status = host_command(&fixture)
        .args(["host", "status", &run_id, "--json"])
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "host status failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let status = envelope_data(&status.stdout);
    assert_eq!(status["state"]["worker"]["state"], "running");
    assert_eq!(status["state"]["worker"]["recovery_needed"], false);
    assert!(status["state"]["worker"]["pid"].as_u64().unwrap() > 0);
    let attempts = status["state"]["attempts"].as_array().unwrap();
    assert!(attempts.iter().any(|attempt| {
        attempt["phase"] == "R1"
            && attempt["state"] == "running"
            && attempt["timeout_seconds"] == 30
    }));

    release_and_wait(&fixture, &run_id);
    cleanup.disarm();
}

#[cfg(feature = "test-adapters")]
#[test]
fn worker_launch_failure_is_terminal_durable_and_reconcilable() {
    let fixture = host_fixture(false);
    let output = host_command(&fixture)
        .env("QUINTE_TEST_FAIL_WORKER_LAUNCH", "1")
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .arg("--json")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fault-injected worker launch failure"), "{stderr}");
    assert!(stderr.contains("reconcile before another launch"), "{stderr}");

    let store = Store::new(fixture.home.clone());
    let manifests = store.list_manifests().unwrap();
    assert_eq!(manifests.len(), 1);
    assert_eq!(manifests[0].status, RunStatus::Failed);
    let receipt_paths = fs::read_dir(fixture.home.join("host/receipts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(receipt_paths.len(), 1);
    let receipt: Value = read_json(&receipt_paths[0]).unwrap();
    assert_valid(&receipt);
    assert_eq!(receipt["operation"], "start");
    assert_eq!(receipt["state"]["code"], "launch_failed");
    assert_eq!(receipt["state"]["active_run_ids"], json!([]));
    assert_eq!(receipt["manifest"]["status"], "failed");
    assert_eq!(receipt["manifest"]["error"]["code"], "worker_failed");
    assert!(
        receipt["state"]["blockers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str().unwrap().contains("worker launch failed"))
    );

    let reconciled = host_command(&fixture)
        .args(["host", "reconcile", "--json"])
        .output()
        .unwrap();
    assert!(reconciled.status.success());
    let reconciled = envelope_data(&reconciled.stdout);
    assert_eq!(reconciled["run_id"], manifests[0].run_id);
    assert_eq!(reconciled["manifest"]["status"], "failed");
}

#[cfg(feature = "test-adapters")]
#[test]
fn worker_launch_and_terminal_record_failures_are_both_durable() {
    let fixture = host_fixture(false);
    let output = host_command(&fixture)
        .env("QUINTE_TEST_FAIL_WORKER_LAUNCH", "1")
        .env("QUINTE_TEST_FAIL_WORKER_FAILURE_RECORD", "1")
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .arg("--json")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fault-injected worker launch failure"), "{stderr}");
    assert!(stderr.contains("terminal state write also failed"), "{stderr}");

    let store = Store::new(fixture.home.clone());
    let manifests = store.list_manifests().unwrap();
    assert_eq!(manifests.len(), 1);
    let run_id = manifests[0].run_id.clone();
    assert_eq!(manifests[0].status, RunStatus::Queued);
    let receipt_path = fs::read_dir(fixture.home.join("host/receipts"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let receipt: Value = read_json(&receipt_path).unwrap();
    assert_valid(&receipt);
    assert_eq!(receipt["state"]["code"], "launch_failed");
    assert_eq!(receipt["state"]["active_run_ids"], json!([run_id]));
    assert_eq!(receipt["manifest"]["status"], "queued");
    let blockers = receipt["state"]["blockers"].as_array().unwrap();
    assert_eq!(blockers.len(), 2);
    assert!(
        blockers[1]
            .as_str()
            .unwrap()
            .contains("failed to record terminal worker state")
    );

    let reconciled = host_command(&fixture)
        .args(["host", "reconcile", "--json"])
        .output()
        .unwrap();
    assert!(reconciled.status.success());
    let reconciled = envelope_data(&reconciled.stdout);
    assert_eq!(reconciled["state"]["code"], "reconciled");
    assert_eq!(reconciled["run_id"], run_id);
    assert_eq!(reconciled["manifest"]["status"], "queued");

    run::cancel(&store, &run_id).unwrap();
}

#[cfg(feature = "test-adapters")]
#[test]
fn reconcile_unknown_run_fails_closed_without_emitting_run_not_found() {
    let fixture = host_fixture(false);
    let unknown = uuid::Uuid::now_v7().to_string();
    let output = host_command(&fixture)
        .args(["host", "reconcile", &unknown, "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown or invalid run") || stderr.contains("cannot reconcile run"),
        "unexpected unknown-run error: {stderr}"
    );
    assert!(
        output.stdout.is_empty(),
        "unknown run must not emit a run_not_found receipt"
    );
    assert!(!fixture.home.join("host/latest.json").exists());
}

#[cfg(feature = "test-adapters")]
#[test]
fn host_status_projects_persisted_retry_deadline_after_scheduler_crash_window() {
    let fixture = host_fixture(false);
    let store = Store::new(fixture.home.clone());
    let policy = fake_policy(&fixture.executable);
    // Use the host path itself to create a queued run, then inject a launch
    // failure whose terminal-state write is also unavailable. This leaves a
    // durable non-terminal manifest without mutating process-global adapter
    // policy in this parallel test suite.
    let start = host_command(&fixture)
        .env("QUINTE_TEST_FAIL_WORKER_LAUNCH", "1")
        .env("QUINTE_TEST_FAIL_WORKER_FAILURE_RECORD", "1")
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .arg("--json")
        .output()
        .unwrap();
    assert!(!start.status.success());
    let run_id = store.list_manifests().unwrap().pop().unwrap().run_id;
    let route = &policy.roster[0];
    let due_at = (chrono::Utc::now() + chrono::Duration::seconds(90)).to_rfc3339();
    let retry_dir = store
        .run_dir(&run_id)
        .unwrap()
        .join("lanes/R1")
        .join(&route.route_id);
    fs::create_dir_all(&retry_dir).unwrap();
    write_json(
        &retry_dir.join("retry-deadline.json"),
        &json!({
            "retry_state_version": "1.0",
            "phase": "R1",
            "route_id": route.route_id,
            "previous_attempt": 1,
            "next_attempt": 2,
            "due_at": due_at,
            "failure_class": "timeout",
            "source": "host_timeout"
        }),
    )
    .unwrap();

    // Simulate the exact durable-state window after the deadline write but
    // before lane.retry_scheduled/lane.retry_wait reaches events.jsonl.
    let status = host_command(&fixture)
        .args(["host", "status", &run_id, "--json"])
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "host status failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let receipt = envelope_data(&status.stdout);
    assert_valid(&receipt);
    let attempt = receipt["state"]["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|attempt| {
            attempt["phase"] == "R1"
                && attempt["route_id"] == route.route_id
                && attempt["attempt"] == 2
        })
        .expect("persisted retry attempt was not projected");
    assert_eq!(attempt["party_id"], route.party_id);
    assert_eq!(attempt["state"], "retry_wait");
    assert_eq!(attempt["failure_class"], "timeout");
    assert_eq!(attempt["retry_due_at"], due_at);
    assert_eq!(attempt["timeout_seconds"], policy.timeout_seconds);
}

#[cfg(feature = "test-adapters")]
#[test]
fn reconcile_rejects_a_tampered_receipt_binding() {
    let fixture = host_fixture(false);
    let started = host_command(&fixture)
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .arg("--json")
        .output()
        .unwrap();
    assert!(started.status.success());
    let start = envelope_data(&started.stdout);
    let store = Store::new(fixture.home.clone());
    let run_id = start["run_id"].as_str().unwrap();
    assert_eq!(
        wait_for_terminal(&store, run_id, Duration::from_secs(60)).status,
        RunStatus::Completed
    );
    let receipt_path = std::path::PathBuf::from(start["receipt_path"].as_str().unwrap());
    let mut tampered: Value = read_json(&receipt_path).unwrap();
    tampered["receipt_path"] = json!("/tmp/not-the-authority.json");
    write_json(&receipt_path, &tampered).unwrap();

    let reconciled = host_command(&fixture)
        .args(["host", "reconcile", "--json"])
        .output()
        .unwrap();
    assert!(!reconciled.status.success());
    let stderr = String::from_utf8_lossy(&reconciled.stderr);
    assert!(stderr.contains("identity is not bound"), "{stderr}");
}

#[cfg(feature = "test-adapters")]
#[test]
fn second_host_start_is_refused_while_the_first_run_is_active() {
    let fixture = host_fixture(true);
    let receipt = start_controlled(&fixture);
    let run_id = receipt["run_id"].as_str().unwrap().to_string();
    let mut cleanup = ControlledRunCleanup::new(&fixture, &run_id);

    let refused = host_command(&fixture)
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(stderr.contains("refuses a second active run"), "{stderr}");
    assert!(stderr.contains(&run_id), "{stderr}");
    assert_eq!(
        Store::new(fixture.home.clone())
            .list_manifests()
            .unwrap()
            .len(),
        1
    );

    release_and_wait(&fixture, &run_id);
    cleanup.disarm();
}

#[cfg(feature = "test-adapters")]
#[test]
fn host_start_fails_closed_for_missing_or_corrupt_run_manifests() {
    for (case, contents) in [("missing", None), ("corrupt", Some(b"{not-json\n".as_slice()))] {
        let fixture = host_fixture(false);
        let orphan_id = uuid::Uuid::now_v7().to_string();
        let orphan = fixture.home.join("runs").join(orphan_id);
        fs::create_dir_all(&orphan).unwrap();
        if let Some(contents) = contents {
            fs::write(orphan.join("manifest.json"), contents).unwrap();
        }

        let output = host_command(&fixture)
            .args(["host", "start", "--brief"])
            .arg(&fixture.brief)
            .args(["--json"])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{case} manifest was accepted");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("cannot trust run directory"), "{case}: {stderr}");
        assert_eq!(
            fs::read_dir(fixture.home.join("runs")).unwrap().count(),
            1,
            "{case} manifest failure created another run"
        );
        assert!(
            !fixture
                .executable
                .parent()
                .unwrap()
                .join("fake-agent-started")
                .exists()
        );
    }
}

#[cfg(feature = "test-adapters")]
#[test]
fn host_start_bad_evidence_does_not_poison_the_state_root() {
    let fixture = host_fixture(false);
    let missing_evidence = fixture._temporary.path().join("missing-evidence.txt");
    let bad_brief = fixture._temporary.path().join("bad-evidence-brief.json");
    write_json(
        &bad_brief,
        &json!({
            "brief_version": "1.1",
            "question": "This source is intentionally missing",
            "evidence_roots": [missing_evidence],
            "attachments": [],
            "action_scope": "test only"
        }),
    )
    .unwrap();

    let failed = host_command(&fixture)
        .args(["host", "start", "--brief"])
        .arg(&bad_brief)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(
        String::from_utf8_lossy(&failed.stderr).contains("path does not exist"),
        "{}",
        String::from_utf8_lossy(&failed.stderr)
    );

    // The failed creation may have initialized the runs root, but it must not
    // leave an unpublished UUID directory that blocks the next host launch.
    let runs = fixture.home.join("runs");
    assert!(runs.exists());
    assert_eq!(fs::read_dir(&runs).unwrap().count(), 0);

    let started = host_command(&fixture)
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "valid launch was poisoned by the failed creation: {}",
        String::from_utf8_lossy(&started.stderr)
    );
    let receipt = envelope_data(&started.stdout);
    let run_id = receipt["run_id"].as_str().unwrap();
    let store = Store::new(fixture.home.clone());
    assert_eq!(
        wait_for_terminal(&store, run_id, Duration::from_secs(60)).status,
        RunStatus::Completed
    );
}

#[cfg(feature = "test-adapters")]
#[test]
fn concurrent_host_starts_create_at_most_one_run() {
    let fixture = host_fixture(true);
    let barrier = Arc::new(Barrier::new(3));
    let mut handles = Vec::new();
    for _ in 0..2 {
        let mut command = host_command_std(&fixture);
        command
            .args(["host", "start", "--brief"])
            .arg(&fixture.brief)
            .arg("--json");
        let barrier = barrier.clone();
        handles.push(thread::spawn(move || {
            barrier.wait();
            command.output().unwrap()
        }));
    }
    barrier.wait();
    let outputs = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();
    let successes = outputs.iter().filter(|output| output.status.success()).count();
    assert_eq!(
        successes, 1,
        "both starts succeeded or both failed: {outputs:?}"
    );
    let failure = outputs.iter().find(|output| !output.status.success()).unwrap();
    let stderr = String::from_utf8_lossy(&failure.stderr);
    assert!(
        stderr.contains("another QUINTE host launch is in progress")
            || stderr.contains("refuses a second active run"),
        "{stderr}"
    );

    let store = Store::new(fixture.home.clone());
    let manifests = store.list_manifests().unwrap();
    assert_eq!(manifests.len(), 1);
    let run_id = manifests[0].run_id.clone();
    let mut cleanup = ControlledRunCleanup::new(&fixture, &run_id);
    wait_for_path(
        &fixture
            .executable
            .parent()
            .unwrap()
            .join("fake-agent-started"),
        Duration::from_secs(30),
    );
    release_and_wait(&fixture, &run_id);
    cleanup.disarm();
}

#[cfg(feature = "test-adapters")]
#[test]
fn reconcile_recovers_a_fast_terminal_run_from_the_durable_start_receipt() {
    let fixture = host_fixture(false);
    let started = host_command(&fixture)
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "host start failed: {}",
        String::from_utf8_lossy(&started.stderr)
    );
    // Simulate a caller that lost the successful start response.  Recovery
    // must derive identity from the durable receipt, not from these bytes.
    drop(started.stdout);
    let store = Store::new(fixture.home.clone());
    let manifests = store.list_manifests().unwrap();
    assert_eq!(manifests.len(), 1);
    let run_id = manifests[0].run_id.clone();
    let manifest = wait_for_terminal(
        &store,
        &run_id,
        Duration::from_secs(60),
    );
    assert_eq!(manifest.status, RunStatus::Completed);

    let reconciled = host_command(&fixture)
        .args(["host", "reconcile", "--json"])
        .output()
        .unwrap();
    assert!(
        reconciled.status.success(),
        "implicit reconcile failed: {}",
        String::from_utf8_lossy(&reconciled.stderr)
    );
    let reconciled = envelope_data(&reconciled.stdout);
    assert_valid(&reconciled);
    assert_eq!(reconciled["state"]["code"], "reconciled");
    assert_eq!(reconciled["state"]["active_run_ids"], json!([]));
    assert_eq!(reconciled["run_id"], run_id);
    assert_eq!(reconciled["manifest"]["status"], "completed");
    assert_eq!(reconciled["recovery"]["outcome"], "reconciled");
    assert_eq!(reconciled["recovery"]["launch_safe"], true);
    assert_eq!(reconciled["result"]["verified"], true);
    let receipt_path = std::path::PathBuf::from(reconciled["receipt_path"].as_str().unwrap());
    assert_eq!(
        reconciled["recovery"]["receipt_path"],
        receipt_path.display().to_string()
    );
    assert_eq!(read_json::<Value>(&receipt_path).unwrap(), reconciled);
}

#[cfg(feature = "test-adapters")]
#[test]
fn reconcile_finds_the_start_receipt_after_a_later_preflight_projection() {
    let fixture = host_fixture(false);
    let started = host_command(&fixture)
        .args(["host", "start", "--brief"])
        .arg(&fixture.brief)
        .args(["--json"])
        .output()
        .unwrap();
    assert!(started.status.success());
    let run_id = envelope_data(&started.stdout)["run_id"]
        .as_str()
        .unwrap()
        .to_string();
    let store = Store::new(fixture.home.clone());
    assert_eq!(
        wait_for_terminal(&store, &run_id, Duration::from_secs(60)).status,
        RunStatus::Completed
    );

    let preflight = host_command(&fixture)
        .args(["host", "preflight", "--json"])
        .output()
        .unwrap();
    assert!(preflight.status.success());
    assert_eq!(
        envelope_data(&preflight.stdout)["operation"],
        "preflight"
    );

    let reconciled = host_command(&fixture)
        .args(["host", "reconcile", "--json"])
        .output()
        .unwrap();
    assert!(reconciled.status.success());
    let receipt = envelope_data(&reconciled.stdout);
    assert_eq!(receipt["state"]["code"], "reconciled");
    assert_eq!(receipt["run_id"], run_id);
    assert_eq!(receipt["result"]["verified"], true);
}
