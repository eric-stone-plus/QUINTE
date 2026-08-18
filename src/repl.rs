//! Bare `quinte` interactive REPL (phase C).
//!
//! Entered only when started with no arguments and stdout is a tty; slash
//! commands are fully parsed by clap and dispatched through
//! cli::execute_command, so the protocol path is identical to the CLI.
//! Line editing / history / terminal management are pure-std
//! (stty raw + Drop restore).

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Command as ProcCommand, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use clap::Parser;

use crate::cli::{self, Cli};
use crate::store::Store;
use crate::ui::{self, Tone};

// ---------------------------------------------------------------------------
// Terminal (stty raw + restore)
// ---------------------------------------------------------------------------

static SAVED_STTY: Mutex<Option<String>> = Mutex::new(None);

fn restore_terminal() {
    let mut out = io::stdout();
    let _ = out.write_all(b"\x1b[0m");
    let _ = out.flush();
    let saved = SAVED_STTY.lock().ok().and_then(|s| s.clone());
    let status = match saved {
        Some(state) => ProcCommand::new("stty")
            .arg(&state)
            .stdin(Stdio::inherit())
            .status(),
        None => ProcCommand::new("stty")
            .arg("sane")
            .stdin(Stdio::inherit())
            .status(),
    };
    if status.map(|s| !s.success()).unwrap_or(true) {
        let _ = ProcCommand::new("stty")
            .arg("sane")
            .stdin(Stdio::inherit())
            .status();
    }
}

struct RawMode {
    _private: (),
}

impl RawMode {
    fn enter() -> Result<RawMode, String> {
        let out = ProcCommand::new("stty")
            .arg("-g")
            .stdin(Stdio::inherit())
            .output()
            .map_err(|e| format!("stty unavailable: {e}"))?;
        if !out.status.success() {
            return Err("stdin is not a TTY".into());
        }
        let saved = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(mut slot) = SAVED_STTY.lock() {
            *slot = Some(saved);
        }
        let status = ProcCommand::new("stty")
            .args(["raw", "-echo"])
            .stdin(Stdio::inherit())
            .status()
            .map_err(|e| format!("stty raw failed: {e}"))?;
        if !status.success() {
            return Err("stty raw -echo failed".into());
        }
        Ok(RawMode { _private: () })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        restore_terminal();
    }
}

// ---------------------------------------------------------------------------
// Key parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    CtrlC,
    CtrlD,
}

#[derive(Default)]
struct KeyParser {
    pending: Vec<u8>,
    in_paste: bool,
}

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

impl KeyParser {
    fn feed(&mut self, bytes: &[u8]) -> Vec<Key> {
        self.pending.extend_from_slice(bytes);
        self.parse()
    }

    fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    fn flush(&mut self) -> Vec<Key> {
        if self.pending == [0x1b] {
            self.pending.clear();
        }
        self.parse()
    }

    fn parse(&mut self) -> Vec<Key> {
        let mut keys = Vec::new();
        let mut rest = std::mem::take(&mut self.pending);
        let mut i = 0;
        while i < rest.len() {
            if self.in_paste {
                if rest[i..].starts_with(PASTE_END) {
                    self.in_paste = false;
                    i += PASTE_END.len();
                    continue;
                }
                if rest[i] == 0x1b {
                    break;
                }
                match consume_char(&rest, i) {
                    Some((c, next)) => {
                        // Pasted content: fold newlines to spaces, never submit
                        keys.push(Key::Char(if c == '\r' || c == '\n' { ' ' } else { c }));
                        i = next;
                    }
                    None => break,
                }
                continue;
            }
            let b = rest[i];
            match b {
                0x1b => {
                    let tail = &rest[i..];
                    if tail.starts_with(PASTE_START) {
                        self.in_paste = true;
                        i += PASTE_START.len();
                    } else if tail.len() == 1 {
                        break;
                    } else if tail[1] == b'[' || tail[1] == b'O' {
                        match parse_csi(tail) {
                            Some((key, len)) => {
                                if let Some(k) = key {
                                    keys.push(k);
                                }
                                i += len;
                            }
                            None => break,
                        }
                    } else {
                        i += 1; // Alt+key: ignored
                    }
                }
                0x0d | 0x0a => {
                    keys.push(Key::Enter);
                    i += 1;
                }
                0x03 => {
                    keys.push(Key::CtrlC);
                    i += 1;
                }
                0x04 => {
                    keys.push(Key::CtrlD);
                    i += 1;
                }
                0x7f => {
                    keys.push(Key::Backspace);
                    i += 1;
                }
                c if c < 0x20 => {
                    i += 1;
                }
                _ => match consume_char(&rest, i) {
                    Some((c, next)) => {
                        keys.push(Key::Char(c));
                        i = next;
                    }
                    None => break,
                },
            }
        }
        self.pending = rest.split_off(i);
        keys
    }
}

fn parse_csi(seq: &[u8]) -> Option<(Option<Key>, usize)> {
    let start = 2;
    if seq.len() <= start {
        return None;
    }
    let mut j = start;
    while j < seq.len() && (seq[j].is_ascii_digit() || seq[j] == b';') {
        j += 1;
    }
    if j >= seq.len() {
        return None;
    }
    let params = std::str::from_utf8(&seq[start..j]).unwrap_or("");
    let final_byte = seq[j];
    let len = j + 1;
    let key = match final_byte {
        b'A' => Some(Key::Up),
        b'B' => Some(Key::Down),
        b'C' => Some(Key::Right),
        b'D' => Some(Key::Left),
        b'H' => Some(Key::Home),
        b'F' => Some(Key::End),
        b'~' => match params {
            "1" | "7" => Some(Key::Home),
            "4" | "8" => Some(Key::End),
            "3" => Some(Key::Delete),
            _ => None,
        },
        _ => None,
    };
    Some((key, len))
}

fn consume_char(bytes: &[u8], i: usize) -> Option<(char, usize)> {
    let first = bytes[i];
    let len = match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    };
    if i + len > bytes.len() {
        return None;
    }
    std::str::from_utf8(&bytes[i..i + len])
        .ok()
        .and_then(|s| s.chars().next())
        .map(|c| (c, i + len))
}

// ---------------------------------------------------------------------------
// History (~/.quinte/.repl_history, 0600, capped at 500, consecutive dedupe)
// ---------------------------------------------------------------------------

const HISTORY_LIMIT: usize = 500;

fn history_load(path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let mut items: Vec<String> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(serde_json::Value::String(s)) = serde_json::from_str::<serde_json::Value>(line)
            && items.last() != Some(&s)
        {
            items.push(s);
        }
    }
    if items.len() > HISTORY_LIMIT {
        items.drain(..items.len() - HISTORY_LIMIT);
    }
    items
}

fn history_save(path: &Path, items: &[String]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut content = String::new();
    for item in items {
        content.push_str(&serde_json::Value::String(item.clone()).to_string());
        content.push('\n');
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, content).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        let _ = std::fs::rename(&tmp, path);
    }
}

fn history_push(items: &mut Vec<String>, line: &str) {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return;
    }
    if items.last().map(|s| s.as_str()) == Some(trimmed) {
        return;
    }
    items.push(trimmed.to_string());
    if items.len() > HISTORY_LIMIT {
        items.drain(..items.len() - HISTORY_LIMIT);
    }
}

// ---------------------------------------------------------------------------
// Command parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplAction {
    Empty,
    Run(Vec<String>),
    Clear,
    Help,
    Quit,
    NotACommand,
    Unknown(String),
}

/// Parse one input line; slash commands map to CLI argv (without the program name).
pub(crate) fn parse_slash(line: &str) -> ReplAction {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return ReplAction::Empty;
    }
    if !trimmed.starts_with('/') {
        return ReplAction::NotACommand;
    }
    let tokens: Vec<String> = trimmed.split_whitespace().map(str::to_string).collect();
    let cmd = tokens[0].as_str();
    let arg = tokens.get(1).cloned();
    let need_arg = |usage: &str| ReplAction::Unknown(format!("usage: {usage}"));
    match cmd {
        "/doctor" => ReplAction::Run(vec!["doctor".into()]),
        "/status" => {
            let mut argv = vec!["status".to_string()];
            if let Some(id) = arg {
                argv.push(id);
            }
            ReplAction::Run(argv)
        }
        "/runs" => ReplAction::Run(vec!["status".into()]),
        "/inspect" => match arg {
            Some(id) => ReplAction::Run(vec!["inspect".into(), id]),
            None => need_arg("/inspect <run_id>"),
        },
        "/run" => match arg {
            Some(file) => {
                ReplAction::Run(vec!["run".into(), "--brief".into(), file, "--wait".into()])
            }
            None => need_arg("/run <brief-file>"),
        },
        "/wait" => match arg {
            Some(id) => ReplAction::Run(vec!["wait".into(), id]),
            None => need_arg("/wait <run_id>"),
        },
        "/resume" => match arg {
            Some(id) => ReplAction::Run(vec!["resume".into(), id]),
            None => need_arg("/resume <run_id>"),
        },
        "/cancel" => match arg {
            Some(id) => ReplAction::Run(vec!["cancel".into(), id]),
            None => need_arg("/cancel <run_id>"),
        },
        "/agents" => ReplAction::Run(vec!["agents".into(), "list".into()]),
        "/policy" => ReplAction::Run(vec!["policy".into(), "show".into()]),
        "/brief" => ReplAction::Run(vec!["brief".into(), "new".into()]),
        "/primary-request" => match arg {
            Some(id) => ReplAction::Run(vec!["primary-arbiter".into(), "request".into(), id]),
            None => need_arg("/primary-request <run_id>"),
        },
        "/primary-submit" => match (arg, tokens.get(2).cloned()) {
            (Some(id), Some(file)) => ReplAction::Run(vec![
                "primary-arbiter".into(),
                "submit".into(),
                id,
                "--response".into(),
                file,
            ]),
            _ => need_arg("/primary-submit <run_id> <file>"),
        },
        "/clear" => ReplAction::Clear,
        "/help" | "/h" | "/?" => ReplAction::Help,
        "/quit" | "/q" | "/exit" => ReplAction::Quit,
        _ => ReplAction::Unknown(format!("unknown command: {cmd} (see /help)")),
    }
}

const HELP_TEXT: &str = "Commands:
  /doctor                 environment check
  /runs                   list runs (= /status)
  /status [run_id]        overview / single run status
  /inspect <run_id>       verdict summary
  /run <brief-file>       start deliberation and wait (progress board)
  /wait <run_id>          wait for a run to advance
  /resume <run_id>        advance one round
  /cancel <run_id>        cancel a run
  /agents                 the fixed five seats and arbiters
  /policy                 current policy
  /primary-request <id>   export a Primary Arbiter request
  /primary-submit <id> <file>  submit a manual verdict
  /brief                  brief wizard (can start a run right after)
  /clear · /help · /quit";

// ---------------------------------------------------------------------------
// Line editor
// ---------------------------------------------------------------------------

struct LineEditor {
    buf: Vec<char>,
    cursor: usize,
}

impl LineEditor {
    fn new() -> LineEditor {
        LineEditor {
            buf: Vec::new(),
            cursor: 0,
        }
    }

    fn text(&self) -> String {
        self.buf.iter().collect()
    }

    fn set(&mut self, text: &str) {
        self.buf = text.chars().collect();
        self.cursor = self.buf.len();
    }

    fn render(&self) {
        let mut out = io::stdout();
        let _ = write!(out, "\r\x1b[K");
        let prompt = format!("{} ", ui::paint(Tone::Gold, "quinte❯"));
        let _ = write!(out, "{prompt}");
        let text = self.text();
        let _ = write!(out, "{text}");
        // Move the terminal cursor to the edit cursor position
        let tail: String = self.buf[self.cursor..].iter().collect();
        if !tail.is_empty() {
            let width: usize = tail.chars().map(display_width).sum();
            let _ = write!(out, "\x1b[{width}D");
        }
        let _ = out.flush();
    }
}

fn display_width(c: char) -> usize {
    let cp = c as u32;
    if cp < 0x7F {
        1
    } else if (0x1100..=0x115F).contains(&cp)
        || (0x2E80..=0xA4CF).contains(&cp)
        || (0xAC00..=0xD7A3).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFF00..=0xFF60).contains(&cp)
        || (0x20000..=0x3FFFD).contains(&cp)
        || (0x1F300..=0x1FAFF).contains(&cp)
    {
        2
    } else {
        1
    }
}

// ---------------------------------------------------------------------------
// REPL main loop
// ---------------------------------------------------------------------------

pub(crate) fn run(home: &Path) -> anyhow::Result<i32> {
    install_panic_hook();
    let store = Store::new(home.to_path_buf());
    let history_path = home.join(".repl_history");
    let mut history = history_load(&history_path);
    let mut history_pos: Option<usize> = None;
    let mut history_stash = String::new();

    // Welcome screen (codex-style plain-text banner, no pixel art)
    {
        let mut out = io::stdout();
        // Version comes from the clap runtime (guard tests forbid referencing
        // cargo's product version macro in source)
        let version = <Cli as clap::CommandFactory>::command()
            .get_version()
            .map(str::to_string)
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "{}",
            ui::paint_bold(Tone::Gold, &format!("QUINTE · LUPA · v{version}"))
        );
        let _ = writeln!(
            out,
            "{}",
            ui::paint(Tone::Gold, "──────────────────────────────")
        );
        let _ = writeln!(
            out,
            "{}",
            ui::paint(
                Tone::Dim,
                "/help for commands · /quit to exit · this is not a conversation — QUINTE is command-driven"
            )
        );
        let _ = out.flush();
    }

    let mut quit = false;
    let mut last_ctrl_c: Option<Instant> = None;
    while !quit {
        let line = {
            let raw = match RawMode::enter() {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{e}");
                    return Ok(1);
                }
            };
            let _raw_guard = raw;
            let mut editor = LineEditor::new();
            let mut parser = KeyParser::default();
            let mut stdin = io::stdin();
            let mut buf = [0u8; 64];
            editor.render();
            let mut submitted: Option<String> = None;
            while submitted.is_none() {
                let n = match stdin.read(&mut buf) {
                    Ok(0) => {
                        // EOF (terminal detached): treat as /quit
                        submitted = Some(String::new());
                        quit = true;
                        break;
                    }
                    Ok(n) => n,
                    Err(_) => break,
                };
                for key in parser.feed(&buf[..n]) {
                    match key {
                        Key::Enter => {
                            let mut out = io::stdout();
                            let _ = writeln!(out);
                            submitted = Some(editor.text());
                            break;
                        }
                        Key::CtrlC => {
                            if editor.buf.is_empty() {
                                // Double Ctrl+C on an empty line exits
                                if last_ctrl_c
                                    .map(|t| t.elapsed() < Duration::from_millis(1500))
                                    .unwrap_or(false)
                                {
                                    submitted = Some(String::new());
                                    quit = true;
                                } else {
                                    let mut out = io::stdout();
                                    let _ = writeln!(
                                        out,
                                        "\r\n{}",
                                        ui::paint(Tone::Dim, "(press Ctrl+C again to exit)")
                                    );
                                }
                                last_ctrl_c = Some(Instant::now());
                            } else {
                                editor.set("");
                            }
                            editor.render();
                        }
                        Key::CtrlD => {
                            if editor.buf.is_empty() {
                                submitted = Some(String::new());
                                quit = true;
                            }
                        }
                        Key::Char(c) => {
                            editor.buf.insert(editor.cursor, c);
                            editor.cursor += 1;
                            history_pos = None;
                            editor.render();
                        }
                        Key::Backspace => {
                            if editor.cursor > 0 {
                                editor.cursor -= 1;
                                editor.buf.remove(editor.cursor);
                            }
                            editor.render();
                        }
                        Key::Delete => {
                            if editor.cursor < editor.buf.len() {
                                editor.buf.remove(editor.cursor);
                            }
                            editor.render();
                        }
                        Key::Left => {
                            if editor.cursor > 0 {
                                editor.cursor -= 1;
                            }
                            editor.render();
                        }
                        Key::Right => {
                            if editor.cursor < editor.buf.len() {
                                editor.cursor += 1;
                            }
                            editor.render();
                        }
                        Key::Home => {
                            editor.cursor = 0;
                            editor.render();
                        }
                        Key::End => {
                            editor.cursor = editor.buf.len();
                            editor.render();
                        }
                        Key::Up => {
                            if !history.is_empty() {
                                match history_pos {
                                    None => {
                                        history_stash = editor.text();
                                        history_pos = Some(history.len() - 1);
                                    }
                                    Some(pos) if pos > 0 => history_pos = Some(pos - 1),
                                    _ => {}
                                }
                                if let Some(pos) = history_pos {
                                    let text = history[pos].clone();
                                    editor.set(&text);
                                }
                            }
                            editor.render();
                        }
                        Key::Down => {
                            if let Some(pos) = history_pos {
                                if pos + 1 < history.len() {
                                    history_pos = Some(pos + 1);
                                    let text = history[pos + 1].clone();
                                    editor.set(&text);
                                } else {
                                    history_pos = None;
                                    let stash = std::mem::take(&mut history_stash);
                                    editor.set(&stash);
                                }
                            }
                            editor.render();
                        }
                    }
                    if submitted.is_some() {
                        break;
                    }
                }
                // Timeout fallback for a lone ESC (discard)
                if parser.has_pending() {
                    let _ = parser.flush();
                }
            }
            submitted.unwrap_or_default()
        };
        // RawMode was restored with the scope (commands run in normal terminal mode)

        let line = line.trim().to_string();
        if !line.is_empty() {
            history_push(&mut history, &line);
            history_save(&history_path, &history);
        }

        match parse_slash(&line) {
            ReplAction::Empty => {}
            ReplAction::Quit => quit = true,
            ReplAction::Clear => {
                let mut out = io::stdout();
                let _ = out.write_all(b"\x1b[2J\x1b[H");
                let _ = out.flush();
            }
            ReplAction::Help => println!("{HELP_TEXT}"),
            ReplAction::NotACommand => {
                println!(
                    "{}",
                    ui::paint(Tone::Dim, "this is not a conversation — QUINTE is command-driven; see /help")
                );
            }
            ReplAction::Unknown(msg) => println!("{}", ui::paint(Tone::Warn, &msg)),
            ReplAction::Run(argv) => {
                let is_brief_new = argv.len() == 2 && argv[0] == "brief" && argv[1] == "new";
                let full: Vec<String> = std::iter::once("quinte".to_string()).chain(argv).collect();
                match Cli::try_parse_from(&full) {
                    Ok(parsed) => {
                        let result =
                            cli::execute_command(&home.to_path_buf(), &store, parsed.command);
                        match result {
                            Ok(code) => {
                                if code != 0 {
                                    println!(
                                        "{}",
                                        ui::paint(Tone::Dim, &format!("(exit code {code})"))
                                    );
                                } else if is_brief_new {
                                    ask_run_latest(home, &store);
                                }
                            }
                            Err(e) => {
                                println!("{}", ui::paint(Tone::Fail, &format!("error: {e:#}")));
                            }
                        }
                    }
                    Err(e) => {
                        // clap's help/usage output prints as usual
                        let _ = e.print();
                    }
                }
            }
        }
    }
    println!("{}", ui::paint(Tone::Dim, "VALE · farewell"));
    Ok(0)
}

/// After /brief succeeds: find the newest brief and offer to start a run immediately.
fn ask_run_latest(home: &Path, store: &Store) {
    let dir = home.join("briefs");
    let latest = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
        .filter_map(|p| {
            p.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| (t, p))
        })
        .max_by_key(|(t, _)| *t)
        .map(|(_, p)| p);
    let Some(path) = latest else { return };
    {
        let mut out = io::stdout();
        let _ = write!(out, "{} ", ui::paint(Tone::Gold, "start deliberation now? [y/N]"));
        let _ = out.flush();
    }
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        return;
    }
    if !answer.trim().eq_ignore_ascii_case("y") {
        return;
    }
    let full = vec![
        "quinte".to_string(),
        "run".into(),
        "--brief".into(),
        path.display().to_string(),
        "--wait".into(),
    ];
    if let Ok(parsed) = Cli::try_parse_from(&full) {
        match cli::execute_command(&home.to_path_buf(), store, parsed.command) {
            Ok(code) => {
                if code != 0 {
                    println!("{}", ui::paint(Tone::Dim, &format!("(exit code {code})")));
                }
            }
            Err(e) => println!("{}", ui::paint(Tone::Fail, &format!("error: {e:#}"))),
        }
    }
}

fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        default_hook(info);
    }));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_commands() {
        assert_eq!(parse_slash(""), ReplAction::Empty);
        assert_eq!(parse_slash("  "), ReplAction::Empty);
        assert_eq!(
            parse_slash("/doctor"),
            ReplAction::Run(vec!["doctor".into()])
        );
        assert_eq!(parse_slash("/runs"), ReplAction::Run(vec!["status".into()]));
        assert_eq!(
            parse_slash("/status abc-1"),
            ReplAction::Run(vec!["status".into(), "abc-1".into()])
        );
        assert_eq!(
            parse_slash("/inspect abc"),
            ReplAction::Run(vec!["inspect".into(), "abc".into()])
        );
        assert_eq!(
            parse_slash("/run b.json"),
            ReplAction::Run(vec![
                "run".into(),
                "--brief".into(),
                "b.json".into(),
                "--wait".into()
            ])
        );
        assert_eq!(
            parse_slash("/wait x"),
            ReplAction::Run(vec!["wait".into(), "x".into()])
        );
        assert_eq!(
            parse_slash("/resume x"),
            ReplAction::Run(vec!["resume".into(), "x".into()])
        );
        assert_eq!(
            parse_slash("/cancel x"),
            ReplAction::Run(vec!["cancel".into(), "x".into()])
        );
        assert_eq!(
            parse_slash("/agents"),
            ReplAction::Run(vec!["agents".into(), "list".into()])
        );
        assert_eq!(
            parse_slash("/policy"),
            ReplAction::Run(vec!["policy".into(), "show".into()])
        );
        assert_eq!(
            parse_slash("/primary-request x"),
            ReplAction::Run(vec!["primary-arbiter".into(), "request".into(), "x".into()])
        );
        assert_eq!(
            parse_slash("/primary-submit x f.json"),
            ReplAction::Run(vec![
                "primary-arbiter".into(),
                "submit".into(),
                "x".into(),
                "--response".into(),
                "f.json".into()
            ])
        );
        assert_eq!(
            parse_slash("/brief"),
            ReplAction::Run(vec!["brief".into(), "new".into()])
        );
        assert_eq!(parse_slash("/clear"), ReplAction::Clear);
        assert_eq!(parse_slash("/help"), ReplAction::Help);
        assert_eq!(parse_slash("/quit"), ReplAction::Quit);
        assert_eq!(parse_slash("你好"), ReplAction::NotACommand);
        assert!(matches!(parse_slash("/inspect"), ReplAction::Unknown(_)));
        assert!(matches!(parse_slash("/run"), ReplAction::Unknown(_)));
        assert!(matches!(
            parse_slash("/primary-submit x"),
            ReplAction::Unknown(_)
        ));
        assert!(matches!(parse_slash("/nope"), ReplAction::Unknown(_)));
    }

    #[test]
    fn history_dedupe_cap_cursor() {
        let mut items = Vec::new();
        history_push(&mut items, "a");
        history_push(&mut items, "a"); // consecutive dedupe
        history_push(&mut items, "b");
        history_push(&mut items, "");
        assert_eq!(items, vec!["a".to_string(), "b".to_string()]);
        let dir = std::env::temp_dir().join(format!("quinte-repl-test-{}", std::process::id()));
        let path = dir.join("history");
        let big: Vec<String> = (0..520).map(|i| format!("cmd-{i}")).collect();
        history_save(&path, &big);
        let loaded = history_load(&path);
        assert_eq!(loaded.len(), 500);
        assert_eq!(loaded.last().map(|s| s.as_str()), Some("cmd-519"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dispatch_status_and_runs_on_fixture_store() {
        let dir = std::env::temp_dir().join(format!(
            "quinte-repl-dispatch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        crate::policy::initialize(&dir.to_path_buf(), true).unwrap();
        let store = Store::new(dir.to_path_buf());
        // /status without args (empty list, exit 0, no crash)
        let parsed = Cli::try_parse_from(["quinte", "status"]).unwrap();
        let code = cli::execute_command(&dir.to_path_buf(), &store, parsed.command).unwrap();
        assert_eq!(code, 0);
        // /runs same path
        let parsed = Cli::try_parse_from(["quinte", "status"]).unwrap();
        let code = cli::execute_command(&dir.to_path_buf(), &store, parsed.command).unwrap();
        assert_eq!(code, 0);
        // /doctor (no agents; exit 2 expected, but no crash)
        let parsed = Cli::try_parse_from(["quinte", "doctor"]).unwrap();
        let code = cli::execute_command(&dir.to_path_buf(), &store, parsed.command).unwrap();
        assert!(code == 0 || code == 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn line_editor_ops() {
        let mut ed = LineEditor::new();
        ed.set("/status");
        assert_eq!(ed.cursor, 7);
        ed.cursor = 1;
        ed.buf.insert(ed.cursor, 'x');
        ed.cursor += 1;
        assert_eq!(ed.text(), "/xstatus");
        ed.cursor -= 1;
        ed.buf.remove(ed.cursor);
        assert_eq!(ed.text(), "/status");
    }
}
