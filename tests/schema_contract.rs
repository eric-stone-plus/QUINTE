mod common;

use quinte::model::LaneOutput;
use quinte::schema::{LANE_OUTPUT_SCHEMA, parse_and_validate};

#[test]
fn lane_output_accepts_valid_closed_document() {
    let bytes = serde_json::to_vec(&common::valid_lane_output()).unwrap();
    let output = parse_and_validate::<LaneOutput>(&bytes, LANE_OUTPUT_SCHEMA).unwrap();

    assert_eq!(output.lane_output_version, "0.1.4");
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
