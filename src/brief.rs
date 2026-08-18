//! `quinte brief` — brief wizard and validation (phase D).
//!
//! Contract validation always reuses schema::validate_versioned_value +
//! contract("brief"); there is no second implementation. Version constants
//! come from contract.rs (currently 1.1, accepts 1.0).

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::contract::{self, BRIEF_VERSION};
use crate::ui::{self, Tone};

/// Interactive wizard: walks each field, writes to
/// <home>/briefs/brief-<utc>.json (0600). Returns (human output, written path).
pub fn wizard_new(home: &Path) -> anyhow::Result<(String, PathBuf)> {
    let question = prompt_line("QUESTION · topic (required, non-empty)", true)?;
    let context = prompt_line("CONTEXT · background (optional)", false)?;
    let action_scope = prompt_line("ACTION SCOPE · allowed actions (optional)", false)?;
    let roots_raw = prompt_line("EVIDENCE ROOTS · comma-separated (optional)", false)?;
    let ignore_raw = prompt_line("SNAPSHOT IGNORE · comma-separated patterns (optional)", false)?;

    let evidence_roots = split_csv(&roots_raw);
    let mut warnings = Vec::new();
    for root in &evidence_roots {
        if !Path::new(root).exists() {
            warnings.push(format!("evidence root does not exist (written anyway): {root}"));
        }
    }
    let snapshot_ignore = split_csv(&ignore_raw);

    let mut brief = json!({
        "brief_version": BRIEF_VERSION,
        "question": question,
    });
    let obj = brief.as_object_mut().expect("brief object");
    if !context.is_empty() {
        obj.insert("context".into(), Value::String(context));
    }
    if !action_scope.is_empty() {
        obj.insert("action_scope".into(), Value::String(action_scope));
    }
    if !evidence_roots.is_empty() {
        obj.insert(
            "evidence_roots".into(),
            Value::Array(evidence_roots.into_iter().map(Value::String).collect()),
        );
    }
    if !snapshot_ignore.is_empty() {
        obj.insert(
            "snapshot_ignore".into(),
            Value::Array(snapshot_ignore.into_iter().map(Value::String).collect()),
        );
    }

    // Validate against the contract before writing (same validation path as run)
    crate::schema::validate_versioned_value(
        &brief,
        contract::contract("brief").expect("brief contract"),
    )?;

    let dir = home.join("briefs");
    std::fs::create_dir_all(&dir)?;
    let filename = format!("brief-{}.json", chrono::Utc::now().format("%Y%m%dT%H%M%SZ"));
    let path = dir.join(&filename);
    let mut content = serde_json::to_string_pretty(&brief)?;
    content.push('\n');
    let tmp = dir.join(format!(".{filename}.tmp"));
    std::fs::write(&tmp, &content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, &path)?;

    let mut out = format!(
        "{} brief written to {}",
        ui::paint(Tone::Ok, ui::mark_ok()),
        path.display()
    );
    for warning in &warnings {
        out.push_str(&format!(
            "\n{}",
            ui::paint(Tone::Warn, &format!("! {warning}"))
        ));
    }
    out.push_str(&format!(
        "\n{}",
        ui::paint(
            Tone::Dim,
            &format!("start deliberation: quinte run --brief {} --wait", path.display())
        )
    ));
    Ok((out, path))
}

/// Non-tty template (for heredoc/scripts): valid JSON whose placeholder
/// values double as instructions.
pub fn print_template() -> String {
    let template = json!({
        "brief_version": BRIEF_VERSION,
        "question": "(required) the question for the five seats to deliberate",
        "context": "(optional) background and constraints",
        "action_scope": "(optional) allowed scope of action",
        "evidence_roots": ["(optional, array) evidence root directories, e.g. data/workspace"],
        "snapshot_ignore": ["(optional, array) snapshot ignore patterns, e.g. *.log"]
    });
    let mut out = serde_json::to_string_pretty(&template).expect("template serializes");
    out.push('\n');
    out
}

/// Validate a brief file against the contract; human output reports ✓/✗ per field.
pub fn validate_file(path: &Path) -> (String, bool) {
    fn record(lines: &mut Vec<String>, ok: bool, label: String, detail: String) {
        let (mark, tone) = if ok {
            (ui::mark_ok(), Tone::Ok)
        } else {
            (ui::mark_fail(), Tone::Fail)
        };
        let suffix = if detail.is_empty() {
            label
        } else {
            format!("{label} · {detail}")
        };
        lines.push(format!("{} {}", ui::paint(tone, mark), suffix));
    }

    let mut lines = Vec::new();
    let mut all_ok = true;

    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            record(&mut lines, false, "read file".into(), format!("{e}"));
            return (lines.join("\n"), false);
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            record(&mut lines, false, "JSON parse".into(), format!("{e}"));
            return (lines.join("\n"), false);
        }
    };
    record(&mut lines, true, "JSON parse".into(), String::new());

    let version = value
        .get("brief_version")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !contract::brief_version_supported(version) {
        all_ok = false;
    }
    record(
        &mut lines,
        contract::brief_version_supported(version),
        "brief_version".into(),
        if version.is_empty() {
            "missing".into()
        } else {
            version.to_string()
        },
    );

    let question = value.get("question").and_then(Value::as_str).unwrap_or("");
    if question.trim().is_empty() {
        all_ok = false;
    }
    record(
        &mut lines,
        !question.trim().is_empty(),
        "question".into(),
        if question.trim().is_empty() {
            "required and non-empty".into()
        } else {
            format!("{} chars", question.chars().count())
        },
    );

    // Evidence-root existence is a warning only, not a failure
    if let Some(roots) = value.get("evidence_roots").and_then(Value::as_array) {
        for root in roots.iter().filter_map(Value::as_str) {
            if !Path::new(root).exists() {
                lines.push(format!(
                    "{} evidence root does not exist (warning): {root}",
                    ui::paint(Tone::Warn, ui::mark_warn())
                ));
            }
        }
    }

    // Whole-file contract validation (the single authoritative path)
    match crate::schema::validate_versioned_value(
        &value,
        contract::contract("brief").expect("brief contract"),
    ) {
        Ok(_) => record(&mut lines, true, "contract check".into(), "passed".into()),
        Err(e) => {
            all_ok = false;
            record(
                &mut lines,
                false,
                "contract check".into(),
                ui::truncate(&e.to_string(), 400),
            );
        }
    }

    (lines.join("\n"), all_ok)
}

fn split_csv(raw: &str) -> Vec<String> {
    raw.split([',', '，'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Read one line (tty wizard); when required, loop until non-empty.
fn prompt_line(label: &str, required: bool) -> anyhow::Result<String> {
    let stdin = std::io::stdin();
    loop {
        {
            let mut out = std::io::stdout();
            let _ = writeln!(out, "{}", ui::paint(Tone::Gold, label));
            let _ = write!(out, "{} ", ui::paint(Tone::Gold, "❯"));
            let _ = out.flush();
        }
        let mut line = String::new();
        let n = stdin.read_line(&mut line)?;
        if n == 0 {
            anyhow::bail!("input ended (EOF); wizard aborted");
        }
        let trimmed = line.trim().to_string();
        if required && trimmed.is_empty() {
            println!("{}", ui::paint(Tone::Warn, "required field; please re-enter"));
            continue;
        }
        return Ok(trimmed);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_passes_validation() {
        let value: Value = serde_json::from_str(&print_template()).unwrap();
        crate::schema::validate_versioned_value(
            &value,
            contract::contract("brief").expect("brief contract"),
        )
        .expect("template must pass contract validation");
    }

    #[test]
    fn validate_reports_each_field() {
        let dir = std::env::temp_dir().join(format!(
            "quinte-brief-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // Good file
        let good = dir.join("good.json");
        std::fs::write(
            &good,
            serde_json::to_string_pretty(&json!({
                "brief_version": BRIEF_VERSION,
                "question": "Adopt the proposal?",
                "evidence_roots": ["/nonexistent-root"]
            }))
            .unwrap(),
        )
        .unwrap();
        let (report, ok) = validate_file(&good);
        assert!(ok, "{report}");
        assert!(report.contains("JSON parse"));
        assert!(report.contains("brief_version"));
        assert!(report.contains("contract check"));
        assert!(report.contains("evidence root does not exist"));
        // Empty question → ✗
        let bad = dir.join("bad.json");
        std::fs::write(
            &bad,
            serde_json::to_string_pretty(
                &json!({"brief_version": BRIEF_VERSION, "question": "  "}),
            )
            .unwrap(),
        )
        .unwrap();
        let (report, ok) = validate_file(&bad);
        assert!(!ok);
        assert!(report.contains("required and non-empty"), "{report}");
        // Not JSON
        let garbage = dir.join("garbage.json");
        std::fs::write(&garbage, "not json").unwrap();
        let (_, ok) = validate_file(&garbage);
        assert!(!ok);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn split_csv_handles_cjk_comma() {
        assert_eq!(split_csv("a, b，c"), vec!["a", "b", "c"]);
        assert!(split_csv("").is_empty());
    }
}
