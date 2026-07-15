use std::fs;

use assert_cmd::Command;
use quinte::policy::default_policy;
use serde_json::{Value, json};
use tempfile::tempdir;

fn legacy_policy() -> Value {
    let mut policy = serde_json::to_value(default_policy()).unwrap();
    let object = policy.as_object_mut().unwrap();
    let mut arbiter = object.remove("counterpart_arbiter").unwrap();
    arbiter["party_id"] = json!("Auditor B");
    object.insert("auditor".into(), arbiter);
    policy
}

#[test]
fn legacy_arbiter_names_are_normalized_without_rewriting_policy() {
    let home = tempdir().unwrap();
    let policy_path = home.path().join("policy.json");
    let original = serde_json::to_vec_pretty(&legacy_policy()).unwrap();
    fs::write(&policy_path, &original).unwrap();

    let output = Command::cargo_bin("quinte")
        .unwrap()
        .args([
            "--home",
            home.path().to_str().unwrap(),
            "policy",
            "show",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    let effective = &envelope["data"];
    assert!(effective.get("auditor").is_none());
    assert_eq!(
        effective["counterpart_arbiter"]["party_id"],
        "Counterpart Arbiter"
    );
    assert_eq!(fs::read(&policy_path).unwrap(), original);

    let output = Command::cargo_bin("quinte")
        .unwrap()
        .args([
            "--home",
            home.path().to_str().unwrap(),
            "policy",
            "validate",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["data"]["valid"], true);
    assert_eq!(fs::read(&policy_path).unwrap(), original);

    let output = Command::cargo_bin("quinte")
        .unwrap()
        .args(["--home", home.path().to_str().unwrap(), "doctor", "--json"])
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        envelope["data"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["party_id"] == "Counterpart Arbiter")
    );
    assert_eq!(fs::read(&policy_path).unwrap(), original);

    let output = Command::cargo_bin("quinte")
        .unwrap()
        .args(["--home", home.path().to_str().unwrap(), "init"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("use --force to replace it"));
    assert_eq!(fs::read(&policy_path).unwrap(), original);
}

#[test]
fn legacy_alias_does_not_relax_the_arbiter_identity_invariant() {
    let home = tempdir().unwrap();
    let policy_path = home.path().join("policy.json");
    let mut policy = legacy_policy();
    policy["auditor"]["party_id"] = json!("Different Arbiter");
    fs::write(&policy_path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();

    let error = quinte::policy::load(&policy_path).unwrap_err().to_string();
    assert!(error.contains("policy must bind required Counterpart Arbiter"));
}

#[test]
fn partial_legacy_arbiter_names_are_rejected() {
    let home = tempdir().unwrap();
    let policy_path = home.path().join("policy.json");

    let mut canonical_field = serde_json::to_value(default_policy()).unwrap();
    canonical_field["counterpart_arbiter"]["party_id"] = json!("Auditor B");
    fs::write(
        &policy_path,
        serde_json::to_vec_pretty(&canonical_field).unwrap(),
    )
    .unwrap();
    let error = quinte::policy::load(&policy_path).unwrap_err().to_string();
    assert!(error.contains("policy must bind required Counterpart Arbiter"));

    let mut legacy_field = legacy_policy();
    legacy_field["auditor"]["party_id"] = json!("Counterpart Arbiter");
    fs::write(
        &policy_path,
        serde_json::to_vec_pretty(&legacy_field).unwrap(),
    )
    .unwrap();
    let error = quinte::policy::load(&policy_path).unwrap_err().to_string();
    assert!(error.contains("policy must bind required Counterpart Arbiter"));
}

#[test]
fn old_and_new_arbiter_fields_together_are_rejected_as_ambiguous() {
    let home = tempdir().unwrap();
    let policy_path = home.path().join("policy.json");
    let mut policy = legacy_policy();
    policy["counterpart_arbiter"] = policy["auditor"].clone();
    fs::write(&policy_path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();

    let error = quinte::policy::load(&policy_path).unwrap_err();
    let detail = format!("{error:#}");
    let error = error.to_string();
    assert!(error.contains("invalid JSON in"));
    assert!(detail.contains("duplicate field"));
}
