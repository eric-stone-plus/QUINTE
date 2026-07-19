//! `quinte brief` — brief 向导与校验（D 阶段）。
//!
//! 契约校验一律复用 schema::validate_versioned_value + contract("brief")，
//! 不另写一套。版本常量以 contract.rs 为准（当前 1.1，接受 1.0）。

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::contract::{self, BRIEF_VERSION};
use crate::ui::{self, Tone};

/// 交互向导：逐字段引导，落盘到 <home>/briefs/brief-<utc>.json（0600）。
/// 返回 (人类输出, 落盘路径)。
pub fn wizard_new(home: &Path) -> anyhow::Result<(String, PathBuf)> {
    let question = prompt_line("QUESTION · 议题（必填非空）", true)?;
    let context = prompt_line("CONTEXT · 背景（可空）", false)?;
    let action_scope = prompt_line("ACTION SCOPE · 行动范围（可空）", false)?;
    let roots_raw = prompt_line("EVIDENCE ROOTS · 证据根（逗号分隔，可空）", false)?;
    let ignore_raw = prompt_line("SNAPSHOT IGNORE · 忽略模式（逗号分隔，可空）", false)?;

    let evidence_roots = split_csv(&roots_raw);
    let mut warnings = Vec::new();
    for root in &evidence_roots {
        if !Path::new(root).exists() {
            warnings.push(format!("证据根不存在（仍写入）：{root}"));
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

    // 写前先过契约（与 run 同一条校验路径）
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
        "{} brief 已写入 {}",
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
            &format!("发起审议: quinte run --brief {} --wait", path.display())
        )
    ));
    Ok((out, path))
}

/// 非 tty 模板（供 heredoc/脚本）：合法 JSON，占位值即说明。
pub fn print_template() -> String {
    let template = json!({
        "brief_version": BRIEF_VERSION,
        "question": "（必填）要五席审议的问题",
        "context": "（可空）背景与约束",
        "action_scope": "（可空）允许的行动范围",
        "evidence_roots": ["（可空，数组）证据根目录，如 data/workspace"],
        "snapshot_ignore": ["（可空，数组）快照忽略模式，如 *.log"]
    });
    let mut out = serde_json::to_string_pretty(&template).expect("template serializes");
    out.push('\n');
    out
}

/// 按契约校验 brief 文件，人类输出逐字段 ✓/✗。
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
            record(&mut lines, false, "读取文件".into(), format!("{e}"));
            return (lines.join("\n"), false);
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            record(&mut lines, false, "JSON 解析".into(), format!("{e}"));
            return (lines.join("\n"), false);
        }
    };
    record(&mut lines, true, "JSON 解析".into(), String::new());

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
            "缺失".into()
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
            "必填且非空".into()
        } else {
            format!("{} 字", question.chars().count())
        },
    );

    // 证据根存在性提示（仅警告，不计失败）
    if let Some(roots) = value.get("evidence_roots").and_then(Value::as_array) {
        for root in roots.iter().filter_map(Value::as_str) {
            if !Path::new(root).exists() {
                lines.push(format!(
                    "{} 证据根不存在（警告）: {root}",
                    ui::paint(Tone::Warn, ui::mark_warn())
                ));
            }
        }
    }

    // 整文件契约校验（唯一权威路径）
    match crate::schema::validate_versioned_value(
        &value,
        contract::contract("brief").expect("brief contract"),
    ) {
        Ok(_) => record(&mut lines, true, "契约校验".into(), "通过".into()),
        Err(e) => {
            all_ok = false;
            record(
                &mut lines,
                false,
                "契约校验".into(),
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

/// 读一行（tty 向导）；required 时循环至非空。
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
            anyhow::bail!("输入结束（EOF），向导中止");
        }
        let trimmed = line.trim().to_string();
        if required && trimmed.is_empty() {
            println!("{}", ui::paint(Tone::Warn, "必填，请重新输入"));
            continue;
        }
        return Ok(trimmed);
    }
}

// ---------------------------------------------------------------------------
// 测试
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
        .expect("模板必须通过契约校验");
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
        // 好文件
        let good = dir.join("good.json");
        std::fs::write(
            &good,
            serde_json::to_string_pretty(&json!({
                "brief_version": BRIEF_VERSION,
                "question": "是否采纳提案？",
                "evidence_roots": ["/nonexistent-root"]
            }))
            .unwrap(),
        )
        .unwrap();
        let (report, ok) = validate_file(&good);
        assert!(ok, "{report}");
        assert!(report.contains("JSON 解析"));
        assert!(report.contains("brief_version"));
        assert!(report.contains("契约校验"));
        assert!(report.contains("证据根不存在"));
        // question 为空 → ✗
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
        assert!(report.contains("必填且非空"), "{report}");
        // 非 JSON
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
