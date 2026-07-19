use crate::doctor;
use crate::error::{QuinteError, Result};
use crate::model::{CliEnvelope, Policy, RunManifest, RunStatus};
use crate::policy;
use crate::run::{self, RunOptions};
use crate::store::Store;
use crate::ui::{self, BoardModel, Tone};
use crate::util::{read_json, user_home};
use crate::wolf;
use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand, error::ErrorKind};
use serde::Serialize;
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
#[derive(Debug, Parser)]
#[command(name = "quinte", version, about = "Protocol-enforcing QUINTE CLI")]
#[command(
    disable_help_subcommand = true,
    subcommand_required = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[arg(long, global = true, env = "QUINTE_HOME", hide = true)]
    home: Option<PathBuf>,
    #[command(subcommand)]
    pub(crate) command: Command,
}
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    Init(InitArgs),
    Status(StatusArgs),
    Doctor(JsonArgs),
    Run(RunArgs),
    Wait(IdArgs),
    Resume(IdArgs),
    Cancel(IdArgs),
    Inspect(IdArgs),
    #[command(name = "primary-arbiter")]
    PrimaryArbiter(PrimaryArbiterArgs),
    Agents(AgentArgs),
    Policy(PolicyArgs),
    Credential(CredentialArgs),
    /// brief 向导与校验
    Brief(BriefArgs),
    /// 输出 shell 补全脚本
    Completions(CompletionsArgs),
    #[command(name = "__worker", hide = true)]
    Worker(WorkerArgs),
    /// Internal Claude Code apiKeyHelper entrypoint. Not a user command.
    #[command(name = "__credential-helper", hide = true)]
    CredentialHelper(CredentialHelperArgs),
}
#[derive(Debug, Args)]
pub(crate) struct InitArgs {
    #[arg(long)]
    force: bool,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct JsonArgs {
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct StatusArgs {
    run_id: Option<String>,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct RunArgs {
    #[arg(long, value_name = "FILE")]
    brief: PathBuf,
    #[arg(long)]
    wait: bool,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct WorkerArgs {
    run_id: String,
}
#[derive(Debug, Args)]
pub(crate) struct IdArgs {
    run_id: String,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct PrimaryArbiterArgs {
    #[command(subcommand)]
    command: PrimaryArbiterCommand,
}
#[derive(Debug, Subcommand)]
pub(crate) enum PrimaryArbiterCommand {
    Request(IdArgs),
    Submit(PrimaryArbiterSubmitArgs),
}
#[derive(Debug, Args)]
pub(crate) struct PrimaryArbiterSubmitArgs {
    run_id: String,
    #[arg(
        long,
        value_name = "FILE",
        required_unless_present = "verdict",
        conflicts_with = "verdict"
    )]
    response: Option<PathBuf>,
    #[arg(
        long,
        value_name = "FILE",
        required_unless_present = "response",
        conflicts_with = "response"
    )]
    verdict: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommand,
}
#[derive(Debug, Subcommand)]
pub(crate) enum AgentCommand {
    List(JsonArgs),
    Describe {
        id: String,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Debug, Args)]
pub(crate) struct PolicyArgs {
    #[command(subcommand)]
    command: PolicyCommand,
}
#[derive(Debug, Subcommand)]
pub(crate) enum PolicyCommand {
    Show(JsonArgs),
    Validate(JsonArgs),
}
#[derive(Debug, Args)]
pub(crate) struct CredentialArgs {
    #[command(subcommand)]
    command: CredentialCommand,
}
#[derive(Debug, Subcommand)]
pub(crate) enum CredentialCommand {
    /// Report whether the Claude credential is available and isolated.
    Status(JsonArgs),
}
#[derive(Debug, Args)]
pub(crate) struct CredentialHelperArgs {
    #[arg(long)]
    service: String,
    #[arg(long, value_name = "DIR")]
    lane_root: PathBuf,
    #[arg(long, value_name = "FILE")]
    authorization: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct BriefArgs {
    #[command(subcommand)]
    command: BriefCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BriefCommand {
    /// tty 交互向导生成 brief；--print-template 输出模板（脚本/heredoc 用）
    New(BriefNewArgs),
    /// 按契约校验 brief 文件
    Validate(BriefValidateArgs),
}

#[derive(Debug, Args)]
pub(crate) struct BriefNewArgs {
    #[arg(long)]
    print_template: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct BriefValidateArgs {
    file: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CompletionsArgs {
    /// bash / zsh / fish
    shell: String,
}
pub fn entrypoint() -> Result<i32> {
    // 裸 `quinte`（无参数）且 stdout 是 tty → 交互 REPL；
    // 非 tty（管道/脚本）保持原 arg_required_else_help 行为，输出字节不变。
    if std::env::args_os().len() == 1 && ui::stdout_is_tty() {
        let home =
            resolve_home(std::env::var_os("QUINTE_HOME").map(PathBuf::from)).map_err(map_error)?;
        return crate::repl::run(&home).map_err(map_error);
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            error
                .print()
                .map_err(|error| QuinteError::Internal(error.into()))?;
            return Ok(0);
        }
        Err(error) => return Err(QuinteError::Usage(error.to_string())),
    };
    execute(cli).map_err(map_error)
}
fn resolve_home(cli_home: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match cli_home {
        Some(path) => Ok(path),
        None => Ok(user_home()?.join(".quinte")),
    }
}

fn execute(cli: Cli) -> anyhow::Result<i32> {
    let home = resolve_home(cli.home)?;
    let store = Store::new(home.clone());
    execute_command(&home, &store, cli.command)
}

/// 命令执行主体（REPL 复用；command 由 clap 解析产生，保证与 CLI 路径完全一致）。
pub(crate) fn execute_command(
    home: &PathBuf,
    store: &Store,
    command: Command,
) -> anyhow::Result<i32> {
    match command {
        Command::Init(args) => {
            let path = policy::initialize(home, args.force)?;
            let human = human_init(home, &path);
            emit(args.json, json!({"policy": path, "home": home}), human)?;
            Ok(0)
        }
        Command::Status(args) => {
            ensure_initialized(store)?;
            if let Some(run_id) = args.run_id {
                let manifest = store.load_manifest(&run_id)?;
                emit(
                    args.json,
                    &manifest,
                    format_status(&manifest.run_id, manifest.status),
                )?;
            } else {
                let manifests = store.list_manifests()?;
                emit(args.json, &manifests, human_status_table(&manifests))?;
            }
            Ok(0)
        }
        Command::Doctor(args) => {
            ensure_initialized(store)?;
            let policy = load_policy(store)?;
            let report = doctor::run(&policy);
            let ok = report.ok;
            emit(args.json, &report, human_doctor(&report))?;
            Ok(if ok { 0 } else { 2 })
        }
        Command::Run(args) => {
            ensure_initialized(store)?;
            let policy = load_policy(store)?;
            let created = run::create(
                store,
                &policy,
                &RunOptions {
                    brief_path: args.brief,
                },
            )?;
            let worker_pid = match run::spawn_worker(store, &created.run_id) {
                Ok(pid) => pid,
                Err(error) => {
                    let _ = run::record_worker_failure(
                        store,
                        &created.run_id,
                        &format!("worker launch failed: {error:#}"),
                    );
                    return Err(error);
                }
            };
            eprintln!(
                "QUINTE run {} created; worker {worker_pid} started",
                created.run_id
            );
            let status = if args.wait {
                match wait_progress(store, home, &created.run_id, args.json) {
                    Ok(status) => status,
                    Err(error) if error.downcast_ref::<run::WaitInterrupted>().is_some() => {
                        return Ok(130);
                    }
                    Err(error) => return Err(error),
                }
            } else {
                created.status
            };
            emit(
                args.json,
                json!({"run_id": created.run_id, "status": status, "run_dir": created.run_dir}),
                format_status(&created.run_id, status),
            )?;
            if status == RunStatus::Failed
                && store
                    .load_manifest(&created.run_id)?
                    .error
                    .as_ref()
                    .is_some_and(|error| error.code == "preflight_failed")
            {
                Ok(2)
            } else {
                Ok(status_code(status))
            }
        }
        Command::Resume(args) => {
            ensure_initialized(store)?;
            let status = run::advance(store, &args.run_id)?;
            emit(
                args.json,
                json!({"run_id": args.run_id, "status": status}),
                format_status(&args.run_id, status),
            )?;
            Ok(status_code(status))
        }
        Command::Wait(args) => {
            ensure_initialized(store)?;
            let status = match wait_progress(store, home, &args.run_id, args.json) {
                Ok(status) => status,
                Err(error) if error.downcast_ref::<run::WaitInterrupted>().is_some() => {
                    return Ok(130);
                }
                Err(error) => return Err(error),
            };
            emit(
                args.json,
                json!({"run_id": args.run_id, "status": status}),
                format_status(&args.run_id, status),
            )?;
            Ok(status_code(status))
        }
        Command::Cancel(args) => {
            ensure_initialized(store)?;
            let status = run::cancel(store, &args.run_id)?;
            emit(
                args.json,
                json!({"run_id": args.run_id, "status": status}),
                format_status(&args.run_id, status),
            )?;
            Ok(0)
        }
        Command::Inspect(args) => {
            ensure_initialized(store)?;
            let manifest = store.load_manifest(&args.run_id)?;
            let integrity = run::verify_result_integrity(store, &args.run_id)?;
            let result_path = store.run_dir(&args.run_id)?.join("result.json");
            let result = if matches!(manifest.status, RunStatus::Completed | RunStatus::Degraded)
                && result_path.exists()
            {
                Some(read_json::<Value>(&result_path)?)
            } else {
                None
            };
            let events = store.events(&args.run_id)?;
            let historical = integrity.as_ref().map(|i| !i.actionable).unwrap_or(false);
            let report_path = {
                let candidate = store.run_dir(&args.run_id)?.join("report.md");
                if candidate.exists() {
                    Some(candidate)
                } else {
                    None
                }
            };
            let human = human_inspect(
                &manifest,
                result.as_ref(),
                historical,
                report_path.as_deref(),
            );
            let result_contract = integrity.map(|integrity| {
                json!({
                    "version": integrity.contract_version,
                    "actionable": integrity.actionable,
                    "mode": if integrity.actionable { "current" } else { "historical_read_only" },
                })
            });
            emit(
                args.json,
                json!({
                    "manifest": manifest,
                    "result": result,
                    "result_contract": result_contract,
                    "events": events
                }),
                human,
            )?;
            Ok(status_code(manifest.status))
        }
        Command::PrimaryArbiter(args) => match args.command {
            PrimaryArbiterCommand::Request(args) => {
                let path = store
                    .run_dir(&args.run_id)?
                    .join("r3/primary-arbiter-request.json");
                let request: Value =
                    read_json(&path).context("primary-arbiter request is not ready")?;
                emit(
                    args.json,
                    request,
                    format!("Primary Arbiter request: {}", path.display()),
                )?;
                Ok(0)
            }
            PrimaryArbiterCommand::Submit(args) => {
                let status = if let Some(verdict) = args.verdict {
                    run::submit_primary_arbiter_verdict(store, &args.run_id, &verdict)?
                } else {
                    run::submit_primary_arbiter(
                        store,
                        &args.run_id,
                        args.response.as_deref().unwrap(),
                    )?
                };
                emit(
                    args.json,
                    json!({"run_id": args.run_id, "status": status}),
                    format_status(&args.run_id, status),
                )?;
                Ok(status_code(status))
            }
        },
        Command::Agents(args) => {
            let policy = load_policy(store)?;
            match args.command {
                AgentCommand::List(args) => {
                    emit(
                        args.json,
                        &policy.roster,
                        format!("{} fixed QUINTE parties", policy.roster.len()),
                    )?;
                }
                AgentCommand::Describe { id, json } => {
                    let route = policy
                        .roster
                        .iter()
                        .chain(std::iter::once(&policy.counterpart_arbiter))
                        .find(|route| route.party_id == id || route.route_id == id)
                        .ok_or_else(|| anyhow::anyhow!("unknown party/route {id}"))?;
                    emit(
                        json,
                        route,
                        format!(
                            "{} -> {} ({})",
                            route.party_id, route.route_id, route.adapter
                        ),
                    )?;
                }
            }
            Ok(0)
        }
        Command::Policy(args) => {
            let policy = load_policy(store)?;
            match args.command {
                PolicyCommand::Show(args) => {
                    emit(args.json, &policy, "Effective QUINTE policy".into())?
                }
                PolicyCommand::Validate(args) => {
                    policy::validate(&policy)?;
                    emit(args.json, json!({"valid": true}), "Policy is valid".into())?;
                }
            }
            Ok(0)
        }
        Command::Worker(args) => {
            let _worker_stdio = run::prepare_worker_stdio()?;
            ensure_initialized(store)?;
            let _heartbeat = run::WorkerHeartbeat::start(store, &args.run_id)?;
            match run::advance(store, &args.run_id) {
                Ok(status) => Ok(status_code(status)),
                Err(error) => {
                    let message = format!("background scheduler failed: {error:#}");
                    if !error.to_string().contains("already being advanced") {
                        let _ = run::record_worker_failure(store, &args.run_id, &message);
                    }
                    Err(error.context(message))
                }
            }
        }
        Command::Credential(args) => match args.command {
            CredentialCommand::Status(args) => {
                let status = crate::credential::probe(crate::credential::DEFAULT_CLAUDE_SERVICE);
                emit(
                    args.json,
                    &status,
                    format!(
                        "Claude credential: available={} isolated={} ({})",
                        status.available, status.isolated, status.message
                    ),
                )?;
                Ok(if status.available { 0 } else { 2 })
            }
        },
        Command::CredentialHelper(args) => {
            crate::credential::authorize_helper(
                &args.service,
                &args.lane_root,
                &args.authorization,
            )?;
            let secret = crate::credential::get_isolated(&args.service)?;
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(secret.as_bytes())?;
            stdout.flush()?;
            Ok(0)
        }
        Command::Brief(args) => match args.command {
            BriefCommand::New(args) => {
                if args.print_template || !ui::stdout_is_tty() {
                    let template = crate::brief::print_template();
                    let value: Value = serde_json::from_str(&template)?;
                    emit(args.json, json!({"template": value}), template)?;
                } else {
                    let (human, path) = crate::brief::wizard_new(home)?;
                    emit(args.json, json!({"path": path}), human)?;
                }
                Ok(0)
            }
            BriefCommand::Validate(args) => {
                let (report, ok) = crate::brief::validate_file(&args.file);
                emit(args.json, json!({"file": args.file, "valid": ok}), report)?;
                Ok(if ok { 0 } else { 2 })
            }
        },
        Command::Completions(args) => match crate::completions::render(&args.shell) {
            Some(script) => {
                eprintln!("{}", crate::completions::install_hint(&args.shell));
                print!("{script}");
                use std::io::Write;
                std::io::stdout().flush()?;
                Ok(0)
            }
            None => bail!("unsupported shell: {}（支持 bash/zsh/fish）", args.shell),
        },
    }
}
fn load_policy(store: &Store) -> anyhow::Result<Policy> {
    policy::load_for_runtime(&store.policy_path())
}
fn ensure_initialized(store: &Store) -> anyhow::Result<()> {
    if !store.policy_path().exists() {
        bail!("QUINTE is not initialized; run `quinte init`");
    }
    Ok(())
}
fn emit<T: Serialize>(json_mode: bool, data: T, human: String) -> anyhow::Result<()> {
    if json_mode {
        println!("{}", serde_json::to_string(&CliEnvelope::ok(data))?);
    } else {
        println!("{human}");
    }
    Ok(())
}
// ---------------------------------------------------------------------------
// 实时进展板（旁路显示；wait 语义零改动）
// ---------------------------------------------------------------------------

/// 包装 run::wait：仅当 stdout 是 tty、非 --json、且颜色未降级时，
/// 旁路起一个显示线程轮询 manifest + events.jsonl 绘制进展板；
/// Ctrl+C/超时/状态推进的语义完全由 run::wait 决定。
/// 不用 join（tty 缓冲满时写会阻塞）：stop+ack 握手后由主线程定格终帧。
fn wait_progress(
    store: &Store,
    home: &std::path::Path,
    run_id: &str,
    json_mode: bool,
) -> anyhow::Result<RunStatus> {
    if json_mode || !ui::color_enabled() {
        return run::wait(store, run_id, Duration::from_millis(500));
    }
    let parties = load_policy(store)
        .map(|p| p.roster.iter().map(|r| r.party_id.clone()).collect())
        .unwrap_or_else(|_| BoardModel::default_parties());
    let stop = Arc::new(AtomicBool::new(false));
    let ack = Arc::new(AtomicBool::new(false));
    let printed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let handle = {
        let stop = Arc::clone(&stop);
        let ack = Arc::clone(&ack);
        let printed = Arc::clone(&printed);
        let home = home.to_path_buf();
        let run_id = run_id.to_string();
        let parties = parties.clone();
        thread::spawn(move || board_loop(&home, &run_id, &parties, &stop, &ack, &printed))
    };
    let result = run::wait(store, run_id, Duration::from_millis(500));
    stop.store(true, Ordering::SeqCst);
    // 等显示线程退出主循环（至多 1s；异常慢 tty 下放弃等待也不致命）
    for _ in 0..20 {
        if ack.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    // 定格最终帧（主线程绘制，避免与显示线程交错）
    if let Some(frame) = board_frame(store, run_id, &parties, 0, terminal_size().0) {
        redraw(&frame, &printed);
    }
    drop(handle);
    result
}

/// 读当前 manifest + events，构建一帧（读失败沿用上一帧）。
fn board_frame(
    store: &Store,
    run_id: &str,
    parties: &[String],
    tick: usize,
    width: usize,
) -> Option<Vec<String>> {
    let manifest = store.load_manifest(run_id).ok()?;
    let events = store.events(run_id).unwrap_or_default();
    let elapsed = chrono::DateTime::parse_from_rfc3339(&manifest.created_at)
        .ok()
        .and_then(|ts| {
            (chrono::Utc::now() - ts.with_timezone(&chrono::Utc))
                .to_std()
                .ok()
        })
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let model = BoardModel::from_events(run_id, manifest.status, elapsed, parties, &events);
    Some(ui::build_board(&model, tick, width))
}

fn board_loop(
    home: &std::path::Path,
    run_id: &str,
    parties: &[String],
    stop: &AtomicBool,
    ack: &AtomicBool,
    printed: &std::sync::atomic::AtomicUsize,
) {
    let store = Store::new(home.to_path_buf());
    let (width, _) = terminal_size();
    let mut tick = 0usize;
    while !stop.load(Ordering::SeqCst) {
        if let Some(frame) = board_frame(&store, run_id, parties, tick, width) {
            redraw(&frame, printed);
        }
        tick += 1;
        // ~500ms 轮询，分片检查停止标志
        for _ in 0..10 {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    ack.store(true, Ordering::SeqCst);
}

/// 光标上提 printed 行后逐行重写（不滚屏）。
fn redraw(frame: &[String], printed: &std::sync::atomic::AtomicUsize) {
    let mut out = std::io::stdout();
    let prev = printed.load(Ordering::SeqCst);
    if prev > 0 {
        let _ = write!(out, "\x1b[{}A", prev);
    }
    for line in frame {
        let _ = writeln!(out, "{}\x1b[K", line);
    }
    let _ = out.flush();
    printed.store(frame.len(), Ordering::SeqCst);
}

/// 终端尺寸（stty size；失败回退 80x24）。
fn terminal_size() -> (usize, usize) {
    let out = std::process::Command::new("stty")
        .arg("size")
        .stdin(std::process::Stdio::inherit())
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let mut parts = text.split_whitespace();
            let rows = parts.next().and_then(|p| p.parse().ok()).unwrap_or(24);
            let cols = parts.next().and_then(|p| p.parse().ok()).unwrap_or(80);
            (cols.max(30), rows.max(8))
        }
        _ => (80, 24),
    }
}

// ---------------------------------------------------------------------------
// 人类输出（加性 UX 层；--json 路径不受影响）
// ---------------------------------------------------------------------------
fn status_tone(status: RunStatus) -> Tone {
    match status {
        RunStatus::Completed => Tone::Ok,
        RunStatus::Degraded => Tone::Warn,
        RunStatus::Failed | RunStatus::FailedPolicy | RunStatus::Cancelled => Tone::Fail,
        RunStatus::WaitingPrimaryArbiter => Tone::Gold,
        RunStatus::Queued | RunStatus::Preflight => Tone::Dim,
        _ => Tone::Run, // R1/R2/R3/Merging 等运行态
    }
}
/// 统一状态行：● <Status>  <run_id>（着色，降级为纯文本）。
fn format_status(run_id: &str, status: RunStatus) -> String {
    let tone = status_tone(status);
    format!(
        "{} {}  {}",
        ui::paint(tone, ui::dot()),
        ui::paint_bold(tone, &format!("{status:?}")),
        ui::paint(Tone::Dim, run_id)
    )
}
fn human_init(home: &std::path::Path, policy_path: &std::path::Path) -> String {
    let mut out = wolf::render_wolf();
    out.push_str(&format!(
        "{} {}\n",
        ui::paint_bold(Tone::Gold, "QUINTE"),
        ui::paint(Tone::Dim, "· LUPA · 五席母狼已就位")
    ));
    out.push_str(&format!(
        "{} Initialized QUINTE at {}\n",
        ui::paint(Tone::Ok, ui::mark_ok()),
        home.display()
    ));
    out.push_str(&format!(
        "{}\n",
        ui::paint(Tone::Dim, &format!("policy: {}", policy_path.display()))
    ));
    out.push_str(&format!("{}\n", ui::paint_bold(Tone::Gold, "下一步")));
    out.push_str("  1. quinte doctor           # 检查 agents / 凭据 / 平台\n");
    out.push_str("  2. 编写 brief.json         # 议题书\n");
    out.push_str("  3. quinte run --brief <file> [--wait]");
    out
}
fn human_status_table(manifests: &[RunManifest]) -> String {
    if manifests.is_empty() {
        return format!(
            "暂无 run · 用 {} 发起第一次审议",
            ui::paint(Tone::Gold, "quinte run --brief <file>")
        );
    }
    let mut out = ui::paint_bold(Tone::Gold, &format!("QUINTE · {} run(s)", manifests.len()));
    for m in manifests {
        let tone = status_tone(m.status);
        out.push_str(&format!(
            "\n{} {} {} {}",
            ui::paint(tone, ui::dot()),
            ui::paint_bold(tone, &ui::pad_right(&format!("{:?}", m.status), 22)),
            m.run_id,
            ui::paint(Tone::Dim, &ui::truncate(&m.updated_at, 19)),
        ));
    }
    out
}
fn human_inspect(
    manifest: &RunManifest,
    result: Option<&Value>,
    historical: bool,
    report_path: Option<&std::path::Path>,
) -> String {
    let mut out = format_status(&manifest.run_id, manifest.status);
    if historical {
        out.push_str(&format!(
            "\n{}",
            ui::paint(
                Tone::Dim,
                "historical_read_only · 历史只读结果（不可再行动）"
            )
        ));
    }
    if let Some(result) = result {
        for key in [
            "recommendation",
            "summary",
            "verdict",
            "decision",
            "outcome",
        ] {
            if let Some(v) = result.get(key) {
                let text = v
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| v.to_string());
                out.push_str(&format!(
                    "\n{} {}",
                    ui::paint_bold(Tone::Gold, "裁决"),
                    ui::truncate(&text, 200)
                ));
                break;
            }
        }
        // 严重度统计：优先 findings，缺省用协议 residuals
        let findings = result
            .get("findings")
            .and_then(Value::as_array)
            .or_else(|| result.get("residuals").and_then(Value::as_array));
        if let Some(findings) = findings {
            let mut counts: std::collections::BTreeMap<String, usize> =
                std::collections::BTreeMap::new();
            for f in findings {
                let severity = f
                    .get("severity")
                    .and_then(Value::as_str)
                    .unwrap_or("UNKNOWN")
                    .to_uppercase();
                *counts.entry(severity).or_insert(0) += 1;
            }
            if !counts.is_empty() {
                let parts: Vec<String> = counts
                    .iter()
                    .map(|(sev, n)| format!("{sev} ×{n}"))
                    .collect();
                out.push_str(&format!(
                    "\n{} {}",
                    ui::paint_bold(Tone::Gold, "发现"),
                    parts.join(" · ")
                ));
            }
        }
    }
    if let Some(path) = report_path {
        out.push_str(&format!(
            "\n{}",
            ui::paint(Tone::Dim, &format!("查看完整报告: {}", path.display()))
        ));
    }
    out
}
fn doctor_hint(name: &str) -> String {
    let hint = match name {
        "os_sandbox" => "进程级隔离 ≠ 内核沙箱；请按威胁模型评估后再依赖 strict 模式",
        "strict_sandbox_policy" => "无可用内核沙箱后端，strict 为 fail-closed；请改用 process 模式",
        "git" => "安装 git 以启用快照溯源（可选）",
        "process_group_supervision" => "当前平台不支持进程组监管，lane 退出可能残留子进程",
        "silent_child_launch" => "子进程静默启动不可用",
        _ if name.contains("credential") => "运行 quinte credential status 检查凭据隔离",
        _ => "运行 quinte doctor --json 查看该检查详情",
    };
    ui::paint(Tone::Dim, &format!("提示: {hint}"))
}
fn human_doctor(report: &doctor::DoctorReport) -> String {
    let head_tone = if report.ok { Tone::Ok } else { Tone::Fail };
    let head_mark = if report.ok {
        ui::mark_ok()
    } else {
        ui::mark_fail()
    };
    let mut text = format!(
        "{} {}",
        ui::paint_bold(head_tone, &format!("QUINTE DOCTOR · {}", report.platform)),
        ui::paint(head_tone, head_mark)
    );
    // 分组：agents（带 party_id）/ credential（名字含 credential）/ platform（其余）
    let groups = ["agents", "credential", "platform"];
    for group in groups {
        let checks: Vec<&Value> = report
            .checks
            .iter()
            .filter(|check| match group {
                "agents" => check.get("party_id").is_some(),
                "credential" => {
                    check.get("party_id").is_none()
                        && check
                            .get("name")
                            .and_then(Value::as_str)
                            .map(|n| n.contains("credential"))
                            .unwrap_or(false)
                }
                _ => {
                    check.get("party_id").is_none()
                        && !check
                            .get("name")
                            .and_then(Value::as_str)
                            .map(|n| n.contains("credential"))
                            .unwrap_or(false)
                }
            })
            .collect();
        if checks.is_empty() {
            continue;
        }
        text.push_str(&format!(
            "\n{}",
            ui::paint_bold(Tone::Gold, &group.to_uppercase())
        ));
        for check in checks {
            let ok = check.get("ok").and_then(Value::as_bool).unwrap_or(false);
            let severity = check.get("severity").and_then(Value::as_str).unwrap_or("");
            let name = check
                .get("party_id")
                .or_else(|| check.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("check");
            let message = check.get("message").and_then(Value::as_str).unwrap_or("");
            let (mark, tone) = if ok {
                (ui::mark_ok(), Tone::Ok)
            } else if severity == "warning" {
                (ui::mark_warn(), Tone::Warn)
            } else {
                (ui::mark_fail(), Tone::Fail)
            };
            text.push_str(&format!(
                "\n  {} {} {}",
                ui::paint(tone, mark),
                ui::pad_right(name, 28),
                ui::paint(Tone::Dim, message)
            ));
            if !ok {
                text.push_str(&format!("\n      {}", doctor_hint(name)));
            }
        }
    }
    text
}
fn status_code(status: RunStatus) -> i32 {
    match status {
        RunStatus::Completed | RunStatus::WaitingPrimaryArbiter => 0,
        RunStatus::Cancelled => 4,
        RunStatus::FailedPolicy => 3,
        RunStatus::Failed | RunStatus::Degraded => 1,
        _ => 0,
    }
}
fn map_error(error: anyhow::Error) -> QuinteError {
    let message = error.to_string();
    if message.contains("policy")
        || message.contains("changed since run creation")
        || message.contains("primary-arbiter response does not bind")
        || message.contains("challenge was already consumed")
        || message.contains("challenge expired")
        || message.contains("not waiting for Primary Arbiter")
        || message.contains("response already exists")
    {
        QuinteError::Policy(message)
    } else if message.contains("not initialized")
        || message.contains("preflight")
        || message.contains("path does not exist")
        || message.contains("brief")
    {
        QuinteError::Usage(message)
    } else {
        QuinteError::Internal(error)
    }
}
#[cfg(test)]
mod tests {
    use super::{Cli, Command};
    use clap::Parser;
    #[test]
    fn credential_helper_rejects_a_token_argument() {
        let parsed = Cli::try_parse_from([
            "quinte",
            "__credential-helper",
            "--service",
            "xiaomi-mimo-token-plan-api-key",
            "--lane-root",
            "/lane",
            "--authorization",
            "/lane/config/credential-helper-authorization.json",
            "--token",
            "secret",
        ]);
        assert!(parsed.is_err());
        let parsed = Cli::try_parse_from([
            "quinte",
            "__credential-helper",
            "--service",
            "xiaomi-mimo-token-plan-api-key",
            "--lane-root",
            "/lane",
            "--authorization",
            "/lane/config/credential-helper-authorization.json",
        ])
        .unwrap();
        assert!(matches!(parsed.command, Command::CredentialHelper(_)));
    }
    // ---- A 阶段：人类输出样式 ----
    fn manifest_fixture(status: crate::model::RunStatus) -> crate::model::RunManifest {
        serde_json::from_value(serde_json::json!({
            "manifest_version": "1",
            "run_id": "019abc fixture-run",
            "created_at": "2026-07-19T01:02:03Z",
            "updated_at": "2026-07-19T02:03:04Z",
            "status": status,
            "brief_sha256": "b",
            "policy_sha256": "p",
            "snapshot_sha256": "s",
            "runtime_sha256": "r",
            "protocol_version": "1",
            "effective_model": "fixture",
            "sandbox_mode": "process",
            "current_phase": null,
            "error": null,
            "r3_input_receipt": null,
            "primary_arbiter_challenge": null,
            "primary_arbiter_submission": null,
            "result_sha256": null
        }))
        .unwrap()
    }
    #[test]
    fn status_line_unified_format() {
        crate::ui::force_no_color();
        let line = super::format_status("run-1", crate::model::RunStatus::Completed);
        assert!(line.starts_with('●'), "应以状态点开头：{line}");
        assert!(line.contains("Completed"));
        assert!(line.contains("run-1"));
    }
    #[test]
    fn status_table_empty_guides_and_rows_format() {
        crate::ui::force_no_color();
        let empty = super::human_status_table(&[]);
        assert!(empty.contains("暂无 run"));
        assert!(empty.contains("quinte run --brief"));
        let rows = super::human_status_table(&[
            manifest_fixture(crate::model::RunStatus::Completed),
            manifest_fixture(crate::model::RunStatus::WaitingPrimaryArbiter),
        ]);
        assert!(rows.contains("2 run(s)"));
        assert!(rows.contains("● Completed"));
        assert!(rows.contains("● WaitingPrimaryArbiter"));
        assert!(rows.contains("019abc fixture-run"));
        assert!(rows.contains("2026-07-19"));
    }
    #[test]
    fn inspect_summary_verdict_findings_and_historical() {
        crate::ui::force_no_color();
        let manifest = manifest_fixture(crate::model::RunStatus::Completed);
        let result = serde_json::json!({
            "verdict": "APPROVE",
            "findings": [
                {"severity": "HIGH"},
                {"severity": "medium"},
                {"severity": "MEDIUM"}
            ]
        });
        let text = super::human_inspect(&manifest, Some(&result), true, None);
        assert!(text.contains("● Completed"));
        assert!(text.contains("historical_read_only"));
        assert!(text.contains("APPROVE"));
        assert!(text.contains("HIGH ×1"), "{text}");
        assert!(text.contains("MEDIUM ×2"), "{text}");
    }
    #[test]
    fn inspect_without_result_keeps_status_only() {
        crate::ui::force_no_color();
        let manifest = manifest_fixture(crate::model::RunStatus::R1Running);
        let text = super::human_inspect(&manifest, None, false, None);
        assert!(text.contains("R1Running"));
        assert!(!text.contains("裁决"));
    }
}
