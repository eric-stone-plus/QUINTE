//! Deterministic HIGHBALL carrier construction from a completed Result 2.1.
//!
//! QUINTE does not route or authorize; it projects its verdict into
//! HIGHBALL's route-request and residual-trace contracts so a delivery
//! stage can consume them without any model judgment. Every field is code,
//! never a model call.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// HIGHBALL's action binding is the sha256 of the canonical JSON of exactly
/// these route-request fields (contracts.rs ACTION_BINDING_FIELDS); the
/// delivered trace must bind to the route request we actually emit.
const ACTION_BINDING_FIELDS: [&str; 4] =
    ["question", "action_boundary", "change_class", "affected_paths"];

/// QUINTE never routes protected writes or code changes. These defaults are
/// the single source of truth for the route request and for the binding a
/// brief carries from intake on.
const DEFAULT_ACTION_BOUNDARY: &str = "none";
const DEFAULT_CHANGE_CLASS: &str = "claim";

pub(crate) fn action_binding_sha256(route_request: &Value) -> String {
    let mut payload = serde_json::Map::new();
    for field in ACTION_BINDING_FIELDS {
        payload.insert(
            field.to_string(),
            route_request.get(field).cloned().unwrap_or(Value::Null),
        );
    }
    let bytes = crate::finance::canonical_json(&Value::Object(payload))
        .expect("action binding payload is finite JSON");
    format!("sha256:{:x}", Sha256::digest(&bytes))
}

/// The action binding fixed at brief intake: the digest HIGHBALL recomputes
/// over the route request this brief's result will later emit. Deriving it
/// here keeps brief, result, and residual trace bound to the same request.
pub fn intake_action_binding(question: &str, affected_paths: &Value) -> String {
    action_binding_sha256(&json!({
        "question": question,
        "action_boundary": DEFAULT_ACTION_BOUNDARY,
        "change_class": DEFAULT_CHANGE_CLASS,
        "affected_paths": affected_paths,
    }))
}

/// HIGHBALL's residual trace expects `required_closure` from a closed enum
/// (none|edit|test|command|block|waiver|human_review) while QUINTE seats
/// write a free-text sentence. Classify deterministically, most binding
/// outcome first; anything unrecognized escalates to human_review rather
/// than guessing a weaker closure. The original sentence stays available in
/// review.result; the trace is a projection, not the record of it.
fn map_required_closure(text: &str) -> &'static str {
    // Already-enum values (older fixtures, disciplined seats) pass through.
    for value in ["none", "edit", "test", "command", "block", "waiver", "human_review"] {
        if text == value {
            return value;
        }
    }
    let lower = text.to_lowercase();
    if lower.contains("waiv") {
        "waiver"
    } else if lower.contains("human") {
        "human_review"
    } else if lower.contains("block") || lower.contains("reject") || lower.contains("do not") {
        "block"
    } else if lower.contains("test") || lower.contains("verify") || lower.contains("validate") {
        "test"
    } else if lower.contains("command") || lower.contains("re-run") || lower.contains("rerun") {
        "command"
    } else if lower.contains("edit")
        || lower.contains("fix")
        || lower.contains("supply")
        || lower.contains("provide")
        || lower.contains("attach")
        || lower.contains("add ")
    {
        "edit"
    } else {
        "human_review"
    }
}

/// HIGHBALL expects `evidence` as a single string or null; QUINTE seats
/// emit a list of refs. Join them so no reference is silently dropped.
fn evidence_string(refs: Option<&Value>) -> Value {
    let joined: Vec<&str> = refs
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if joined.is_empty() {
        Value::Null
    } else {
        json!(joined.join(", "))
    }
}

/// Map a QUINTE residual_type onto HIGHBALL's closed type vocabulary. The
/// vocabularies overlap imperfectly; the mapping picks the closest
/// structural reading and never invents a type.
fn map_type(residual_type: &str) -> &'static str {
    match residual_type {
        "evidence-gap" => "evidence_gap",
        "model-limitation" => "confidence_mismatch",
        "engineering-defect" => "execution_mismatch",
        "methodology-flaw" => "contradiction",
        "contract-ambiguity" | "compliance-risk" => "contradiction",
        "data-quality" | "protocol-gap" | "scope-limitation" => "omission",
        _ => "omission",
    }
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "P0" => 5,
        "CRITICAL" => 4,
        "HIGH" => 3,
        "MEDIUM" => 2,
        "LOW" => 1,
        _ => 0,
    }
}

fn is_high(severity: &str) -> bool {
    matches!(severity, "HIGH" | "CRITICAL" | "P0")
}

fn is_open_closure(state: Option<&str>) -> bool {
    !matches!(state, Some("closed") | Some("waived") | Some("not_applicable"))
}

pub fn residuals_of(result: &Value) -> Vec<Value> {
    result
        .get("residuals")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// The delivery decision, derived from the residual set only: no residuals
/// pass; any open high-risk residual blocks; everything else reviews.
pub fn decision(result: &Value) -> &'static str {
    let residuals = residuals_of(result);
    if residuals.is_empty() {
        return "pass";
    }
    if residuals.iter().any(|r| {
        is_high(r.get("severity").and_then(Value::as_str).unwrap_or(""))
            && is_open_closure(r.get("closure_state").and_then(Value::as_str))
    }) {
        return "block";
    }
    "review"
}

/// Build HIGHBALL's route request (the submitter's action intent as
/// reviewed). Conservative defaults: no protected writes, claim-class
/// change, not executable — a stricter submission overrides per field.
pub fn route_request(result: &Value) -> Value {
    let residuals = residuals_of(result);
    let max_severity = residuals
        .iter()
        .map(|r| r.get("severity").and_then(Value::as_str).unwrap_or("LOW"))
        .max_by_key(|s| severity_rank(s))
        .unwrap_or("LOW")
        .to_string();
    let open_high = residuals
        .iter()
        .filter(|r| {
            is_high(r.get("severity").and_then(Value::as_str).unwrap_or(""))
                && is_open_closure(r.get("closure_state").and_then(Value::as_str))
        })
        .count() as i64;
    json!({
        "question": result.get("question").cloned().unwrap_or(json!("review verdict")),
        "action_boundary": DEFAULT_ACTION_BOUNDARY,
        "change_class": DEFAULT_CHANGE_CLASS,
        "affected_paths": result.get("affected_paths").cloned().unwrap_or_else(|| json!([])),
        "action_scope": result.get("action_scope").cloned().unwrap_or(Value::Null),
        "executable": false,
        "risk": max_severity,
        "trace_quality_gate": decision(result),
        "open_high_risk_count": open_high,
    })
}

/// Build HIGHBALL's residual trace (the review's evidence trail).
pub fn residual_trace(result: &Value) -> Value {
    let residuals: Vec<Value> = residuals_of(result)
        .iter()
        .map(|r| {
            let residual_type = r
                .get("residual_type")
                .and_then(Value::as_str)
                .unwrap_or("scope-limitation");
            let closure_text = r
                .get("required_closure")
                .and_then(Value::as_str)
                .unwrap_or("none");
            json!({
                "id": r.get("id").cloned().unwrap_or(Value::Null),
                "severity": r.get("severity").cloned().unwrap_or(json!("LOW")),
                "type": map_type(residual_type),
                "source": r.get("source").cloned().unwrap_or(Value::Null),
                "finding": r.get("finding").cloned().unwrap_or(json!("")),
                "affected_paths": r.get("affected_paths").cloned().unwrap_or_else(|| json!([])),
                "error_signature": r.get("error_signature").cloned().unwrap_or(Value::Null),
                "evidence": evidence_string(r.get("evidence_refs")),
                "disposition": r.get("disposition").cloned().unwrap_or(json!("unresolved")),
                "required_closure": map_required_closure(closure_text),
                "closure_state": r.get("closure_state").cloned().unwrap_or(json!("open")),
                "closure_evidence": r.get("closure_evidence").cloned().unwrap_or_else(|| json!([])),
                "scope": r.get("scope").cloned().unwrap_or(json!("")),
            })
        })
        .collect();
    json!({
        "trace_version": "1.1",
        "question": result.get("question").cloned().unwrap_or(json!("review verdict")),
        "instrument": "QUINTE",
        "residuals": residuals,
        "action_boundary": DEFAULT_ACTION_BOUNDARY,
        "highball_decision": decision(result),
        "action_binding_sha256": action_binding_sha256(&route_request(result)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> Value {
        json!({
            "question": "Is this strategy acceptable?",
            "action_scope": "decision support only",
            "affected_paths": [],
            "action_binding_sha256": null,
            "residuals": [
                {"id": "r1", "severity": "LOW", "residual_type": "evidence-gap",
                 "source": "lane-a", "finding": "metric missing",
                 "evidence_refs": [], "disposition": "unresolved",
                 "required_closure": "edit", "closure_state": "open",
                 "closure_evidence": [], "scope": "walkforward"},
                {"id": "r2", "severity": "HIGH", "residual_type": "methodology-flaw",
                 "source": "lane-b", "finding": "train/test leak",
                 "evidence_refs": ["snapshot://root-0/x.json"], "disposition": "unresolved",
                 "required_closure": "block", "closure_state": "open",
                 "closure_evidence": [], "scope": "walkforward"}
            ]
        })
    }

    #[test]
    fn decision_blocks_on_open_high_residual() {
        assert_eq!(decision(&sample_result()), "block");
        let clean = json!({"residuals": []});
        assert_eq!(decision(&clean), "pass");
        let low = json!({"residuals": [
            {"id": "r1", "severity": "LOW", "closure_state": "open",
             "residual_type": "evidence-gap", "finding": "x",
             "source": "a", "evidence_refs": [], "disposition": "unresolved",
             "required_closure": "edit", "closure_evidence": [], "scope": "s"}
        ]});
        assert_eq!(decision(&low), "review");
    }

    #[test]
    fn trace_carries_mapped_types_and_bounded_fields() {
        let trace = residual_trace(&sample_result());
        assert_eq!(trace["trace_version"], "1.1");
        assert_eq!(trace["instrument"], "QUINTE");
        assert_eq!(trace["highball_decision"], "block");
        let residuals = trace["residuals"].as_array().unwrap();
        assert_eq!(residuals.len(), 2);
        assert_eq!(residuals[0]["type"], "evidence_gap");
        assert_eq!(residuals[1]["type"], "contradiction");
        assert!(residuals[0].get("error_signature").is_some());
    }

    #[test]
    fn trace_matches_highball_contract_shape() {
        // Regression: the live trace failed HIGHBALL validation on all three
        // projections below (array evidence, free-text closure, null binding).
        let trace = residual_trace(&sample_result());
        let binding = trace["action_binding_sha256"].as_str().unwrap();
        assert!(binding.starts_with("sha256:") && binding.len() == 7 + 64);
        for residual in trace["residuals"].as_array().unwrap() {
            let evidence = &residual["evidence"];
            assert!(evidence.is_null() || evidence.is_string());
            let closure = residual["required_closure"].as_str().unwrap();
            assert!(
                ["none", "edit", "test", "command", "block", "waiver", "human_review"]
                    .contains(&closure),
                "unexpected required_closure {closure}"
            );
        }
    }

    #[test]
    fn action_binding_matches_highball_golden_vector() {
        // Golden vector from HIGHBALL contracts.rs: the canonicalization and
        // digest must agree byte-for-byte across the two repositories.
        let request = json!({
            "action_boundary": "protected_write",
            "affected_paths": ["HIGHBALL\\bin\\tool.py", "a/b.py"],
            "change_class": "code",
            "question": "May this change proceed?"
        });
        assert_eq!(
            action_binding_sha256(&request),
            "sha256:05f2997ec8dfce94e74fb15b12a6901ac34b7265905cbca8ce5dc35cad110c9e"
        );
    }

    #[test]
    fn trace_binds_to_the_emitted_route_request() {
        let result = sample_result();
        let trace = residual_trace(&result);
        assert_eq!(
            trace["action_binding_sha256"].as_str().unwrap(),
            action_binding_sha256(&route_request(&result))
        );
    }

    #[test]
    fn intake_binding_matches_route_request_defaults() {
        // The intake helper and route_request share the none/claim defaults;
        // this pins them against silent drift.
        let result = json!({
            "question": "Adopt the proposal?",
            "affected_paths": ["a/b.py"],
            "residuals": []
        });
        assert_eq!(
            intake_action_binding("Adopt the proposal?", &json!(["a/b.py"])),
            action_binding_sha256(&route_request(&result))
        );
    }

    #[test]
    fn required_closure_free_text_maps_conservatively() {
        assert_eq!(map_required_closure("edit"), "edit");
        assert_eq!(
            map_required_closure("Supply an event calendar or explicitly waive it"),
            "waiver"
        );
        assert_eq!(
            map_required_closure("Escalate to the desk head for a ruling"),
            "human_review"
        );
        assert_eq!(map_required_closure("Rerun the intake command"), "command");
        assert_eq!(map_required_closure("Fix the walkforward window"), "edit");
    }

    #[test]
    fn evidence_refs_join_into_one_string() {
        assert_eq!(evidence_string(None), Value::Null);
        assert_eq!(evidence_string(Some(&json!([]))), Value::Null);
        assert_eq!(
            evidence_string(Some(&json!(["snapshot://root-0/a.json", "snapshot://root-0/b.csv"]))),
            json!("snapshot://root-0/a.json, snapshot://root-0/b.csv")
        );
    }

    #[test]
    fn route_request_is_conservative() {
        let route = route_request(&sample_result());
        assert_eq!(route["action_boundary"], "none");
        assert_eq!(route["change_class"], "claim");
        assert_eq!(route["executable"], false);
        assert_eq!(route["risk"], "HIGH");
        assert_eq!(route["open_high_risk_count"], 1);
        assert_eq!(route["trace_quality_gate"], "block");
    }
}
