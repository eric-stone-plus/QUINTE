use std::fs;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, Utc};
use quinte::model::{
    ArbiterVerdict, Brief, HmResponse, HmSubmissionReceipt, HmSubmissionState, Policy, RunStatus,
    SandboxMode, TEXT_MODEL,
};
use quinte::run::{self, RunOptions};
use quinte::store::Store;
use quinte::util::{read_json, sha256_file, write_json};

mod common;

struct FakeAdapterEnv {
    previous: Option<std::ffi::OsString>,
    _lock: MutexGuard<'static, ()>,
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

struct ControlledWorker<T> {
    release: std::path::PathBuf,
    home: std::path::PathBuf,
    run_id: String,
    handle: Option<thread::JoinHandle<T>>,
}

impl<T> ControlledWorker<T> {
    fn new(
        release: std::path::PathBuf,
        home: std::path::PathBuf,
        run_id: String,
        handle: thread::JoinHandle<T>,
    ) -> Self {
        Self {
            release,
            home,
            run_id,
            handle: Some(handle),
        }
    }

    fn join(mut self) -> thread::Result<T> {
        fs::write(&self.release, "release\n").unwrap();
        self.handle.take().unwrap().join()
    }
}

impl<T> Drop for ControlledWorker<T> {
    fn drop(&mut self) {
        if self.handle.is_none() {
            return;
        }
        let _ = run::cancel(&Store::new(self.home.clone()), &self.run_id);
        let _ = fs::write(&self.release, "release\n");
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
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

fn create_waiting_run(
    temporary: &std::path::Path,
    executable: &std::path::Path,
    suffix: &str,
) -> (Store, String, HmResponse) {
    let home = temporary.join(format!("home-{suffix}"));
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let policy = fake_policy(executable);
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.join(format!("evidence-{suffix}.txt"));
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.join(format!("brief-{suffix}.json"));
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "What remains unresolved?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    let challenge = store
        .load_manifest(&created.run_id)
        .unwrap()
        .hm_challenge
        .unwrap();
    let cc: ArbiterVerdict =
        read_json(&store.run_dir(&created.run_id).join("r3/cc-response.json")).unwrap();
    let response = HmResponse {
        hm_response_version: "1.0".into(),
        run_id: challenge.run_id,
        nonce: challenge.nonce,
        policy_sha256: challenge.policy_sha256,
        evidence_packet_sha256: challenge.evidence_packet_sha256,
        input_receipt_sha256: challenge.input_receipt_sha256,
        action_scope: challenge.action_scope,
        verdict: cc,
    };
    (store, created.run_id, response)
}

#[test]
fn full_fake_run_reaches_hm_then_completes() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let policy = fake_policy(&executable);
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief = Brief {
        brief_version: "1.0".into(),
        question: "What remains unresolved?".into(),
        context: None,
        evidence_roots: vec![evidence],
        attachments: Vec::new(),
        action_scope: Some("test only".into()),
    };
    let brief_path = temporary.path().join("brief.json");
    write_json(&brief_path, &brief).unwrap();

    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let advanced = run::advance(&store, &created.run_id).unwrap();
    let after_advance = store.load_manifest(&created.run_id).unwrap();
    assert_eq!(
        advanced,
        RunStatus::WaitingHm,
        "run failed before Hermes handoff: {:?}",
        after_advance.error
    );

    let challenge = store
        .load_manifest(&created.run_id)
        .unwrap()
        .hm_challenge
        .unwrap();
    let cc: ArbiterVerdict =
        read_json(&store.run_dir(&created.run_id).join("r3/cc-response.json")).unwrap();
    let response = HmResponse {
        hm_response_version: "1.0".into(),
        run_id: challenge.run_id,
        nonce: challenge.nonce,
        policy_sha256: challenge.policy_sha256,
        evidence_packet_sha256: challenge.evidence_packet_sha256,
        input_receipt_sha256: challenge.input_receipt_sha256,
        action_scope: challenge.action_scope,
        verdict: cc,
    };
    let response_path = temporary.path().join("hm-response.json");
    write_json(&response_path, &response).unwrap();
    assert_eq!(
        run::submit_hm(&store, &created.run_id, &response_path).unwrap(),
        RunStatus::Completed
    );
    assert!(store.run_dir(&created.run_id).join("result.json").is_file());
    assert!(store.run_dir(&created.run_id).join("report.md").is_file());
    assert!(
        store
            .load_manifest(&created.run_id)
            .unwrap()
            .result_sha256
            .is_some()
    );
    run::verify_result_integrity(&store, &created.run_id).unwrap();
    for phase in ["R1", "R2"] {
        for index in 0..5 {
            assert!(
                store
                    .run_dir(&created.run_id)
                    .join(format!("lanes/{phase}/fake-{index}/accepted.json"))
                    .is_file()
            );
        }
    }
}

#[test]
fn completed_result_tampering_is_rejected() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "result-integrity");
    let response_path = temporary.path().join("result-integrity-response.json");
    write_json(&response_path, &response).unwrap();
    assert_eq!(
        run::submit_hm(&store, &run_id, &response_path).unwrap(),
        RunStatus::Completed
    );
    fs::write(store.run_dir(&run_id).join("result.json"), b"{}\n").unwrap();
    assert!(run::verify_result_integrity(&store, &run_id).is_err());
}

#[test]
fn verdict_submission_constructs_scheduler_owned_binding_envelope() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "verdict-submit");
    let verdict_path = temporary.path().join("hm-verdict.json");
    write_json(&verdict_path, &response.verdict).unwrap();

    assert_eq!(
        run::submit_hm_verdict(&store, &run_id, &verdict_path).unwrap(),
        RunStatus::Completed
    );
    let owned: HmResponse = read_json(&store.run_dir(&run_id).join("r3/hm-response.json")).unwrap();
    let challenge = store.load_manifest(&run_id).unwrap().hm_challenge.unwrap();
    assert_eq!(owned.input_receipt_sha256, challenge.input_receipt_sha256);
    assert_eq!(owned.nonce, challenge.nonce);
}

#[test]
fn preplaced_hm_response_cannot_bypass_scheduler_acceptance() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) = create_waiting_run(temporary.path(), &executable, "preplaced");
    write_json(
        &store.run_dir(&run_id).join("r3/hm-response.json"),
        &response,
    )
    .unwrap();

    assert_eq!(run::advance(&store, &run_id).unwrap(), RunStatus::WaitingHm);
    let manifest = store.load_manifest(&run_id).unwrap();
    assert!(manifest.hm_submission.is_none());
    assert!(!store.run_dir(&run_id).join("result.json").exists());
}

#[test]
fn r3_receipt_blocks_tampering_of_every_accepted_input() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let targets = [
        "lanes/R1/fake-0/accepted.json",
        "lanes/R2/fake-1/accepted.json",
        "r3/evidence-packet.json",
        "r3/cc-response.json",
    ];

    for (index, target) in targets.iter().enumerate() {
        let (store, run_id, _) =
            create_waiting_run(temporary.path(), &executable, &format!("tamper-{index}"));
        fs::write(store.run_dir(&run_id).join(target), b"{}\n").unwrap();

        assert_eq!(
            run::advance(&store, &run_id).unwrap(),
            RunStatus::FailedPolicy
        );
        let manifest = store.load_manifest(&run_id).unwrap();
        assert_eq!(manifest.error.unwrap().code, "integrity_drift");
        assert!(!store.run_dir(&run_id).join("result.json").exists());
    }
}

#[test]
fn hm_staging_receipt_is_retryable_when_response_write_never_happened() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "staged-no-file");
    let response_path = temporary.path().join("staged-no-file-response.json");
    write_json(&response_path, &response).unwrap();
    let mut manifest = store.load_manifest(&run_id).unwrap();
    manifest.hm_submission = Some(HmSubmissionReceipt {
        submission_receipt_version: "1.0".into(),
        state: HmSubmissionState::Staging,
        response_ref: "r3/hm-response.json".into(),
        response_sha256: sha256_file(&response_path).unwrap(),
        input_receipt_sha256: manifest.r3_input_receipt.as_ref().unwrap().sha256.clone(),
        staged_at: manifest.updated_at.clone(),
        accepted_at: None,
    });
    store.save_manifest(&manifest).unwrap();

    assert_eq!(
        run::submit_hm(&store, &run_id, &response_path).unwrap(),
        RunStatus::Completed
    );
}

#[test]
fn hm_staged_file_is_recovered_without_resubmission() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "staged-file");
    let response_path = store.run_dir(&run_id).join("r3/hm-response.json");
    write_json(&response_path, &response).unwrap();
    let mut manifest = store.load_manifest(&run_id).unwrap();
    manifest.hm_submission = Some(HmSubmissionReceipt {
        submission_receipt_version: "1.0".into(),
        state: HmSubmissionState::Staging,
        response_ref: "r3/hm-response.json".into(),
        response_sha256: sha256_file(&response_path).unwrap(),
        input_receipt_sha256: manifest.r3_input_receipt.as_ref().unwrap().sha256.clone(),
        staged_at: manifest.updated_at.clone(),
        accepted_at: None,
    });
    store.save_manifest(&manifest).unwrap();

    assert_eq!(run::advance(&store, &run_id).unwrap(), RunStatus::Completed);
    assert_eq!(
        store
            .load_manifest(&run_id)
            .unwrap()
            .hm_submission
            .unwrap()
            .state,
        HmSubmissionState::Accepted
    );
}

#[test]
fn accepted_hm_submission_resumes_after_expiry_and_is_idempotent() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "accepted-crash");
    let internal_response = store.run_dir(&run_id).join("r3/hm-response.json");
    let external_response = temporary.path().join("accepted-crash-response.json");
    write_json(&internal_response, &response).unwrap();
    write_json(&external_response, &response).unwrap();
    let mut manifest = store.load_manifest(&run_id).unwrap();
    manifest.hm_challenge.as_mut().unwrap().consumed = true;
    manifest.hm_challenge.as_mut().unwrap().expires_at = "2000-01-01T00:00:00Z".into();
    manifest.hm_submission = Some(HmSubmissionReceipt {
        submission_receipt_version: "1.0".into(),
        state: HmSubmissionState::Accepted,
        response_ref: "r3/hm-response.json".into(),
        response_sha256: sha256_file(&internal_response).unwrap(),
        input_receipt_sha256: manifest.r3_input_receipt.as_ref().unwrap().sha256.clone(),
        staged_at: manifest.updated_at.clone(),
        accepted_at: Some(manifest.updated_at.clone()),
    });
    store.save_manifest(&manifest).unwrap();

    assert_eq!(run::advance(&store, &run_id).unwrap(), RunStatus::Completed);
    assert_eq!(
        run::submit_hm(&store, &run_id, &external_response).unwrap(),
        RunStatus::Completed
    );
}

#[test]
fn cancelling_active_workers_is_terminal_and_cannot_be_overwritten_by_failure() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let started = temporary.path().join("fake-agent-started");
    let release = temporary.path().join("fake-agent-release");
    fs::write(
        temporary.path().join("fake-agent-controlled"),
        "controlled\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let policy = fake_policy(&executable);
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Can an active run be cancelled safely?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let run_id = created.run_id.clone();
    let worker_home = home.clone();
    let worker_run_id = run_id.clone();
    let worker = ControlledWorker::new(
        release,
        home,
        run_id.clone(),
        thread::spawn(move || run::advance(&Store::new(worker_home), &worker_run_id)),
    );

    let deadline = Instant::now() + Duration::from_secs(120);
    while !started.is_file() || store.active_pids(&run_id).unwrap().is_empty() {
        assert!(
            Instant::now() < deadline,
            "fake agent did not start with a registered active PID"
        );
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(run::cancel(&store, &run_id).unwrap(), RunStatus::Cancelling);
    assert_eq!(worker.join().unwrap().unwrap(), RunStatus::Cancelled);

    let final_manifest = store.load_manifest(&run_id).unwrap();
    assert_eq!(final_manifest.status, RunStatus::Cancelled);
    assert_eq!(final_manifest.error.as_ref().unwrap().code, "cancelled");
    assert!(store.active_pids(&run_id).unwrap().is_empty());
    assert_eq!(run::advance(&store, &run_id).unwrap(), RunStatus::Cancelled);
}

#[test]
fn invalid_early_r1_lane_still_drains_all_workers() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-invalid-party"),
        "Party A\n",
    )
    .unwrap();
    fs::write(temporary.path().join("fake-agent-delay-other-ms"), "500\n").unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let policy = fake_policy(&executable);
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Do failed parallel lanes retain scheduler ownership?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::Failed
    );
    assert!(store.active_pids(&created.run_id).unwrap().is_empty());
    let events = fs::read_to_string(store.run_dir(&created.run_id).join("events.jsonl")).unwrap();
    for index in 1..5 {
        assert!(
            store
                .run_dir(&created.run_id)
                .join(format!("lanes/R1/fake-{index}/attempt-1/stdout.bin"))
                .is_file()
        );
        assert!(
            events.lines().any(|line| {
                let event: serde_json::Value = serde_json::from_str(line).unwrap();
                event["event_type"] == "lane.finished"
                    && event["phase"] == "R1"
                    && event["party_id"] == format!("Party {}", (b'A' + index as u8) as char)
            }),
            "slower R1 lane {index} did not publish a terminal event"
        );
    }
}

#[test]
fn output_limit_caps_captured_memory_and_fails_the_lane() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(temporary.path().join("fake-agent-flood-party"), "Party A\n").unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.max_output_bytes = 4 * 1024;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does QUINTE cap child output while reading it?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::Failed
    );
    let stdout = store
        .run_dir(&created.run_id)
        .join("lanes/R1/fake-0/attempt-1/stdout.bin");
    assert!(fs::metadata(stdout).unwrap().len() <= policy.max_output_bytes as u64);
    let events = fs::read_to_string(store.run_dir(&created.run_id).join("events.jsonl")).unwrap();
    assert!(events.contains("adapter output exceeds policy limit"));
    assert!(store.active_pids(&created.run_id).unwrap().is_empty());
}

#[test]
fn r2_rate_limit_retries_same_route_with_persisted_scheduler_events() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-rate-limit-party"),
        "Party A\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.max_attempts = 2;
    policy.retry_backoff_seconds = 1;
    policy.retry_backoff_max_seconds = 1;
    policy.r2_min_interval_seconds = 1;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does the scheduler recover a typed R2 rate limit?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .join("lanes/R2/fake-0/attempt-2/stdout.bin")
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(temporary.path().join("fake-agent-rate-limit-count"))
            .unwrap()
            .trim(),
        "2"
    );
    let events = fs::read_to_string(store.run_dir(&created.run_id).join("events.jsonl")).unwrap();
    assert!(events.contains("lane.retry_scheduled"));
    assert!(events.contains("\"failure_class\":\"rate_limit\""));
    assert!(events.contains("lane.retry_started"));
    assert!(events.contains("r2.pacing_wait"));
}

#[test]
fn typed_mimo_repetition_error_retries_and_preserves_the_real_error() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-repetition-party"),
        "Party D\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.roster[3].adapter = "fake_mimo".into();
    policy.max_attempts = 2;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does a typed MiMo repetition failure recover?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .join("lanes/R1/fake-3/attempt-2/stdout.bin")
            .is_file()
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .join("lanes/R1/fake-3/accepted.json")
            .is_file()
    );
    let events = store.events(&created.run_id).unwrap();
    let failed = events
        .iter()
        .find(|event| {
            event.event_type == "lane.finished"
                && event.phase.as_deref() == Some("R1")
                && event.party_id.as_deref() == Some("Party D")
                && event.attempt == Some(1)
        })
        .unwrap();
    assert_eq!(failed.data["accepted"], false);
    assert_eq!(failed.data["retryable"], true);
    assert_eq!(failed.data["failure_class"], "transient_adapter");
    assert_eq!(
        failed.data["error"],
        "Text repetition detected: repeated n-grams after 2 recovery attempts. Session terminated."
    );
    assert!(events.iter().any(|event| {
        event.event_type == "lane.retry_scheduled"
            && event.party_id.as_deref() == Some("Party D")
            && event.attempt == Some(1)
            && event.data["source"] == "adapter_structured_error"
    }));
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.party_id.as_deref() == Some("Party D")
            && event.attempt == Some(2)
            && event.data["accepted"] == true
    }));
}

#[test]
fn typed_mimo_repetition_stops_after_the_bounded_attempts() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-repetition-party"),
        "Party D\n",
    )
    .unwrap();
    fs::write(
        temporary.path().join("fake-agent-repetition-always"),
        "true\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.roster[3].adapter = "fake_mimo".into();
    policy.max_attempts = 2;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does bounded retry stop?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::Failed
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .join("lanes/R1/fake-3/attempt-2/stdout.bin")
            .is_file()
    );
    assert!(
        !store
            .run_dir(&created.run_id)
            .join("lanes/R1/fake-3/attempt-3")
            .exists()
    );
    let events = store.events(&created.run_id).unwrap();
    let exhausted = events
        .iter()
        .find(|event| {
            event.event_type == "lane.finished"
                && event.party_id.as_deref() == Some("Party D")
                && event.attempt == Some(2)
        })
        .unwrap();
    assert_eq!(exhausted.data["failure_class"], "transient_adapter");
    assert_eq!(exhausted.data["retryable"], false);
    assert_eq!(
        events
            .iter()
            .filter(|event| {
                event.event_type == "lane.retry_scheduled"
                    && event.party_id.as_deref() == Some("Party D")
            })
            .count(),
        1
    );
}

#[test]
fn timeout_recovers_a_flushed_valid_output_without_retrying() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-timeout-output-party"),
        "Party A\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.max_attempts = 2;
    policy.timeout_seconds = 5;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Can a complete output be recovered at timeout?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    let lane_dir = store.run_dir(&created.run_id).join("lanes/R1/fake-0");
    assert!(lane_dir.join("accepted.json").is_file());
    assert!(!lane_dir.join("attempt-2").exists());
    let events = store.events(&created.run_id).unwrap();
    let recovered = events
        .iter()
        .find(|event| {
            event.event_type == "lane.finished"
                && event.phase.as_deref() == Some("R1")
                && event.party_id.as_deref() == Some("Party A")
                && event.attempt == Some(1)
        })
        .unwrap();
    assert_eq!(recovered.data["timed_out"], true);
    assert_eq!(recovered.data["accepted"], true);
    assert_eq!(recovered.data["output_recovered_after_timeout"], true);
    assert!(recovered.data["error"].is_null());
    assert!(recovered.data["failure_class"].is_null());
    assert_eq!(recovered.data["retryable"], false);
}

#[test]
fn invalid_evidence_is_rejected_before_lane_finished_is_recorded_as_accepted() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-invalid-evidence-party"),
        "Party A\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let policy = fake_policy(&executable);
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Can invalid evidence be marked accepted?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::Failed
    );
    let events = store.events(&created.run_id).unwrap();
    let rejected = events
        .iter()
        .find(|event| {
            event.event_type == "lane.finished"
                && event.phase.as_deref() == Some("R1")
                && event.party_id.as_deref() == Some("Party A")
        })
        .unwrap();
    assert_eq!(rejected.data["accepted"], false);
    assert_eq!(rejected.data["retryable"], false);
    assert_eq!(rejected.data["failure_class"], "non_retryable");
    assert!(
        rejected.data["error"]
            .as_str()
            .unwrap()
            .contains("unresolvable evidence reference")
    );
}

#[test]
fn completed_codewhale_without_lane_output_retries_on_the_same_route() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-codewhale-invalid-party"),
        "Party A\n",
    )
    .unwrap();
    fs::write(
        temporary.path().join("fake-agent-codewhale-party"),
        "Party A\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.roster[0].adapter = "fake_codewhale".into();
    policy.max_attempts = 2;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does a completed CodeWhale stream retry without LaneOutput?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    let events = store.events(&created.run_id).unwrap();
    let first = events
        .iter()
        .find(|event| {
            event.event_type == "lane.finished"
                && event.phase.as_deref() == Some("R1")
                && event.party_id.as_deref() == Some("Party A")
                && event.attempt == Some(1)
        })
        .unwrap();
    assert_eq!(first.data["accepted"], false);
    assert_eq!(first.data["failure_class"], "transient_adapter");
    assert_eq!(first.data["retryable"], true);
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.phase.as_deref() == Some("R1")
            && event.party_id.as_deref() == Some("Party A")
            && event.attempt == Some(2)
            && event.data["accepted"] == true
    }));
}

#[test]
fn r3_auditor_timeout_uses_the_same_bounded_retry_policy() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-timeout-once-party"),
        "Auditor B\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.max_attempts = 2;
    policy.timeout_seconds = 5;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does the R3 auditor recover from a transient timeout?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    let events = store.events(&created.run_id).unwrap();
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.phase.as_deref() == Some("R3")
            && event.party_id.as_deref() == Some("Auditor B")
            && event.attempt == Some(1)
            && event.data["failure_class"] == "timeout"
            && event.data["retryable"] == true
    }));
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.phase.as_deref() == Some("R3")
            && event.party_id.as_deref() == Some("Auditor B")
            && event.attempt == Some(2)
            && event.data["accepted"] == true
    }));
    assert!(
        store
            .run_dir(&created.run_id)
            .join("r3/cc-response.json")
            .is_file()
    );
}

#[test]
fn resume_consumes_existing_attempt_directories_in_r1_and_r3() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.max_attempts = 2;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does resume preserve the attempt budget?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let run_dir = store.run_dir(&created.run_id);

    // A crash after creating an attempt directory still consumes that attempt.
    fs::create_dir_all(run_dir.join("lanes/R1/fake-0/attempt-1")).unwrap();
    fs::create_dir_all(run_dir.join("lanes/R3/cc/attempt-1")).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    assert!(
        run_dir
            .join("lanes/R1/fake-0/attempt-2/stdout.bin")
            .is_file()
    );
    assert!(run_dir.join("lanes/R3/cc/attempt-2/stdout.bin").is_file());
    let events = store.events(&created.run_id).unwrap();
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.phase.as_deref() == Some("R1")
            && event.party_id.as_deref() == Some("Party A")
            && event.attempt == Some(2)
            && event.data["accepted"] == true
    }));
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.phase.as_deref() == Some("R3")
            && event.party_id.as_deref() == Some("Auditor B")
            && event.attempt == Some(2)
            && event.data["accepted"] == true
    }));
}

#[test]
fn resume_honors_a_persisted_r1_retry_deadline_before_starting_the_next_attempt() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.max_attempts = 2;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does resume preserve a pending retry cooldown?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let run_dir = store.run_dir(&created.run_id);
    let lane_dir = run_dir.join("lanes/R1/fake-0");
    fs::create_dir_all(lane_dir.join("attempt-1")).unwrap();
    let due_at = Utc::now() + ChronoDuration::milliseconds(10_000);
    write_json(
        &lane_dir.join("retry-deadline.json"),
        &serde_json::json!({
            "retry_state_version": "1.0",
            "phase": "R1",
            "route_id": "fake-0",
            "previous_attempt": 1,
            "next_attempt": 2,
            "due_at": due_at.to_rfc3339(),
            "failure_class": "timeout",
            "source": "host_timeout"
        }),
    )
    .unwrap();

    let started = Instant::now();
    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    assert!(started.elapsed() >= Duration::from_millis(8_000));
    assert!(!lane_dir.join("retry-deadline.json").exists());
    assert!(lane_dir.join("attempt-2/stdout.bin").is_file());
    let events = store.events(&created.run_id).unwrap();
    let retry_wait = events
        .iter()
        .find(|event| {
            event.event_type == "lane.retry_wait"
                && event.phase.as_deref() == Some("R1")
                && event.party_id.as_deref() == Some("Party A")
                && event.attempt == Some(2)
        })
        .unwrap();
    assert_eq!(retry_wait.data["previous_attempt"], 1);
    assert_eq!(retry_wait.data["source"], "host_timeout");
    assert!(retry_wait.data["delay_milliseconds"].as_u64().unwrap() > 0);
}

#[test]
fn resume_honors_a_persisted_r3_retry_deadline_before_starting_the_auditor() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.max_attempts = 2;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Does resume preserve the auditor retry cooldown?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let run_dir = store.run_dir(&created.run_id);
    let lane_dir = run_dir.join("lanes/R3/cc");
    fs::create_dir_all(lane_dir.join("attempt-1")).unwrap();
    // R1/R2 run before R3 is reached, so keep this deadline comfortably ahead.
    let due_at = Utc::now() + ChronoDuration::milliseconds(10_000);
    write_json(
        &lane_dir.join("retry-deadline.json"),
        &serde_json::json!({
            "retry_state_version": "1.0",
            "phase": "R3",
            "route_id": "fake-cc",
            "previous_attempt": 1,
            "next_attempt": 2,
            "due_at": due_at.to_rfc3339(),
            "failure_class": "timeout",
            "source": "host_timeout"
        }),
    )
    .unwrap();

    let started = Instant::now();
    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingHm
    );
    assert!(started.elapsed() >= Duration::from_millis(8_000));
    assert!(!lane_dir.join("retry-deadline.json").exists());
    assert!(lane_dir.join("attempt-2/stdout.bin").is_file());
    let events = store.events(&created.run_id).unwrap();
    let retry_wait = events
        .iter()
        .find(|event| {
            event.event_type == "lane.retry_wait"
                && event.phase.as_deref() == Some("R3")
                && event.party_id.as_deref() == Some("Auditor B")
                && event.attempt == Some(2)
        })
        .unwrap();
    assert_eq!(retry_wait.data["previous_attempt"], 1);
    assert_eq!(retry_wait.data["source"], "host_timeout");
    assert!(retry_wait.data["delay_milliseconds"].as_u64().unwrap() > 0);
}

#[test]
fn resume_fails_closed_when_an_existing_attempt_consumed_the_budget() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let policy = fake_policy(&executable);
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: "1.0".into(),
            question: "Can resume bypass an exhausted attempt budget?".into(),
            context: None,
            evidence_roots: vec![evidence],
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let lane_dir = store.run_dir(&created.run_id).join("lanes/R1/fake-0");
    fs::create_dir_all(lane_dir.join("attempt-1")).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::Failed
    );
    assert!(!lane_dir.join("attempt-2").exists());
    let manifest = store.load_manifest(&created.run_id).unwrap();
    let error = manifest.error.unwrap();
    assert_eq!(error.code, "r1_failed");
    assert!(error.message.contains("attempt budget exhausted"));
}

#[test]
fn valid_model_prose_containing_429_never_triggers_retry() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-prose-429-party"),
        "Party A\n",
    )
    .unwrap();
    let (store, run_id, _) = create_waiting_run(temporary.path(), &executable, "prose-429");

    assert!(
        store
            .run_dir(&run_id)
            .join("lanes/R1/fake-0/accepted.json")
            .is_file()
    );
    assert!(
        !store
            .run_dir(&run_id)
            .join("lanes/R1/fake-0/attempt-2")
            .exists()
    );
    let events = fs::read_to_string(store.run_dir(&run_id).join("events.jsonl")).unwrap();
    assert!(!events.contains("lane.retry_scheduled"));
}
