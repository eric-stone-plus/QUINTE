use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use quinte::model::{RunManifest, RunStatus, SandboxMode};
use quinte::store::{Store, validate_run_id};
use serde_json::{Value, json};

const RUN_ID: &str = "019bf52a-73b0-7000-8000-000000000001";
const OLD_ID: &str = "019bf52a-73b0-7000-8000-000000000002";
const NEW_ID: &str = "019bf52a-73b0-7000-8000-000000000003";
const LEGACY_ID: &str = "019bf52a-73b0-7000-8000-000000000004";
const VALID_ID: &str = "019bf52a-73b0-7000-8000-000000000005";
const CORRUPT_ID: &str = "019bf52a-73b0-7000-8000-000000000006";

#[test]
fn run_ids_are_canonical_uuid_v7_and_cannot_escape_the_runs_root() {
    assert!(validate_run_id(RUN_ID).is_ok());
    for invalid in [
        "../escape",
        "/absolute",
        r"..\escape",
        ".",
        "",
        "run-1",
        "550e8400-e29b-41d4-a716-446655440000",
        "019BF52A-73B0-7000-8000-000000000001",
        "019bf52a-73b0-6000-8000-000000000001",
        "019bf52a-73b0-7000-0000-000000000001",
    ] {
        assert!(validate_run_id(invalid).is_err(), "accepted {invalid:?}");
    }

    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join("home"));
    for invalid in ["../escape", "/absolute", r"..\escape", ".", "run-1"] {
        assert!(store.run_dir(invalid).is_err());
        assert!(store.create_run_dirs(invalid).is_err());
        assert!(store.load_manifest(invalid).is_err());
    }
    assert!(!temporary.path().join("escape").exists());
}

#[test]
fn generic_artifact_writes_reject_path_traversal() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join("home"));
    store.create_run_dirs(RUN_ID).unwrap();
    for invalid in ["../escape.json", "/absolute.json", r"..\escape.json", "."] {
        assert!(
            store
                .write_artifact(RUN_ID, invalid, &json!({"unsafe": true}))
                .is_err(),
            "accepted {invalid:?}"
        );
    }
    assert!(!temporary.path().join("home/escape.json").exists());
}

#[test]
fn manifest_run_id_must_match_the_validated_directory_id() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join("home"));
    store.create_run_dirs(RUN_ID).unwrap();
    let mut wrong = manifest(OLD_ID);
    wrong.created_at = "2026-07-12T00:00:00.000Z".into();
    quinte::util::write_json(&store.manifest_path(RUN_ID).unwrap(), &wrong).unwrap();
    assert!(store.load_manifest(RUN_ID).is_err());
    assert!(store.list_manifests().unwrap().is_empty());
}

fn manifest(run_id: &str) -> RunManifest {
    RunManifest {
        manifest_version: "1.0".into(),
        run_id: run_id.into(),
        created_at: "2026-07-13T00:00:00.000Z".into(),
        updated_at: "2026-07-13T00:00:00.000Z".into(),
        status: RunStatus::Queued,
        brief_sha256: format!("sha256:{}", "a".repeat(64)),
        policy_sha256: format!("sha256:{}", "b".repeat(64)),
        snapshot_sha256: format!("sha256:{}", "c".repeat(64)),
        runtime_sha256: format!("sha256:{}", "d".repeat(64)),
        protocol_version: "1.0".into(),
        effective_model: "mimo-v2.5-pro".into(),
        sandbox_mode: SandboxMode::Process,
        current_phase: None,
        error: None,
        r3_input_receipt: None,
        primary_arbiter_challenge: None,
        primary_arbiter_submission: None,
        result_sha256: None,
    }
}

#[test]
fn create_run_dirs_is_complete_and_refuses_reuse() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    let run_dir = store.create_run_dirs(RUN_ID).unwrap();

    for relative in [
        "input/snapshot",
        "input/attachments",
        "lanes/R1",
        "lanes/R2",
        "packets",
        "r3",
        "diagnostics",
    ] {
        assert!(run_dir.join(relative).is_dir(), "missing {relative}");
    }
    assert!(store.create_run_dirs(RUN_ID).is_err());
}

#[cfg(unix)]
#[test]
fn state_root_run_directories_and_files_are_private() {
    use std::os::unix::fs::MetadataExt;

    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join(".quinte");
    let store = Store::new(home.clone());
    let run_dir = store.create_run_dirs(RUN_ID).unwrap();
    store.save_manifest(&manifest(RUN_ID)).unwrap();
    store
        .write_artifact(RUN_ID, "diagnostics/private.json", &json!({"ok": true}))
        .unwrap();

    for directory in [
        &home,
        &store.runs_dir(),
        &run_dir,
        &run_dir.join("diagnostics"),
    ] {
        assert_eq!(
            fs::metadata(directory).unwrap().mode() & 0o777,
            0o700,
            "{} was not private",
            directory.display()
        );
    }
    for file in [
        store.manifest_path(RUN_ID).unwrap(),
        run_dir.join("diagnostics/private.json"),
    ] {
        assert_eq!(
            fs::metadata(&file).unwrap().mode() & 0o777,
            0o600,
            "{} was not private",
            file.display()
        );
    }
}

#[test]
fn transition_persists_manifest_and_appends_ordered_events() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    store.create_run_dirs(RUN_ID).unwrap();
    let mut current = manifest(RUN_ID);
    store.save_manifest(&current).unwrap();

    store
        .transition(
            &mut current,
            RunStatus::Preflight,
            Some("preflight"),
            json!({"snapshot": "accepted"}),
        )
        .unwrap();
    let second = store
        .event(
            RUN_ID,
            "lane.started",
            Some("R1"),
            Some("Party A"),
            Some(1),
            json!({"route_id": "codewhale"}),
        )
        .unwrap();

    let saved = store.load_manifest(RUN_ID).unwrap();
    assert_eq!(saved.status, RunStatus::Preflight);
    assert_eq!(saved.current_phase.as_deref(), Some("preflight"));
    assert_ne!(saved.updated_at, "2026-07-13T00:00:00.000Z");
    assert_eq!(second.sequence, 2);

    let events_path = store.run_dir(RUN_ID).unwrap().join("events.jsonl");
    let events = fs::read_to_string(events_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["sequence"], 1);
    assert_eq!(events[0]["event_type"], "run.transition");
    assert_eq!(events[0]["data"]["from"], "queued");
    assert_eq!(events[0]["data"]["to"], "preflight");
    assert_eq!(events[1]["sequence"], 2);
    assert_eq!(events[1]["party_id"], "Party A");
    assert_eq!(events[1]["attempt"], 1);
}

#[test]
fn run_lock_is_exclusive_and_released_on_drop() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    store.create_run_dirs(RUN_ID).unwrap();

    let first = store.lock(RUN_ID).unwrap();
    let error = store.lock(RUN_ID).err().expect("second lock must fail");
    assert!(error.to_string().contains("already being advanced"));

    drop(first);
    let reacquired = store.lock(RUN_ID).unwrap();
    drop(reacquired);
}

#[test]
fn artifact_write_and_manifest_listing_are_typed_and_stable() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    for (run_id, created_at) in [
        (OLD_ID, "2026-07-12T00:00:00.000Z"),
        (NEW_ID, "2026-07-13T00:00:00.000Z"),
    ] {
        store.create_run_dirs(run_id).unwrap();
        let mut value = manifest(run_id);
        value.created_at = created_at.into();
        store.save_manifest(&value).unwrap();
    }

    let artifact = store
        .write_artifact(NEW_ID, "diagnostics/sample.json", &json!({"ok": true}))
        .unwrap();
    let decoded: Value = serde_json::from_slice(&fs::read(artifact).unwrap()).unwrap();
    assert_eq!(decoded, json!({"ok": true}));

    let manifests = store.list_manifests().unwrap();
    assert_eq!(manifests.len(), 2);
    assert_eq!(manifests[0].run_id, NEW_ID);
    assert_eq!(manifests[1].run_id, OLD_ID);
}

#[test]
fn legacy_hm_manifest_names_load_without_rewriting_disk_state() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    store.create_run_dirs(LEGACY_ID).unwrap();
    let path = store.manifest_path(LEGACY_ID).unwrap();
    let mut value = serde_json::to_value(manifest(LEGACY_ID)).unwrap();
    value["status"] = json!("waiting_hm");
    let challenge = json!({
        "challenge_version": "1.0",
        "run_id": LEGACY_ID,
        "nonce": "n".repeat(32),
        "policy_sha256": format!("sha256:{}", "b".repeat(64)),
        "evidence_packet_sha256": format!("sha256:{}", "e".repeat(64)),
        "input_receipt_sha256": format!("sha256:{}", "f".repeat(64)),
        "action_scope": null,
        "issued_at": "2026-07-13T00:00:00Z",
        "expires_at": "2026-07-13T01:00:00Z",
        "consumed": false
    });
    let object = value.as_object_mut().unwrap();
    object.remove("primary_arbiter_challenge");
    object.remove("primary_arbiter_submission");
    object.insert("hm_challenge".into(), challenge);
    object.insert("hm_submission".into(), Value::Null);
    let original = serde_json::to_vec_pretty(&value).unwrap();
    fs::write(&path, &original).unwrap();

    let loaded = store.load_manifest(LEGACY_ID).unwrap();
    assert_eq!(loaded.status, RunStatus::WaitingPrimaryArbiter);
    assert_eq!(
        loaded.primary_arbiter_challenge.unwrap().nonce,
        "n".repeat(32)
    );
    assert!(loaded.primary_arbiter_submission.is_none());
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn manifest_listing_skips_one_corrupt_run_without_hiding_valid_runs() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    for run_id in [VALID_ID, CORRUPT_ID] {
        store.create_run_dirs(run_id).unwrap();
    }
    store.save_manifest(&manifest(VALID_ID)).unwrap();
    fs::write(store.manifest_path(CORRUPT_ID).unwrap(), b"{not-json").unwrap();
    let invalid_dir = store.runs_dir().join("not-a-run-id");
    fs::create_dir_all(&invalid_dir).unwrap();
    quinte::util::write_json(&invalid_dir.join("manifest.json"), &manifest(VALID_ID)).unwrap();

    let manifests = store.list_manifests().unwrap();
    assert_eq!(manifests.len(), 1);
    assert_eq!(manifests[0].run_id, VALID_ID);
}

#[test]
fn concurrent_events_have_unique_contiguous_sequences_and_valid_jsonl() {
    const THREADS: usize = 12;
    const EVENTS_PER_THREAD: usize = 25;

    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join(".quinte");
    let store = Store::new(home.clone());
    store.create_run_dirs(RUN_ID).unwrap();
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles = (0..THREADS)
        .map(|worker| {
            let home = home.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let store = Store::new(home);
                barrier.wait();
                for index in 0..EVENTS_PER_THREAD {
                    store
                        .event(
                            RUN_ID,
                            "test.concurrent",
                            Some("R1"),
                            Some("Party A"),
                            Some(1),
                            json!({"worker": worker, "index": index}),
                        )
                        .unwrap();
                }
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }

    let events = fs::read_to_string(store.run_dir(RUN_ID).unwrap().join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.len(), THREADS * EVENTS_PER_THREAD);
    let sequences = events
        .iter()
        .map(|event| event["sequence"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        sequences,
        (1..=(THREADS * EVENTS_PER_THREAD) as u64).collect::<Vec<_>>()
    );
}

#[test]
fn concurrent_active_pid_updates_do_not_lose_entries() {
    const PIDS: u32 = 64;

    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join(".quinte");
    let store = Store::new(home.clone());
    store.create_run_dirs(RUN_ID).unwrap();
    let barrier = Arc::new(Barrier::new(PIDS as usize));

    let handles = (1..=PIDS)
        .map(|pid| {
            let home = home.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let store = Store::new(home);
                barrier.wait();
                store.add_active_pid(RUN_ID, pid).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(
        store.active_pids(RUN_ID).unwrap(),
        (1..=PIDS).collect::<Vec<_>>()
    );

    let barrier = Arc::new(Barrier::new(PIDS as usize));
    let handles = (1..=PIDS)
        .map(|pid| {
            let home = home.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let store = Store::new(home);
                barrier.wait();
                store.remove_active_pid(RUN_ID, pid).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(store.active_pids(RUN_ID).unwrap().is_empty());
}

#[test]
fn cancellation_marker_wins_over_a_late_worker_failure() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join(".quinte");
    let store = Store::new(home.clone());
    store.create_run_dirs(RUN_ID).unwrap();
    let mut initial = manifest(RUN_ID);
    initial.status = RunStatus::R1Running;
    initial.current_phase = Some("R1".into());
    store.save_manifest(&initial).unwrap();

    let mut canceller = store.load_manifest(RUN_ID).unwrap();
    let phase = canceller.current_phase.clone();
    assert_eq!(
        store
            .transition(
                &mut canceller,
                RunStatus::Cancelling,
                phase.as_deref(),
                json!({}),
            )
            .unwrap(),
        RunStatus::Cancelling
    );

    let worker = thread::spawn(move || {
        let store = Store::new(home);
        let mut stale = initial;
        let phase = stale.current_phase.clone();
        store
            .transition(
                &mut stale,
                RunStatus::Failed,
                phase.as_deref(),
                json!({"error": "late worker failure"}),
            )
            .unwrap()
    });
    assert_eq!(worker.join().unwrap(), RunStatus::Cancelled);

    let saved = store.load_manifest(RUN_ID).unwrap();
    assert_eq!(saved.status, RunStatus::Cancelled);
    assert_eq!(saved.error.as_ref().unwrap().code, "cancelled");

    let events = fs::read_to_string(store.run_dir(RUN_ID).unwrap().join("events.jsonl")).unwrap();
    assert!(events.contains("\"requested\":\"failed\""));
    assert!(events.contains("\"to\":\"cancelled\""));
}

#[test]
fn terminal_cancelled_manifest_rejects_late_nonterminal_save() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    store.create_run_dirs(RUN_ID).unwrap();
    let mut stale = manifest(RUN_ID);
    stale.status = RunStatus::R1Running;
    store.save_manifest(&stale).unwrap();

    let mut cancelled = stale.clone();
    let phase = cancelled.current_phase.clone();
    store
        .transition(
            &mut cancelled,
            RunStatus::Cancelling,
            phase.as_deref(),
            json!({}),
        )
        .unwrap();
    store
        .transition(
            &mut cancelled,
            RunStatus::Cancelled,
            phase.as_deref(),
            json!({}),
        )
        .unwrap();

    stale.status = RunStatus::Failed;
    store.save_manifest(&stale).unwrap();
    assert_eq!(
        store.load_manifest(RUN_ID).unwrap().status,
        RunStatus::Cancelled
    );
}
