use std::fs;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Duration as ChronoDuration, Utc};
use quinte::model::{
    ArbiterVerdict, BRIEF_VERSION, Brief, Policy, PrimaryArbiterResponse,
    PrimaryArbiterSubmissionReceipt, PrimaryArbiterSubmissionState, RunStatus, SandboxMode,
    TEXT_MODEL,
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
        legacy_v1_source: false,
        policy_version: "2.0".into(),
        seat: Default::default(),
        roster: parties
            .iter()
            .enumerate()
            .map(|(index, party)| quinte::model::RoutePolicy {
                party_id: (*party).into(),
                route_id: format!("fake-{index}"),
                adapter: "fake".into(),
                executable: executable.display().to_string(),
                required: true,
                family: "mimo".into(),
                provider: "xiaomi-token-plan-cn".into(),
                text_model: TEXT_MODEL.into(),
                multimodal_model: "mimo-v2.5".into(),
                perspective: String::new(),
            })
            .collect(),
        counterpart_arbiter: quinte::model::RoutePolicy {
            party_id: "Counterpart Arbiter".into(),
            route_id: "fake-cc".into(),
            adapter: "fake".into(),
            executable: executable.display().to_string(),
            required: true,
            family: "mimo".into(),
            provider: "xiaomi-token-plan-cn".into(),
            text_model: TEXT_MODEL.into(),
            multimodal_model: "mimo-v2.5".into(),
            perspective: String::new(),
        },
        primary_arbiter: quinte::model::RoutePolicy {
            party_id: "Primary Arbiter".into(),
            route_id: "fake-pa".into(),
            adapter: "fake".into(),
            executable: executable.display().to_string(),
            required: true,
            family: "mimo".into(),
            provider: "xiaomi-token-plan-cn".into(),
            text_model: TEXT_MODEL.into(),
            multimodal_model: "mimo-v2.5".into(),
            perspective: String::new(),
        },
        auto_primary_arbiter: false,
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

fn create_waiting_run(
    temporary: &std::path::Path,
    executable: &std::path::Path,
    suffix: &str,
) -> (Store, String, PrimaryArbiterResponse) {
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
            brief_version: BRIEF_VERSION.into(),
            question: "What remains unresolved?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
    );
    let challenge = store
        .load_manifest(&created.run_id)
        .unwrap()
        .primary_arbiter_challenge
        .unwrap();
    let counterpart_arbiter: ArbiterVerdict = read_json(
        &store
            .run_dir(&created.run_id)
            .unwrap()
            .join("r3/cc-response.json"),
    )
    .unwrap();
    let response = PrimaryArbiterResponse {
        primary_arbiter_response_version: "1.0".into(),
        run_id: challenge.run_id,
        nonce: challenge.nonce,
        policy_sha256: challenge.policy_sha256,
        evidence_packet_sha256: challenge.evidence_packet_sha256,
        input_receipt_sha256: challenge.input_receipt_sha256,
        action_scope: challenge.action_scope,
        verdict: counterpart_arbiter,
    };
    (store, created.run_id, response)
}

fn create_auto_primary_run(
    temporary: &std::path::Path,
    executable: &std::path::Path,
    suffix: &str,
) -> (Store, String) {
    let home = temporary.join(format!("home-{suffix}"));
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(executable);
    policy.auto_primary_arbiter = true;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.join(format!("evidence-{suffix}.txt"));
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.join(format!("brief-{suffix}.json"));
    write_json(
        &brief_path,
        &Brief {
            brief_version: BRIEF_VERSION.into(),
            question: "Can the automatic Primary Arbiter finish the run?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    (store, created.run_id)
}

#[test]
fn invalid_snapshot_ignore_does_not_create_an_orphan_run_directory() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let executable = temporary.path().join("unused-fake-adapter");
    let policy = fake_policy(&executable);
    let brief_path = temporary.path().join("invalid-ignore-brief.json");
    fs::write(
        &brief_path,
        r#"{
            "brief_version": "1.1",
            "question": "What remains unresolved?",
            "snapshot_ignore": ["[invalid"]
        }"#,
    )
    .unwrap();

    let error = run::create(&store, &policy, &RunOptions { brief_path }).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("invalid snapshot_ignore pattern")
    );
    assert!(!store.runs_dir().exists());
}

#[test]
fn reasonix_attachment_rejection_precedes_run_state_creation() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = quinte::policy::default_policy();
    policy.seat.seat_id = "seat-deepseek".into();
    policy.seat.family = "deepseek".into();
    policy.seat.provider = "deepseek".into();
    policy.seat.text_model = "deepseek-v4-pro".into();
    policy.seat.multimodal_model = "deepseek-v4-pro".into();
    policy.text_model = policy.seat.text_model.clone();
    policy.multimodal_model = policy.seat.multimodal_model.clone();
    for route in policy
        .roster
        .iter_mut()
        .chain(std::iter::once(&mut policy.counterpart_arbiter))
        .chain(std::iter::once(&mut policy.primary_arbiter))
    {
        route.adapter = "reasonix".into();
        route.executable = "reasonix".into();
        route.family = policy.seat.family.clone();
        route.provider = policy.seat.provider.clone();
        route.text_model = policy.seat.text_model.clone();
        route.multimodal_model = policy.seat.multimodal_model.clone();
    }
    let attachment = temporary.path().join("evidence.png");
    fs::write(&attachment, b"\x89PNG\r\n\x1a\n").unwrap();
    let brief_path = temporary.path().join("attachment-brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: BRIEF_VERSION.into(),
            question: "Inspect the image".into(),
            context: None,
            evidence_roots: Vec::new(),
            snapshot_ignore: Vec::new(),
            attachments: vec![attachment],
            action_scope: None,
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();

    let error = run::create(&store, &policy, &RunOptions { brief_path }).unwrap_err();

    assert!(error.to_string().contains("no native image carrier"));
    assert!(!store.runs_dir().exists());
}

#[test]
fn legacy_brief_is_normalized_before_persistence_and_hashing() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join("home"));
    fs::create_dir_all(store.home()).unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let policy = fake_policy(&executable);
    let brief_path = temporary.path().join("legacy-brief.json");
    fs::write(
        &brief_path,
        r#"{"brief_version":"1.0","question":"Normalize this legacy brief"}"#,
    )
    .unwrap();

    let created = run::create(
        &store,
        &policy,
        &RunOptions {
            brief_path: brief_path.clone(),
        },
    )
    .unwrap();
    let persisted: Brief = read_json(
        &store
            .run_dir(&created.run_id)
            .unwrap()
            .join("input/brief.json"),
    )
    .unwrap();
    let manifest = store.load_manifest(&created.run_id).unwrap();

    assert_eq!(persisted.brief_version, "1.1");
    assert_eq!(
        manifest.brief_sha256,
        quinte::util::sha256_bytes(&serde_json::to_vec(&persisted).unwrap())
    );
    assert_eq!(
        fs::read_to_string(&brief_path).unwrap(),
        r#"{"brief_version":"1.0","question":"Normalize this legacy brief"}"#
    );
}

#[test]
fn full_fake_run_reaches_primary_arbiter_then_completes() {
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
        brief_version: BRIEF_VERSION.into(),
        question: "What remains unresolved?".into(),
        context: None,
        evidence_roots: vec![evidence],
        snapshot_ignore: Vec::new(),
        attachments: Vec::new(),
        action_scope: Some("test only".into()),
        affected_paths: Vec::new(),
        action_binding_sha256: None,
    };
    let brief_path = temporary.path().join("brief.json");
    write_json(&brief_path, &brief).unwrap();

    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let advanced = run::advance(&store, &created.run_id).unwrap();
    let after_advance = store.load_manifest(&created.run_id).unwrap();
    assert_eq!(
        advanced,
        RunStatus::WaitingPrimaryArbiter,
        "run failed before Primary Arbiter handoff: {:?}",
        after_advance.error
    );
    let task_packet: serde_json::Value = read_json(
        &store
            .run_dir(&created.run_id)
            .unwrap()
            .join("input/task-packet.json"),
    )
    .unwrap();
    assert_eq!(
        task_packet["allowed_evidence_prefix"],
        serde_json::json!("snapshot://")
    );
    assert_eq!(
        task_packet["allowed_attachment_prefix"],
        serde_json::json!("attachment://")
    );

    let challenge = store
        .load_manifest(&created.run_id)
        .unwrap()
        .primary_arbiter_challenge
        .unwrap();
    let counterpart_arbiter: ArbiterVerdict = read_json(
        &store
            .run_dir(&created.run_id)
            .unwrap()
            .join("r3/cc-response.json"),
    )
    .unwrap();
    let response = PrimaryArbiterResponse {
        primary_arbiter_response_version: "1.0".into(),
        run_id: challenge.run_id,
        nonce: challenge.nonce,
        policy_sha256: challenge.policy_sha256,
        evidence_packet_sha256: challenge.evidence_packet_sha256,
        input_receipt_sha256: challenge.input_receipt_sha256,
        action_scope: challenge.action_scope,
        verdict: counterpart_arbiter,
    };
    let response_path = temporary.path().join("primary-arbiter-response.json");
    write_json(&response_path, &response).unwrap();
    assert_eq!(
        run::submit_primary_arbiter(&store, &created.run_id, &response_path).unwrap(),
        RunStatus::Completed
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .unwrap()
            .join("result.json")
            .is_file()
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .unwrap()
            .join("report.md")
            .is_file()
    );
    assert!(
        store
            .load_manifest(&created.run_id)
            .unwrap()
            .result_sha256
            .is_some()
    );
    let integrity = run::verify_result_integrity(&store, &created.run_id)
        .unwrap()
        .unwrap();
    assert_eq!(integrity.contract_version, "2.1");
    assert!(integrity.actionable);
    for phase in ["R1", "R2"] {
        for index in 0..5 {
            assert!(
                store
                    .run_dir(&created.run_id)
                    .unwrap()
                    .join(format!("lanes/{phase}/fake-{index}/accepted.json"))
                    .is_file()
            );
        }
    }
}

#[test]
fn automatic_primary_arbiter_sees_counterpart_and_completes_without_host_submission() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id) = create_auto_primary_run(temporary.path(), &executable, "auto-pa");

    assert_eq!(run::advance(&store, &run_id).unwrap(), RunStatus::Completed);
    let run_dir = store.run_dir(&run_id).unwrap();
    let packet: serde_json::Value =
        read_json(&run_dir.join("r3/primary-arbiter-packet.json")).unwrap();
    assert_eq!(packet["primary_arbiter_packet_version"], "1.0");
    assert_eq!(packet["run_id"], run_id);
    assert!(packet["evidence_packet"].is_object());
    assert!(packet["counterpart_arbiter_verdict"].is_object());
    assert!(
        run_dir
            .join("lanes/R3/fake-cc/attempt-1/stdout.bin")
            .is_file()
    );
    assert!(
        run_dir
            .join("lanes/R3/fake-pa/attempt-1/stdout.bin")
            .is_file()
    );
    assert!(run_dir.join("r3/primary-arbiter-response.json").is_file());
    assert!(run_dir.join("result.json").is_file());
    let manifest = store.load_manifest(&run_id).unwrap();
    let expected_roles = [
        "Party A",
        "Party B",
        "Party C",
        "Party D",
        "Party E",
        "Counterpart Arbiter",
        "Primary Arbiter",
    ];
    assert_eq!(
        manifest
            .route_bindings
            .iter()
            .map(|binding| binding.party_id.as_str())
            .collect::<Vec<_>>(),
        expected_roles
    );
    let result: quinte::model::ResultEnvelope = read_json(&run_dir.join("result.json")).unwrap();
    assert_eq!(result.seat_binding, manifest.seat_binding);
    assert_eq!(result.route_bindings, manifest.route_bindings);
    assert!(manifest.primary_arbiter_challenge.unwrap().consumed);
    assert_eq!(
        manifest.primary_arbiter_submission.unwrap().state,
        PrimaryArbiterSubmissionState::Accepted
    );
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
        run::submit_primary_arbiter(&store, &run_id, &response_path).unwrap(),
        RunStatus::Completed
    );
    fs::write(store.run_dir(&run_id).unwrap().join("result.json"), b"{}\n").unwrap();
    assert!(run::verify_result_integrity(&store, &run_id).is_err());
}

#[test]
fn legacy_completed_result_is_verified_read_only_without_contract_rewrite() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join("home"));
    let run_id = "019bf52a-73b0-7000-8000-000000000001";
    store.create_run_dirs(run_id).unwrap();
    let parties = ["Party A", "Party B", "Party C", "Party D", "Party E"];
    let legacy_result = serde_json::json!({
        "result_version": "1.0",
        "run_id": run_id,
        "status": "completed",
        "summary": "legacy",
        "recommendation": "archive",
        "dissent": [],
        "residuals": [],
        "trial_manifest": {
            "manifest_version": "1.0",
            "base_model_relation": "same_model",
            "perspective_count": 5,
            "perspectives": parties.iter().enumerate().map(|(index, party)| serde_json::json!({
                "party_id": party,
                "route_id": format!("legacy-{index}"),
                "r1_artifact": format!("lanes/R1/legacy-{index}/accepted.json"),
                "r2_artifact": format!("lanes/R2/legacy-{index}/accepted.json"),
                "independent_first_pass": true
            })).collect::<Vec<_>>(),
            "perturbation_axes": [],
            "independence_controls": [],
            "contamination_risks": [],
            "wall_time_seconds": null
        }
    });
    let result_path = store.run_dir(run_id).unwrap().join("result.json");
    write_json(&result_path, &legacy_result).unwrap();
    let original_result = fs::read(&result_path).unwrap();
    let now = "2026-07-13T00:00:00.000Z";
    let legacy_manifest = serde_json::json!({
        "manifest_version": "1.0",
        "run_id": run_id,
        "created_at": now,
        "updated_at": now,
        "status": "completed",
        "brief_sha256": format!("sha256:{}", "a".repeat(64)),
        "policy_sha256": format!("sha256:{}", "b".repeat(64)),
        "snapshot_sha256": format!("sha256:{}", "c".repeat(64)),
        "runtime_sha256": format!("sha256:{}", "d".repeat(64)),
        "protocol_version": "1.0",
        "effective_model": "mimo-v2.5-pro",
        "sandbox_mode": "process",
        "current_phase": "R3",
        "error": null,
        "r3_input_receipt": null,
        "primary_arbiter_challenge": null,
        "primary_arbiter_submission": null,
        "result_sha256": sha256_file(&result_path).unwrap()
    });
    let manifest_path = store.manifest_path(run_id).unwrap();
    write_json(&manifest_path, &legacy_manifest).unwrap();
    let original_manifest = fs::read(&manifest_path).unwrap();

    let integrity = run::verify_result_integrity(&store, run_id)
        .unwrap()
        .unwrap();
    assert_eq!(integrity.contract_version, "1.0");
    assert!(!integrity.actionable);
    assert_eq!(fs::read(&result_path).unwrap(), original_result);
    assert_eq!(fs::read(&manifest_path).unwrap(), original_manifest);
}

#[test]
fn verdict_submission_constructs_scheduler_owned_binding_envelope() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "verdict-submit");
    let verdict_path = temporary.path().join("primary-arbiter-verdict.json");
    write_json(&verdict_path, &response.verdict).unwrap();

    assert_eq!(
        run::submit_primary_arbiter_verdict(&store, &run_id, &verdict_path, false).unwrap(),
        RunStatus::Completed
    );
    let owned: PrimaryArbiterResponse = read_json(
        &store
            .run_dir(&run_id)
            .unwrap()
            .join("r3/primary-arbiter-response.json"),
    )
    .unwrap();
    let challenge = store
        .load_manifest(&run_id)
        .unwrap()
        .primary_arbiter_challenge
        .unwrap();
    assert_eq!(owned.input_receipt_sha256, challenge.input_receipt_sha256);
    assert_eq!(owned.nonce, challenge.nonce);
}

#[test]
fn degenerate_verdict_submission_is_rejected_without_force() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, _) = create_waiting_run(temporary.path(), &executable, "degenerate");
    let stub = temporary.path().join("stub-verdict.json");
    // schema 合法但明显是 stub：空 residuals + 极短文本
    fs::write(
        &stub,
        r#"{"arbiter_verdict_version":"1.0","summary":"test","recommendation":"ok","residuals":[]}"#,
    )
    .unwrap();

    let error = run::submit_primary_arbiter_verdict(&store, &run_id, &stub, false).unwrap_err();
    assert!(error.to_string().contains("degenerate"), "{error}");
    assert!(error.to_string().contains("--force"), "{error}");
    // 护栏拒绝后 run 保持等待态，不产生 result
    assert_eq!(
        store.load_manifest(&run_id).unwrap().status,
        RunStatus::WaitingPrimaryArbiter
    );
    assert!(!store.run_dir(&run_id).unwrap().join("result.json").exists());

    // --force 是显式操作员决定：同一 stub 可以 finalize
    assert_eq!(
        run::submit_primary_arbiter_verdict(&store, &run_id, &stub, true).unwrap(),
        RunStatus::Completed
    );
    assert!(
        store
            .run_dir(&run_id)
            .unwrap()
            .join("result.json")
            .is_file()
    );
}

#[test]
fn verdict_schema_error_is_not_reported_as_a_syntax_error() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, _) = create_waiting_run(temporary.path(), &executable, "schema-msg");
    let broken = temporary.path().join("broken-verdict.json");
    // 合法 JSON，但 residual 缺 evidence_refs 等必填字段
    fs::write(
        &broken,
        r#"{"arbiter_verdict_version":"1.0","summary":"A long enough summary text.","recommendation":"A long enough recommendation.","residuals":[{"id":"r1","severity":"LOW"}]}"#,
    )
    .unwrap();

    let error = run::submit_primary_arbiter_verdict(&store, &run_id, &broken, false).unwrap_err();
    let detail = format!("{error:#}");
    assert!(
        detail.contains("does not match expected schema"),
        "{detail}"
    );
    assert!(!detail.contains("invalid JSON syntax"), "{detail}");
    assert_eq!(
        store.load_manifest(&run_id).unwrap().status,
        RunStatus::WaitingPrimaryArbiter
    );
}

#[test]
fn invalid_verdict_fails_before_any_staging_side_effects() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, _) = create_waiting_run(temporary.path(), &executable, "pre-stage");
    let bad = temporary.path().join("bad-ref-verdict.json");
    // schema-valid verdict whose closure_evidence points at the filesystem
    // instead of snapshot:// refs — must fail semantic validation early.
    fs::write(
        &bad,
        r#"{"arbiter_verdict_version":"1.0","summary":"A long enough summary text.","recommendation":"A long enough recommendation.","residuals":[{"id":"r1","severity":"LOW","residual_type":"evidence-gap","source":"test","finding":"f","evidence_refs":[],"disposition":"verified","required_closure":"done","closure_state":"closed","closure_evidence":["/tmp/not-a-snapshot.txt"],"scope":"s"}]}"#,
    )
    .unwrap();

    let error = run::submit_primary_arbiter_verdict(&store, &run_id, &bad, false).unwrap_err();
    assert!(
        format!("{error:#}").contains("unresolvable evidence reference"),
        "{error:#}"
    );
    // No receipt staged, no response file written — a retry with a fixed
    // verdict must find the challenge untouched.
    let manifest = store.load_manifest(&run_id).unwrap();
    assert!(manifest.primary_arbiter_submission.is_none());
    assert!(
        !store
            .run_dir(&run_id)
            .unwrap()
            .join("r3/primary-arbiter-response.json")
            .exists()
    );
    assert_eq!(manifest.status, RunStatus::WaitingPrimaryArbiter);
}

fn completed_run(
    temporary: &std::path::Path,
    executable: &std::path::Path,
    suffix: &str,
) -> (Store, String) {
    let (store, run_id, response) = create_waiting_run(temporary, executable, suffix);
    let response_path = temporary.join(format!("{suffix}-response.json"));
    write_json(&response_path, &response).unwrap();
    assert_eq!(
        run::submit_primary_arbiter(&store, &run_id, &response_path).unwrap(),
        RunStatus::Completed
    );
    (store, run_id)
}

fn replacement_verdict(summary: &str, recommendation: &str) -> ArbiterVerdict {
    ArbiterVerdict {
        arbiter_verdict_version: "1.0".into(),
        summary: summary.into(),
        recommendation: recommendation.into(),
        residuals: vec![],
    }
}

#[test]
fn amend_rewrites_result_for_a_completed_run() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id) = completed_run(temporary.path(), &executable, "amend-rewrite");
    let run_dir = store.run_dir(&run_id).unwrap();
    let old_result = fs::read_to_string(run_dir.join("result.json")).unwrap();
    let owned_response = fs::read(run_dir.join("r3/primary-arbiter-response.json")).unwrap();
    let old_report = fs::read_to_string(run_dir.join("report.md")).unwrap();

    let verdict_path = temporary.path().join("amend-verdict.json");
    let verdict = replacement_verdict(
        "Amended summary: the original verdict was a stub and is now replaced.",
        "Amended recommendation: track every counterpart residual to closure.",
    );
    write_json(&verdict_path, &verdict).unwrap();

    assert_eq!(
        run::amend_primary_arbiter_verdict(&store, &run_id, &verdict_path, false).unwrap(),
        RunStatus::Completed
    );

    // result.json 与 report.md 由同一条 finalize 路径重写
    let result: serde_json::Value = read_json(&run_dir.join("result.json")).unwrap();
    assert_eq!(result["summary"], verdict.summary);
    assert_eq!(result["recommendation"], verdict.recommendation);
    assert_ne!(
        fs::read_to_string(run_dir.join("result.json")).unwrap(),
        old_result
    );
    let report = fs::read_to_string(run_dir.join("report.md")).unwrap();
    assert!(report.contains(&verdict.summary), "{report}");
    assert_ne!(report, old_report);

    // manifest 摘要更新，完整性校验通过
    let manifest = store.load_manifest(&run_id).unwrap();
    assert_eq!(manifest.status, RunStatus::Completed);
    assert_eq!(
        manifest.result_sha256.as_deref(),
        Some(sha256_file(&run_dir.join("result.json")).unwrap().as_str())
    );
    let integrity = run::verify_result_integrity(&store, &run_id)
        .unwrap()
        .unwrap();
    assert!(integrity.actionable);

    // 审计 event 带 verdict 文件 sha256；r3/ 目录保持原样
    let events = store.events(&run_id).unwrap();
    let amended = events
        .iter()
        .find(|event| event.event_type == "primary_arbiter.amended")
        .expect("amend must append an audit event");
    assert_eq!(
        amended.data["verdict_sha256"].as_str().unwrap(),
        sha256_file(&verdict_path).unwrap()
    );
    assert_eq!(
        fs::read(run_dir.join("r3/primary-arbiter-response.json")).unwrap(),
        owned_response
    );
}

#[test]
fn amend_rejects_a_run_that_is_not_completed() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, _) = create_waiting_run(temporary.path(), &executable, "amend-active");
    let verdict_path = temporary.path().join("amend-active-verdict.json");
    write_json(
        &verdict_path,
        &replacement_verdict(
            "Amended summary with plenty of substance.",
            "Amended recommendation with plenty of substance.",
        ),
    )
    .unwrap();

    let error =
        run::amend_primary_arbiter_verdict(&store, &run_id, &verdict_path, false).unwrap_err();
    assert!(error.to_string().contains("not completed"), "{error}");
    assert!(error.to_string().contains("submit"), "{error}");
    assert!(!store.run_dir(&run_id).unwrap().join("result.json").exists());
}

#[test]
fn amend_enforces_guardrail_and_schema_on_completed_runs() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id) = completed_run(temporary.path(), &executable, "amend-guard");
    let run_dir = store.run_dir(&run_id).unwrap();
    let original_sha = sha256_file(&run_dir.join("result.json")).unwrap();

    // 退化 verdict 无 --force → 拒绝，result 不动
    let stub = temporary.path().join("amend-stub.json");
    fs::write(
        &stub,
        r#"{"arbiter_verdict_version":"1.0","summary":"test","recommendation":"ok","residuals":[]}"#,
    )
    .unwrap();
    let error = run::amend_primary_arbiter_verdict(&store, &run_id, &stub, false).unwrap_err();
    assert!(error.to_string().contains("degenerate"), "{error}");
    assert_eq!(
        sha256_file(&run_dir.join("result.json")).unwrap(),
        original_sha
    );

    // schema 不合法 --force 也不豁免
    let broken = temporary.path().join("amend-broken.json");
    fs::write(
        &broken,
        r#"{"arbiter_verdict_version":"1.0","summary":"test","residuals":[]}"#,
    )
    .unwrap();
    let error = run::amend_primary_arbiter_verdict(&store, &run_id, &broken, true).unwrap_err();
    assert!(
        format!("{error:#}").contains("does not match expected schema"),
        "{error:#}"
    );
    assert_eq!(
        sha256_file(&run_dir.join("result.json")).unwrap(),
        original_sha
    );

    // --force 只豁免护栏：同一 stub 可写入，完整性保持
    assert_eq!(
        run::amend_primary_arbiter_verdict(&store, &run_id, &stub, true).unwrap(),
        RunStatus::Completed
    );
    assert_ne!(
        sha256_file(&run_dir.join("result.json")).unwrap(),
        original_sha
    );
    run::verify_result_integrity(&store, &run_id).unwrap();
}

#[test]
fn preplaced_primary_arbiter_response_cannot_bypass_scheduler_acceptance() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) = create_waiting_run(temporary.path(), &executable, "preplaced");
    write_json(
        &store
            .run_dir(&run_id)
            .unwrap()
            .join("r3/primary-arbiter-response.json"),
        &response,
    )
    .unwrap();

    assert_eq!(
        run::advance(&store, &run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
    );
    let manifest = store.load_manifest(&run_id).unwrap();
    assert!(manifest.primary_arbiter_submission.is_none());
    assert!(!store.run_dir(&run_id).unwrap().join("result.json").exists());
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
        fs::write(store.run_dir(&run_id).unwrap().join(target), b"{}\n").unwrap();

        assert_eq!(
            run::advance(&store, &run_id).unwrap(),
            RunStatus::FailedPolicy
        );
        let manifest = store.load_manifest(&run_id).unwrap();
        assert_eq!(manifest.error.unwrap().code, "integrity_drift");
        assert!(!store.run_dir(&run_id).unwrap().join("result.json").exists());
    }
}

#[test]
fn primary_arbiter_staging_receipt_is_retryable_when_response_write_never_happened() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "staged-no-file");
    let response_path = temporary.path().join("staged-no-file-response.json");
    write_json(&response_path, &response).unwrap();
    let mut manifest = store.load_manifest(&run_id).unwrap();
    manifest.primary_arbiter_submission = Some(PrimaryArbiterSubmissionReceipt {
        submission_receipt_version: "1.0".into(),
        state: PrimaryArbiterSubmissionState::Staging,
        response_ref: "r3/primary-arbiter-response.json".into(),
        response_sha256: sha256_file(&response_path).unwrap(),
        input_receipt_sha256: manifest.r3_input_receipt.as_ref().unwrap().sha256.clone(),
        staged_at: manifest.updated_at.clone(),
        accepted_at: None,
    });
    store.save_manifest(&manifest).unwrap();

    assert_eq!(
        run::submit_primary_arbiter(&store, &run_id, &response_path).unwrap(),
        RunStatus::Completed
    );
}

#[test]
fn primary_arbiter_staged_file_is_recovered_without_resubmission() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "staged-file");
    let response_path = store
        .run_dir(&run_id)
        .unwrap()
        .join("r3/primary-arbiter-response.json");
    write_json(&response_path, &response).unwrap();
    let mut manifest = store.load_manifest(&run_id).unwrap();
    manifest.primary_arbiter_submission = Some(PrimaryArbiterSubmissionReceipt {
        submission_receipt_version: "1.0".into(),
        state: PrimaryArbiterSubmissionState::Staging,
        response_ref: "r3/primary-arbiter-response.json".into(),
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
            .primary_arbiter_submission
            .unwrap()
            .state,
        PrimaryArbiterSubmissionState::Accepted
    );
}

#[test]
fn legacy_hm_staged_file_is_recovered_without_rewrite() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "legacy-hm-staged-file");
    let response_path = store.run_dir(&run_id).unwrap().join("r3/hm-response.json");
    let mut legacy = serde_json::to_value(&response).unwrap();
    let version = legacy
        .as_object_mut()
        .unwrap()
        .remove("primary_arbiter_response_version")
        .unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .insert("hm_response_version".into(), version);
    write_json(&response_path, &legacy).unwrap();
    let response_bytes = fs::read(&response_path).unwrap();
    let mut manifest = store.load_manifest(&run_id).unwrap();
    manifest.primary_arbiter_submission = Some(PrimaryArbiterSubmissionReceipt {
        submission_receipt_version: "1.0".into(),
        state: PrimaryArbiterSubmissionState::Staging,
        response_ref: "r3/hm-response.json".into(),
        response_sha256: sha256_file(&response_path).unwrap(),
        input_receipt_sha256: manifest.r3_input_receipt.as_ref().unwrap().sha256.clone(),
        staged_at: manifest.updated_at.clone(),
        accepted_at: None,
    });
    store.save_manifest(&manifest).unwrap();

    assert_eq!(run::advance(&store, &run_id).unwrap(), RunStatus::Completed);
    assert_eq!(fs::read(&response_path).unwrap(), response_bytes);
    assert!(
        !store
            .run_dir(&run_id)
            .unwrap()
            .join("r3/primary-arbiter-response.json")
            .exists()
    );
    assert_eq!(
        store
            .load_manifest(&run_id)
            .unwrap()
            .primary_arbiter_submission
            .unwrap()
            .response_ref,
        "r3/hm-response.json"
    );
}

#[test]
fn primary_arbiter_recovery_rejects_ambiguous_current_and_legacy_files() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "ambiguous-arbiter-files");
    let run_dir = store.run_dir(&run_id).unwrap();
    let current_path = run_dir.join("r3/primary-arbiter-response.json");
    let legacy_path = run_dir.join("r3/hm-response.json");
    write_json(&current_path, &response).unwrap();
    write_json(&legacy_path, &serde_json::to_value(&response).unwrap()).unwrap();
    let mut manifest = store.load_manifest(&run_id).unwrap();
    manifest.primary_arbiter_submission = Some(PrimaryArbiterSubmissionReceipt {
        submission_receipt_version: "1.0".into(),
        state: PrimaryArbiterSubmissionState::Staging,
        response_ref: "r3/primary-arbiter-response.json".into(),
        response_sha256: sha256_file(&current_path).unwrap(),
        input_receipt_sha256: manifest.r3_input_receipt.as_ref().unwrap().sha256.clone(),
        staged_at: manifest.updated_at.clone(),
        accepted_at: None,
    });
    store.save_manifest(&manifest).unwrap();

    assert_eq!(
        run::advance(&store, &run_id).unwrap(),
        RunStatus::FailedPolicy
    );
    assert_eq!(
        store.load_manifest(&run_id).unwrap().error.unwrap().code,
        "integrity_drift"
    );
    assert!(!run_dir.join("result.json").exists());
}

#[test]
fn accepted_primary_arbiter_submission_resumes_after_expiry_and_is_idempotent() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let (store, run_id, response) =
        create_waiting_run(temporary.path(), &executable, "accepted-crash");
    let internal_response = store
        .run_dir(&run_id)
        .unwrap()
        .join("r3/primary-arbiter-response.json");
    let external_response = temporary.path().join("accepted-crash-response.json");
    write_json(&internal_response, &response).unwrap();
    write_json(&external_response, &response).unwrap();
    let mut manifest = store.load_manifest(&run_id).unwrap();
    manifest
        .primary_arbiter_challenge
        .as_mut()
        .unwrap()
        .consumed = true;
    manifest
        .primary_arbiter_challenge
        .as_mut()
        .unwrap()
        .expires_at = "2000-01-01T00:00:00Z".into();
    manifest.primary_arbiter_submission = Some(PrimaryArbiterSubmissionReceipt {
        submission_receipt_version: "1.0".into(),
        state: PrimaryArbiterSubmissionState::Accepted,
        response_ref: "r3/primary-arbiter-response.json".into(),
        response_sha256: sha256_file(&internal_response).unwrap(),
        input_receipt_sha256: manifest.r3_input_receipt.as_ref().unwrap().sha256.clone(),
        staged_at: manifest.updated_at.clone(),
        accepted_at: Some(manifest.updated_at.clone()),
    });
    store.save_manifest(&manifest).unwrap();

    assert_eq!(run::advance(&store, &run_id).unwrap(), RunStatus::Completed);
    assert_eq!(
        run::submit_primary_arbiter(&store, &run_id, &external_response).unwrap(),
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
            brief_version: BRIEF_VERSION.into(),
            question: "Can an active run be cancelled safely?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
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
            brief_version: BRIEF_VERSION.into(),
            question: "Do failed parallel lanes retain scheduler ownership?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::Failed
    );
    assert!(store.active_pids(&created.run_id).unwrap().is_empty());
    let events =
        fs::read_to_string(store.run_dir(&created.run_id).unwrap().join("events.jsonl")).unwrap();
    for index in 1..5 {
        assert!(
            store
                .run_dir(&created.run_id)
                .unwrap()
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does QUINTE cap child output while reading it?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
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
        .unwrap()
        .join("lanes/R1/fake-0/attempt-1/stdout.bin");
    assert!(fs::metadata(stdout).unwrap().len() <= policy.max_output_bytes as u64);
    let events =
        fs::read_to_string(store.run_dir(&created.run_id).unwrap().join("events.jsonl")).unwrap();
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does the scheduler recover a typed R2 rate limit?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .unwrap()
            .join("lanes/R2/fake-0/attempt-2/stdout.bin")
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(temporary.path().join("fake-agent-rate-limit-count"))
            .unwrap()
            .trim(),
        "2"
    );
    let events =
        fs::read_to_string(store.run_dir(&created.run_id).unwrap().join("events.jsonl")).unwrap();
    assert!(events.contains("lane.retry_scheduled"));
    assert!(events.contains("\"failure_class\":\"rate_limit\""));
    assert!(events.contains("lane.retry_started"));
    assert!(events.contains("r2.pacing_wait"));
}

#[test]
fn r2_parallel_fans_out_lanes_without_serial_pacing() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    // Barrier probe: every R2 lane registers its start and then blocks until
    // all five parties have registered. Serial scheduling cannot release the
    // barrier before its 30s deadlock timeout, so a fast advance proves the
    // parallel fan-out actually happened.
    fs::write(temporary.path().join("fake-agent-r2-barrier"), "5\n").unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.r2_parallel = true;
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: BRIEF_VERSION.into(),
            question: "Does r2_parallel fan out the cross-examination?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();

    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let started_at = Instant::now();
    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
    );
    assert!(
        started_at.elapsed() < Duration::from_secs(25),
        "R2 barrier must be released by concurrent lane starts, not by its serial deadlock timeout"
    );

    let run_dir = store.run_dir(&created.run_id).unwrap();
    for index in 0..5 {
        assert!(
            run_dir
                .join(format!("lanes/R2/fake-{index}/accepted.json"))
                .is_file()
        );
    }
    let registered =
        fs::read_to_string(temporary.path().join("fake-agent-r2-barrier-started")).unwrap();
    for party in ["Party A", "Party B", "Party C", "Party D", "Party E"] {
        assert!(registered.contains(party), "R2 lane {party} never started");
    }
    // Parallel R2 replaces serial pacing with the fan-out soft-stagger: no
    // pacing state file and no pacing events may be produced.
    let events = fs::read_to_string(run_dir.join("events.jsonl")).unwrap();
    assert!(!events.contains("r2.pacing_wait"));
    assert!(!run_dir.join("diagnostics/r2-rate-state.json").exists());
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does a typed MiMo repetition failure recover?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .unwrap()
            .join("lanes/R1/fake-3/attempt-2/stdout.bin")
            .is_file()
    );
    assert!(
        store
            .run_dir(&created.run_id)
            .unwrap()
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does bounded retry stop?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
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
            .unwrap()
            .join("lanes/R1/fake-3/attempt-2/stdout.bin")
            .is_file()
    );
    assert!(
        !store
            .run_dir(&created.run_id)
            .unwrap()
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
            brief_version: BRIEF_VERSION.into(),
            question: "Can a complete output be recovered at timeout?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
    );
    let lane_dir = store
        .run_dir(&created.run_id)
        .unwrap()
        .join("lanes/R1/fake-0");
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
            brief_version: BRIEF_VERSION.into(),
            question: "Can invalid evidence be marked accepted?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
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
fn completed_codewhale_with_a_truncated_final_candidate_retries_on_the_same_route() {
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does a completed CodeWhale stream retry a truncated final candidate?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
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
fn r3_counterpart_arbiter_timeout_uses_the_same_bounded_retry_policy() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-timeout-once-party"),
        "Counterpart Arbiter\n",
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does the R3 counterpart arbiter recover from a transient timeout?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
    );
    let events = store.events(&created.run_id).unwrap();
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.phase.as_deref() == Some("R3")
            && event.party_id.as_deref() == Some("Counterpart Arbiter")
            && event.attempt == Some(1)
            && event.data["failure_class"] == "timeout"
            && event.data["retryable"] == true
    }));
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.phase.as_deref() == Some("R3")
            && event.party_id.as_deref() == Some("Counterpart Arbiter")
            && event.attempt == Some(2)
            && event.data["accepted"] == true
    }));
    assert!(
        store
            .run_dir(&created.run_id)
            .unwrap()
            .join("r3/cc-response.json")
            .is_file()
    );
}

#[test]
fn automatic_primary_arbiter_retries_an_empty_completed_verdict_in_its_own_attempt_tree() {
    let _fake_env = FakeAdapterEnv::enable();
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    fs::write(
        temporary.path().join("fake-agent-empty-once-party"),
        "Primary Arbiter\n",
    )
    .unwrap();
    let home = temporary.path().join("home");
    let store = Store::new(home.clone());
    fs::create_dir_all(&home).unwrap();
    let mut policy = fake_policy(&executable);
    policy.auto_primary_arbiter = true;
    policy.max_attempts = 2;
    policy.primary_arbiter.adapter = "fake_envelope".into();
    write_json(&store.policy_path(), &policy).unwrap();
    let evidence = temporary.path().join("evidence.txt");
    fs::write(&evidence, "bounded evidence\n").unwrap();
    let brief_path = temporary.path().join("brief.json");
    write_json(
        &brief_path,
        &Brief {
            brief_version: BRIEF_VERSION.into(),
            question: "Does automatic PA retry an empty terminal verdict?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();

    let status = run::advance(&store, &created.run_id).unwrap();
    let run_dir = store.run_dir(&created.run_id).unwrap();
    let manifest = store.load_manifest(&created.run_id).unwrap();
    let events = fs::read_to_string(run_dir.join("events.jsonl")).unwrap_or_default();
    assert_eq!(
        status,
        RunStatus::Completed,
        "manifest={manifest:?} events={events}"
    );
    assert!(
        run_dir
            .join("lanes/R3/fake-pa/attempt-1/stdout.bin")
            .is_file()
    );
    assert!(
        run_dir
            .join("lanes/R3/fake-pa/attempt-2/stdout.bin")
            .is_file()
    );
    assert!(
        run_dir
            .join("lanes/R3/fake-cc/attempt-1/stdout.bin")
            .is_file()
    );
    assert!(!run_dir.join("lanes/R3/fake-cc/attempt-2").exists());
    let events = store.events(&created.run_id).unwrap();
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.party_id.as_deref() == Some("Primary Arbiter")
            && event.attempt == Some(1)
            && event.data["failure_class"] == "transient_adapter"
            && event.data["retryable"] == true
    }));
    assert!(events.iter().any(|event| {
        event.event_type == "lane.finished"
            && event.party_id.as_deref() == Some("Primary Arbiter")
            && event.attempt == Some(2)
            && event.data["accepted"] == true
    }));
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does resume preserve the attempt budget?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let run_dir = store.run_dir(&created.run_id).unwrap();

    // A crash after creating an attempt directory still consumes that attempt.
    fs::create_dir_all(run_dir.join("lanes/R1/fake-0/attempt-1")).unwrap();
    fs::create_dir_all(run_dir.join("lanes/R3/fake-cc/attempt-1")).unwrap();

    assert_eq!(
        run::advance(&store, &created.run_id).unwrap(),
        RunStatus::WaitingPrimaryArbiter
    );
    assert!(
        run_dir
            .join("lanes/R1/fake-0/attempt-2/stdout.bin")
            .is_file()
    );
    assert!(
        run_dir
            .join("lanes/R3/fake-cc/attempt-2/stdout.bin")
            .is_file()
    );
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
            && event.party_id.as_deref() == Some("Counterpart Arbiter")
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does resume preserve a pending retry cooldown?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let run_dir = store.run_dir(&created.run_id).unwrap();
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
        RunStatus::WaitingPrimaryArbiter
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
fn resume_honors_a_persisted_r3_retry_deadline_before_starting_the_counterpart_arbiter() {
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
            brief_version: BRIEF_VERSION.into(),
            question: "Does resume preserve the counterpart arbiter retry cooldown?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let run_dir = store.run_dir(&created.run_id).unwrap();
    let lane_dir = run_dir.join("lanes/R3/fake-cc");
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
        RunStatus::WaitingPrimaryArbiter
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
                && event.party_id.as_deref() == Some("Counterpart Arbiter")
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
            brief_version: BRIEF_VERSION.into(),
            question: "Can resume bypass an exhausted attempt budget?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test only".into()),
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        },
    )
    .unwrap();
    let created = run::create(&store, &policy, &RunOptions { brief_path }).unwrap();
    let lane_dir = store
        .run_dir(&created.run_id)
        .unwrap()
        .join("lanes/R1/fake-0");
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
            .unwrap()
            .join("lanes/R1/fake-0/accepted.json")
            .is_file()
    );
    assert!(
        !store
            .run_dir(&run_id)
            .unwrap()
            .join("lanes/R1/fake-0/attempt-2")
            .exists()
    );
    let events = fs::read_to_string(store.run_dir(&run_id).unwrap().join("events.jsonl")).unwrap();
    assert!(!events.contains("lane.retry_scheduled"));
}
