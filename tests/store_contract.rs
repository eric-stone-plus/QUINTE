use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use quinte::model::{RunManifest, RunStatus, SandboxMode};
use quinte::store::Store;
use serde_json::{Value, json};

fn manifest(run_id: &str) -> RunManifest {
    RunManifest {
        manifest_version: "0.1.4".into(),
        run_id: run_id.into(),
        created_at: "2026-07-13T00:00:00.000Z".into(),
        updated_at: "2026-07-13T00:00:00.000Z".into(),
        status: RunStatus::Queued,
        brief_sha256: format!("sha256:{}", "a".repeat(64)),
        policy_sha256: format!("sha256:{}", "b".repeat(64)),
        snapshot_sha256: format!("sha256:{}", "c".repeat(64)),
        runtime_sha256: format!("sha256:{}", "d".repeat(64)),
        protocol_version: "0.1.4".into(),
        effective_model: "mimo-v2.5-pro".into(),
        sandbox_mode: SandboxMode::Process,
        current_phase: None,
        error: None,
        r3_input_receipt: None,
        hm_challenge: None,
        hm_submission: None,
        result_sha256: None,
    }
}

#[test]
fn create_run_dirs_is_complete_and_refuses_reuse() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    let run_dir = store.create_run_dirs("run-1").unwrap();

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
    assert!(store.create_run_dirs("run-1").is_err());
}

#[test]
fn transition_persists_manifest_and_appends_ordered_events() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    store.create_run_dirs("run-1").unwrap();
    let mut current = manifest("run-1");
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
            "run-1",
            "lane.started",
            Some("R1"),
            Some("Party A"),
            Some(1),
            json!({"route_id": "codewhale"}),
        )
        .unwrap();

    let saved = store.load_manifest("run-1").unwrap();
    assert_eq!(saved.status, RunStatus::Preflight);
    assert_eq!(saved.current_phase.as_deref(), Some("preflight"));
    assert_ne!(saved.updated_at, "2026-07-13T00:00:00.000Z");
    assert_eq!(second.sequence, 2);

    let events_path = store.run_dir("run-1").join("events.jsonl");
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
    store.create_run_dirs("run-1").unwrap();

    let first = store.lock("run-1").unwrap();
    let error = store.lock("run-1").err().expect("second lock must fail");
    assert!(error.to_string().contains("already being advanced"));

    drop(first);
    let reacquired = store.lock("run-1").unwrap();
    drop(reacquired);
}

#[test]
fn artifact_write_and_manifest_listing_are_typed_and_stable() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    for (run_id, created_at) in [
        ("older", "2026-07-12T00:00:00.000Z"),
        ("newer", "2026-07-13T00:00:00.000Z"),
    ] {
        store.create_run_dirs(run_id).unwrap();
        let mut value = manifest(run_id);
        value.created_at = created_at.into();
        store.save_manifest(&value).unwrap();
    }

    let artifact = store
        .write_artifact("newer", "diagnostics/sample.json", &json!({"ok": true}))
        .unwrap();
    let decoded: Value = serde_json::from_slice(&fs::read(artifact).unwrap()).unwrap();
    assert_eq!(decoded, json!({"ok": true}));

    let manifests = store.list_manifests().unwrap();
    assert_eq!(manifests.len(), 2);
    assert_eq!(manifests[0].run_id, "newer");
    assert_eq!(manifests[1].run_id, "older");
}

#[test]
fn concurrent_events_have_unique_contiguous_sequences_and_valid_jsonl() {
    const THREADS: usize = 12;
    const EVENTS_PER_THREAD: usize = 25;

    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join(".quinte");
    let store = Store::new(home.clone());
    store.create_run_dirs("run-1").unwrap();
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
                            "run-1",
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

    let events = fs::read_to_string(store.run_dir("run-1").join("events.jsonl"))
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
    store.create_run_dirs("run-1").unwrap();
    let barrier = Arc::new(Barrier::new(PIDS as usize));

    let handles = (1..=PIDS)
        .map(|pid| {
            let home = home.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let store = Store::new(home);
                barrier.wait();
                store.add_active_pid("run-1", pid).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    assert_eq!(
        store.active_pids("run-1").unwrap(),
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
                store.remove_active_pid("run-1", pid).unwrap();
            })
        })
        .collect::<Vec<_>>();
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(store.active_pids("run-1").unwrap().is_empty());
}

#[test]
fn cancellation_marker_wins_over_a_late_worker_failure() {
    let temporary = tempfile::tempdir().unwrap();
    let home = temporary.path().join(".quinte");
    let store = Store::new(home.clone());
    store.create_run_dirs("run-1").unwrap();
    let mut initial = manifest("run-1");
    initial.status = RunStatus::R1Running;
    initial.current_phase = Some("R1".into());
    store.save_manifest(&initial).unwrap();

    let mut canceller = store.load_manifest("run-1").unwrap();
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

    let saved = store.load_manifest("run-1").unwrap();
    assert_eq!(saved.status, RunStatus::Cancelled);
    assert_eq!(saved.error.as_ref().unwrap().code, "cancelled");

    let events = fs::read_to_string(store.run_dir("run-1").join("events.jsonl")).unwrap();
    assert!(events.contains("\"requested\":\"failed\""));
    assert!(events.contains("\"to\":\"cancelled\""));
}

#[test]
fn terminal_cancelled_manifest_rejects_late_nonterminal_save() {
    let temporary = tempfile::tempdir().unwrap();
    let store = Store::new(temporary.path().join(".quinte"));
    store.create_run_dirs("run-1").unwrap();
    let mut stale = manifest("run-1");
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
        store.load_manifest("run-1").unwrap().status,
        RunStatus::Cancelled
    );
}
