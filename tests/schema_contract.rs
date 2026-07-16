mod common;

use quinte::contract::{CONTRACT_REGISTRY, contract, schema_id, version_supported};
use quinte::model::{Brief, LaneOutput};
use quinte::schema::{
    BRIEF_SCHEMA, LANE_OUTPUT_SCHEMA, LEGACY_RESULT_SCHEMA, RESULT_SCHEMA, parse_and_validate,
    parse_versioned, validate_value, validate_versioned_value,
};

#[test]
fn brief_snapshot_ignore_is_optional_and_backward_compatible() {
    let legacy = br#"{
        "brief_version": "1.0",
        "question": "What remains?",
        "evidence_roots": []
    }"#;
    let brief = parse_versioned::<Brief>(legacy, contract("brief").unwrap()).unwrap();
    assert!(brief.snapshot_ignore.is_empty());

    let configured = br#"{
        "brief_version": "1.0",
        "question": "What remains?",
        "evidence_roots": [],
        "snapshot_ignore": [".firecrawl", "tools/r4se-packages", "**/*.tmp"]
    }"#;
    let brief = parse_versioned::<Brief>(configured, contract("brief").unwrap()).unwrap();
    assert_eq!(brief.snapshot_ignore.len(), 3);
}

#[test]
fn legacy_brief_serializes_in_canonical_field_order_with_explicit_defaults() {
    let legacy = br#"{
        "brief_version": "1.0",
        "question": "What remains?"
    }"#;
    let brief = parse_versioned::<Brief>(legacy, contract("brief").unwrap()).unwrap();

    assert_eq!(
        serde_json::to_string(&brief).unwrap(),
        r#"{"brief_version":"1.0","question":"What remains?","context":null,"evidence_roots":[],"snapshot_ignore":[],"attachments":[],"action_scope":null,"affected_paths":[],"action_binding_sha256":null}"#
    );
}

#[test]
fn brief_snapshot_ignore_rejects_non_relative_path_syntax() {
    for pattern in [r#""/cache""#, r#""cache/""#, r#""tools\\cache""#] {
        let document = format!(
            r#"{{
                "brief_version": "1.0",
                "question": "What remains?",
                "snapshot_ignore": [{pattern}]
            }}"#
        );
        assert!(parse_versioned::<Brief>(document.as_bytes(), contract("brief").unwrap()).is_err());
    }
}

#[test]
fn lane_output_accepts_valid_closed_document() {
    let bytes = serde_json::to_vec(&common::valid_lane_output()).unwrap();
    let output = parse_and_validate::<LaneOutput>(&bytes, LANE_OUTPUT_SCHEMA).unwrap();

    assert_eq!(output.lane_output_version, "1.0");
    assert_eq!(output.claims.len(), 1);
    assert_eq!(output.residuals.len(), 1);
}

#[test]
fn lane_output_rejects_unknown_top_level_field() {
    let mut value = common::valid_lane_output();
    value["next_phase"] = serde_json::json!("R3");
    let bytes = serde_json::to_vec(&value).unwrap();

    let error = parse_and_validate::<LaneOutput>(&bytes, LANE_OUTPUT_SCHEMA).unwrap_err();
    assert!(error.to_string().contains("schema validation failed"));
}

#[test]
fn lane_output_rejects_unknown_nested_field() {
    let mut value = common::valid_lane_output();
    value["claims"][0]["model_override"] = serde_json::json!("another-model");
    let bytes = serde_json::to_vec(&value).unwrap();

    let error = parse_and_validate::<LaneOutput>(&bytes, LANE_OUTPUT_SCHEMA).unwrap_err();
    assert!(error.to_string().contains("schema validation failed"));
}

#[test]
fn lane_output_rejects_invalid_utf8_before_json_parsing() {
    let error =
        parse_and_validate::<LaneOutput>(&[b'{', 0xff, b'}'], LANE_OUTPUT_SCHEMA).unwrap_err();

    assert!(error.to_string().contains("payload is not strict UTF-8"));
}

#[test]
fn every_wire_format_has_one_revision_registry_entry() {
    let mut names = std::collections::BTreeSet::new();
    let mut schema_ids = std::collections::BTreeSet::new();
    assert!(CONTRACT_REGISTRY.len() >= 18);
    for contract in CONTRACT_REGISTRY {
        assert!(names.insert(contract.name), "duplicate {}", contract.name);
        assert!(version_supported(contract, contract.current_version));
        for version in contract.accepted_versions {
            assert!(
                version_supported(contract, version),
                "{} accepts {version} without a usable revision",
                contract.name
            );
        }
        for revision in contract.revisions {
            assert!(
                contract.accepted_versions.contains(&revision.version),
                "{} schema {} is not accepted",
                contract.name,
                revision.version
            );
            let document: serde_json::Value = serde_json::from_str(revision.schema).unwrap();
            let id = schema_id(contract, revision);
            assert_eq!(document["$id"], id);
            assert!(schema_ids.insert(id), "duplicate schema identity");
            assert_eq!(
                document["properties"][contract.version_field]["const"], revision.version,
                "{} {} schema does not own exactly one revision",
                contract.name, revision.version
            );
        }
    }
}

#[test]
fn brief_revisions_have_distinct_identities_and_do_not_cross_accept() {
    let brief = contract("brief").unwrap();
    let legacy = serde_json::json!({"brief_version": "1.0", "question": "Legacy"});
    let current = serde_json::json!({"brief_version": "1.1", "question": "Current"});

    assert_eq!(
        validate_versioned_value(&legacy, brief).unwrap().version,
        "1.0"
    );
    assert_eq!(
        validate_versioned_value(&current, brief).unwrap().version,
        "1.1"
    );
    assert!(validate_value(&legacy, BRIEF_SCHEMA).is_err());
    assert!(validate_value(&current, quinte::schema::LEGACY_BRIEF_SCHEMA).is_err());
}

fn trial_manifest() -> serde_json::Value {
    let parties = ["Party A", "Party B", "Party C", "Party D", "Party E"];
    serde_json::json!({
        "manifest_version": "1.0",
        "base_model_relation": "same_model",
        "perspective_count": 5,
        "perspectives": parties.iter().enumerate().map(|(index, party)| serde_json::json!({
            "party_id": party,
            "route_id": format!("route-{index}"),
            "r1_artifact": format!("lanes/R1/route-{index}/accepted.json"),
            "r2_artifact": format!("lanes/R2/route-{index}/accepted.json"),
            "independent_first_pass": true
        })).collect::<Vec<_>>(),
        "perturbation_axes": [],
        "independence_controls": [],
        "contamination_risks": [],
        "wall_time_seconds": null
    })
}

#[test]
fn result_revisions_are_strict_and_legacy_is_non_actionable_by_shape() {
    let legacy = serde_json::json!({
        "result_version": "1.0",
        "run_id": "legacy-run",
        "status": "completed",
        "summary": "historical",
        "recommendation": "inspect only",
        "dissent": [],
        "residuals": [],
        "trial_manifest": trial_manifest()
    });
    let current = serde_json::json!({
        "result_version": "2.0",
        "run_id": "current-run",
        "status": "completed",
        "brief_sha256": format!("sha256:{}", "a".repeat(64)),
        "question": "May this action proceed?",
        "action_scope": "test",
        "affected_paths": ["a/b"],
        "action_binding_sha256": format!("sha256:{}", "b".repeat(64)),
        "summary": "current",
        "recommendation": "bound",
        "dissent": [],
        "residuals": [],
        "trial_manifest": trial_manifest()
    });

    let result = contract("result").unwrap();
    assert_eq!(
        validate_versioned_value(&legacy, result).unwrap().version,
        "1.0"
    );
    assert_eq!(
        validate_versioned_value(&current, result).unwrap().version,
        "2.0"
    );
    assert!(validate_value(&legacy, RESULT_SCHEMA).is_err());
    assert!(validate_value(&current, LEGACY_RESULT_SCHEMA).is_err());
    let mut forged = legacy;
    forged["brief_sha256"] = serde_json::json!(format!("sha256:{}", "c".repeat(64)));
    assert!(validate_versioned_value(&forged, result).is_err());
}

#[test]
fn highball_action_binding_known_answer_is_accepted_verbatim() {
    let brief = r#"{
            "brief_version": "1.1",
            "question": "允许改动吗？",
            "action_scope": "protected_write",
            "affected_paths": ["HIGHBALL\\bin\\tool.py", "a/b.py"],
            "action_binding_sha256": "sha256:7fe45882922fdb9c9dc748dabc2a23b2590187e017b29b73c35ae7f92c320a5e"
        }"#;
    let brief = parse_versioned::<Brief>(brief.as_bytes(), contract("brief").unwrap()).unwrap();
    assert_eq!(brief.question, "允许改动吗？");
    assert_eq!(brief.affected_paths[0], r"HIGHBALL\bin\tool.py");
    assert_eq!(
        brief.action_binding_sha256.as_deref(),
        Some("sha256:7fe45882922fdb9c9dc748dabc2a23b2590187e017b29b73c35ae7f92c320a5e")
    );
}
