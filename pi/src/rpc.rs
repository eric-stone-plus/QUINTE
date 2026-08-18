//! JSON-RPC dispatch: SendMessage (starts one background task) and
//! GetTask (returns state + artifact). No other methods exist.

use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Value, json};

use crate::prompt::Seat;
use crate::provider::Provider;
use crate::task::{TaskRecord, TaskState, TaskStore};

const ERR_INVALID_REQUEST: i64 = -32600;
const ERR_METHOD_NOT_FOUND: i64 = -32601;
const ERR_INVALID_PARAMS: i64 = -32602;
const ERR_INTERNAL: i64 = -32603;

pub fn dispatch(
    body: &str,
    store: &Arc<Mutex<TaskStore>>,
    seat: &Seat,
    provider: &Provider,
) -> String {
    let request: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return rpc_error(Value::Null, ERR_INVALID_REQUEST, &format!("parse error: {e}")),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let params = request.get("params").cloned().unwrap_or(json!({}));
    match method.as_str() {
        "SendMessage" => {
            let result = send_message(params, store, seat, provider);
            match result {
                Ok(task) => rpc_result(id, task),
                Err((code, message)) => rpc_error(id, code, &message),
            }
        }
        "GetTask" => match get_task(params, store) {
            Ok(task) => rpc_result(id, task),
            Err((code, message)) => rpc_error(id, code, &message),
        },
        _ => rpc_error(id, ERR_METHOD_NOT_FOUND, &format!("unknown method '{method}'")),
    }
}

fn send_message(
    params: Value,
    store: &Arc<Mutex<TaskStore>>,
    seat: &Seat,
    provider: &Provider,
) -> std::result::Result<Value, (i64, String)> {
    let task_id = params
        .get("message")
        .and_then(|m| m.get("taskId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let material = fold_parts(&params).ok_or((
        ERR_INVALID_PARAMS,
        "message carries no JSON parts".to_string(),
    ))?;
    let record = TaskRecord {
        task_id: task_id.clone(),
        state: TaskState::Working,
        artifact: None,
        error: None,
        created_at: crate::task::now(),
    };
    store
        .lock()
        .unwrap()
        .insert(record.clone())
        .map_err(|e| (ERR_INTERNAL, e.to_string()))?;

    let store = Arc::clone(store);
    let seat = seat.clone();
    let provider = provider.clone();
    let worker_task_id = task_id.clone();
    thread::spawn(move || {
        // Fail closed even on panic: a worker that dies without updating its
        // task must not leave the orchestrator polling `working` forever.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let (system, user) = crate::prompt::build(&seat, &material);
            provider.complete(&system, &user).and_then(|content| {
                let schema = match seat.phase {
                    crate::prompt::Phase::R3Arbiter => "arbiter-verdict.schema.json",
                    _ => "lane-output.schema.json",
                };
                crate::contract::extract_artifact(&content, schema, &seat.role)
            })
        }))
        .unwrap_or_else(|_| {
            Err(anyhow::anyhow!(
                "seat worker panicked while processing the task"
            ))
        });
        let finished = match outcome {
            Ok(artifact) => TaskRecord {
                task_id: worker_task_id.clone(),
                state: TaskState::Completed,
                artifact: Some(artifact),
                error: None,
                created_at: record.created_at.clone(),
            },
            Err(e) => TaskRecord {
                task_id: worker_task_id.clone(),
                state: TaskState::Failed,
                artifact: None,
                error: Some(e.to_string()),
                created_at: record.created_at.clone(),
            },
        };
        let _ = store.lock().unwrap().update(&finished);
    });
    Ok(json!({"id": task_id, "status": {"state": "working"}}))
}

fn get_task(
    params: Value,
    store: &Arc<Mutex<TaskStore>>,
) -> std::result::Result<Value, (i64, String)> {
    let task_id = params
        .get("id")
        .and_then(Value::as_str)
        .ok_or((ERR_INVALID_PARAMS, "params.id must be a string".to_string()))?;
    let record = store
        .lock()
        .unwrap()
        .get(task_id)
        .cloned()
        .ok_or((-32001, format!("task {task_id} not found")))?;
    let (state, artifact) = match record.state {
        TaskState::Working => ("working", None),
        TaskState::Completed => ("completed", record.artifact.clone()),
        TaskState::Failed => ("failed", None),
    };
    Ok(json!({
        "id": record.task_id,
        "status": {"state": state, "timestamp": crate::task::now()},
        "artifacts": artifact
            .map(|a| vec![json!({
                "name": "result.json",
                "parts": [{"data": a, "mediaType": "application/json"}]
            })])
            .unwrap_or_default(),
        "error": record.error,
    }))
}

/// Fold message parts into one material document: every JSON part is
/// serialized under its filename (or its index), text parts appended.
fn fold_parts(params: &Value) -> Option<String> {
    let parts = params.get("message")?.get("parts")?.as_array()?;
    if parts.is_empty() {
        return None;
    }
    let mut material = serde_json::Map::new();
    let mut text_bits: Vec<String> = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if let Some(t) = part.get("text").and_then(Value::as_str) {
            text_bits.push(t.to_string());
            continue;
        }
        if let Some(data) = part.get("data") {
            let name = part
                .get("filename")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("part-{i}"));
            material.insert(name, data.clone());
        }
    }
    if material.is_empty() && text_bits.is_empty() {
        return None;
    }
    let mut out = String::new();
    if !text_bits.is_empty() {
        out.push_str("TEXT:\n");
        out.push_str(&text_bits.join("\n"));
        out.push('\n');
    }
    out.push_str(&serde_json::to_string_pretty(&Value::Object(material)).ok()?);
    Some(out)
}

fn rpc_result(id: Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn rpc_error(id: Value, code: i64, message: &str) -> String {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}).to_string()
}
