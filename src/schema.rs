use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use crate::contract::{
    BRIEF_SCHEMA, LANE_OUTPUT_SCHEMA, LEGACY_BRIEF_SCHEMA, LEGACY_RESULT_SCHEMA,
    PRIMARY_ARBITER_RESPONSE_SCHEMA, R3_INPUT_RECEIPT_SCHEMA, RESULT_SCHEMA, RUN_EVENT_SCHEMA,
    RUN_MANIFEST_SCHEMA,
};
use crate::contract::{CONTRACT_REGISTRY, ContractRevision, ContractSpec};
use crate::util::read_json;

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

pub fn parse_versioned<T: DeserializeOwned>(
    bytes: &[u8],
    contract: &'static ContractSpec,
) -> anyhow::Result<T> {
    let text = std::str::from_utf8(bytes).context("payload is not strict UTF-8")?;
    let value: Value = serde_json::from_str(text).context("payload is not valid JSON")?;
    validate_versioned_value(&value, contract)?;
    serde_json::from_value(value).context("payload does not match typed contract")
}

pub fn validate_versioned_file<T: DeserializeOwned>(
    path: &Path,
    contract: &'static ContractSpec,
) -> anyhow::Result<T> {
    let value: Value = read_json(path)?;
    validate_versioned_value(&value, contract)?;
    serde_json::from_value(value).context("payload does not match typed contract")
}

pub fn validate_versioned_value(
    value: &Value,
    contract: &'static ContractSpec,
) -> anyhow::Result<&'static ContractRevision> {
    let version = value
        .get(contract.version_field)
        .and_then(Value::as_str)
        .with_context(|| {
            format!(
                "{} payload has no string {}",
                contract.name, contract.version_field
            )
        })?;
    let revision = crate::contract::revision(contract, version).ok_or_else(|| {
        anyhow::anyhow!(
            "unsupported {} contract revision in {}: {}",
            contract.name,
            contract.version_field,
            version
        )
    })?;
    if !crate::contract::version_supported(contract, version) {
        bail!("unsupported {} contract revision: {version}", contract.name);
    }
    validate_value(value, revision.schema)?;
    Ok(revision)
}

pub fn validate_value(value: &Value, schema: &str) -> anyhow::Result<()> {
    let schema: Value = serde_json::from_str(schema).context("embedded JSON schema is invalid")?;
    // External references are resolved only from this embedded registry. Validation never
    // performs network or filesystem retrieval, so schema behavior is reproducible offline.
    let mut embedded = Vec::new();
    for contract in CONTRACT_REGISTRY {
        for revision in contract.revisions {
            let document: Value = serde_json::from_str(revision.schema).with_context(|| {
                format!("{} {} schema is invalid", contract.name, revision.version)
            })?;
            let id = document
                .get("$id")
                .and_then(Value::as_str)
                .with_context(|| {
                    format!("{} {} schema has no $id", contract.name, revision.version)
                })?
                .to_string();
            let expected = crate::contract::schema_id(contract, revision);
            if id != expected {
                bail!(
                    "{} {} schema $id does not match its contract revision: expected {expected}, got {id}",
                    contract.name,
                    revision.version
                );
            }
            embedded.push((id, document));
        }
    }
    let mut registry = jsonschema::Registry::new();
    for (id, document) in &embedded {
        registry = registry
            .add(id.as_str(), document)
            .with_context(|| format!("cannot register schema {id}"))?;
    }
    let registry = registry
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
