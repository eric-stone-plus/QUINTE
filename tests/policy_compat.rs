use std::fs;

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::tempdir;

fn legacy_policy() -> Value {
    json!({
        "policy_version": "1.0",
        "roster": [
            {"party_id":"Party A","route_id":"codewhale","adapter":"codewhale","executable":"codewhale","required":true},
            {"party_id":"Party B","route_id":"opencode","adapter":"opencode","executable":"opencode","required":true},
            {"party_id":"Party C","route_id":"kilo","adapter":"kilo","executable":"kilo","required":true},
            {"party_id":"Party D","route_id":"mimo","adapter":"mimo","executable":"mimo","required":true},
            {"party_id":"Party E","route_id":"omp","adapter":"omp","executable":"omp","required":true}
        ],
        "auditor": {"party_id":"Auditor B","route_id":"cc","adapter":"claude","executable":"claude","required":true},
        "text_model": "mimo-v2.5-pro",
        "multimodal_model": "mimo-v2.5",
        "max_parallel_r1": 5,
        "max_parallel_r2": 1,
        "r2_parallel": false,
        "max_attempts": 3,
        "timeout_seconds": 300,
        "retry_backoff_seconds": 15,
        "retry_backoff_max_seconds": 120,
        "r2_min_interval_seconds": 10,
        "max_output_bytes": 1048576,
        "max_snapshot_files": 2000,
        "max_snapshot_bytes": 20971520,
        "max_attachment_bytes": 10485760,
        "sandbox_mode": "process"
    })
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
    assert_eq!(effective["policy_version"], "2.0");
    assert_eq!(effective["auto_primary_arbiter"], false);
    assert_eq!(effective["primary_arbiter"]["party_id"], "Primary Arbiter");
    let seat = &effective["seat"];
    let routes = effective["roster"]
        .as_array()
        .unwrap()
        .iter()
        .chain(std::iter::once(&effective["counterpart_arbiter"]))
        .chain(std::iter::once(&effective["primary_arbiter"]));
    for route in routes {
        for field in ["family", "provider", "text_model", "multimodal_model"] {
            assert_eq!(route[field], seat[field]);
        }
    }
    assert_eq!(fs::read(&policy_path).unwrap(), original);

    let error = quinte::policy::load_for_runtime(&policy_path)
        .unwrap_err()
        .to_string();
    assert!(error.contains("read-only compatible"));
    assert!(error.contains("quinte init --force"));
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

    let mut canonical_field = serde_json::to_value(quinte::policy::default_policy()).unwrap();
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
fn v2_manual_primary_arbiter_policy_is_visible_but_cannot_start_new_runs() {
    let home = tempdir().unwrap();
    let policy_path = home.path().join("policy.json");
    let mut policy = quinte::policy::default_policy();
    policy.auto_primary_arbiter = false;
    fs::write(&policy_path, serde_json::to_vec_pretty(&policy).unwrap()).unwrap();

    quinte::policy::load(&policy_path).unwrap();
    let error = quinte::policy::load_for_runtime(&policy_path)
        .unwrap_err()
        .to_string();
    assert!(error.contains("auto_primary_arbiter=true"));
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

#[test]
fn r2_parallel_defaults_false_for_legacy_policies_and_parses_when_present() {
    let home = tempdir().unwrap();
    let policy_path = home.path().join("policy.json");

    // Pre-0.1.8 policy.json has no r2_parallel key; it must load as serial.
    let mut legacy = legacy_policy();
    legacy.as_object_mut().unwrap().remove("r2_parallel");
    fs::write(&policy_path, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
    let policy = quinte::policy::load(&policy_path).unwrap();
    assert!(!policy.r2_parallel);
    quinte::policy::validate(&policy).unwrap();

    // The opt-in switch is honored when explicitly present, in both states.
    let mut parallel = legacy.clone();
    parallel["r2_parallel"] = json!(true);
    fs::write(&policy_path, serde_json::to_vec_pretty(&parallel).unwrap()).unwrap();
    let policy = quinte::policy::load(&policy_path).unwrap();
    assert!(policy.r2_parallel);
    quinte::policy::validate(&policy).unwrap();

    let mut serial = legacy;
    serial["r2_parallel"] = json!(false);
    fs::write(&policy_path, serde_json::to_vec_pretty(&serial).unwrap()).unwrap();
    assert!(!quinte::policy::load(&policy_path).unwrap().r2_parallel);
}

#[test]
fn duplicate_or_empty_school_perspective_fails_preflight() {
    use quinte::policy::{default_policy, validate};

    let mut duplicate = default_policy();
    duplicate.roster[1].perspective = duplicate.roster[0].perspective.clone();
    assert!(validate(&duplicate).is_err());

    let mut empty = default_policy();
    empty.roster[2].perspective = String::new();
    assert!(validate(&empty).is_err());

    // Legacy v1 policies stay exempt until cutover.
    let mut legacy_value = legacy_policy();
    for route in legacy_value["roster"]
        .as_array_mut()
        .expect("legacy roster")
    {
        route["perspective"] = json!("");
    }
    let dir = tempfile::tempdir().unwrap();
    let policy_path = dir.path().join("policy.json");
    fs::write(&policy_path, serde_json::to_vec_pretty(&legacy_value).unwrap()).unwrap();
    let legacy = quinte::policy::load(&policy_path).unwrap();
    assert!(quinte::policy::validate(&legacy).is_ok());
}
