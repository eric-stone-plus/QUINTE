use crate::doctor;
use crate::error::{QuinteError, Result};
use crate::host;
use crate::model::{ArbiterVerdict, Brief, CliEnvelope, Policy, RunManifest, RunStatus};
use crate::policy;
use crate::run::{self, RunOptions};
use crate::store::Store;
use crate::ui::{self, BoardModel, Tone};
use crate::util::{read_json, user_home};
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
    /// Validate and deterministically finalize a Protocol 2.0 finance bundle.
    #[command(name = "finance-finalize")]
    FinanceFinalize(FinanceFinalizeArgs),
    /// Offline verification of a terminal Protocol 2.0 finance bundle.
    #[command(name = "finance-verify")]
    FinanceVerify(FinanceVerifyArgs),
    #[command(name = "finance-init")]
    FinanceInit(FinanceInitArgs),
    #[command(name = "finance-advance", alias = "finance-resume")]
    FinanceAdvance(FinanceAdvanceArgs),
    Wait(IdArgs),
    Resume(IdArgs),
    Cancel(IdArgs),
    Inspect(IdArgs),
    /// Stable detached machine boundary for external orchestrators.
    Host(HostArgs),
    #[command(name = "primary-arbiter")]
    PrimaryArbiter(PrimaryArbiterArgs),
    Agents(AgentArgs),
    Policy(PolicyArgs),
    /// Brief wizard and validation
    Brief(BriefArgs),
    /// Validate the syntax and schema of a brief/verdict file (read-only, changes no state)
    Validate(ValidateArgs),
    /// Print a shell completion script
    Completions(CompletionsArgs),
    #[command(name = "__worker", hide = true)]
    Worker(WorkerArgs),
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
pub(crate) struct FinanceFinalizeArgs {
    /// Directory containing policy/profile/invocation/evidence and R1/R2 outputs.
    #[arg(long, value_name = "DIR")]
    input: PathBuf,
    /// New or existing output directory for result, Manifest 3.0, and HIGHBALL carriers.
    #[arg(long, value_name = "DIR")]
    output: PathBuf,
    #[arg(long, value_name = "ACK")]
    enable_dormant_finance_writer: String,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct FinanceVerifyArgs {
    #[arg(long, value_name = "DIR")]
    bundle: PathBuf,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct FinanceInitArgs {
    #[arg(long, value_name = "DIR")]
    source: PathBuf,
    #[arg(long, value_name = "DIR")]
    state: PathBuf,
    #[arg(long, value_name = "ACK")]
    enable_dormant_finance_writer: String,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct FinanceAdvanceArgs {
    #[arg(long, value_name = "DIR")]
    state: PathBuf,
    #[arg(long, value_name = "ACK")]
    enable_dormant_finance_writer: String,
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
pub(crate) struct HostArgs {
    #[command(subcommand)]
    command: HostCommand,
}
#[derive(Debug, Subcommand)]
pub(crate) enum HostCommand {
    /// Validate runtime readiness and report active runs without launching.
    Preflight(JsonArgs),
    /// Atomically enforce one-active and start a detached run.
    Start(HostStartArgs),
    /// Return a one-shot machine receipt for a run.
    Status(IdArgs),
    /// Verify terminal result integrity when available.
    Inspect(IdArgs),
    /// Recover launch identity after an ambiguous host-side interruption.
    Reconcile(HostReconcileArgs),
    /// Serve the A2A v1.0 JSON-RPC front door (HOST.md) over this CLI host.
    Serve(HostServeArgs),
}
#[derive(Debug, Args)]
pub(crate) struct HostServeArgs {
    /// Listen address. Loopback-only unless `--token-env` is set.
    #[arg(long, default_value = "127.0.0.1:8801")]
    bind: String,
    /// Environment variable holding the bearer token.
    #[arg(long)]
    token_env: Option<String>,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct HostStartArgs {
    #[arg(long, value_name = "FILE")]
    brief: PathBuf,
    #[arg(long)]
    json: bool,
}
#[derive(Debug, Args)]
pub(crate) struct HostReconcileArgs {
    run_id: Option<String>,
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
    /// Rewrite a completed run's result.json with a replacement verdict (does not re-run any party)
    Amend(PrimaryArbiterAmendArgs),
}
#[derive(Debug, Args)]
pub(crate) struct PrimaryArbiterAmendArgs {
    run_id: String,
    #[arg(long, value_name = "FILE")]
    verdict: PathBuf,
    /// Waive the degenerate-verdict guardrail (schema validation is not waived)
    #[arg(long)]
    force: bool,
    #[arg(long)]
    json: bool,
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
    /// Waive the degenerate-verdict guardrail (--verdict path only; schema validation is not waived)
    #[arg(long)]
    force: bool,
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
pub(crate) struct BriefArgs {
    #[command(subcommand)]
    command: BriefCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum BriefCommand {
    /// Interactive tty wizard to author a brief; --print-template prints a template (for scripts/heredocs)
    New(BriefNewArgs),
    /// Validate a brief file against the contract
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
pub(crate) struct ValidateArgs {
    /// Validation target kind: brief (proposal) / verdict (primary-arbiter ruling)
    #[arg(long, value_enum)]
    kind: ValidateKind,
    /// JSON file to validate
    file: PathBuf,
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum ValidateKind {
    Brief,
    Verdict,
}

impl ValidateKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Brief => "brief",
            Self::Verdict => "verdict",
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct CompletionsArgs {
    /// bash / zsh / fish
    shell: String,
}
pub fn entrypoint() -> Result<i32> {
    // Bare `quinte` (no args) with stdout a tty → interactive REPL;
    // non-tty (pipe/script) keeps the original arg_required_else_help
    // behavior with byte-identical output.
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

/// Command execution body (shared with the REPL; command is clap-parsed,
/// guaranteeing an identical path to the CLI).
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
            let policy = policy::load(&store.policy_path())?;
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
        Command::FinanceFinalize(args) => {
            let finalized = crate::finance::finalize_bundle_with_ack(
                &args.input,
                &args.output,
                &args.enable_dormant_finance_writer,
            )?;
            emit(
                args.json,
                json!({
                    "result": finalized.result_path,
                    "manifest": finalized.manifest_path,
                    "highball_route_request": finalized.highball_route_request_path,
                    "highball_residual_trace": finalized.highball_residual_trace_path,
                    "publication_posture": finalized.publication_posture,
                }),
                format!(
                    "Finance review finalized: {:?}; {}",
                    finalized.publication_posture,
                    finalized.result_path.display()
                ),
            )?;
            Ok(0)
        }
        Command::FinanceVerify(args) => {
            let verified = crate::finance::verify_bundle(&args.bundle)?;
            emit(
                args.json,
                &verified,
                format!(
                    "Finance bundle {} verified: {:?}",
                    verified.run_id, verified.publication_posture
                ),
            )?;
            Ok(0)
        }
        Command::FinanceInit(args) => {
            let manifest = crate::finance::dormant_init(
                &args.source,
                &args.state,
                &args.enable_dormant_finance_writer,
            )?;
            emit(
                args.json,
                &manifest,
                format!("Dormant finance run {} initialized", manifest.run_id),
            )?;
            Ok(0)
        }
        Command::FinanceAdvance(args) => {
            let manifest =
                crate::finance::dormant_advance(&args.state, &args.enable_dormant_finance_writer)?;
            emit(
                args.json,
                &manifest,
                format!(
                    "Dormant finance run {}: {:?}",
                    manifest.run_id, manifest.status
                ),
            )?;
            Ok(0)
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
        Command::Host(args) => match args.command {
            HostCommand::Preflight(args) => {
                ensure_initialized(store)?;
                let observed = host::preflight(store)?;
                let code = observed.receipt["state"]["code"]
                    .as_str()
                    .unwrap_or("unknown");
                emit(
                    args.json,
                    &observed.receipt,
                    format!(
                        "QUINTE host preflight: {code}; receipt {}",
                        observed.receipt_path.display()
                    ),
                )?;
                Ok(if code == "ready" { 0 } else { 2 })
            }
            HostCommand::Start(args) => {
                ensure_initialized(store)?;
                let started = host::start(store, &args.brief)?;
                let run_id = started.receipt["run_id"].as_str().unwrap_or("unknown");
                emit(
                    args.json,
                    &started.receipt,
                    format!(
                        "QUINTE host started {run_id}; receipt {}",
                        started.receipt_path.display()
                    ),
                )?;
                Ok(0)
            }
            HostCommand::Status(args) => {
                ensure_initialized(store)?;
                let observed = host::status(store, &args.run_id)?;
                let status = store.load_manifest(&args.run_id)?.status;
                emit(
                    args.json,
                    &observed.receipt,
                    format!(
                        "{}; receipt {}",
                        format_status(&args.run_id, status),
                        observed.receipt_path.display()
                    ),
                )?;
                Ok(0)
            }
            HostCommand::Inspect(args) => {
                ensure_initialized(store)?;
                let observed = host::inspect(store, &args.run_id)?;
                let status = store.load_manifest(&args.run_id)?.status;
                emit(
                    args.json,
                    &observed.receipt,
                    format!(
                        "QUINTE host inspection: {}; receipt {}",
                        format_status(&args.run_id, status),
                        observed.receipt_path.display()
                    ),
                )?;
                Ok(status_code(status))
            }
            HostCommand::Reconcile(args) => {
                ensure_initialized(store)?;
                let observed = host::reconcile(store, args.run_id.as_deref())?;
                let code = observed.receipt["state"]["code"]
                    .as_str()
                    .unwrap_or("unknown");
                emit(
                    args.json,
                    &observed.receipt,
                    format!(
                        "QUINTE host reconcile: {code}; receipt {}",
                        observed.receipt_path.display()
                    ),
                )?;
                Ok(if code == "ambiguous_active_runs" {
                    2
                } else {
                    0
                })
            }
            HostCommand::Serve(args) => {
                ensure_initialized(store)?;
                let token = match args.token_env.as_deref() {
                    Some(name) => match std::env::var(name) {
                        Ok(value) if !value.is_empty() => Some(value),
                        Ok(_) => bail!("token env var '{name}' is set but empty"),
                        Err(_) => bail!("token env var '{name}' is not set"),
                    },
                    None => None,
                };
                let server = crate::a2a::A2aServer::start(
                    Store::new(store.home().to_path_buf()),
                    crate::a2a::ServeOptions {
                        bind: args.bind,
                        token,
                    },
                )?;
                let payload = json!({
                    "endpoint": server.endpoint,
                    "card_url": server.card_url,
                });
                emit(
                    args.json,
                    &payload,
                    format!(
                        "QUINTE A2A listening on {} (card {})",
                        server.endpoint, server.card_url
                    ),
                )?;
                let _ = std::io::stdout().flush();
                let stop = server.stop_flag();
                let _ = ctrlc::set_handler(move || {
                    stop.store(true, std::sync::atomic::Ordering::SeqCst);
                });
                server.join();
                Ok(0)
            }
        },
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
                    run::submit_primary_arbiter_verdict(store, &args.run_id, &verdict, args.force)?
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
            PrimaryArbiterCommand::Amend(args) => {
                let status = run::amend_primary_arbiter_verdict(
                    store,
                    &args.run_id,
                    &args.verdict,
                    args.force,
                )?;
                emit(
                    args.json,
                    json!({"run_id": args.run_id, "status": status}),
                    format_status(&args.run_id, status),
                )?;
                Ok(status_code(status))
            }
        },
        Command::Agents(args) => {
            let policy = policy::load(&store.policy_path())?;
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
            let policy = policy::load(&store.policy_path())?;
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
        Command::Validate(args) => {
            // Shares the same read_json path as submit/run: syntax errors and
            // schema mismatches surface here as two-tier field-level messages,
            // exit code 0/1.
            let result = match args.kind {
                ValidateKind::Brief => read_json::<Brief>(&args.file).map(|_| ()),
                ValidateKind::Verdict => read_json::<ArbiterVerdict>(&args.file).map(|_| ()),
            };
            let kind = args.kind.as_str();
            match result {
                Ok(()) => {
                    emit(
                        args.json,
                        json!({"kind": kind, "file": args.file, "valid": true}),
                        format!(
                            "{} {} is a valid {kind}",
                            ui::paint(Tone::Ok, ui::mark_ok()),
                            args.file.display()
                        ),
                    )?;
                    Ok(0)
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    emit(
                        args.json,
                        json!({"kind": kind, "file": args.file, "valid": false, "error": message}),
                        format!(
                            "{} {} is not a valid {kind}: {message}",
                            ui::paint(Tone::Fail, ui::mark_fail()),
                            args.file.display()
                        ),
                    )?;
                    Ok(1)
                }
            }
        }
        Command::Completions(args) => match crate::completions::render(&args.shell) {
            Some(script) => {
                eprintln!("{}", crate::completions::install_hint(&args.shell));
                print!("{script}");
                use std::io::Write;
                std::io::stdout().flush()?;
                Ok(0)
            }
            None => bail!(
                "unsupported shell: {} (supported: bash/zsh/fish)",
                args.shell
            ),
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
// Live progress board (side-channel display; zero change to wait semantics)
// ---------------------------------------------------------------------------

/// Wrap run::wait: only when stdout is a tty, not --json, and color is not
/// degraded, spawn a side-channel display thread polling manifest +
/// events.jsonl to draw the progress board; Ctrl+C/timeout/state-advance
/// semantics are decided entirely by run::wait.
/// No join (a full tty buffer would block writes): after the stop+ack
/// handshake the main thread freezes the final frame.
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
    // Wait for the display thread to leave its main loop (max 1s; giving up on
    // a pathologically slow tty is not fatal)
    for _ in 0..20 {
        if ack.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    // Freeze the final frame (drawn by the main thread to avoid interleaving
    // with the display thread)
    if let Some(frame) = board_frame(store, run_id, &parties, 0, terminal_size().0) {
        redraw(&frame, &printed);
    }
    drop(handle);
    result
}

/// Read the current manifest + events and build one frame (on read failure
/// keep the previous frame).
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
        // ~500ms polling, checking the stop flag in slices
        for _ in 0..10 {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    ack.store(true, Ordering::SeqCst);
}

/// Move the cursor up `printed` lines and rewrite line by line (no scrolling).
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

/// Terminal size (stty size; falls back to 80x24).
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
// Human output (additive UX layer; the --json path is unaffected)
// ---------------------------------------------------------------------------
fn status_tone(status: RunStatus) -> Tone {
    match status {
        RunStatus::Completed => Tone::Ok,
        RunStatus::Degraded => Tone::Warn,
        RunStatus::Failed | RunStatus::FailedPolicy | RunStatus::Cancelled => Tone::Fail,
        RunStatus::WaitingPrimaryArbiter => Tone::Gold,
        RunStatus::Queued | RunStatus::Preflight => Tone::Dim,
        _ => Tone::Run, // R1/R2/R3/Merging and other running states
    }
}
/// Unified status line: ● <Status>  <run_id> (colored, degrades to plain text).
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
    let mut out = format!(
        "{} {}\n",
        ui::paint_bold(Tone::Gold, "QUINTE · LUPA"),
        ui::paint(Tone::Dim, "the five-seat lupa is ready")
    );
    out.push_str(&format!(
        "{} Initialized QUINTE at {}\n",
        ui::paint(Tone::Ok, ui::mark_ok()),
        home.display()
    ));
    out.push_str(&format!(
        "{}\n",
        ui::paint(Tone::Dim, &format!("policy: {}", policy_path.display()))
    ));
    out.push_str(&format!("{}\n", ui::paint_bold(Tone::Gold, "Next steps")));
    out.push_str("  1. quinte doctor           # check agents / credentials / platform\n");
    out.push_str("  2. write brief.json        # the proposal\n");
    out.push_str("  3. quinte run --brief <file> [--wait]");
    out
}
fn human_status_table(manifests: &[RunManifest]) -> String {
    if manifests.is_empty() {
        return format!(
            "no runs yet · start your first deliberation with {}",
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
                "historical_read_only · read-only historical result (no further action)"
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
                    ui::paint_bold(Tone::Gold, "Verdict"),
                    ui::truncate(&text, 200)
                ));
                break;
            }
        }
        // Severity counts: prefer findings, fall back to protocol residuals
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
                    ui::paint_bold(Tone::Gold, "Findings"),
                    parts.join(" · ")
                ));
            }
        }
    }
    if let Some(path) = report_path {
        out.push_str(&format!(
            "\n{}",
            ui::paint(Tone::Dim, &format!("full report: {}", path.display()))
        ));
    }
    out
}
fn doctor_hint(name: &str) -> String {
    let hint = match name {
        "os_sandbox" => {
            "process-level isolation is not a kernel sandbox; evaluate against your threat model before relying on strict mode"
        }
        "strict_sandbox_policy" => {
            "no kernel sandbox backend available, strict is fail-closed; use process mode instead"
        }
        "git" => "install git to enable snapshot provenance (optional)",
        "process_group_supervision" => {
            "process-group supervision unsupported on this platform; lane exits may leave child processes behind"
        }
        "silent_child_launch" => "silent child-process launch unavailable",
        _ => "run quinte doctor --json for details of this check",
    };
    ui::paint(Tone::Dim, &format!("hint: {hint}"))
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
    let groups = ["agents", "platform"];
    for group in groups {
        let checks: Vec<&Value> = report
            .checks
            .iter()
            .filter(|check| match group {
                "agents" => check.get("party_id").is_some(),
                _ => check.get("party_id").is_none(),
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
    // Classification looks only at the outermost context (matching historical
    // behavior), but the persisted message carries the full error chain so the
    // Usage/Policy variants don't swallow root causes like serde's.
    let outer = error.to_string();
    let message = format!("{error:#}");
    if outer.contains("policy")
        || outer.contains("changed since run creation")
        || outer.contains("primary-arbiter response does not bind")
        || outer.contains("challenge was already consumed")
        || outer.contains("challenge expired")
        || outer.contains("not waiting for Primary Arbiter")
        || outer.contains("response already exists")
    {
        QuinteError::Policy(message)
    } else if outer.contains("not initialized")
        || outer.contains("preflight")
        || outer.contains("path does not exist")
        || outer.contains("brief")
    {
        QuinteError::Usage(message)
    } else {
        QuinteError::Internal(error)
    }
}
#[cfg(test)]
mod tests {
    // ---- Phase A: human-output styling ----
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
        assert!(
            line.starts_with('●'),
            "must start with the status dot: {line}"
        );
        assert!(line.contains("Completed"));
        assert!(line.contains("run-1"));
    }
    #[test]
    fn status_table_empty_guides_and_rows_format() {
        crate::ui::force_no_color();
        let empty = super::human_status_table(&[]);
        assert!(empty.contains("no runs yet"));
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
        assert!(!text.contains("Verdict"));
    }
}
