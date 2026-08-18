use assert_cmd::Command;
use predicates::prelude::*;

fn quinte(home: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("quinte").unwrap();
    command.env("QUINTE_HOME", home);
    command
}

fn write(dir: &tempfile::TempDir, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

const VALID_VERDICT: &str = r#"{
    "arbiter_verdict_version": "1.0",
    "summary": "Both arbiters converge on the same bounded recommendation.",
    "recommendation": "Adopt the proposal with the listed residuals tracked to closure.",
    "residuals": [{
        "id": "residual-1",
        "severity": "MEDIUM",
        "residual_type": "evidence_gap",
        "source": "R1/Party A",
        "finding": "One assertion lacks independent confirmation.",
        "evidence_refs": [],
        "disposition": "unresolved",
        "required_closure": "human_review",
        "closure_state": "open",
        "closure_evidence": [],
        "scope": "This review only"
    }]
}"#;

#[test]
fn validate_accepts_a_full_verdict_and_a_brief() {
    let temporary = tempfile::tempdir().unwrap();
    let verdict = write(&temporary, "verdict.json", VALID_VERDICT);
    quinte(temporary.path())
        .args(["validate", "--kind", "verdict"])
        .arg(&verdict)
        .assert()
        .success()
        .stdout(predicate::str::contains("is a valid verdict"));

    let brief = write(
        &temporary,
        "brief.json",
        r#"{"brief_version": "1.1", "question": "Adopt the proposal?"}"#,
    );
    quinte(temporary.path())
        .args(["validate", "--kind", "brief"])
        .arg(&brief)
        .assert()
        .success()
        .stdout(predicate::str::contains("is a valid brief"));
}

#[test]
fn validate_distinguishes_syntax_errors_from_schema_mismatches() {
    let temporary = tempfile::tempdir().unwrap();

    let syntax = write(&temporary, "syntax.json", "{\"arbiter_verdict_version\":");
    quinte(temporary.path())
        .args(["validate", "--kind", "verdict"])
        .arg(&syntax)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("invalid JSON syntax in"));

    // Syntactically valid but missing required Residual fields: report schema,
    // with the field-level cause
    let mismatch = write(
        &temporary,
        "mismatch.json",
        r#"{
            "arbiter_verdict_version": "1.0",
            "summary": "s",
            "recommendation": "r",
            "residuals": [{"id": "r1", "severity": "LOW"}]
        }"#,
    );
    quinte(temporary.path())
        .args(["validate", "--kind", "verdict"])
        .arg(&mismatch)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("does not match expected schema"))
        .stdout(predicate::str::contains("missing field `residual_type`"));

    // deny_unknown_fields: extra fields report schema too
    let unknown = write(
        &temporary,
        "unknown.json",
        r#"{
            "arbiter_verdict_version": "1.0",
            "summary": "s",
            "recommendation": "r",
            "residuals": [],
            "surprise": true
        }"#,
    );
    quinte(temporary.path())
        .args(["validate", "--kind", "verdict"])
        .arg(&unknown)
        .assert()
        .code(1)
        .stdout(predicate::str::contains("unknown field `surprise`"));
}

#[test]
fn validate_json_output_marks_validity_and_keeps_exit_codes() {
    let temporary = tempfile::tempdir().unwrap();
    let verdict = write(&temporary, "verdict.json", VALID_VERDICT);

    let output = quinte(temporary.path())
        .args(["validate", "--kind", "verdict", "--json"])
        .arg(&verdict)
        .output()
        .unwrap();
    assert!(output.status.success());
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["valid"], true);
    assert_eq!(envelope["data"]["kind"], "verdict");

    let brief = write(&temporary, "brief.json", r#"{"brief_version": "1.1"}"#);
    let output = quinte(temporary.path())
        .args(["validate", "--kind", "brief", "--json"])
        .arg(&brief)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["data"]["valid"], false);
    assert!(
        envelope["data"]["error"]
            .as_str()
            .unwrap()
            .contains("missing field `question`")
    );
}

#[test]
fn validate_rejects_an_unknown_kind_and_a_missing_file() {
    let temporary = tempfile::tempdir().unwrap();
    quinte(temporary.path())
        .args(["validate", "--kind", "policy", "x.json"])
        .assert()
        .failure();

    quinte(temporary.path())
        .args(["validate", "--kind", "verdict"])
        .arg(temporary.path().join("missing.json"))
        .assert()
        .code(1)
        .stdout(predicate::str::contains("cannot read"));
}
