//! HOST.md §8 map: A2A operations onto the existing CLI host
//! (`host start` / `status` / `inspect` / `reconcile` / `cancel`).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::host;
use crate::model::RunStatus;
use crate::run;
use crate::store::Store;
use crate::util::{create_private_dir_all, read_json, utc_now, write_json};

use super::wire::{
    ARTIFACT_NAME, A2aError, ERR_INTERNAL, ERR_NOT_CANCELABLE, ERR_POLICY, ERR_TASK_NOT_FOUND,
    extract_brief, map_run_status, send_configuration,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: String,
    pub context_id: Option<String>,
    pub created_at: String,
    pub message: Value,
    pub artifact_id: Option<String>,
}

fn tasks_dir(store: &Store) -> PathBuf {
    store.home().join("a2a").join("tasks")
}

fn task_path(store: &Store, task_id: &str) -> PathBuf {
    tasks_dir(store).join(format!("{task_id}.json"))
}

pub fn load_record(store: &Store, task_id: &str) -> anyhow::Result<Option<TaskRecord>> {
    let path = task_path(store, task_id);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_json(&path)?))
}

pub fn save_record(store: &Store, record: &TaskRecord) -> anyhow::Result<()> {
    create_private_dir_all(&tasks_dir(store))?;
    write_json(&task_path(store, &record.task_id), record)
}

pub fn list_records(store: &Store) -> anyhow::Result<Vec<TaskRecord>> {
    let dir = tasks_dir(store);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        if let Ok(record) = read_json::<TaskRecord>(&path) {
            out.push(record);
        }
    }
    Ok(out)
}

pub fn active_run_ids(store: &Store) -> anyhow::Result<Vec<String>> {
    Ok(store
        .list_manifests()?
        .into_iter()
        .filter(|m| !m.status.terminal())
        .map(|m| m.run_id)
        .collect())
}

pub fn send_message(store: &Store, params: &Value) -> Result<Value, A2aError> {
    let message = params
        .get("message")
        .ok_or_else(|| A2aError::brief_invalid("SendMessage params.message is required"))?;
    if message.get("taskId").and_then(Value::as_str).is_some() {
        return continue_task(store, params, message);
    }

    let brief = extract_brief(message)?;
    let active = active_run_ids(store).map_err(internal)?;
    if !active.is_empty() {
        return Err(A2aError::busy_run(&active));
    }

    let context_id = message
        .get("contextId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let inbox = store.home().join("a2a").join("inbox");
    create_private_dir_all(&inbox).map_err(internal)?;
    let brief_path = inbox.join(format!("{}.json", Uuid::now_v7()));
    write_json(&brief_path, &brief).map_err(internal)?;

    let started = host::start(store, &brief_path).map_err(|e| map_start_error(e, store))?;
    let run_id = started
        .receipt
        .get("run_id")
        .and_then(Value::as_str)
        .ok_or_else(|| A2aError::new(ERR_INTERNAL, "host start returned no run_id"))?
        .to_string();

    let record = TaskRecord {
        task_id: run_id.clone(),
        context_id,
        created_at: utc_now(),
        message: message.clone(),
        artifact_id: None,
    };
    save_record(store, &record).map_err(internal)?;

    let (return_immediately, history_length) = send_configuration(params);
    if !return_immediately {
        wait_until_interruptible(store, &run_id);
    }
    let task = project_task(store, &record, history_length)?;
    Ok(json!({ "task": task }))
}

fn continue_task(store: &Store, _params: &Value, message: &Value) -> Result<Value, A2aError> {
    let task_id = message
        .get("taskId")
        .and_then(Value::as_str)
        .ok_or_else(|| A2aError::new(ERR_INVALID_CONTINUE, "taskId required"))?;
    let _ = (store, task_id);
    Err(A2aError::with_state(
        super::wire::ERR_CHALLENGE,
        "arbiter-challenge continue is not accepted on this 0.2.x host map",
        "challenge_rejected",
    ))
}

const ERR_INVALID_CONTINUE: i64 = super::wire::ERR_INVALID_PARAMS;

fn map_start_error(error: anyhow::Error, store: &Store) -> A2aError {
    let text = format!("{error:#}");
    if text.contains("refuses a second active run") {
        let active = active_run_ids(store).unwrap_or_default();
        return A2aError::busy_run(&active);
    }
    if text.contains("preflight failed") || text.contains("required adapters") {
        return A2aError::with_state(ERR_POLICY, text, "preflight_failed");
    }
    A2aError::new(ERR_INTERNAL, text)
}

fn wait_until_interruptible(store: &Store, run_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(3600);
    while Instant::now() < deadline {
        match store.load_manifest(run_id) {
            Ok(manifest)
                if manifest.status.terminal()
                    || matches!(manifest.status, RunStatus::WaitingPrimaryArbiter) =>
            {
                return;
            }
            _ => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

pub fn get_task(store: &Store, params: &Value) -> Result<Value, A2aError> {
    let task_id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| A2aError::new(super::wire::ERR_INVALID_PARAMS, "GetTask.id is required"))?;
    let history_length = params.get("historyLength").and_then(Value::as_u64).unwrap_or(0);
    let record = load_or_recover(store, task_id)?;
    project_task(store, &record, history_length)
}

pub fn list_tasks(store: &Store, params: &Value) -> Result<Value, A2aError> {
    let page_size = params
        .get("pageSize")
        .and_then(Value::as_u64)
        .unwrap_or(50)
        .min(200) as usize;
    let token = params
        .get("pageToken")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut records = list_records(store).map_err(internal)?;
    if records.is_empty() {
        for manifest in store.list_manifests().map_err(internal)? {
            records.push(TaskRecord {
                task_id: manifest.run_id,
                context_id: None,
                created_at: manifest.created_at,
                message: json!({}),
                artifact_id: None,
            });
        }
    }
    let start = if token.is_empty() {
        0
    } else {
        records
            .iter()
            .position(|r| r.task_id == token)
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    let slice: Vec<_> = records.iter().skip(start).take(page_size).cloned().collect();
    let next = if start + slice.len() < records.len() {
        slice.last().map(|r| r.task_id.clone())
    } else {
        None
    };
    let mut tasks = Vec::new();
    for record in &slice {
        tasks.push(project_task(store, record, 0)?);
    }
    Ok(json!({
        "tasks": tasks,
        "nextPageToken": next
    }))
}

pub fn cancel_task(store: &Store, params: &Value) -> Result<Value, A2aError> {
    let task_id = params
        .get("id")
        .or_else(|| params.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| A2aError::new(super::wire::ERR_INVALID_PARAMS, "CancelTask.id is required"))?;
    let record = load_or_recover(store, task_id)?;
    let manifest = store.load_manifest(task_id).map_err(|_| {
        A2aError::with_state(ERR_TASK_NOT_FOUND, format!("task {task_id} not found"), "not_found")
    })?;
    if matches!(
        manifest.status,
        RunStatus::Completed | RunStatus::Degraded | RunStatus::Failed | RunStatus::FailedPolicy
    ) {
        return Err(A2aError::with_state(
            ERR_NOT_CANCELABLE,
            format!("task {task_id} is not cancelable"),
            "not_cancelable",
        ));
    }
    if !matches!(manifest.status, RunStatus::Cancelled | RunStatus::Cancelling) {
        run::cancel(store, task_id).map_err(internal)?;
    }
    project_task(store, &record, 0)
}

fn load_or_recover(store: &Store, task_id: &str) -> Result<TaskRecord, A2aError> {
    if let Some(record) = load_record(store, task_id).map_err(internal)? {
        return Ok(record);
    }
    let manifest = store.load_manifest(task_id).map_err(|_| {
        A2aError::with_state(ERR_TASK_NOT_FOUND, format!("task {task_id} not found"), "not_found")
    })?;
    let record = TaskRecord {
        task_id: manifest.run_id,
        context_id: None,
        created_at: manifest.created_at,
        message: json!({}),
        artifact_id: None,
    };
    let _ = save_record(store, &record);
    Ok(record)
}

pub fn project_task(
    store: &Store,
    record: &TaskRecord,
    history_length: u64,
) -> Result<Value, A2aError> {
    let _ = host::status(store, &record.task_id).map_err(|e| {
        if e.to_string().contains("unknown or invalid run") {
            A2aError::with_state(
                ERR_TASK_NOT_FOUND,
                format!("task {} not found", record.task_id),
                "not_found",
            )
        } else {
            internal(e)
        }
    })?;
    let manifest = store.load_manifest(&record.task_id).map_err(internal)?;
    let state = map_run_status(manifest.status);
    let mut artifacts = Vec::new();
    let mut artifact_id = record.artifact_id.clone();
    if matches!(manifest.status, RunStatus::Completed | RunStatus::Degraded) {
        let inspected = host::inspect(store, &record.task_id).map_err(internal)?;
        if inspected.receipt.get("result").is_none() {
            return Err(A2aError::new(
                ERR_INTERNAL,
                format!(
                    "COMPLETED task {} has no verifiable host result",
                    record.task_id
                ),
            ));
        }
        let path = store
            .run_dir(&record.task_id)
            .map_err(internal)?
            .join("result.json");
        let result: Value = read_json(&path)
            .with_context(|| format!("cannot read {}", path.display()))
            .map_err(internal)?;
        if artifact_id.is_none() {
            artifact_id = Some(Uuid::now_v7().to_string());
            let mut persisted = record.clone();
            persisted.artifact_id = artifact_id.clone();
            save_record(store, &persisted).map_err(internal)?;
        }
        artifacts.push(json!({
            "artifactId": artifact_id,
            "name": ARTIFACT_NAME,
            "parts": [{
                "data": result,
                "mediaType": "application/json"
            }]
        }));
        // HIGHBALL carriers: deterministic projections of the verdict for a
        // downstream delivery stage (route request + residual trace). They
        // are code, never model output.
        artifacts.push(json!({
            "artifactId": Uuid::now_v7().to_string(),
            "name": "highball.route-request.json",
            "parts": [{
                "data": crate::highball_carriers::route_request(&result),
                "mediaType": "application/json"
            }]
        }));
        artifacts.push(json!({
            "artifactId": Uuid::now_v7().to_string(),
            "name": "highball.residual-trace.json",
            "parts": [{
                "data": crate::highball_carriers::residual_trace(&result),
                "mediaType": "application/json"
            }]
        }));
    }

    let mut history = Vec::new();
    if history_length > 0 && !record.message.is_null() && record.message.get("role").is_some() {
        history.push(record.message.clone());
    }
    if history.len() > history_length as usize {
        let skip = history.len() - history_length as usize;
        history = history.into_iter().skip(skip).collect();
    }

    Ok(json!({
        "id": record.task_id,
        "contextId": record.context_id,
        "status": {
            "state": state,
            "timestamp": utc_now()
        },
        "artifacts": artifacts,
        "history": history
    }))
}

fn internal(error: impl std::fmt::Display) -> A2aError {
    A2aError::new(ERR_INTERNAL, error.to_string())
}
