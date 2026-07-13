use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::util::read_json;

pub const BRIEF_SCHEMA: &str = include_str!("../schemas/brief.schema.json");
pub const LANE_OUTPUT_SCHEMA: &str = include_str!("../schemas/lane-output.schema.json");
pub const HM_RESPONSE_SCHEMA: &str = include_str!("../schemas/hm-response.schema.json");
pub const R3_INPUT_RECEIPT_SCHEMA: &str = include_str!("../schemas/r3-input-receipt.schema.json");
pub const RESULT_SCHEMA: &str = include_str!("../schemas/result.schema.json");
pub const RUN_MANIFEST_SCHEMA: &str = include_str!("../schemas/run-manifest.schema.json");
pub const RUN_EVENT_SCHEMA: &str = include_str!("../schemas/run-event.schema.json");

pub fn schema_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas")
}

pub fn parse_and_validate<T: DeserializeOwned>(bytes: &[u8], schema: &str) -> anyhow::Result<T> {
    let text = std::str::from_utf8(bytes).context("payload is not strict UTF-8")?;
    let value: Value = serde_json::from_str(text).context("payload is not valid JSON")?;
    validate_value(&value, schema)?;
    serde_json::from_value(value).context("payload does not match typed contract")
}

pub fn validate_file<T: DeserializeOwned>(path: &Path, schema: &str) -> anyhow::Result<T> {
    let value: Value = read_json(path)?;
    validate_value(&value, schema)?;
    serde_json::from_value(value).context("payload does not match typed contract")
}

pub fn validate_value(value: &Value, schema: &str) -> anyhow::Result<()> {
    let schema: Value = serde_json::from_str(schema).context("embedded JSON schema is invalid")?;
    // External references are resolved only from this embedded registry. Validation never
    // performs network or filesystem retrieval, so schema behavior is reproducible offline.
    let lane_schema: Value =
        serde_json::from_str(LANE_OUTPUT_SCHEMA).context("lane schema is invalid")?;
    let lane_id = lane_schema
        .get("$id")
        .and_then(Value::as_str)
        .context("lane schema has no $id")?;
    let registry = jsonschema::Registry::new()
        .add(lane_id, &lane_schema)
        .context("cannot register lane schema")?
        .prepare()
        .context("cannot prepare schema registry")?;
    let validator = jsonschema::options()
        .with_registry(&registry)
        .build(&schema)
        .context("cannot compile JSON schema")?;
    let errors = validator
        .iter_errors(value)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        bail!("schema validation failed: {}", errors.join("; "));
    }
    Ok(())
}
