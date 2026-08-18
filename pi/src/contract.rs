//! Contract extraction: the model reply must carry exactly one valid
//! contract artifact. Prose, fences, and trailing text are tolerated only
//! as wrappers around one well-formed JSON object; schema validation is
//! strict and fail-closed.

use anyhow::{Result, bail};
use serde_json::Value;

/// Extract one JSON object from model output (fenced or raw), then
/// validate it against the seat's contract schema.
pub fn extract_artifact(content: &str, schema_name: &str, seat_role: &str) -> Result<Value> {
    let text = content.trim();
    let candidates: Vec<&str> = if let Some(rest) = text.strip_prefix("```json") {
        rest.strip_suffix("```").map(|s| vec![s.trim()]).unwrap_or_default()
    } else if let Some(rest) = text.strip_prefix("```") {
        rest.strip_suffix("```").map(|s| vec![s.trim()]).unwrap_or_default()
    } else {
        // Raw JSON, possibly with trailing prose: try the whole text, then
        // the largest balanced {...} span.
        vec![text]
    };

    let mut last_err: Option<anyhow::Error> = None;
    for candidate in candidates {
        if let Ok(value) = serde_json::from_str::<Value>(candidate) {
            if let Err(e) = validate(&value, schema_name, seat_role) {
                last_err = Some(e);
                continue;
            }
            return Ok(value);
        }
        // Largest balanced object span as a fallback for prose-wrapped JSON.
        if let Some(block) = largest_object(candidate) {
            if let Ok(value) = serde_json::from_str::<Value>(block) {
                if let Err(e) = validate(&value, schema_name, seat_role) {
                    last_err = Some(e);
                    continue;
                }
                return Ok(value);
            }
        }
    }
    match last_err {
        Some(e) => Err(e),
        None => bail!("model output contains no valid {schema_name} JSON object"),
    }
}

fn validate(value: &Value, schema_name: &str, seat_role: &str) -> Result<()> {
    // Compile-time manifest dir, overridable at runtime for relocated
    // deployments. Release binaries have no CARGO_MANIFEST_DIR env var.
    let schema_path = std::env::var("PI_SCHEMAS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas"))
        .join(schema_name);
    let schema_text = std::fs::read_to_string(&schema_path)
        .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", schema_path.display()))?;
    let schema: Value = serde_json::from_str(&schema_text)?;
    // The arbiter verdict references the lane-output residual definition
    // by its public contract URL; register the sibling schema under its
    // $id so cross-file references resolve offline, exactly like the
    // QUINTE schema registry.
    // Documents live in a Vec so the registry can borrow them for the
    // validator's lifetime (QUINTE's embedded-registry pattern).
    let mut documents: Vec<(String, Value)> = Vec::new();
    let lane_path = schema_path
        .parent()
        .map(|p| p.join("lane-output.schema.json"))
        .unwrap_or_else(|| std::path::PathBuf::from("lane-output.schema.json"));
    if let Ok(lane_text) = std::fs::read_to_string(&lane_path) {
        if let Ok(lane_schema) = serde_json::from_str::<Value>(&lane_text) {
            if let Some(id) = lane_schema.get("$id").and_then(Value::as_str) {
                documents.push((id.to_string(), lane_schema));
            }
        }
    }
    let mut registry = jsonschema::Registry::new();
    for (id, document) in &documents {
        registry = registry.add(id.as_str(), document)?;
    }
    let prepared = registry.prepare()?;
    let validator = jsonschema::options().with_registry(&prepared).build(&schema)?;
    let messages: Vec<String> = validator
        .iter_errors(value)
        .take(5)
        .map(|e| e.to_string())
        .collect();
    if !messages.is_empty() {
        bail!(
            "{} contract violated by seat '{}': {}",
            schema_name,
            seat_role,
            messages.join("; ")
        );
    }
    Ok(())
}

/// Largest balanced `{...}` span in a string. Returns None if unbalanced.
fn largest_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn largest_object_handles_nested_and_fenced() {
        let text = "prose\n```json\n{\"a\": {\"b\": 1}}\n```\ntail";
        let block = largest_object(text).unwrap();
        assert_eq!(block, "{\"a\": {\"b\": 1}}");
        assert!(largest_object("no braces").is_none());
    }
}
