//! PI — minimal A2A v1.0 review seat agent.
//!
//! `pi serve` listens on a loopback port, answers the A2A agent-card
//! endpoint, and accepts JSON-RPC `SendMessage` / `GetTask` calls. Each
//! task is executed in one background thread: the message parts are folded
//! into the seat prompt, one OpenAI-compatible completion is requested,
//! and the model's reply must parse as the seat's contract artifact.

mod card;
mod contract;
mod http;
mod prompt;
mod provider;
mod rpc;
mod task;

use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{Context, Result};
use clap::Parser;

use crate::task::TaskStore;

/// One seat: a role (school + phase), a provider binding, and a port.
#[derive(Parser, Clone)]
#[command(name = "pi", about = "Minimal A2A v1.0 review seat agent")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Parser, Clone)]
enum Command {
    /// Start the A2A server.
    Serve(ServeArgs),
}

#[derive(Parser, Clone)]
struct ServeArgs {
    /// Seat role: the review school (a–e) and phase (r1, r2, r3-arbiter).
    #[arg(long, env = "PI_ROLE")]
    role: String,

    /// Listen address (loopback default; PI never listens on a public
    /// interface unless the operator explicitly asks).
    #[arg(long, default_value = "127.0.0.1:8900")]
    addr: String,

    /// State directory for task records (default: ~/.pi).
    #[arg(long, env = "PI_HOME")]
    home: Option<String>,

    /// Model name (default: deepseek-v4-pro).
    #[arg(long, env = "PI_MODEL", default_value = "deepseek-v4-pro")]
    model: String,

    /// Environment variable holding the provider API key.
    #[arg(long, env = "PI_KEY_ENV", default_value = "DEEPSEEK_API_KEY")]
    key_env: String,

    /// Provider base URL (OpenAI-compatible).
    #[arg(long, env = "PI_BASE_URL", default_value = "https://api.deepseek.com/v1")]
    base_url: String,

    /// Directory holding the contract schemas (default: the schemas/
    /// directory shipped with this source tree).
    #[arg(long, env = "PI_SCHEMAS_DIR")]
    schemas_dir: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve(args) => serve(args),
    }
}

fn serve(args: ServeArgs) -> Result<()> {
    // reqwest is built without a bundled crypto provider (rustls-no-provider);
    // the process must install one exactly once before the first request.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let mut seat = prompt::seat(&args.role).with_context(|| {
        format!(
            "unknown seat role '{}'; expected e.g. a-r1, c-r2, r3-arbiter",
            args.role
        )
    })?;
    let schemas_dir = args
        .schemas_dir
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas")
        });
    let schema_name = match seat.phase {
        prompt::Phase::R3Arbiter => "arbiter-verdict.schema.json",
        _ => "lane-output.schema.json",
    };
    let mut schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schemas_dir.join(schema_name))?)?;
    if let Some(object) = schema.as_object_mut() {
        object.remove("$schema");
        object.remove("$id");
        object.remove("title");
    }
    seat.schema = serde_json::to_string(&schema)?;
    let provider = provider::Provider::new(&args.key_env, &args.base_url, &args.model)?;
    let home = match args.home {
        Some(h) => std::path::PathBuf::from(h),
        None => std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(".pi"),
    };
    std::fs::create_dir_all(&home)?;
    let store = Arc::new(Mutex::new(TaskStore::open(&home.join("tasks.jsonl"))?));
    let listener = TcpListener::bind(&args.addr)
        .with_context(|| format!("cannot bind {}", args.addr))?;
    eprintln!(
        "pi seat '{}' (model {}) listening on http://{}/",
        args.role, args.model, args.addr
    );
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("accept: {e}");
                continue;
            }
        };
        let store = Arc::clone(&store);
        let seat = seat.clone();
        let provider = provider.clone();
        thread::spawn(move || {
            if let Err(e) = http::handle(stream, &store, &seat, &provider) {
                eprintln!("connection: {e:#}");
            }
        });
    }
    Ok(())
}
