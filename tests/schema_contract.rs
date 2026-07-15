mod common;

use quinte::model::{Brief, LaneOutput};
use quinte::schema::{BRIEF_SCHEMA, LANE_OUTPUT_SCHEMA, parse_and_validate};

#[test]
fn brief_snapshot_ignore_is_optional_and_backward_compatible() {
    let legacy = br#"{
        "brief_version": "1.0",
        "question": "What remains?",
        "evidence_roots": []
    }"#;
    let brief = parse_and_validate::<Brief>(legacy, BRIEF_SCHEMA).unwrap();
    assert!(brief.snapshot_ignore.is_empty());

    let configured = br#"{
        "brief_version": "1.0",
        "question": "What remains?",
        "evidence_roots": [],
        "snapshot_ignore": [".firecrawl", "tools/r4se-packages", "**/*.tmp"]
    }"#;
    let brief = parse_and_validate::<Brief>(configured, BRIEF_SCHEMA).unwrap();
    assert_eq!(brief.snapshot_ignore.len(), 3);
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
        assert!(parse_and_validate::<Brief>(document.as_bytes(), BRIEF_SCHEMA).is_err());
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
