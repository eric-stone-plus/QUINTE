//! QUINTE CLI 人类输出样式层（纯 std ANSI，无新依赖）。
//!
//! 降级规则：NO_COLOR / QUINTE_NO_COLOR 环境变量、TERM=dumb、非 tty 输出
//! 一律降级为纯文本（无转义、ASCII 符号）。--json 路径与本模块无关。

use std::sync::atomic::{AtomicBool, Ordering};

static COLOR_FORCED_OFF: AtomicBool = AtomicBool::new(false);

/// 进程级一次性判定（测试可覆盖）。
fn detect_color() -> bool {
    if COLOR_FORCED_OFF.load(Ordering::Relaxed) {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() || std::env::var_os("QUINTE_NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("TERM").map(|t| t == "dumb").unwrap_or(false) {
        return false;
    }
    unsafe { libc_isatty(1) }
}

/// 测试与 --json 安全网：强制关闭颜色。
pub fn force_no_color() {
    COLOR_FORCED_OFF.store(true, Ordering::Relaxed);
}

pub fn color_enabled() -> bool {
    detect_color()
}

/// stdout 是否为 tty（REPL 入口判定；与颜色降级独立）。
pub fn stdout_is_tty() -> bool {
    unsafe { libc_isatty(1) }
}

// libc 的 isatty 直接声明（不加 crate）。
unsafe extern "C" {
    fn isatty(fd: i32) -> i32;
}

unsafe fn libc_isatty(fd: i32) -> bool {
    unsafe { isatty(fd) == 1 }
}

// ---------------------------------------------------------------------------
// 颜色（256 色近似：金 178、亮金 220；成功绿 71；警告黄 178→金黄 214；失败红 167；暗灰 240；运行蓝 75）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tone {
    Gold,
    GoldBright,
    Ok,
    Warn,
    Fail,
    Dim,
    Run,
    Plain,
}

fn tone_code(tone: Tone) -> &'static str {
    match tone {
        Tone::Gold => "38;5;178",
        Tone::GoldBright => "38;5;220",
        Tone::Ok => "38;5;71",
        Tone::Warn => "38;5;214",
        Tone::Fail => "38;5;167",
        Tone::Dim => "38;5;240",
        Tone::Run => "38;5;75",
        Tone::Plain => "0",
    }
}

pub fn paint(tone: Tone, text: &str) -> String {
    paint_styled(tone, text, false)
}

pub fn paint_bold(tone: Tone, text: &str) -> String {
    paint_styled(tone, text, true)
}

fn paint_styled(tone: Tone, text: &str, bold: bool) -> String {
    if !color_enabled() || tone == Tone::Plain {
        return text.to_string();
    }
    if bold {
        format!("\x1b[1;{}m{}\x1b[0m", tone_code(tone), text)
    } else {
        format!("\x1b[{}m{}\x1b[0m", tone_code(tone), text)
    }
}

// ---------------------------------------------------------------------------
// 状态符号（降级为 ASCII 词）
// ---------------------------------------------------------------------------

pub fn mark_ok() -> &'static str {
    if color_enabled() { "✓" } else { "PASS" }
}

pub fn mark_warn() -> &'static str {
    if color_enabled() { "!" } else { "WARN" }
}

pub fn mark_fail() -> &'static str {
    if color_enabled() { "✗" } else { "FAIL" }
}

pub fn mark_dim() -> &'static str {
    "-"
}

/// ● 状态点（着色由调用方加；降级输出同样字符，无色）。
pub fn dot() -> &'static str {
    "●"
}

/// 截断到显示宽度（ASCII 为主；超出补 …）。
pub fn truncate(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else if max <= 1 {
        s.chars().take(max).collect()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}

pub fn pad_right(s: &str, width: usize) -> String {
    let n = s.chars().count();
    if n >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_env_disables_paint() {
        force_no_color();
        assert!(!color_enabled());
        assert_eq!(paint(Tone::Gold, "abc"), "abc");
        assert_eq!(mark_ok(), "PASS");
        assert_eq!(mark_warn(), "WARN");
        assert_eq!(mark_fail(), "FAIL");
    }

    #[test]
    fn truncate_and_pad() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("ab", 4), "ab");
        assert_eq!(pad_right("ab", 5), "ab   ");
    }
}

// ---------------------------------------------------------------------------
// 实时进展板（quinte run --wait / quinte wait）
//
// 数据全部来自 manifest 状态 + events.jsonl 事件流的真实形状：
// lane.started / lane.finished / lane.accepted / lane.retry_scheduled /
// lane.retry_started（带 phase、party_id、attempt、data.route_id）、
// run.transition、primary_arbiter.accepted。渲染为纯函数，可测。
// ---------------------------------------------------------------------------

use crate::model::{Event, RunStatus};

#[derive(Debug, Clone, PartialEq)]
pub enum LaneState {
    Pending,
    Running { attempt: usize },
    Retrying { attempt: usize },
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub struct LaneRow {
    pub label: String,
    pub state: LaneState,
    pub detail: String,
}

impl LaneRow {
    fn new(label: impl Into<String>) -> LaneRow {
        LaneRow {
            label: label.into(),
            state: LaneState::Pending,
            detail: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BoardModel {
    pub run_id: String,
    pub status: String,
    pub elapsed_secs: u64,
    pub r1: Vec<LaneRow>,
    pub r2_done: usize,
    pub r2_total: usize,
    pub r2_current: Option<String>,
    pub r3_cc: LaneRow,
    pub primary: LaneRow,
    pub created: bool,
}

const DEFAULT_PARTIES: [&str; 5] = ["Party A", "Party B", "Party C", "Party D", "Party E"];

impl BoardModel {
    /// 从事件流构建模型。parties 为五席 party_id 顺序（roster 顺序）。
    pub fn from_events(
        run_id: &str,
        status: RunStatus,
        elapsed_secs: u64,
        parties: &[String],
        events: &[Event],
    ) -> BoardModel {
        let lane_by_id = |id: &str, lanes: &mut Vec<(String, LaneRow)>| -> usize {
            if let Some(idx) = lanes.iter().position(|(pid, _)| pid == id) {
                return idx;
            }
            lanes.push((id.to_string(), LaneRow::new(id)));
            lanes.len() - 1
        };

        let mut r3_cc = LaneRow::new("Counterpart Arbiter");
        let mut primary = LaneRow::new("Primary Arbiter");
        let mut created = false;

        // R1 / R2 行均按 roster 顺序
        let mut r1_rows: Vec<(String, LaneRow)> = parties
            .iter()
            .map(|p| (p.clone(), LaneRow::new(p)))
            .collect();
        let mut r2_rows: Vec<(String, LaneRow)> = parties
            .iter()
            .map(|p| (p.clone(), LaneRow::new(p)))
            .collect();

        for event in events {
            let phase = event.phase.as_deref().unwrap_or("");
            let party = event.party_id.as_deref().unwrap_or("");
            let attempt = event.attempt.unwrap_or(1);
            match event.event_type.as_str() {
                "run.created" => created = true,
                "lane.started" | "lane.retry_started" => {
                    let detail = if event.event_type == "lane.retry_started" {
                        "重试".to_string()
                    } else {
                        String::new()
                    };
                    match phase {
                        "R1" => {
                            let idx = lane_by_id(party, &mut r1_rows);
                            r1_rows[idx].1.state = LaneState::Running { attempt };
                            r1_rows[idx].1.detail = detail;
                        }
                        "R2" => {
                            let idx = lane_by_id(party, &mut r2_rows);
                            r2_rows[idx].1.state = LaneState::Running { attempt };
                            r2_rows[idx].1.detail = detail;
                        }
                        "R3" => {
                            r3_cc.state = LaneState::Running { attempt };
                            r3_cc.detail = detail;
                        }
                        _ => {}
                    }
                }
                "lane.retry_scheduled" => {
                    let class = event
                        .data
                        .get("failure_class")
                        .and_then(|v| v.as_str())
                        .unwrap_or("retry");
                    let delay = event
                        .data
                        .get("retry_after_seconds")
                        .and_then(|v| v.as_f64())
                        .or_else(|| {
                            event
                                .data
                                .get("delay_milliseconds")
                                .and_then(|v| v.as_f64())
                                .map(|ms| ms / 1000.0)
                        });
                    let detail = match delay {
                        Some(d) => format!("重试等待 {d:.0}s · {class}"),
                        None => format!("重试调度 · {class}"),
                    };
                    match phase {
                        "R1" => {
                            let idx = lane_by_id(party, &mut r1_rows);
                            r1_rows[idx].1.state = LaneState::Retrying { attempt };
                            r1_rows[idx].1.detail = detail;
                        }
                        "R2" => {
                            let idx = lane_by_id(party, &mut r2_rows);
                            r2_rows[idx].1.state = LaneState::Retrying { attempt };
                            r2_rows[idx].1.detail = detail;
                        }
                        "R3" => {
                            r3_cc.state = LaneState::Retrying { attempt };
                            r3_cc.detail = detail;
                        }
                        _ => {}
                    }
                }
                "lane.finished" => {
                    let exit = event
                        .data
                        .get("exit_code")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let timed_out = event
                        .data
                        .get("timed_out")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let cancelled = event
                        .data
                        .get("cancelled")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let target: &mut LaneRow = match phase {
                        "R1" => {
                            let idx = lane_by_id(party, &mut r1_rows);
                            &mut r1_rows[idx].1
                        }
                        "R2" => {
                            let idx = lane_by_id(party, &mut r2_rows);
                            &mut r2_rows[idx].1
                        }
                        "R3" => &mut r3_cc,
                        _ => continue,
                    };
                    if timed_out || cancelled {
                        target.state = LaneState::Failed;
                        target.detail = if timed_out { "超时" } else { "已取消" }.to_string();
                    } else if matches!(target.state, LaneState::Done) {
                        // 已 accepted，忽略迟到 finished
                    } else if exit != 0 {
                        target.state = LaneState::Failed;
                        target.detail = format!("exit {exit}");
                    } else {
                        target.detail = format!("exit {exit} · 评审中");
                    }
                }
                "lane.accepted" => match phase {
                    "R1" => {
                        let idx = lane_by_id(party, &mut r1_rows);
                        r1_rows[idx].1.state = LaneState::Done;
                        r1_rows[idx].1.detail = "已验收".into();
                    }
                    "R2" => {
                        let idx = lane_by_id(party, &mut r2_rows);
                        r2_rows[idx].1.state = LaneState::Done;
                        r2_rows[idx].1.detail = "已评审".into();
                    }
                    "R3" => {
                        r3_cc.state = LaneState::Done;
                        r3_cc.detail = "裁决已出".into();
                    }
                    _ => {}
                },
                "primary_arbiter.accepted" => {
                    primary.state = LaneState::Done;
                    primary.detail = "人工裁决已受理".into();
                }
                _ => {}
            }
        }

        let r1: Vec<LaneRow> = r1_rows.into_iter().map(|(_, row)| row).collect();
        let r2_rows: Vec<LaneRow> = r2_rows.into_iter().map(|(_, row)| row).collect();
        let r2_done = r2_rows
            .iter()
            .filter(|r| r.state == LaneState::Done)
            .count();
        let r2_current = r2_rows
            .iter()
            .find(|r| {
                matches!(
                    r.state,
                    LaneState::Running { .. } | LaneState::Retrying { .. }
                )
            })
            .map(|r| r.label.clone());

        // Primary Arbiter：到达 waiting 状态且尚未受理 → 等待中
        if status == RunStatus::WaitingPrimaryArbiter && primary.state == LaneState::Pending {
            primary.state = LaneState::Running { attempt: 1 };
            primary.detail = "等待人工裁决 · quinte primary-arbiter request".into();
        }

        BoardModel {
            run_id: run_id.to_string(),
            status: format!("{status:?}"),
            elapsed_secs,
            r1,
            r2_done,
            r2_total: parties.len().max(1),
            r2_current,
            r3_cc,
            primary,
            created,
        }
    }

    pub fn default_parties() -> Vec<String> {
        DEFAULT_PARTIES.iter().map(|s| s.to_string()).collect()
    }
}

const SPINNER: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

fn lane_mark(state: &LaneState, tick: usize) -> (&'static str, Tone) {
    match state {
        LaneState::Pending => ("○", Tone::Dim),
        LaneState::Running { .. } => {
            if color_enabled() {
                (SPINNER[tick % SPINNER.len()], Tone::Run)
            } else {
                ("*", Tone::Run)
            }
        }
        LaneState::Retrying { .. } => ("!", Tone::Warn),
        LaneState::Done => (mark_ok(), Tone::Ok),
        LaneState::Failed => (mark_fail(), Tone::Fail),
    }
}

fn lane_line(row: &LaneRow, tick: usize) -> String {
    let (mark, tone) = lane_mark(&row.state, tick);
    let state_word = match &row.state {
        LaneState::Pending => "pending".to_string(),
        LaneState::Running { attempt } => format!("running · attempt {attempt}"),
        LaneState::Retrying { attempt } => format!("retrying · attempt {attempt}"),
        LaneState::Done => "done".to_string(),
        LaneState::Failed => "failed".to_string(),
    };
    let detail = if row.detail.is_empty() {
        state_word
    } else {
        format!("{state_word} · {}", row.detail)
    };
    format!(
        "  {} {} {}",
        paint(tone, mark),
        paint(Tone::Plain, &pad_right(&truncate(&row.label, 28), 28)),
        paint(Tone::Dim, &detail)
    )
}

/// 进展板帧构建（纯函数）：返回若干行（已含样式；降级为纯文本）。
pub fn build_board(model: &BoardModel, tick: usize, _width: usize) -> Vec<String> {
    let short: String = model
        .run_id
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let mut out = Vec::new();
    out.push(format!(
        "{} {} {} {}",
        paint_bold(Tone::Gold, "QUINTE RUN"),
        paint(Tone::GoldBright, &format!("…{short}")),
        paint_bold(status_tone_public(&model.status), &model.status),
        paint(Tone::Dim, &format!("· {}s", model.elapsed_secs))
    ));
    out.push(paint(Tone::Gold, "R1 · 五席并行"));
    for row in &model.r1 {
        out.push(lane_line(row, tick));
    }
    let r2_detail = match &model.r2_current {
        Some(current) => format!("{}/{} · 当前 {}", model.r2_done, model.r2_total, current),
        None => format!("{}/{}", model.r2_done, model.r2_total),
    };
    out.push(format!(
        "{} {}",
        paint(Tone::Gold, "R2 · 交叉评审"),
        paint(Tone::Dim, &r2_detail)
    ));
    out.push(paint(Tone::Gold, "R3 · 双裁决"));
    out.push(lane_line(&model.r3_cc, tick));
    out.push(lane_line(&model.primary, tick));
    out
}

fn status_tone_public(status: &str) -> Tone {
    match status {
        "Completed" => Tone::Ok,
        "Degraded" => Tone::Warn,
        "Failed" | "FailedPolicy" | "Cancelled" => Tone::Fail,
        "WaitingPrimaryArbiter" => Tone::Gold,
        "Queued" | "Preflight" => Tone::Dim,
        _ => Tone::Run,
    }
}

// ---- B 阶段：进展板帧构建 ----

#[cfg(test)]
fn ev(
    event_type: &str,
    phase: Option<&str>,
    party: Option<&str>,
    attempt: Option<usize>,
    data: serde_json::Value,
) -> crate::model::Event {
    serde_json::from_value(serde_json::json!({
        "event_version": "1.0",
        "sequence": 1,
        "timestamp": "2026-07-19T00:00:00Z",
        "run_id": "fixture-run",
        "event_type": event_type,
        "phase": phase,
        "party_id": party,
        "attempt": attempt,
        "data": data
    }))
    .unwrap()
}

#[cfg(test)]
fn parties() -> Vec<String> {
    BoardModel::default_parties()
}

#[test]
fn board_r1_partial_progress() {
    force_no_color();
    let events = vec![
        ev("run.created", None, None, None, serde_json::json!({})),
        ev(
            "lane.started",
            Some("R1"),
            Some("Party A"),
            Some(1),
            serde_json::json!({}),
        ),
        ev(
            "lane.started",
            Some("R1"),
            Some("Party B"),
            Some(1),
            serde_json::json!({}),
        ),
        ev(
            "lane.accepted",
            Some("R1"),
            Some("Party B"),
            Some(1),
            serde_json::json!({}),
        ),
    ];
    let model = BoardModel::from_events(
        "fixture-run",
        crate::model::RunStatus::R1Running,
        12,
        &parties(),
        &events,
    );
    let frame = build_board(&model, 0, 100).join("\n");
    assert!(frame.contains("QUINTE RUN"));
    assert!(frame.contains("R1Running"));
    assert!(frame.contains("R1 · 五席并行"));
    assert!(frame.contains("Party A"));
    assert!(frame.contains("attempt 1"));
    assert!(frame.contains("已验收"), "{frame}");
    assert!(frame.contains("pending"));
}

#[test]
fn board_retry_and_failed_states() {
    force_no_color();
    let events = vec![
        ev(
            "lane.retry_scheduled",
            Some("R1"),
            Some("Party C"),
            Some(2),
            serde_json::json!({"failure_class":"rate_limited","retry_after_seconds":12.0}),
        ),
        ev(
            "lane.finished",
            Some("R1"),
            Some("Party D"),
            Some(1),
            serde_json::json!({"exit_code":3,"timed_out":false,"cancelled":false}),
        ),
    ];
    let model = BoardModel::from_events(
        "fixture-run",
        crate::model::RunStatus::R1Running,
        3,
        &parties(),
        &events,
    );
    let frame = build_board(&model, 0, 100).join("\n");
    assert!(frame.contains("重试等待 12s · rate_limited"), "{frame}");
    assert!(frame.contains("exit 3"), "{frame}");
}

#[test]
fn board_r2_progress_and_waiting_primary() {
    force_no_color();
    let events = vec![
        ev(
            "lane.accepted",
            Some("R2"),
            Some("Party A"),
            Some(1),
            serde_json::json!({}),
        ),
        ev(
            "lane.accepted",
            Some("R2"),
            Some("Party B"),
            Some(1),
            serde_json::json!({}),
        ),
        ev(
            "lane.started",
            Some("R2"),
            Some("Party C"),
            Some(1),
            serde_json::json!({}),
        ),
        ev(
            "lane.started",
            Some("R3"),
            Some("Counterpart Arbiter"),
            Some(1),
            serde_json::json!({}),
        ),
    ];
    let model = BoardModel::from_events(
        "fixture-run",
        crate::model::RunStatus::WaitingPrimaryArbiter,
        61,
        &parties(),
        &events,
    );
    let frame = build_board(&model, 0, 100).join("\n");
    assert!(
        frame.contains("R2 · 交叉评审 2/5 · 当前 Party C"),
        "{frame}"
    );
    assert!(frame.contains("等待人工裁决"), "{frame}");
    assert!(frame.contains("Counterpart Arbiter"));
    assert!(frame.contains("61s"));
}

#[test]
fn board_plain_mode_has_no_ansi() {
    force_no_color();
    let events = vec![
        ev(
            "lane.started",
            Some("R1"),
            Some("Party A"),
            Some(1),
            serde_json::json!({}),
        ),
        ev(
            "lane.accepted",
            Some("R1"),
            Some("Party A"),
            Some(1),
            serde_json::json!({}),
        ),
    ];
    let model = BoardModel::from_events(
        "fixture-run",
        crate::model::RunStatus::R1Gate,
        5,
        &parties(),
        &events,
    );
    let frame = build_board(&model, 0, 100).join("\n");
    assert!(!frame.contains('\x1b'), "降级帧不得含 ANSI：{:?}", frame);
    assert!(frame.contains("PASS"), "{frame}");
}

#[test]
fn board_primary_accepted_marks_done() {
    force_no_color();
    let events = vec![ev(
        "primary_arbiter.accepted",
        None,
        None,
        None,
        serde_json::json!({}),
    )];
    let model = BoardModel::from_events(
        "fixture-run",
        crate::model::RunStatus::Merging,
        5,
        &parties(),
        &events,
    );
    let frame = build_board(&model, 0, 100).join("\n");
    assert!(frame.contains("人工裁决已受理"), "{frame}");
}
