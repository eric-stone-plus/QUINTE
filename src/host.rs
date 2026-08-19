use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow, bail};
use fs2::FileExt;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::doctor;
use crate::model::{Policy, RunManifest, RunStatus};
use crate::policy;
use crate::run;
use crate::store::Store;
use crate::util::{create_private_dir_all, read_json, sha256_bytes, utc_now, write_json};

pub const HOST_RECEIPT_VERSION: &str = crate::contract::HOST_RECEIPT_VERSION;

pub struct HostStart {
    pub receipt: Value,
    pub receipt_path: PathBuf,
}

pub struct HostReceipt {
    pub receipt: Value,
    pub receipt_path: PathBuf,
}

struct HostLock {
    _file: File,
}

fn host_dir(store: &Store) -> PathBuf {
    store.home().join("host")
}

fn receipts_dir(store: &Store) -> PathBuf {
    host_dir(store).join("receipts")
}

fn acquire_host_lock(store: &Store) -> anyhow::Result<HostLock> {
    create_private_dir_all(&host_dir(store))?;
    let path = host_dir(store).join("launch.lock");
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    file.try_lock_exclusive()
        .context("another QUINTE host launch is in progress")?;
    Ok(HostLock { _file: file })
}

fn active_manifests(store: &Store) -> anyhow::Result<Vec<RunManifest>> {
    let runs_dir = store.runs_dir();
    if !runs_dir.exists() {
        return Ok(Vec::new());
    }
    let mut manifests = Vec::new();
    for entry in fs::read_dir(&runs_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            bail!(
                "QUINTE host found an unexpected non-directory entry under {}: {}",
                runs_dir.display(),
                entry.path().display()
            );
        }
        let run_id = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .context("QUINTE host found a non-UTF-8 run directory")?;
        crate::store::validate_run_id(&run_id).with_context(|| {
            format!(
                "QUINTE host found an invalid run directory {}; reconcile manually",
                entry.path().display()
            )
        })?;
        let manifest = store.load_manifest(&run_id).with_context(|| {
            format!(
                "QUINTE host cannot trust run directory {}; reconcile manually",
                entry.path().display()
            )
        })?;
        if !manifest.status.terminal() {
            manifests.push(manifest);
        }
    }
    manifests.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(manifests)
}

fn active_ids(manifests: &[RunManifest]) -> Vec<String> {
    manifests
        .iter()
        .map(|manifest| manifest.run_id.clone())
        .collect()
}

fn state(code: &str, active_run_ids: Vec<String>) -> Value {
    json!({
        "code": code,
        "active_run_ids": active_run_ids
    })
}

fn base_receipt(
    store: &Store,
    invocation_id: &str,
    operation: &str,
    state: Value,
) -> Value {
    json!({
        "host_receipt_version": HOST_RECEIPT_VERSION,
        "invocation_id": invocation_id,
        "operation": operation,
        "observed_at": utc_now(),
        "state_root": store.home(),
        "state": state
    })
}

fn receipt_path(store: &Store, invocation_id: &str) -> PathBuf {
    receipts_dir(store).join(format!("{invocation_id}.json"))
}

fn bind_receipt_path(store: &Store, receipt: &mut Value) -> anyhow::Result<PathBuf> {
    let invocation_id = receipt
        .get("invocation_id")
        .and_then(Value::as_str)
        .context("host receipt has no invocation_id")?;
    let path = receipt_path(store, invocation_id);
    receipt["receipt_path"] = json!(path);
    Ok(path)
}

fn persist_receipt(store: &Store, receipt: &mut Value) -> anyhow::Result<PathBuf> {
    let path = bind_receipt_path(store, receipt)?;
    validate_receipt(receipt)?;
    write_json(&path, receipt)?;
    if let Err(error) = write_json(&host_dir(store).join("latest.json"), receipt) {
        eprintln!(
            "warning: durable host receipt {} was persisted, but latest.json could not be updated: {error:#}",
            path.display()
        );
    }
    Ok(path)
}

fn validate_receipt_binding(
    store: &Store,
    path: &Path,
    receipt: &Value,
) -> anyhow::Result<()> {
    let invocation_id = receipt
        .get("invocation_id")
        .and_then(Value::as_str)
        .context("host receipt has no invocation_id")?;
    let parsed = Uuid::parse_str(invocation_id)
        .context("host receipt invocation_id is not a canonical lowercase UUIDv7")?;
    if parsed.get_version() != Some(uuid::Version::SortRand)
        || parsed.get_variant() != uuid::Variant::RFC4122
        || parsed.to_string() != invocation_id
    {
        bail!("host receipt invocation_id is not a canonical lowercase UUIDv7");
    }
    let expected_path = receipt_path(store, invocation_id);
    if path != expected_path {
        bail!(
            "host receipt path does not match its invocation identity: {}",
            path.display()
        );
    }
    if receipt.get("receipt_path") != Some(&serde_json::to_value(&expected_path)?) {
        bail!("host receipt receipt_path is not bound to its durable file");
    }
    if receipt.get("state_root") != Some(&serde_json::to_value(store.home())?) {
        bail!("host receipt state_root is not bound to this QUINTE store");
    }
    Ok(())
}

fn manifest_projection(manifest: &RunManifest) -> Value {
    json!({
        "status": manifest.status,
        "manifest_version": manifest.manifest_version,
        "brief_sha256": manifest.brief_sha256,
        "policy_sha256": manifest.policy_sha256,
        "snapshot_sha256": manifest.snapshot_sha256,
        "runtime_sha256": manifest.runtime_sha256,
        "error": manifest.error,
        "result_sha256": manifest.result_sha256
    })
}

fn latest_start_receipt(store: &Store) -> anyhow::Result<Option<Value>> {
    let directory = receipts_dir(store);
    if !directory.exists() {
        return Ok(None);
    }
    let mut paths = fs::read_dir(&directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    for path in paths {
        let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        if file_name.starts_with('.') && file_name.ends_with(".tmp") {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("json")
        {
            bail!(
                "QUINTE host found an unexpected receipt entry: {}",
                path.display()
            );
        }
        let receipt: Value = read_json(&path)
            .with_context(|| format!("host receipt is invalid: {}", path.display()))?;
        validate_receipt(&receipt)
            .with_context(|| format!("host receipt violates the contract: {}", path.display()))?;
        validate_receipt_binding(store, &path, &receipt).with_context(|| {
            format!("host receipt identity is not bound to its file: {}", path.display())
        })?;
        if receipt.get("operation").and_then(Value::as_str) == Some("start") {
            return Ok(Some(receipt));
        }
    }
    Ok(None)
}

fn validate_receipt(receipt: &Value) -> anyhow::Result<()> {
    crate::schema::validate_value(receipt, crate::contract::HOST_RECEIPT_SCHEMA)
        .context("generated host receipt violates the host contract")
}

fn policy_or_error(store: &Store) -> anyhow::Result<Policy> {
    policy::load_for_runtime(&store.policy_path())
}

fn parse_time(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|value| value.with_timezone(&chrono::Utc))
}

fn age_seconds(value: &str) -> Option<u64> {
    let then = parse_time(value)?;
    let now = chrono::Utc::now();
    u64::try_from((now - then).num_seconds().max(0)).ok()
}

fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "pid="])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return false;
        }
        let mut exit_code = 0;
        let ok = unsafe { GetExitCodeProcess(process, &mut exit_code) } != 0;
        unsafe { CloseHandle(process) };
        ok && exit_code == STILL_ACTIVE as u32
    }
}

fn worker_observation(store: &Store, manifest: &RunManifest) -> anyhow::Result<Value> {
    let run_dir = store.run_dir(&manifest.run_id)?;
    let diagnostics = run_dir.join("diagnostics");
    let worker: Option<Value> = if diagnostics.join("worker.json").exists() {
        Some(read_json(&diagnostics.join("worker.json"))?)
    } else {
        None
    };
    let finished_at = fs::read_to_string(diagnostics.join("worker-finished"))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| parse_time(value).is_some());
    let heartbeat_at = fs::read_to_string(diagnostics.join("worker-heartbeat"))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| parse_time(value).is_some());
    let heartbeat_age_seconds = heartbeat_at.as_deref().and_then(age_seconds);
    let pid = worker
        .as_ref()
        .and_then(|worker| worker.get("pid"))
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok());
    let state = if finished_at.is_some() || manifest.status.terminal() {
        "finished"
    } else if pid.is_none() {
        "missing"
    } else if heartbeat_age_seconds.is_some_and(|age| age > 15) {
        "stale"
    } else if pid.is_some_and(process_alive) {
        "running"
    } else {
        "dead"
    };
    let mut observation = json!({
        "state": state,
        "recovery_needed": !manifest.status.terminal()
            && matches!(state, "missing" | "stale" | "dead")
    });
    if let Some(pid) = pid {
        observation["pid"] = json!(pid);
    }
    if let Some(heartbeat_at) = heartbeat_at {
        observation["heartbeat_at"] = json!(heartbeat_at);
    }
    if let Some(heartbeat_age_seconds) = heartbeat_age_seconds {
        observation["heartbeat_age_seconds"] = json!(heartbeat_age_seconds);
    }
    if let Some(finished_at) = finished_at {
        observation["finished_at"] = json!(finished_at);
    }
    Ok(observation)
}

fn duration_milliseconds(started_at: &str, finished_at: &str) -> Option<u64> {
    let started = parse_time(started_at)?;
    let finished = parse_time(finished_at)?;
    u64::try_from((finished - started).num_milliseconds().max(0)).ok()
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedRetryDeadline {
    retry_state_version: String,
    phase: String,
    route_id: String,
    previous_attempt: usize,
    next_attempt: usize,
    due_at: String,
    failure_class: String,
    #[allow(dead_code)]
    source: String,
}

fn persisted_retry_deadlines(
    store: &Store,
    manifest: &RunManifest,
) -> anyhow::Result<Vec<(String, String, String, usize, String, String)>> {
    let lanes = store.run_dir(&manifest.run_id)?.join("lanes");
    let mut found = Vec::new();
    for phase in ["R1", "R2", "R3"] {
        let phase_dir = lanes.join(phase);
        if !phase_dir.exists() {
            continue;
        }
        for route_entry in fs::read_dir(&phase_dir)? {
            let route_entry = route_entry?;
            if !route_entry.file_type()?.is_dir() {
                continue;
            }
            let route_id = route_entry
                .file_name()
                .to_str()
                .map(str::to_owned)
                .context("retry deadline route directory is not UTF-8")?;
            let path = route_entry.path().join("retry-deadline.json");
            // The worker removes this file immediately after its wait.  Read
            // directly and treat a concurrent removal as an absent snapshot;
            // an exists()+read_json() pair would turn that normal race into a
            // spurious host-status failure.
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("cannot read retry deadline: {}", path.display()))
                }
            };
            let state: PersistedRetryDeadline = serde_json::from_slice(&bytes)
                .with_context(|| format!("invalid retry deadline: {}", path.display()))?;
            if state.retry_state_version != crate::contract::RETRY_STATE_VERSION
                || state.phase != phase
                || state.route_id != route_id
                || state.previous_attempt.checked_add(1) != Some(state.next_attempt)
                || state.next_attempt == 0
            {
                bail!("retry deadline does not match {phase}/{route_id}");
            }
            parse_time(&state.due_at).with_context(|| {
                format!("retry deadline has invalid due_at: {}", path.display())
            })?;
            let party_id = manifest
                .route_bindings
                .iter()
                .find(|binding| binding.route_id == route_id)
                .map(|binding| binding.party_id.clone())
                .with_context(|| {
                    format!("retry deadline route is not bound by manifest: {route_id}")
                })?;
            found.push((
                phase.to_owned(),
                party_id,
                route_id,
                state.next_attempt,
                state.due_at,
                state.failure_class,
            ));
        }
    }
    Ok(found)
}

fn attempt_observations(store: &Store, manifest: &RunManifest) -> anyhow::Result<Vec<Value>> {
    use std::collections::BTreeMap;
    let mut attempts = BTreeMap::<(String, String, usize), Value>::new();
    let mut party_routes = BTreeMap::<String, String>::new();
    for event in store.events(&manifest.run_id)? {
        let (Some(phase), Some(party_id), Some(attempt)) =
            (event.phase.as_deref(), event.party_id.as_deref(), event.attempt)
        else {
            continue;
        };
        if !matches!(phase, "R1" | "R2" | "R3") {
            continue;
        }
        let route_id = event
            .data
            .get("route_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| party_routes.get(party_id).cloned())
            .or_else(|| {
                manifest
                    .route_bindings
                    .iter()
                    .find(|binding| binding.party_id == party_id)
                    .map(|binding| binding.route_id.clone())
            });
        let Some(route_id) = route_id else { continue };
        party_routes.insert(party_id.to_owned(), route_id.clone());
        let key = (phase.to_owned(), route_id.clone(), attempt);
        let row = attempts.entry(key).or_insert_with(|| {
            json!({
                "phase": phase,
                "party_id": party_id,
                "route_id": route_id,
                "attempt": attempt,
                "state": "running"
            })
        });
        match event.event_type.as_str() {
            "lane.started" | "lane.retry_started" => {
                row["state"] = json!("running");
                row["started_at"] = json!(event.timestamp);
            }
            "lane.retry_wait" => {
                row["state"] = json!("retry_wait");
                if let Some(value) = event.data.get("due_at") {
                    row["retry_due_at"] = value.clone();
                }
                if let Some(value) = event.data.get("failure_class") {
                    row["failure_class"] = value.clone();
                }
            }
            "lane.retry_scheduled" => {
                let next_attempt = attempt
                    .checked_add(1)
                    .context("retry attempt overflow in host projection")?;
                let row = attempts
                    .entry((phase.to_owned(), route_id.clone(), next_attempt))
                    .or_insert_with(|| {
                        json!({
                            "phase": phase,
                            "party_id": party_id,
                            "route_id": route_id,
                            "attempt": next_attempt,
                            "state": "retry_wait"
                        })
                    });
                if row["state"] != json!("running") && row["state"] != json!("accepted") {
                    row["state"] = json!("retry_wait");
                }
                if let Some(value) = event.data.get("due_at") {
                    row["retry_due_at"] = value.clone();
                }
                if let Some(value) = event.data.get("failure_class") {
                    row["failure_class"] = value.clone();
                }
            }
            "lane.finished" => {
                row["state"] = json!(if event.data.get("accepted") == Some(&Value::Bool(true)) {
                    "accepted"
                } else {
                    "failed"
                });
                row["finished_at"] = json!(event.timestamp);
                for field in ["duration_ms", "timed_out", "retryable", "failure_class"] {
                    if let Some(value) = event.data.get(field).filter(|value| !value.is_null()) {
                        row[field] = value.clone();
                    }
                }
                if row.get("duration_ms").is_none()
                    && let Some(started_at) = row.get("started_at").and_then(Value::as_str)
                    && let Some(duration) = duration_milliseconds(started_at, &event.timestamp)
                {
                    row["duration_ms"] = json!(duration);
                }
            }
            "lane.accepted" => row["state"] = json!("accepted"),
            _ => {}
        }
    }
    // A scheduler writes retry-deadline.json before publishing
    // lane.retry_scheduled.  If it crashes in that window, retain the
    // durable retry identity instead of reporting a missing attempt.
    for (phase, party_id, route_id, attempt, due_at, failure_class) in
        persisted_retry_deadlines(store, manifest)?
    {
        let row = attempts
            .entry((phase.clone(), route_id.clone(), attempt))
            .or_insert_with(|| {
                json!({
                    "phase": phase,
                    "party_id": party_id,
                    "route_id": route_id,
                    "attempt": attempt,
                    "state": "retry_wait"
                })
            });
        if !matches!(
            row["state"].as_str(),
            Some("running" | "accepted" | "failed")
        ) {
            row["state"] = json!("retry_wait");
        }
        if row.get("retry_due_at").is_none() {
            row["retry_due_at"] = json!(due_at);
        }
        if row.get("failure_class").is_none() {
            row["failure_class"] = json!(failure_class);
        }
    }
    // Observation, not a start gate: an attempt annotation must read any
    // structurally valid policy, including a legacy manual-handoff home
    // whose `auto_primary_arbiter=false` could never start a new run.
    let timeout_seconds = policy::load_for_observation(&store.policy_path())?.timeout_seconds;
    for row in attempts.values_mut() {
        row["timeout_seconds"] = json!(timeout_seconds);
    }
    Ok(attempts.into_values().collect())
}

pub fn preflight(store: &Store) -> anyhow::Result<HostReceipt> {
    let invocation_id = Uuid::now_v7().to_string();
    let policy = policy_or_error(store)?;
    let report = doctor::run(&policy);
    let active = active_manifests(store)?;
    let code = if !report.ok {
        "preflight_failed"
    } else if active.is_empty() {
        "ready"
    } else {
        "active_run_present"
    };
    let mut receipt = base_receipt(
        store,
        &invocation_id,
        "preflight",
        state(code, active_ids(&active)),
    );
    receipt["preflight"] = serde_json::to_value(report)?;
    let receipt_path = persist_receipt(store, &mut receipt)?;
    Ok(HostReceipt {
        receipt,
        receipt_path,
    })
}

pub fn start(store: &Store, brief_path: &Path) -> anyhow::Result<HostStart> {
    let _lock = acquire_host_lock(store)?;
    let invocation_id = Uuid::now_v7().to_string();
    let policy = policy_or_error(store)?;
    let report = doctor::run(&policy);
    if !report.ok {
        bail!("QUINTE host preflight failed; inspect `quinte host preflight --json`");
    }
    let active = active_manifests(store)?;
    if !active.is_empty() {
        bail!(
            "QUINTE host refuses a second active run; active run ids: {}",
            active_ids(&active).join(", ")
        );
    }
    let brief_bytes = fs::read(brief_path)
        .with_context(|| format!("cannot read {}", brief_path.display()))?;
    let supplied_brief_sha256 = sha256_bytes(&brief_bytes);
    let brief_contract = crate::contract::contract("brief").expect("brief contract is registered");
    let created = run::create_from_brief_bytes(store, &policy, &brief_bytes, brief_contract)?;
    let manifest = store.load_manifest(&created.run_id)?;
    let mut receipt = base_receipt(
        store,
        &invocation_id,
        "start",
        state("created", vec![created.run_id.clone()]),
    );
    receipt["run_id"] = json!(created.run_id);
    receipt["brief"] = json!({
        "source": brief_path,
        "source_sha256": supplied_brief_sha256,
        "canonical_sha256": manifest.brief_sha256
    });
    receipt["manifest"] = manifest_projection(&manifest);
    persist_receipt(store, &mut receipt).with_context(|| {
        format!(
            "host invocation {invocation_id}, run {}: run created but host receipt could not be persisted",
            created.run_id
        )
    })?;

    let worker_pid = match run::spawn_worker(store, &created.run_id) {
        Ok(pid) => pid,
        Err(launch_error) => {
            let launch_message = format!("worker launch failed: {launch_error:#}");
            let record_error = run::record_worker_failure(
                store,
                &created.run_id,
                &launch_message,
            )
            .err();

            receipt["state"]["code"] = json!("launch_failed");
            receipt["observed_at"] = json!(utc_now());
            let mut blockers = vec![launch_message];
            if let Some(error) = record_error.as_ref() {
                blockers.push(format!("failed to record terminal worker state: {error:#}"));
            }
            match store.load_manifest(&created.run_id) {
                Ok(manifest) => {
                    receipt["manifest"] = manifest_projection(&manifest);
                    receipt["state"]["active_run_ids"] = json!(
                        if manifest.status.terminal() {
                            Vec::<String>::new()
                        } else {
                            vec![created.run_id.clone()]
                        }
                    );
                }
                Err(error) => blockers.push(format!(
                    "failed to reload the created run manifest: {error:#}"
                )),
            }
            receipt["state"]["blockers"] = json!(blockers);
            let receipt_result = persist_receipt(store, &mut receipt);

            let mut failure = format!(
                "{launch_error:#}; run {} was created; reconcile before another launch",
                created.run_id
            );
            if let Some(error) = record_error {
                failure.push_str(&format!("; terminal state write also failed: {error:#}"));
            }
            match receipt_result {
                Ok(path) => failure.push_str(&format!("; receipt {}", path.display())),
                Err(error) => failure.push_str(&format!(
                    "; host invocation {invocation_id}, run {}: launch-failure receipt persistence also failed: {error:#}",
                    created.run_id
                )),
            }
            return Err(anyhow!(failure));
        }
    };
    receipt["state"]["code"] = json!("started");
    receipt["observed_at"] = json!(utc_now());
    receipt["manifest"]["worker_pid"] = json!(worker_pid);
    let path = persist_receipt(store, &mut receipt).with_context(|| {
        format!(
            "host invocation {invocation_id}, run {}: worker started but final host receipt could not be persisted",
            created.run_id
        )
    })?;
    Ok(HostStart {
        receipt,
        receipt_path: path,
    })
}

fn receipt_for_run(
    store: &Store,
    operation: &str,
    run_id: &str,
) -> anyhow::Result<HostReceipt> {
    let invocation_id = Uuid::now_v7().to_string();
    let manifest = store.load_manifest(run_id)?;
    let active = active_manifests(store)?;
    let mut receipt = base_receipt(
        store,
        &invocation_id,
        operation,
        state(
            if manifest.status.terminal() { "terminal" } else { "observed" },
            active_ids(&active),
        ),
    );
    receipt["run_id"] = json!(run_id);
    receipt["manifest"] = manifest_projection(&manifest);
    receipt["state"]["worker"] = worker_observation(store, &manifest)?;
    receipt["state"]["attempts"] = json!(attempt_observations(store, &manifest)?);
    if matches!(manifest.status, RunStatus::Completed | RunStatus::Degraded) {
        let integrity = run::verify_result_integrity(store, run_id)?
            .context("terminal success has no verifiable result")?;
        receipt["result"] = json!({
            "verified": true,
            "actionable": integrity.actionable,
            "contract_version": integrity.contract_version,
            "sha256": manifest.result_sha256,
            "path": store.run_dir(run_id)?.join("result.json")
        });
    }
    let receipt_path = persist_receipt(store, &mut receipt)?;
    Ok(HostReceipt {
        receipt,
        receipt_path,
    })
}

pub fn status(store: &Store, run_id: &str) -> anyhow::Result<HostReceipt> {
    receipt_for_run(store, "status", run_id)
}

pub fn inspect(store: &Store, run_id: &str) -> anyhow::Result<HostReceipt> {
    receipt_for_run(store, "inspect", run_id)
}

pub fn reconcile(store: &Store, run_id: Option<&str>) -> anyhow::Result<HostReceipt> {
    let _lock = acquire_host_lock(store)?;
    let invocation_id = Uuid::now_v7().to_string();
    let active = active_manifests(store)?;
    let durable_start_receipt = if run_id.is_none() && active.is_empty() {
        latest_start_receipt(store)?
    } else {
        None
    };
    let durable_start_run_id = durable_start_receipt
        .as_ref()
        .and_then(|receipt| receipt.get("run_id"))
        .and_then(Value::as_str);
    if let Some(run_id) = durable_start_run_id {
        crate::store::validate_run_id(run_id)?;
    }
    let selected = match run_id {
        Some(run_id) => Some(store.load_manifest(run_id)?),
        None if active.len() == 1 => active.first().cloned(),
        None if active.is_empty() => durable_start_run_id
            .as_deref()
            .map(|run_id| store.load_manifest(run_id))
            .transpose()?,
        None => None,
    };
    let code = match (run_id, active.len(), selected.is_some()) {
        (_, _, true) => "reconciled",
        (None, 0, false) => "no_active_run",
        (None, _, false) => "ambiguous_active_runs",
        (Some(run_id), _, false) => {
            bail!("cannot reconcile run {run_id}: no manifest was selected")
        }
    };
    let mut receipt = base_receipt(
        store,
        &invocation_id,
        "reconcile",
        state(code, active_ids(&active)),
    );
    if let Some(manifest) = selected {
        let manifest_status = manifest.status;
        receipt["run_id"] = json!(manifest.run_id);
        receipt["manifest"] = manifest_projection(&manifest);
        if matches!(manifest_status, RunStatus::Completed | RunStatus::Degraded) {
            let integrity = run::verify_result_integrity(store, &manifest.run_id)?
                .context("terminal success has no verifiable result")?;
            receipt["result"] = json!({
                "verified": true,
                "actionable": integrity.actionable,
                "contract_version": integrity.contract_version,
                "sha256": manifest.result_sha256,
                "path": store.run_dir(&manifest.run_id)?.join("result.json")
            });
        }
    }
    if let Some(brief) = durable_start_receipt
        .as_ref()
        .and_then(|receipt| receipt.get("brief"))
    {
        receipt["brief"] = brief.clone();
    }
    let path = receipt_path(store, &invocation_id);
    receipt["recovery"] = json!({
        "outcome": code,
        "launch_safe": active.is_empty(),
        "receipt_path": path
    });
    let receipt_path = persist_receipt(store, &mut receipt)?;
    Ok(HostReceipt {
        receipt,
        receipt_path,
    })
}
