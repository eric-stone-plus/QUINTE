//! Deterministic HIGHBALL carrier construction from a completed Result 2.1.
//!
//! QUINTE does not route or authorize; it projects its verdict into
//! HIGHBALL's route-request and residual-trace contracts so a delivery
//! stage can consume them without any model judgment. Every field is code,
//! never a model call.

use serde_json::{json, Value};

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
        "action_boundary": "none",
        "change_class": "claim",
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
            json!({
                "id": r.get("id").cloned().unwrap_or(Value::Null),
                "severity": r.get("severity").cloned().unwrap_or(json!("LOW")),
                "type": map_type(residual_type),
                "source": r.get("source").cloned().unwrap_or(Value::Null),
                "finding": r.get("finding").cloned().unwrap_or(json!("")),
                "affected_paths": r.get("affected_paths").cloned().unwrap_or_else(|| json!([])),
                "error_signature": r.get("error_signature").cloned().unwrap_or(Value::Null),
                "evidence": r.get("evidence_refs").cloned().unwrap_or_else(|| json!([])),
                "disposition": r.get("disposition").cloned().unwrap_or(json!("unresolved")),
                "required_closure": r.get("required_closure").cloned().unwrap_or(json!("none")),
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
        "action_boundary": "none",
        "highball_decision": decision(result),
        "action_binding_sha256": result.get("action_binding_sha256").cloned().unwrap_or(Value::Null),
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
