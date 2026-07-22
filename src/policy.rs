use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Deserializer};

use crate::model::{
    MULTIMODAL_MODEL, POLICY_VERSION, Policy, RoutePolicy, SandboxMode, TEXT_MODEL,
};
use crate::util::{create_private_dir_all, read_json, write_json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompatiblePolicy {
    policy_version: String,
    roster: Vec<RoutePolicy>,
    #[serde(default, deserialize_with = "deserialize_present_route")]
    counterpart_arbiter: Option<RoutePolicy>,
    #[serde(default, deserialize_with = "deserialize_present_route")]
    auditor: Option<RoutePolicy>,
    text_model: String,
    multimodal_model: String,
    max_parallel_r1: usize,
    max_parallel_r2: usize,
    max_attempts: usize,
    timeout_seconds: u64,
    retry_backoff_seconds: u64,
    retry_backoff_max_seconds: u64,
    r2_min_interval_seconds: u64,
    max_output_bytes: usize,
    max_snapshot_files: usize,
    max_snapshot_bytes: u64,
    max_attachment_bytes: u64,
    sandbox_mode: SandboxMode,
}

fn deserialize_present_route<'de, D>(deserializer: D) -> Result<Option<RoutePolicy>, D::Error>
where
    D: Deserializer<'de>,
{
    RoutePolicy::deserialize(deserializer).map(Some)
}

pub fn default_policy() -> Policy {
    Policy {
        policy_version: POLICY_VERSION.to_string(),
        roster: vec![
            route("Party A", "codewhale", "codewhale", "codewhale"),
            route("Party B", "opencode", "opencode", "opencode"),
            route("Party C", "kilo", "kilo", "kilo"),
            route("Party D", "mimo", "mimo", "mimo"),
            route("Party E", "omp", "omp", "omp"),
        ],
        counterpart_arbiter: route("Counterpart Arbiter", "cc", "claude", "claude"),
        text_model: TEXT_MODEL.to_string(),
        multimodal_model: MULTIMODAL_MODEL.to_string(),
        max_parallel_r1: 5,
        max_parallel_r2: 1,
        max_attempts: 3,
        // Hang recovery: real R1 lanes typically finish in 1–4 min. 300s fails
        // stuck adapters faster without starving healthy long reviews; R2 stays
        // serial with fixed 10s pacing so this does not increase 429 pressure.
        timeout_seconds: 300,
        retry_backoff_seconds: 15,
        retry_backoff_max_seconds: 120,
        r2_min_interval_seconds: 10,
        max_output_bytes: 1_048_576,
        max_snapshot_files: 2_000,
        max_snapshot_bytes: 20 * 1024 * 1024,
        max_attachment_bytes: 10 * 1024 * 1024,
        sandbox_mode: SandboxMode::Process,
    }
}

fn route(party_id: &str, route_id: &str, adapter: &str, executable: &str) -> RoutePolicy {
    RoutePolicy {
        party_id: party_id.to_string(),
        route_id: route_id.to_string(),
        adapter: adapter.to_string(),
        executable: executable.to_string(),
        required: true,
    }
}

pub fn load(path: &Path) -> anyhow::Result<Policy> {
    let policy = read_compatible(path)?;
    validate(&policy)?;
    Ok(policy)
}

pub fn load_for_runtime(path: &Path) -> anyhow::Result<Policy> {
    let policy = read_compatible(path)?;
    validate_for_runtime(&policy)?;
    Ok(policy)
}

fn read_compatible(path: &Path) -> anyhow::Result<Policy> {
    let compatible: CompatiblePolicy = read_json(path)?;
    let counterpart_arbiter = match (compatible.counterpart_arbiter, compatible.auditor) {
        (Some(route), None) => route,
        (None, Some(mut route)) => {
            if route.party_id != "Auditor B" {
                bail!("policy must bind required Counterpart Arbiter");
            }
            route.party_id = "Counterpart Arbiter".into();
            route
        }
        (Some(_), Some(_)) => {
            return Err(anyhow::anyhow!("duplicate field `counterpart_arbiter`")
                .context(format!("invalid JSON in {}", path.display())));
        }
        (None, None) => {
            return Err(anyhow::anyhow!("missing field `counterpart_arbiter`")
                .context(format!("invalid JSON in {}", path.display())));
        }
    };
    Ok(Policy {
        policy_version: compatible.policy_version,
        roster: compatible.roster,
        counterpart_arbiter,
        text_model: compatible.text_model,
        multimodal_model: compatible.multimodal_model,
        max_parallel_r1: compatible.max_parallel_r1,
        max_parallel_r2: compatible.max_parallel_r2,
        max_attempts: compatible.max_attempts,
        timeout_seconds: compatible.timeout_seconds,
        retry_backoff_seconds: compatible.retry_backoff_seconds,
        retry_backoff_max_seconds: compatible.retry_backoff_max_seconds,
        r2_min_interval_seconds: compatible.r2_min_interval_seconds,
        max_output_bytes: compatible.max_output_bytes,
        max_snapshot_files: compatible.max_snapshot_files,
        max_snapshot_bytes: compatible.max_snapshot_bytes,
        max_attachment_bytes: compatible.max_attachment_bytes,
        sandbox_mode: compatible.sandbox_mode,
    })
}

pub fn validate(policy: &Policy) -> anyhow::Result<()> {
    validate_with_options(policy, false)
}

pub fn validate_for_runtime(policy: &Policy) -> anyhow::Result<()> {
    #[cfg(feature = "test-adapters")]
    let allow_fake = std::env::var_os("QUINTE_ALLOW_FAKE_ADAPTERS").is_some();
    #[cfg(not(feature = "test-adapters"))]
    let allow_fake = false;
    validate_with_options(policy, allow_fake)
}

fn validate_with_options(policy: &Policy, allow_fake: bool) -> anyhow::Result<()> {
    if policy.policy_version != POLICY_VERSION {
        bail!("policy_version must be {POLICY_VERSION}");
    }
    if policy.roster.len() != 5 {
        bail!("QUINTE policy must contain exactly five R1/R2 parties");
    }
    let expected = [
        ("Party A", "codewhale", "codewhale", "codewhale"),
        ("Party B", "opencode", "opencode", "opencode"),
        ("Party C", "kilo", "kilo", "kilo"),
        ("Party D", "mimo", "mimo", "mimo"),
        ("Party E", "omp", "omp", "omp"),
    ];
    let mut route_ids = BTreeSet::new();
    for (index, route) in policy.roster.iter().enumerate() {
        let (party_id, route_id, adapter, executable) = expected[index];
        if route.party_id != party_id || !route.required {
            bail!("roster must bind required Party A through Party E in order");
        }
        validate_route_id(&route.route_id)?;
        if !route_ids.insert(route.route_id.as_str()) {
            bail!("route_id values must be globally unique");
        }
        let fake = allow_fake
            && matches!(
                route.adapter.as_str(),
                "fake" | "fake_mimo" | "fake_codewhale"
            );
        if !fake
            && (route.route_id != route_id
                || route.adapter != adapter
                || route.executable != executable)
        {
            bail!(
                "{} must use fixed route/adapter/executable tuple ({route_id}, {adapter}, {executable})",
                route.party_id
            );
        }
        if fake && route.executable.trim().is_empty() {
            bail!("{} has an empty test executable", route.party_id);
        }
        if fake {
            validate_fake_executable(&route.executable)?;
        }
    }
    if policy.counterpart_arbiter.party_id != "Counterpart Arbiter"
        || !policy.counterpart_arbiter.required
    {
        bail!("policy must bind required Counterpart Arbiter");
    }
    validate_route_id(&policy.counterpart_arbiter.route_id)?;
    if !route_ids.insert(policy.counterpart_arbiter.route_id.as_str()) {
        bail!("route_id values must be globally unique");
    }
    let fake_arbiter = allow_fake
        && matches!(
            policy.counterpart_arbiter.adapter.as_str(),
            "fake" | "fake_arbiter"
        );
    if !fake_arbiter
        && (policy.counterpart_arbiter.route_id != "cc"
            || policy.counterpart_arbiter.adapter != "claude"
            || policy.counterpart_arbiter.executable != "claude")
    {
        bail!(
            "Counterpart Arbiter must use fixed route/adapter/executable tuple (cc, claude, claude)"
        );
    }
    if fake_arbiter && policy.counterpart_arbiter.executable.trim().is_empty() {
        bail!("Counterpart Arbiter has an empty test executable");
    }
    if fake_arbiter {
        validate_fake_executable(&policy.counterpart_arbiter.executable)?;
    }
    if policy.text_model != TEXT_MODEL || policy.multimodal_model != MULTIMODAL_MODEL {
        bail!("model routing is fixed to {TEXT_MODEL} and {MULTIMODAL_MODEL}");
    }
    if policy.max_parallel_r1 != 5 || policy.max_parallel_r2 != 1 {
        bail!("phase concurrency is fixed to R1=5 and R2=1");
    }
    if allow_fake {
        if policy.max_attempts == 0 || policy.max_attempts > 3 {
            bail!("max_attempts must be between 1 and 3");
        }
    } else if policy.max_attempts != 3 {
        bail!("max_attempts is fixed to 3");
    }
    if policy.timeout_seconds < 5 || policy.timeout_seconds > 3600 {
        bail!("timeout_seconds must be between 5 and 3600");
    }
    if allow_fake {
        if policy.retry_backoff_seconds > 300 {
            bail!("retry_backoff_seconds must be at most 300");
        }
        if policy.retry_backoff_max_seconds < policy.retry_backoff_seconds
            || policy.retry_backoff_max_seconds > 900
        {
            bail!("retry_backoff_max_seconds must be at least the base backoff and at most 900");
        }
        if policy.r2_min_interval_seconds > 120 {
            bail!("r2_min_interval_seconds must be at most 120");
        }
    } else if policy.retry_backoff_seconds != 15
        || policy.retry_backoff_max_seconds != 120
        || policy.r2_min_interval_seconds != 10
    {
        bail!("R2 rate-limit controls are fixed to base=15s, cap=120s, and pacing=10s");
    }
    if !(4 * 1024..=16 * 1024 * 1024).contains(&policy.max_output_bytes) {
        bail!("max_output_bytes must be between 4096 and 16777216");
    }
    Ok(())
}

fn validate_fake_executable(executable: &str) -> anyhow::Result<()> {
    let path = Path::new(executable);
    if path
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        bail!("test executable must not contain parent traversal");
    }
    let resolved = crate::util::resolve_command(executable)
        .ok_or_else(|| anyhow::anyhow!("test executable is not a resolvable regular executable"))?;
    if !resolved.program.is_file() || !resolved.source.is_file() {
        bail!("test executable must resolve to regular files");
    }
    Ok(())
}

fn validate_route_id(route_id: &str) -> anyhow::Result<()> {
    let valid = !route_id.is_empty()
        && route_id.len() <= 64
        && route_id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
        && route_id
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric);
    if !valid {
        bail!(
            "route_id {route_id:?} must be 1-64 lowercase ASCII letters, digits, '-' or '_', and start with a letter or digit"
        );
    }
    Ok(())
}

pub fn initialize(home: &Path, force: bool) -> anyhow::Result<PathBuf> {
    create_private_dir_all(home).with_context(|| format!("cannot create {}", home.display()))?;
    let policy_path = home.join("policy.json");
    if policy_path.exists() && !force {
        bail!(
            "{} already exists; use --force to replace it",
            policy_path.display()
        );
    }
    write_json(&policy_path, &default_policy())?;
    create_private_dir_all(&home.join("runs"))?;
    Ok(policy_path)
}
