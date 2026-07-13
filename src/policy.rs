use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};

use crate::model::{
    MULTIMODAL_MODEL, POLICY_VERSION, Policy, RoutePolicy, SandboxMode, TEXT_MODEL,
};
use crate::util::{read_json, write_json};

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
        auditor: route("Auditor B", "cc", "claude", "claude"),
        text_model: TEXT_MODEL.to_string(),
        multimodal_model: MULTIMODAL_MODEL.to_string(),
        max_parallel_r1: 5,
        max_parallel_r2: 1,
        max_attempts: 3,
        timeout_seconds: 600,
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
    let policy: Policy = read_json(path)?;
    validate(&policy)?;
    Ok(policy)
}

pub fn load_for_runtime(path: &Path) -> anyhow::Result<Policy> {
    let policy: Policy = read_json(path)?;
    validate_for_runtime(&policy)?;
    Ok(policy)
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
    let expected = ["Party A", "Party B", "Party C", "Party D", "Party E"];
    for (index, route) in policy.roster.iter().enumerate() {
        if route.party_id != expected[index] || !route.required {
            bail!("roster must bind required Party A through Party E in order");
        }
        if route.executable.trim().is_empty() || route.adapter.trim().is_empty() {
            bail!("{} has an empty executable or adapter", route.party_id);
        }
        let expected_adapters = ["codewhale", "opencode", "kilo", "mimo", "omp"];
        if route.adapter != expected_adapters[index] && !(allow_fake && route.adapter == "fake") {
            bail!(
                "{} must use the fixed {} adapter",
                route.party_id,
                expected_adapters[index]
            );
        }
    }
    if policy.auditor.party_id != "Auditor B" || !policy.auditor.required {
        bail!("policy must bind required Auditor B");
    }
    if policy.auditor.adapter != "claude"
        && !(allow_fake && matches!(policy.auditor.adapter.as_str(), "fake" | "fake_arbiter"))
    {
        bail!("Auditor B must use the fixed claude adapter");
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

pub fn initialize(home: &Path, force: bool) -> anyhow::Result<PathBuf> {
    fs::create_dir_all(home).with_context(|| format!("cannot create {}", home.display()))?;
    let policy_path = home.join("policy.json");
    if policy_path.exists() && !force {
        bail!(
            "{} already exists; use --force to replace it",
            policy_path.display()
        );
    }
    write_json(&policy_path, &default_policy())?;
    fs::create_dir_all(home.join("runs"))?;
    Ok(policy_path)
}
