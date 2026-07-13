mod common;

use std::fs;
use std::process::Command as StdCommand;
use std::time::Duration;

use assert_cmd::Command;
use quinte::model::{Policy, RunStatus, SandboxMode, TEXT_MODEL};
use quinte::store::Store;
use quinte::util::{read_json, write_json};
use serde_json::Value;

fn fake_policy(executable: &std::path::Path) -> Policy {
    let parties = ["Party A", "Party B", "Party C", "Party D", "Party E"];
    Policy {
        policy_version: "1.0".into(),
        roster: parties
            .iter()
            .enumerate()
            .map(|(index, party)| quinte::model::RoutePolicy {
                party_id: (*party).into(),
                route_id: format!("fake-{index}"),
                adapter: "fake".into(),
                executable: executable.display().to_string(),
                required: true,
            })
            .collect(),
        auditor: quinte::model::RoutePolicy {
            party_id: "Auditor B".into(),
            route_id: "fake-cc".into(),
            adapter: "fake".into(),
            executable: executable.display().to_string(),
            required: true,
        },
        text_model: TEXT_MODEL.into(),
        multimodal_model: "mimo-v2.5".into(),
        max_parallel_r1: 5,
        max_parallel_r2: 1,
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

struct Fixture {
    _temporary: tempfile::TempDir,
    home: std::path::PathBuf,
    brief: std::path::PathBuf,
    executable: std::path::PathBuf,
}

fn fixture(delay_ms: u64) -> Fixture {
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-delay-ms"),
        delay_ms.to_string(),
    )
    .unwrap();
    let home = temporary.path().join("home");
    fs::create_dir_all(&home).unwrap();
    write_json(&home.join("policy.json"), &fake_policy(&executable)).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief = temporary.path().join("brief.json");
    write_json(
        &brief,
        &serde_json::json!({
            "brief_version": "1.0",
            "question": "What remains unresolved?",
            "evidence_roots": [evidence],
            "attachments": [],
            "action_scope": "test only"
        }),
    )
    .unwrap();
    Fixture {
        _temporary: temporary,
        home,
        brief,
        executable,
    }
}

fn run_id_from(output: &[u8]) -> String {
    let envelope: Value = serde_json::from_slice(output).unwrap();
    envelope["data"]["run_id"].as_str().unwrap().to_string()
}

fn quinte(fixture: &Fixture) -> Command {
    let mut command = Command::cargo_bin("quinte").unwrap();
    command
        .env("QUINTE_HOME", &fixture.home)
        .env("QUINTE_ALLOW_FAKE_ADAPTERS", "1");
    command
}

#[test]
fn run_returns_queued_immediately_and_worker_reaches_waiting_hm() {
    let fixture = fixture(5_000);
    let mut child = StdCommand::new(assert_cmd::cargo::cargo_bin!("quinte"))
        .env("QUINTE_HOME", &fixture.home)
        .env("QUINTE_ALLOW_FAKE_ADAPTERS", "1")
        .args(["run", "--brief"])
        .arg(&fixture.brief)
        .args(["--json"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "run CLI stayed attached to the worker"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["data"]["status"], "queued");
    let run_id = run_id_from(&output.stdout);
    assert_ne!(
        Store::new(fixture.home.clone())
            .load_manifest(&run_id)
            .unwrap()
            .status,
        RunStatus::WaitingHm,
        "run returned only after the delayed worker finished"
    );

    let waited = quinte(&fixture)
        .args(["wait", &run_id, "--json"])
        .timeout(Duration::from_secs(20))
        .output()
        .unwrap();
    assert!(
        waited.status.success(),
        "{}",
        String::from_utf8_lossy(&waited.stderr)
    );
    let envelope: Value = serde_json::from_slice(&waited.stdout).unwrap();
    assert_eq!(envelope["data"]["status"], "waiting_hm");
    assert!(
        Store::new(fixture.home.clone())
            .run_dir(&run_id)
            .join("diagnostics/worker.json")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn sigint_interrupts_wait_without_cancelling_run() {
    use quinte::run::{self, RunOptions};

    let fixture = fixture(0);
    let store = Store::new(fixture.home.clone());
    let previous_allow_fake = std::env::var_os("QUINTE_ALLOW_FAKE_ADAPTERS");
    unsafe { std::env::set_var("QUINTE_ALLOW_FAKE_ADAPTERS", "1") };
    let created = run::create(
        &store,
        &fake_policy(&fixture.executable),
        &RunOptions {
            brief_path: fixture.brief.clone(),
        },
    )
    .unwrap();
    unsafe {
        if let Some(value) = previous_allow_fake {
            std::env::set_var("QUINTE_ALLOW_FAKE_ADAPTERS", value);
        } else {
            std::env::remove_var("QUINTE_ALLOW_FAKE_ADAPTERS");
        }
    }

    let mut waiter = StdCommand::new(assert_cmd::cargo::cargo_bin!("quinte"))
        .env("QUINTE_HOME", &fixture.home)
        .env("QUINTE_ALLOW_FAKE_ADAPTERS", "1")
        .args(["wait", &created.run_id, "--json"])
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(250));
    let signal = StdCommand::new("kill")
        .args(["-INT", &waiter.id().to_string()])
        .status()
        .unwrap();
    assert!(signal.success());
    let status = waiter.wait().unwrap();
    assert_eq!(status.code(), Some(130));

    let manifest =
        read_json::<quinte::model::RunManifest>(&store.manifest_path(&created.run_id)).unwrap();
    assert_eq!(manifest.status, RunStatus::Queued);
    assert!(
        !store
            .run_dir(&created.run_id)
            .join("cancel.requested")
            .exists()
    );
}

#[test]
fn wait_reports_a_dead_background_worker() {
    use quinte::run::{self, RunOptions};

    let fixture = fixture(0);
    let store = Store::new(fixture.home.clone());
    let previous_allow_fake = std::env::var_os("QUINTE_ALLOW_FAKE_ADAPTERS");
    unsafe { std::env::set_var("QUINTE_ALLOW_FAKE_ADAPTERS", "1") };
    let created = run::create(
        &store,
        &fake_policy(&fixture.executable),
        &RunOptions {
            brief_path: fixture.brief.clone(),
        },
    )
    .unwrap();
    write_json(
        &store
            .run_dir(&created.run_id)
            .join("diagnostics/worker.json"),
        &serde_json::json!({"pid": u32::MAX, "started_at": "2026-01-01T00:00:00Z"}),
    )
    .unwrap();
    unsafe {
        if let Some(value) = previous_allow_fake {
            std::env::set_var("QUINTE_ALLOW_FAKE_ADAPTERS", value);
        } else {
            std::env::remove_var("QUINTE_ALLOW_FAKE_ADAPTERS");
        }
    }

    let output = quinte(&fixture)
        .args(["wait", &created.run_id, "--json"])
        .timeout(Duration::from_secs(2))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("quinte resume"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
