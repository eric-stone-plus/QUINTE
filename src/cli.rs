use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, bail};
use clap::{Args, Parser, Subcommand, error::ErrorKind};
use serde::Serialize;
use serde_json::{Value, json};

use crate::doctor;
use crate::error::{QuinteError, Result};
use crate::model::{CliEnvelope, Policy, RunStatus};
use crate::policy;
use crate::run::{self, RunOptions};
use crate::store::Store;
use crate::util::{read_json, user_home};

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
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init(InitArgs),
    Status(StatusArgs),
    Doctor(JsonArgs),
    Run(RunArgs),
    Wait(IdArgs),
    Resume(IdArgs),
    Cancel(IdArgs),
    Inspect(IdArgs),
    #[command(name = "primary-arbiter", alias = "hm")]
    PrimaryArbiter(PrimaryArbiterArgs),
    Agents(AgentArgs),
    Policy(PolicyArgs),
    Credential(CredentialArgs),
    #[command(name = "__worker", hide = true)]
    Worker(WorkerArgs),
    /// Internal Claude Code apiKeyHelper entrypoint. Not a user command.
    #[command(name = "__credential-helper", hide = true)]
    CredentialHelper(CredentialHelperArgs),
}

#[derive(Debug, Args)]
struct InitArgs {
    #[arg(long)]
    force: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct JsonArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct StatusArgs {
    run_id: Option<String>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct RunArgs {
    #[arg(long, value_name = "FILE")]
    brief: PathBuf,
    #[arg(long)]
    wait: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct WorkerArgs {
    run_id: String,
}

#[derive(Debug, Args)]
struct IdArgs {
    run_id: String,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct PrimaryArbiterArgs {
    #[command(subcommand)]
    command: PrimaryArbiterCommand,
}

#[derive(Debug, Subcommand)]
enum PrimaryArbiterCommand {
    Request(IdArgs),
    Submit(PrimaryArbiterSubmitArgs),
}

#[derive(Debug, Args)]
struct PrimaryArbiterSubmitArgs {
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
struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommand,
}

#[derive(Debug, Subcommand)]
enum AgentCommand {
    List(JsonArgs),
    Describe {
        id: String,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
struct PolicyArgs {
    #[command(subcommand)]
    command: PolicyCommand,
}

#[derive(Debug, Subcommand)]
enum PolicyCommand {
    Show(JsonArgs),
    Validate(JsonArgs),
}

#[derive(Debug, Args)]
struct CredentialArgs {
    #[command(subcommand)]
    command: CredentialCommand,
}

#[derive(Debug, Subcommand)]
enum CredentialCommand {
    /// Store the Claude/MiMo token in the platform protected store.
    Set(CredentialSetArgs),
    /// Report whether the Claude credential is available and isolated.
    Status(JsonArgs),
}

#[derive(Debug, Args)]
struct CredentialSetArgs {
    #[arg(long, default_value = "xiaomi-mimo-token-plan-api-key")]
    service: String,
    /// Read the secret from a file instead of the terminal (file is not logged).
    #[arg(long, value_name = "FILE")]
    from_file: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct CredentialHelperArgs {
    #[arg(long, default_value = "xiaomi-mimo-token-plan-api-key")]
    service: String,
}

pub fn entrypoint() -> Result<i32> {
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

fn execute(cli: Cli) -> anyhow::Result<i32> {
    let home = match cli.home {
        Some(path) => path,
        None => user_home()?.join(".quinte"),
    };
    let store = Store::new(home.clone());
    match cli.command {
        Command::Init(args) => {
            let path = policy::initialize(&home, args.force)?;
            emit(
                args.json,
                json!({"policy": path, "home": home}),
                format!("Initialized QUINTE at {}", home.display()),
            )?;
            Ok(0)
        }
        Command::Status(args) => {
            ensure_initialized(&store)?;
            if let Some(run_id) = args.run_id {
                let manifest = store.load_manifest(&run_id)?;
                emit(
                    args.json,
                    &manifest,
                    format_status(&manifest.run_id, manifest.status),
                )?;
            } else {
                let manifests = store.list_manifests()?;
                emit(
                    args.json,
                    &manifests,
                    format!("QUINTE: {} run(s)", manifests.len()),
                )?;
            }
            Ok(0)
        }
        Command::Doctor(args) => {
            ensure_initialized(&store)?;
            let policy = load_policy(&store)?;
            let report = doctor::run(&policy);
            let ok = report.ok;
            emit(args.json, &report, human_doctor(&report))?;
            Ok(if ok { 0 } else { 2 })
        }
        Command::Run(args) => {
            ensure_initialized(&store)?;
            let policy = load_policy(&store)?;
            let created = run::create(
                &store,
                &policy,
                &RunOptions {
                    brief_path: args.brief,
                },
            )?;
            let worker_pid = match run::spawn_worker(&store, &created.run_id) {
                Ok(pid) => pid,
                Err(error) => {
                    let _ = run::record_worker_failure(
                        &store,
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
                match run::wait(&store, &created.run_id, Duration::from_millis(500)) {
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
            ensure_initialized(&store)?;
            let status = run::advance(&store, &args.run_id)?;
            emit(
                args.json,
                json!({"run_id": args.run_id, "status": status}),
                format_status(&args.run_id, status),
            )?;
            Ok(status_code(status))
        }
        Command::Wait(args) => {
            ensure_initialized(&store)?;
            let status = match run::wait(&store, &args.run_id, Duration::from_millis(500)) {
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
            ensure_initialized(&store)?;
            let status = run::cancel(&store, &args.run_id)?;
            emit(
                args.json,
                json!({"run_id": args.run_id, "status": status}),
                format_status(&args.run_id, status),
            )?;
            Ok(0)
        }
        Command::Inspect(args) => {
            ensure_initialized(&store)?;
            let manifest = store.load_manifest(&args.run_id)?;
            run::verify_result_integrity(&store, &args.run_id)?;
            let result_path = store.run_dir(&args.run_id).join("result.json");
            let result = if matches!(manifest.status, RunStatus::Completed | RunStatus::Degraded)
                && result_path.exists()
            {
                Some(read_json::<Value>(&result_path)?)
            } else {
                None
            };
            let events = store.events(&args.run_id)?;
            emit(
                args.json,
                json!({"manifest": manifest, "result": result, "events": events}),
                format_status(&args.run_id, manifest.status),
            )?;
            Ok(status_code(manifest.status))
        }
        Command::PrimaryArbiter(args) => match args.command {
            PrimaryArbiterCommand::Request(args) => {
                let path = store
                    .run_dir(&args.run_id)
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
                    run::submit_primary_arbiter_verdict(&store, &args.run_id, &verdict)?
                } else {
                    run::submit_primary_arbiter(
                        &store,
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
            let policy = load_policy(&store)?;
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
            let policy = load_policy(&store)?;
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
            ensure_initialized(&store)?;
            let _heartbeat = run::WorkerHeartbeat::start(&store, &args.run_id);
            match run::advance(&store, &args.run_id) {
                Ok(status) => Ok(status_code(status)),
                Err(error) => {
                    let message = format!("background scheduler failed: {error:#}");
                    if !error.to_string().contains("already being advanced") {
                        let _ = run::record_worker_failure(&store, &args.run_id, &message);
                    }
                    Err(error.context(message))
                }
            }
        }
        Command::Credential(args) => match args.command {
            CredentialCommand::Set(args) => {
                let secret = if let Some(path) = args.from_file {
                    std::fs::read_to_string(&path)
                        .with_context(|| format!("cannot read {}", path.display()))?
                } else {
                    eprint!("Enter credential for service {} (input hidden not guaranteed on all terminals): ", args.service);
                    let mut line = String::new();
                    std::io::stdin()
                        .read_line(&mut line)
                        .context("failed to read credential from stdin")?;
                    line
                };
                crate::credential::set(&args.service, &secret)?;
                emit(
                    args.json,
                    json!({
                        "service": args.service,
                        "stored": true,
                        "source": if cfg!(target_os = "macos") {
                            "keychain"
                        } else if cfg!(windows) {
                            "windows_credential_manager"
                        } else {
                            "unsupported"
                        }
                    }),
                    format!(
                        "Stored credential for service {} in the platform protected store",
                        args.service
                    ),
                )?;
                Ok(0)
            }
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
            // Fail closed unless the adapter marked this process as a lane helper.
            let allowed = std::env::var_os("QUINTE_CREDENTIAL_HELPER_ALLOWED").as_deref()
                == Some(std::ffi::OsStr::new("1"));
            let lane_root = std::env::var_os("QUINTE_LANE_ROOT");
            if !allowed || lane_root.as_ref().is_none_or(|value| value.is_empty()) {
                bail!(
                    "credential helper is only callable from a QUINTE lane context \
                     (missing QUINTE_CREDENTIAL_HELPER_ALLOWED / QUINTE_LANE_ROOT)"
                );
            }
            let secret = crate::credential::get(&args.service)?;
            print!("{secret}");
            Ok(0)
        }
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

fn human_doctor(report: &doctor::DoctorReport) -> String {
    let status = if report.ok { "PASS" } else { "FAIL" };
    let mut text = format!("QUINTE doctor: {status} ({})", report.platform);
    for check in &report.checks {
        let ok = check.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let name = check
            .get("party_id")
            .or_else(|| check.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("check");
        let message = check.get("message").and_then(Value::as_str).unwrap_or("");
        text.push_str(&format!(
            "\n{} {name}: {message}",
            if ok { "PASS" } else { "WARN" }
        ));
    }
    text
}

fn format_status(run_id: &str, status: RunStatus) -> String {
    format!("{run_id}: {status:?}")
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
