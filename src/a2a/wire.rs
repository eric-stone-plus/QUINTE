//! JSON-RPC 2.0 / A2A envelope helpers. No run I/O: tests hit these
//! handlers with representative STAMMTISCH bodies and invalid messages.

use serde_json::{Value, json};

use crate::model::RunStatus;
use crate::schema::{BRIEF_SCHEMA, validate_value};

pub const A2A_VERSION: &str = "1.0";
pub const A2A_VERSION_HEADER: &str = "A2A-Version";

pub const ERR_PARSE: i64 = -32700;
pub const ERR_INVALID_REQUEST: i64 = -32600;
pub const ERR_METHOD_NOT_FOUND: i64 = -32601;
pub const ERR_INVALID_PARAMS: i64 = -32602;
pub const ERR_INTERNAL: i64 = -32603;
pub const ERR_VERSION: i64 = -32000;
pub const ERR_TASK_NOT_FOUND: i64 = -32001;
pub const ERR_NOT_CANCELABLE: i64 = -32002;
pub const ERR_BUSY_RUN: i64 = -32010;
pub const ERR_BRIEF_INVALID: i64 = -32011;
pub const ERR_POLICY: i64 = -32012;
pub const ERR_CHALLENGE: i64 = -32013;

pub const ARTIFACT_NAME: &str = "review.result";

#[derive(Debug, Clone)]
pub struct RpcRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub struct A2aError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

impl A2aError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_state(code: i64, message: impl Into<String>, state_code: &str) -> Self {
        Self {
            code,
            message: message.into(),
            data: Some(json!({ "state": { "code": state_code } })),
        }
    }

    pub fn brief_invalid(detail: impl Into<String>) -> Self {
        Self::with_state(ERR_BRIEF_INVALID, detail, "brief_invalid")
    }

    pub fn busy_run(active: &[String]) -> Self {
        let mut err = Self::with_state(
            ERR_BUSY_RUN,
            "a non-terminal review task already exists",
            "busy_run",
        );
        if let Some(data) = err.data.as_mut() {
            data["state"]["active_run_ids"] = json!(active);
        }
        err
    }
}

pub fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

pub fn rpc_error(id: Value, err: &A2aError) -> Value {
    let mut error = json!({
        "code": err.code,
        "message": err.message,
    });
    if let Some(data) = &err.data {
        error["data"] = data.clone();
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

pub fn parse_rpc(body: &str) -> Result<RpcRequest, A2aError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| A2aError::new(ERR_PARSE, format!("parse error: {e}")))?;
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(A2aError::new(
            ERR_INVALID_REQUEST,
            "jsonrpc must be \"2.0\"",
        ));
    }
    let id = value.get("id").cloned().unwrap_or(Value::Null);
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| A2aError::new(ERR_INVALID_REQUEST, "method must be a string"))?
        .to_string();
    if method.is_empty() {
        return Err(A2aError::new(
            ERR_INVALID_REQUEST,
            "method must be a string",
        ));
    }
    let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
    Ok(RpcRequest { id, method, params })
}

pub fn check_a2a_version(header: Option<&str>) -> Result<(), A2aError> {
    match header.map(str::trim) {
        Some(A2A_VERSION) => Ok(()),
        Some(other) => Err(A2aError::new(
            ERR_VERSION,
            format!("unsupported A2A-Version '{other}'"),
        )),
        None => Err(A2aError::new(ERR_VERSION, "missing A2A-Version header")),
    }
}

/// Map a QUINTE run status onto the HOST.md §4 A2A task state.
pub fn map_run_status(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued | RunStatus::Preflight => "TASK_STATE_SUBMITTED",
        RunStatus::R1Running
        | RunStatus::R1Gate
        | RunStatus::R2Packet
        | RunStatus::R2Running
        | RunStatus::R2Gate
        | RunStatus::R3Cc
        | RunStatus::Merging => "TASK_STATE_WORKING",
        RunStatus::WaitingPrimaryArbiter => "TASK_STATE_INPUT_REQUIRED",
        RunStatus::Completed | RunStatus::Degraded => "TASK_STATE_COMPLETED",
        RunStatus::Failed | RunStatus::FailedPolicy => "TASK_STATE_FAILED",
        RunStatus::Cancelled | RunStatus::Cancelling => "TASK_STATE_CANCELED",
    }
}

/// Pull the closed-schema Brief out of a STAMMTISCH (or any) SendMessage
/// body. Accepts a native QUINTE Brief, or a host-facing JSON part
/// (filename `brief.json`, GALAHAD brief, etc.) that can be projected
/// onto the QUINTE Brief revision.
pub fn extract_brief(message: &Value) -> Result<Value, A2aError> {
    let parts = message
        .get("parts")
        .and_then(Value::as_array)
        .ok_or_else(|| A2aError::brief_invalid("message carries no parts"))?;

    if parts.iter().any(|part| {
        part.get("data")
            .and_then(|data| data.get("finance_review_invocation_version"))
            .is_some()
    }) {
        return Err(A2aError::brief_invalid(
            "finance invocation cannot enter the generic Brief endpoint",
        ));
    }

    let mut named: Option<&Value> = None;
    let mut json_parts: Vec<&Value> = Vec::new();
    for part in parts {
        let data = match part.get("data") {
            Some(d) if !d.is_null() => d,
            _ => continue,
        };
        let filename = part.get("filename").and_then(Value::as_str).unwrap_or("");
        let media = part.get("mediaType").and_then(Value::as_str).unwrap_or("");
        if filename == "brief.json" || media == "application/json" || data.is_object() {
            json_parts.push(data);
            if filename == "brief.json" {
                named = Some(data);
            }
        }
    }

    let candidate = if let Some(named) = named {
        named
    } else if json_parts.len() == 1 {
        json_parts[0]
    } else if json_parts.is_empty() {
        return Err(A2aError::brief_invalid(
            "message carries no closed-schema Brief",
        ));
    } else {
        return Err(A2aError::brief_invalid(
            "message carries multiple JSON parts and none is named brief.json",
        ));
    };

    project_brief(candidate)
}

pub fn project_brief(candidate: &Value) -> Result<Value, A2aError> {
    if !candidate.is_object() {
        return Err(A2aError::brief_invalid("Brief must be a JSON object"));
    }
    if candidate.get("brief_version").is_some() {
        validate_value(candidate, BRIEF_SCHEMA).map_err(|e| {
            A2aError::brief_invalid(format!("Brief violates the closed schema: {e:#}"))
        })?;
        // A native brief may arrive with a null binding; derive it at intake
        // so brief, result, and residual trace bind the same route request.
        let mut brief = candidate.clone();
        if brief.get("action_binding_sha256").map_or(true, Value::is_null) {
            let question = brief
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("");
            let affected_paths = brief
                .get("affected_paths")
                .cloned()
                .unwrap_or_else(|| json!([]));
            brief["action_binding_sha256"] =
                json!(crate::highball_carriers::intake_action_binding(
                    question,
                    &affected_paths
                ));
        }
        return Ok(brief);
    }
    let mapped = map_host_brief(candidate)?;
    validate_value(&mapped, BRIEF_SCHEMA).map_err(|e| {
        A2aError::brief_invalid(format!(
            "projected Brief is not a closed-schema Brief: {e:#}"
        ))
    })?;
    Ok(mapped)
}

fn map_host_brief(value: &Value) -> Result<Value, A2aError> {
    let title = value
        .get("title")
        .and_then(Value::as_str)
        .or_else(|| value.get("question").and_then(Value::as_str))
        .unwrap_or("")
        .trim();
    let objectives = value
        .get("objectives")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let question = if !title.is_empty() && !objectives.is_empty() {
        format!("{title}: {objectives}")
    } else if !title.is_empty() {
        title.to_string()
    } else {
        objectives
    };
    if question.is_empty() {
        return Err(A2aError::brief_invalid(
            "message carries no closed-schema Brief",
        ));
    }
    let action_scope = match value.get("action_scope") {
        Some(Value::String(s)) if !s.is_empty() => json!(s),
        Some(Value::Null) | None => json!("decision support only"),
        Some(other) => other.clone(),
    };
    let context = serde_json::to_string(value).ok();
    // Host-facing briefs may carry evidence_roots (operator-local files the
    // review must snapshot); projecting them away would silently ship a
    // review over no evidence. Non-string entries are dropped, never
    // coerced.
    let evidence_roots: Vec<Value> = value
        .get("evidence_roots")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.as_str().is_some_and(|s| !s.is_empty()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let affected_paths = json!([]);
    // Bind the brief at intake to the route request its result will emit;
    // HIGHBALL product evidence rejects a null or drifting binding.
    let action_binding =
        crate::highball_carriers::intake_action_binding(&question, &affected_paths);
    Ok(json!({
        "brief_version": "1.1",
        "question": question,
        "context": context,
        "evidence_roots": evidence_roots,
        "snapshot_ignore": [],
        "attachments": [],
        "action_scope": action_scope,
        "affected_paths": affected_paths,
        "action_binding_sha256": action_binding
    }))
}

pub fn send_configuration(params: &Value) -> (bool, u64) {
    let cfg = params.get("configuration").cloned().unwrap_or(json!({}));
    let return_immediately = cfg
        .get("returnImmediately")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let history_length = cfg
        .get("historyLength")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    (return_immediately, history_length)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stammtisch_message(brief: Value) -> Value {
        json!({
            "messageId": "m-1",
            "contextId": "run-1",
            "role": "ROLE_USER",
            "parts": [
                {"text": "STAMMTISCH pipeline 'quant' run 'run-1' stage 'review': review the attached evidence artifact(s) and return the product's contract artifacts."},
                {
                    "data": brief,
                    "filename": "brief.json",
                    "mediaType": "application/json"
                }
            ]
        })
    }

    #[test]
    fn parse_rpc_echoes_numeric_id() {
        let req = parse_rpc(r#"{"jsonrpc":"2.0","id":7,"method":"GetTask","params":{"id":"t"}}"#)
            .unwrap();
        assert_eq!(req.id, json!(7));
        assert_eq!(req.method, "GetTask");
    }

    #[test]
    fn missing_version_is_a_version_error() {
        let err = check_a2a_version(None).unwrap_err();
        assert_eq!(err.code, ERR_VERSION);
    }

    #[test]
    fn stammtisch_galahad_brief_projects_to_quinte() {
        let brief = json!({
            "schema": "galahad.brief.v0",
            "title": "Daily quant research brief",
            "pipeline": "quant-research-daily",
            "run_id": "run-1",
            "pack_sha256": format!("sha256:{}", "a".repeat(64)),
            "objectives": ["Evaluate one candidate strategy"],
            "acceptance_gates": ["quinte_result_21"]
        });
        let extracted = extract_brief(&stammtisch_message(brief)).unwrap();
        assert_eq!(extracted["brief_version"], "1.1");
        assert!(
            extracted["question"]
                .as_str()
                .unwrap()
                .contains("Daily quant research brief")
        );
        assert_eq!(
            extracted["action_binding_sha256"].as_str().unwrap(),
            crate::highball_carriers::intake_action_binding(
                extracted["question"].as_str().unwrap(),
                &extracted["affected_paths"]
            )
        );
        validate_value(&extracted, BRIEF_SCHEMA).unwrap();
    }

    #[test]
    fn host_brief_evidence_roots_are_projected_not_dropped() {
        let brief = json!({
            "schema": "galahad.brief.v0",
            "title": "Daily quant research brief",
            "pipeline": "stock-quinte-evidence",
            "run_id": "run-1",
            "pack_sha256": format!("sha256:{}", "a".repeat(64)),
            "objectives": ["Evaluate one candidate strategy"],
            "acceptance_gates": ["quinte_result_21"],
            "evidence_roots": ["/evidence/candidate.json", 42, ""]
        });
        let extracted = extract_brief(&stammtisch_message(brief)).unwrap();
        let roots = extracted["evidence_roots"].as_array().unwrap();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], "/evidence/candidate.json");
        validate_value(&extracted, BRIEF_SCHEMA).unwrap();
    }

    #[test]
    fn native_quinte_brief_is_accepted() {
        let brief = json!({
            "brief_version": "1.1",
            "question": "Which residuals remain?",
            "context": "full protocol",
            "evidence_roots": [],
            "snapshot_ignore": [],
            "attachments": [],
            "action_scope": "decision support only",
            "affected_paths": [],
            "action_binding_sha256": null
        });
        let extracted = extract_brief(&stammtisch_message(brief.clone())).unwrap();
        let mut expected = brief;
        // A null binding is derived at intake, not carried through.
        expected["action_binding_sha256"] =
            json!(crate::highball_carriers::intake_action_binding(
                "Which residuals remain?",
                &json!([])
            ));
        assert_eq!(extracted, expected);
    }

    #[test]
    fn intake_binding_matches_the_emitted_route_request() {
        // HIGHBALL product evidence: brief and result must both carry the
        // binding of the route request derived from that result.
        let brief = json!({
            "schema": "galahad.brief.v0",
            "title": "Daily quant research brief",
            "objectives": ["Evaluate one candidate strategy"]
        });
        let extracted = extract_brief(&stammtisch_message(brief)).unwrap();
        // run.rs copies question, affected_paths, and the binding from brief
        // to result unchanged.
        let result = json!({
            "question": extracted["question"].clone(),
            "affected_paths": extracted["affected_paths"].clone(),
            "action_binding_sha256": extracted["action_binding_sha256"].clone(),
        });
        assert_eq!(
            crate::highball_carriers::action_binding_sha256(
                &crate::highball_carriers::route_request(&result)
            ),
            extracted["action_binding_sha256"].as_str().unwrap()
        );
    }

    #[test]
    fn message_without_brief_is_minus_32011() {
        let message = json!({
            "messageId": "m-1",
            "role": "ROLE_USER",
            "parts": [{"text": "hello only"}]
        });
        let err = extract_brief(&message).unwrap_err();
        assert_eq!(err.code, ERR_BRIEF_INVALID);
    }

    #[test]
    fn closed_schema_rejects_unknown_fields() {
        let brief = json!({
            "brief_version": "1.1",
            "question": "x",
            "extra": true
        });
        let err = extract_brief(&stammtisch_message(brief)).unwrap_err();
        assert_eq!(err.code, ERR_BRIEF_INVALID);
    }

    #[test]
    fn generic_endpoint_rejects_native_finance_invocation() {
        let message = json!({"parts": [{"data": {"finance_review_invocation_version": "1.0"}}]});
        let error = extract_brief(&message).unwrap_err();
        assert_eq!(error.code, ERR_BRIEF_INVALID);
        assert!(error.message.contains("finance invocation"));
    }

    #[test]
    fn run_status_mapping_matches_host_md() {
        assert_eq!(map_run_status(RunStatus::Queued), "TASK_STATE_SUBMITTED");
        assert_eq!(map_run_status(RunStatus::R1Running), "TASK_STATE_WORKING");
        assert_eq!(
            map_run_status(RunStatus::WaitingPrimaryArbiter),
            "TASK_STATE_INPUT_REQUIRED"
        );
        assert_eq!(map_run_status(RunStatus::Degraded), "TASK_STATE_COMPLETED");
        assert_eq!(map_run_status(RunStatus::FailedPolicy), "TASK_STATE_FAILED");
        assert_eq!(map_run_status(RunStatus::Cancelled), "TASK_STATE_CANCELED");
    }
}
