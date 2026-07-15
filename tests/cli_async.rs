mod common;

use std::fs;
use std::io::Read;
use std::process::Command as StdCommand;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

use assert_cmd::Command;
use quinte::model::{Policy, RunStatus, SandboxMode, TEXT_MODEL};
use quinte::store::Store;
#[cfg(unix)]
use quinte::util::read_json;
use quinte::util::write_json;
use serde_json::Value;

struct FakeAdapterEnv {
    previous: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
}

struct ReleaseOnDrop(std::path::PathBuf);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        let _ = fs::write(&self.0, b"release\n");
    }
}

fn read_in_background<R>(mut reader: R) -> Receiver<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = reader.read_to_end(&mut bytes).map(|_| bytes);
        let _ = sender.send(result);
    });
    receiver
}

fn receive_pipe(
    receiver: Receiver<std::io::Result<Vec<u8>>>,
    release: &std::path::Path,
) -> (Vec<u8>, bool) {
    match receiver.recv_timeout(Duration::from_secs(10)) {
        Ok(result) => (result.unwrap(), false),
        Err(_) => {
            fs::write(release, b"release\n").unwrap();
            let bytes = receiver
                .recv_timeout(Duration::from_secs(60))
                .expect("worker did not close the inherited output pipe after release")
                .unwrap();
            (bytes, true)
        }
    }
}

impl FakeAdapterEnv {
    fn enable() -> Self {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let lock = LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os("QUINTE_ALLOW_FAKE_ADAPTERS");
        unsafe { std::env::set_var("QUINTE_ALLOW_FAKE_ADAPTERS", "1") };
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for FakeAdapterEnv {
    fn drop(&mut self) {
        unsafe {
            if let Some(value) = self.previous.take() {
                std::env::set_var("QUINTE_ALLOW_FAKE_ADAPTERS", value);
            } else {
                std::env::remove_var("QUINTE_ALLOW_FAKE_ADAPTERS");
            }
        }
    }
}

#[cfg(unix)]
fn wait_for_file(child: &mut std::process::Child, path: &std::path::Path, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while !path.is_file() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("wait process exited before becoming ready: {status}");
        }
        assert!(
            Instant::now() < deadline,
            "wait process did not become ready"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn wait_for_exit(child: &mut std::process::Child, timeout: Duration) -> std::process::ExitStatus {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("wait process did not exit after SIGINT");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

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
        counterpart_arbiter: quinte::model::RoutePolicy {
            party_id: "Counterpart Arbiter".into(),
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
fn run_returns_queued_immediately_and_worker_reaches_waiting_primary_arbiter() {
    let fixture = fixture(0);
    let fixture_dir = fixture.executable.parent().unwrap();
    let started = fixture_dir.join("fake-agent-started");
    let release = fixture_dir.join("fake-agent-release");
    let _release_on_drop = ReleaseOnDrop(release.clone());
    fs::write(fixture_dir.join("fake-agent-controlled"), b"controlled\n").unwrap();
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
    let stdout = read_in_background(child.stdout.take().unwrap());
    let stderr = read_in_background(child.stderr.take().unwrap());
    let parent_deadline = std::time::Instant::now() + Duration::from_secs(120);
    let worker_deadline = std::time::Instant::now() + Duration::from_secs(300);
    let mut parent_exited = false;
    while !parent_exited || !started.is_file() {
        if !parent_exited {
            parent_exited = child.try_wait().unwrap().is_some();
            assert!(
                std::time::Instant::now() < parent_deadline,
                "run CLI stayed attached to its background worker"
            );
        }
        assert!(
            started.is_file() || std::time::Instant::now() < worker_deadline,
            "detached worker did not reach the fake-agent handshake"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let parent_status = child.wait().unwrap();
    let store = Store::new(fixture.home.clone());
    let manifests = store.list_manifests().unwrap();
    assert_eq!(manifests.len(), 1);
    let run_id = manifests[0].run_id.clone();
    assert_eq!(
        store.load_manifest(&run_id).unwrap().status,
        RunStatus::R1Running,
        "worker advanced while its first agent was blocked"
    );

    let (stdout, stdout_leaked) = receive_pipe(stdout, &release);
    let (stderr, stderr_leaked) = receive_pipe(stderr, &release);
    assert!(
        parent_status.success(),
        "{}",
        String::from_utf8_lossy(&stderr)
    );
    assert!(
        !stdout_leaked && !stderr_leaked,
        "detached worker inherited the run CLI output pipes"
    );
    let envelope: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(envelope["data"]["status"], "queued");
    assert_eq!(run_id_from(&stdout), run_id);
    fs::write(release, b"release\n").unwrap();

    let waited = quinte(&fixture)
        .args(["wait", &run_id, "--json"])
        .timeout(Duration::from_secs(120))
        .output()
        .unwrap();
    let manifest = store.load_manifest(&run_id).unwrap();
    let run_dir = store.run_dir(&run_id);
    let events = fs::read_to_string(run_dir.join("events.jsonl")).unwrap_or_default();
    let worker_log = fs::read_to_string(run_dir.join("diagnostics/worker.log")).unwrap_or_default();
    assert!(
        waited.status.success(),
        "wait failed: status={} stdout={} stderr={} manifest={manifest:?} events={events} worker_log={worker_log}",
        waited.status,
        String::from_utf8_lossy(&waited.stdout),
        String::from_utf8_lossy(&waited.stderr)
    );
    let envelope: Value = serde_json::from_slice(&waited.stdout).unwrap();
    assert_eq!(envelope["data"]["status"], "waiting_primary_arbiter");
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
    let _fake_adapter_env = FakeAdapterEnv::enable();
    let policy = fake_policy(&fixture.executable);
    let created = run::create(
        &store,
        &policy,
        &RunOptions {
            brief_path: fixture.brief.clone(),
        },
    )
    .unwrap();
    let mut waiter = StdCommand::new(assert_cmd::cargo::cargo_bin!("quinte"))
        .env("QUINTE_HOME", &fixture.home)
        .env("QUINTE_ALLOW_FAKE_ADAPTERS", "1")
        .env("QUINTE_TEST_WAIT_READY", "1")
        .args(["wait", &created.run_id, "--json"])
        .spawn()
        .unwrap();
    let ready = store
        .run_dir(&created.run_id)
        .join("diagnostics/wait-handler-ready");
    wait_for_file(&mut waiter, &ready, Duration::from_secs(10));
    let signal = StdCommand::new("kill")
        .args(["-INT", &waiter.id().to_string()])
        .status()
        .unwrap();
    assert!(signal.success());
    let status = wait_for_exit(&mut waiter, Duration::from_secs(10));
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
    let _fake_adapter_env = FakeAdapterEnv::enable();
    let policy = fake_policy(&fixture.executable);
    let created = run::create(
        &store,
        &policy,
        &RunOptions {
            brief_path: fixture.brief.clone(),
        },
    )
    .unwrap();
    write_json(
        &store
            .run_dir(&created.run_id)
            .join("diagnostics/worker.json"),
        &serde_json::json!({"pid": 2_147_483_647_u32, "started_at": "2026-01-01T00:00:00Z"}),
    )
    .unwrap();
    let output = quinte(&fixture)
        .args(["wait", &created.run_id, "--json"])
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("quinte resume"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn wait_accepts_a_durable_waiting_primary_arbiter_state_after_worker_exit() {
    use quinte::run::{self, RunOptions};

    let fixture = fixture(0);
    let store = Store::new(fixture.home.clone());
    let _fake_adapter_env = FakeAdapterEnv::enable();
    let policy = fake_policy(&fixture.executable);
    let created = run::create(
        &store,
        &policy,
        &RunOptions {
            brief_path: fixture.brief.clone(),
        },
    )
    .unwrap();
    let mut manifest = store.load_manifest(&created.run_id).unwrap();
    manifest.status = RunStatus::WaitingPrimaryArbiter;
    manifest.current_phase = Some("R3".into());
    store.save_manifest(&manifest).unwrap();
    write_json(
        &store
            .run_dir(&created.run_id)
            .join("diagnostics/worker.json"),
        &serde_json::json!({"pid": 2_147_483_647_u32, "started_at": "2026-01-01T00:00:00Z"}),
    )
    .unwrap();
    let output = quinte(&fixture)
        .args(["wait", &created.run_id, "--json"])
        .timeout(Duration::from_secs(10))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["data"]["status"], "waiting_primary_arbiter");
}
