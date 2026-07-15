use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{Event, RunError, RunManifest, RunStatus};
use crate::schema::{RUN_EVENT_SCHEMA, RUN_MANIFEST_SCHEMA, validate_value};
use crate::util::{append_jsonl, atomic_write, read_json, utc_now, write_json};

pub struct Store {
    home: PathBuf,
}

pub struct RunLock {
    _file: File,
}

pub struct ProcessRegistration {
    _file: File,
    pids_path: PathBuf,
    cancel_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActiveProcess {
    pub pid: u32,
    pub identity: String,
    pub program: String,
}

pub struct CancellationSnapshot {
    pub status: RunStatus,
    pub processes: Vec<ActiveProcess>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PendingTransition {
    transition_id: String,
    timestamp: String,
    from: RunStatus,
    to: RunStatus,
    requested: RunStatus,
    phase: Option<String>,
    detail: Value,
}

pub struct FinalizationGuard {
    _file: File,
    cancel_path: PathBuf,
}

impl FinalizationGuard {
    pub fn cancellation_requested(&self) -> bool {
        self.cancel_path.exists()
    }
}

impl ProcessRegistration {
    pub fn cancellation_requested(&self) -> bool {
        self.cancel_path.exists()
    }

    pub fn add_process(&self, process: ActiveProcess) -> anyhow::Result<()> {
        let mut processes = read_processes(&self.pids_path)?;
        if !processes.iter().any(|entry| entry == &process) {
            processes.push(process);
            processes.sort_by_key(|entry| entry.pid);
        }
        write_json(&self.pids_path, &processes)
    }
}

impl Store {
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn policy_path(&self) -> PathBuf {
        self.home.join("policy.json")
    }

    pub fn runs_dir(&self) -> PathBuf {
        self.home.join("runs")
    }

    pub fn run_dir(&self, run_id: &str) -> PathBuf {
        self.runs_dir().join(run_id)
    }

    pub fn create_run_dirs(&self, run_id: &str) -> anyhow::Result<PathBuf> {
        let run_dir = self.run_dir(run_id);
        if run_dir.exists() {
            bail!("run {run_id} already exists");
        }
        for relative in [
            "input/snapshot",
            "input/attachments",
            "lanes/R1",
            "lanes/R2",
            "packets",
            "r3",
            "diagnostics",
        ] {
            fs::create_dir_all(run_dir.join(relative))?;
        }
        Ok(run_dir)
    }

    pub fn manifest_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id).join("manifest.json")
    }

    pub fn load_manifest(&self, run_id: &str) -> anyhow::Result<RunManifest> {
        let manifest = read_json(&self.manifest_path(run_id))
            .with_context(|| format!("unknown or invalid run {run_id}"))?;
        self.recover_pending_transition(&manifest)?;
        Ok(manifest)
    }

    pub fn save_manifest(&self, manifest: &RunManifest) -> anyhow::Result<()> {
        let _lock = self.resource_lock(&manifest.run_id, ".manifest.lock")?;
        let path = self.manifest_path(&manifest.run_id);
        if path.exists() {
            let current: RunManifest = read_json(&path)?;
            if current.status.terminal()
                || current.status == RunStatus::Cancelling
                || self.cancellation_requested(&manifest.run_id)
            {
                return Ok(());
            }
        }
        self.save_manifest_unlocked(manifest)
    }

    fn save_manifest_unlocked(&self, manifest: &RunManifest) -> anyhow::Result<()> {
        validate_value(&serde_json::to_value(manifest)?, RUN_MANIFEST_SCHEMA)?;
        write_json(&self.manifest_path(&manifest.run_id), manifest)
    }

    pub fn lock(&self, run_id: &str) -> anyhow::Result<RunLock> {
        let path = self.run_dir(run_id).join(".run.lock");
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        file.try_lock_exclusive()
            .with_context(|| format!("run {run_id} is already being advanced"))?;
        Ok(RunLock { _file: file })
    }

    pub fn transition(
        &self,
        manifest: &mut RunManifest,
        status: RunStatus,
        phase: Option<&str>,
        data: Value,
    ) -> anyhow::Result<RunStatus> {
        let _lock = self.resource_lock(&manifest.run_id, ".manifest.lock")?;
        let current = if self.manifest_path(&manifest.run_id).exists() {
            self.load_manifest(&manifest.run_id)?
        } else {
            manifest.clone()
        };
        if current.status.terminal() {
            *manifest = current.clone();
            return Ok(current.status);
        }

        if status == RunStatus::Cancelling {
            fs::write(
                self.run_dir(&manifest.run_id).join("cancel.requested"),
                utc_now(),
            )?;
        }

        let cancellation_wins = self.cancellation_requested(&manifest.run_id)
            || current.status == RunStatus::Cancelling;
        let effective_status = if cancellation_wins
            && !matches!(status, RunStatus::Cancelling | RunStatus::Cancelled)
        {
            RunStatus::Cancelled
        } else {
            status
        };
        let previous = current.status;
        let mut next = if matches!(
            effective_status,
            RunStatus::Cancelling | RunStatus::Cancelled
        ) {
            current
        } else {
            manifest.clone()
        };
        next.status = effective_status;
        next.current_phase = phase.map(str::to_string);
        next.updated_at = utc_now();
        if effective_status == RunStatus::Cancelled {
            next.error = Some(RunError {
                code: "cancelled".into(),
                message: "run cancelled by explicit request".into(),
                retryable: false,
            });
        }
        if !transition_allowed(previous, effective_status) {
            bail!(
                "illegal run transition from {:?} to {:?}",
                previous,
                effective_status
            );
        }
        let pending = PendingTransition {
            transition_id: uuid::Uuid::now_v7().to_string(),
            timestamp: utc_now(),
            from: previous,
            to: effective_status,
            requested: status,
            phase: phase.map(str::to_string),
            detail: data,
        };
        write_json(&self.pending_transition_path(&manifest.run_id), &pending)?;
        self.save_manifest_unlocked(&next)?;
        *manifest = next;
        let event_result = self.event_at(
            &manifest.run_id,
            "run.transition",
            phase,
            None,
            None,
            json!({
                "transition_id": pending.transition_id,
                "from": previous,
                "to": effective_status,
                "requested": status,
                "detail": pending.detail
            }),
            &pending.timestamp,
        );
        if event_result.is_ok() {
            remove_if_exists(&self.pending_transition_path(&manifest.run_id))?;
        }
        Ok(effective_status)
    }

    pub fn event(
        &self,
        run_id: &str,
        event_type: &str,
        phase: Option<&str>,
        party_id: Option<&str>,
        attempt: Option<usize>,
        data: Value,
    ) -> anyhow::Result<Event> {
        self.event_at(
            run_id,
            event_type,
            phase,
            party_id,
            attempt,
            data,
            &utc_now(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn event_at(
        &self,
        run_id: &str,
        event_type: &str,
        phase: Option<&str>,
        party_id: Option<&str>,
        attempt: Option<usize>,
        data: Value,
        timestamp: &str,
    ) -> anyhow::Result<Event> {
        let _lock = self.resource_lock(run_id, ".events.lock")?;
        let path = self.run_dir(run_id).join("events.jsonl");
        let existing = read_events(&path)?;
        if event_type == "run.transition"
            && let Some(transition_id) = data.get("transition_id").and_then(Value::as_str)
            && let Some(event) = existing.iter().find(|event| {
                event.data.get("transition_id").and_then(Value::as_str) == Some(transition_id)
            })
        {
            return Ok(event.clone());
        }
        let sequence = existing.last().map_or(1, |event| event.sequence + 1);
        let event = Event {
            event_version: "1.0".to_string(),
            sequence,
            timestamp: timestamp.to_string(),
            run_id: run_id.to_string(),
            event_type: event_type.to_string(),
            phase: phase.map(str::to_string),
            party_id: party_id.map(str::to_string),
            attempt,
            data,
        };
        validate_value(&serde_json::to_value(&event)?, RUN_EVENT_SCHEMA)?;
        append_jsonl(&path, &event)?;
        Ok(event)
    }

    pub fn add_active_pid(&self, run_id: &str, pid: u32) -> anyhow::Result<()> {
        let _lock = self.resource_lock(run_id, ".active-pids.lock")?;
        let path = self.run_dir(run_id).join("active-pids.json");
        let mut processes = read_processes(&path)?;
        let process = ActiveProcess {
            pid,
            identity: format!("test:{pid}"),
            program: "test".into(),
        };
        if !processes.iter().any(|entry| entry == &process) {
            processes.push(process);
            processes.sort_by_key(|entry| entry.pid);
        }
        write_json(&path, &processes)
    }

    /// Serializes the final cancellation check with child spawn and PID registration.
    pub fn process_registration(&self, run_id: &str) -> anyhow::Result<ProcessRegistration> {
        let file = self.open_resource_lock(run_id, ".active-pids.lock")?;
        Ok(ProcessRegistration {
            _file: file,
            pids_path: self.run_dir(run_id).join("active-pids.json"),
            cancel_path: self.run_dir(run_id).join("cancel.requested"),
        })
    }

    /// Serializes the final cancellation check and result publication with cancellation requests.
    pub fn finalization_guard(&self, run_id: &str) -> anyhow::Result<FinalizationGuard> {
        Ok(FinalizationGuard {
            _file: self.open_resource_lock(run_id, ".active-pids.lock")?,
            cancel_path: self.run_dir(run_id).join("cancel.requested"),
        })
    }

    /// Publishes cancellation and snapshots supervised PIDs under the spawn lock.
    pub fn request_cancellation(&self, run_id: &str) -> anyhow::Result<CancellationSnapshot> {
        let _lock = self.resource_lock(run_id, ".active-pids.lock")?;
        let manifest: RunManifest = read_json(&self.manifest_path(run_id))?;
        if manifest.status.terminal() {
            return Ok(CancellationSnapshot {
                status: manifest.status,
                processes: Vec::new(),
            });
        }
        atomic_write(
            &self.run_dir(run_id).join("cancel.requested"),
            utc_now().as_bytes(),
        )?;
        // A result is public only after a success terminal manifest is committed.
        // Remove any artifacts left by a worker crash in the finalization window.
        remove_if_exists(&self.run_dir(run_id).join("result.json"))?;
        remove_if_exists(&self.run_dir(run_id).join("report.md"))?;
        Ok(CancellationSnapshot {
            status: manifest.status,
            processes: read_processes(&self.run_dir(run_id).join("active-pids.json"))?,
        })
    }

    pub fn remove_active_pid(&self, run_id: &str, pid: u32) -> anyhow::Result<()> {
        let _lock = self.resource_lock(run_id, ".active-pids.lock")?;
        let path = self.run_dir(run_id).join("active-pids.json");
        let mut processes = read_processes(&path)?;
        processes.retain(|entry| entry.pid != pid);
        write_json(&path, &processes)
    }

    pub fn remove_active_process(
        &self,
        run_id: &str,
        process: &ActiveProcess,
    ) -> anyhow::Result<()> {
        let _lock = self.resource_lock(run_id, ".active-pids.lock")?;
        let path = self.run_dir(run_id).join("active-pids.json");
        let mut processes = read_processes(&path)?;
        processes.retain(|entry| entry != process);
        write_json(&path, &processes)
    }

    pub fn active_pids(&self, run_id: &str) -> anyhow::Result<Vec<u32>> {
        let _lock = self.resource_lock(run_id, ".active-pids.lock")?;
        Ok(
            read_processes(&self.run_dir(run_id).join("active-pids.json"))?
                .into_iter()
                .map(|entry| entry.pid)
                .collect(),
        )
    }

    pub fn active_processes(&self, run_id: &str) -> anyhow::Result<Vec<ActiveProcess>> {
        let _lock = self.resource_lock(run_id, ".active-pids.lock")?;
        read_processes(&self.run_dir(run_id).join("active-pids.json"))
    }

    pub fn events(&self, run_id: &str) -> anyhow::Result<Vec<Event>> {
        let manifest = self.load_manifest(run_id)?;
        self.recover_pending_transition(&manifest)?;
        let _lock = self.resource_lock(run_id, ".events.lock")?;
        read_events(&self.run_dir(run_id).join("events.jsonl"))
    }

    pub fn write_artifact<T: Serialize>(
        &self,
        run_id: &str,
        relative: &str,
        value: &T,
    ) -> anyhow::Result<PathBuf> {
        let path = self.run_dir(run_id).join(relative);
        write_json(&path, value)?;
        Ok(path)
    }

    pub fn list_manifests(&self) -> anyhow::Result<Vec<RunManifest>> {
        if !self.runs_dir().exists() {
            return Ok(Vec::new());
        }
        let mut manifests = Vec::new();
        for entry in fs::read_dir(self.runs_dir())? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path().join("manifest.json");
            if path.exists() {
                manifests.push(read_json(&path)?);
            }
        }
        manifests.sort_by(|left: &RunManifest, right| right.created_at.cmp(&left.created_at));
        Ok(manifests)
    }

    fn resource_lock(&self, run_id: &str, name: &str) -> anyhow::Result<RunLock> {
        Ok(RunLock {
            _file: self.open_resource_lock(run_id, name)?,
        })
    }

    fn open_resource_lock(&self, run_id: &str, name: &str) -> anyhow::Result<File> {
        let path = self.run_dir(run_id).join(name);
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.lock_exclusive()?;
        Ok(file)
    }

    fn cancellation_requested(&self, run_id: &str) -> bool {
        self.run_dir(run_id).join("cancel.requested").exists()
    }

    fn pending_transition_path(&self, run_id: &str) -> PathBuf {
        self.run_dir(run_id)
            .join("diagnostics/pending-transition.json")
    }

    fn recover_pending_transition(&self, manifest: &RunManifest) -> anyhow::Result<()> {
        let path = self.pending_transition_path(&manifest.run_id);
        if !path.exists() {
            return Ok(());
        }
        let pending: PendingTransition = read_json(&path)?;
        if manifest.status == pending.from {
            remove_if_exists(&path)?;
            return Ok(());
        }
        if manifest.status != pending.to {
            bail!(
                "pending transition does not match manifest: {:?} -> {:?}, manifest is {:?}",
                pending.from,
                pending.to,
                manifest.status
            );
        }
        let _lock = self.resource_lock(&manifest.run_id, ".events.lock")?;
        let events_path = self.run_dir(&manifest.run_id).join("events.jsonl");
        let events = read_events(&events_path)?;
        let already_recorded = events.iter().any(|event| {
            event.data.get("transition_id").and_then(Value::as_str)
                == Some(pending.transition_id.as_str())
        });
        if !already_recorded {
            let event = Event {
                event_version: "1.0".into(),
                sequence: events.last().map_or(1, |event| event.sequence + 1),
                timestamp: pending.timestamp.clone(),
                run_id: manifest.run_id.clone(),
                event_type: "run.transition".into(),
                phase: pending.phase.clone(),
                party_id: None,
                attempt: None,
                data: json!({
                    "transition_id": pending.transition_id,
                    "from": pending.from,
                    "to": pending.to,
                    "requested": pending.requested,
                    "detail": pending.detail
                }),
            };
            validate_value(&serde_json::to_value(&event)?, RUN_EVENT_SCHEMA)?;
            append_jsonl(&events_path, &event)?;
        }
        remove_if_exists(&path)
    }
}

fn read_processes(path: &Path) -> anyhow::Result<Vec<ActiveProcess>> {
    if path.exists() {
        read_json(path)
    } else {
        Ok(Vec::new())
    }
}

fn read_events(path: &Path) -> anyhow::Result<Vec<Event>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(path)?;
    let complete_end = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    let mut events = Vec::new();
    for line in bytes[..complete_end].split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        events.push(
            serde_json::from_slice::<Event>(line)
                .context("events.jsonl contains an invalid committed record")?,
        );
    }
    let tail = &bytes[complete_end..];
    if !tail.is_empty() {
        match serde_json::from_slice::<Event>(tail) {
            Ok(event) => {
                events.push(event);
                let mut file = File::options().append(true).open(path)?;
                use std::io::Write;
                file.write_all(b"\n")?;
                file.sync_data()?;
            }
            Err(_) => {
                let file = File::options().write(true).open(path)?;
                file.set_len(complete_end as u64)?;
                file.sync_data()?;
            }
        }
    }
    for (index, event) in events.iter().enumerate() {
        let expected = index as u64 + 1;
        if event.sequence != expected {
            bail!(
                "events.jsonl sequence mismatch: expected {expected}, found {}",
                event.sequence
            );
        }
    }
    Ok(events)
}

fn transition_allowed(from: RunStatus, to: RunStatus) -> bool {
    if from == to {
        return true;
    }
    if matches!(
        to,
        RunStatus::Failed | RunStatus::FailedPolicy | RunStatus::Cancelling | RunStatus::Cancelled
    ) {
        return !from.terminal();
    }
    matches!(
        (from, to),
        (RunStatus::Queued, RunStatus::Preflight)
            | (RunStatus::Preflight, RunStatus::R1Running)
            | (RunStatus::R1Running, RunStatus::R1Gate)
            | (RunStatus::R1Gate, RunStatus::R2Packet)
            | (RunStatus::R1Gate, RunStatus::R2Running)
            | (RunStatus::R2Packet, RunStatus::R2Running)
            | (RunStatus::R2Packet, RunStatus::R2Gate)
            | (RunStatus::R2Running, RunStatus::R2Gate)
            | (RunStatus::R2Gate, RunStatus::R3Cc)
            | (RunStatus::R2Gate, RunStatus::WaitingPrimaryArbiter)
            | (RunStatus::R3Cc, RunStatus::WaitingPrimaryArbiter)
            | (RunStatus::WaitingPrimaryArbiter, RunStatus::Merging)
            | (RunStatus::Merging, RunStatus::Completed)
            | (RunStatus::Merging, RunStatus::Degraded)
    )
}

fn remove_if_exists(path: &Path) -> anyhow::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    use serde_json::json;

    use super::Store;
    use crate::model::{RunManifest, RunStatus, SandboxMode};

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
    fn rejects_state_regression() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::new(temporary.path().join("home"));
        store.create_run_dirs("run-1").unwrap();
        let mut manifest = manifest("run-1");
        store.save_manifest(&manifest).unwrap();
        let error = store
            .transition(
                &mut manifest,
                RunStatus::WaitingPrimaryArbiter,
                Some("R3"),
                json!({}),
            )
            .unwrap_err();
        assert!(error.to_string().contains("illegal run transition"));
        assert_eq!(
            store.load_manifest("run-1").unwrap().status,
            RunStatus::Queued
        );
    }

    #[test]
    fn truncates_only_an_uncommitted_event_tail() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::new(temporary.path().join("home"));
        store.create_run_dirs("run-1").unwrap();
        store.save_manifest(&manifest("run-1")).unwrap();
        store
            .event("run-1", "first", None, None, None, json!({}))
            .unwrap();
        let path = store.run_dir("run-1").join("events.jsonl");
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"{\"torn\":")
            .unwrap();
        let second = store
            .event("run-1", "second", None, None, None, json!({}))
            .unwrap();
        assert_eq!(second.sequence, 2);
        assert_eq!(store.events("run-1").unwrap().len(), 2);
        assert!(fs::read(path).unwrap().ends_with(b"\n"));
    }
}
