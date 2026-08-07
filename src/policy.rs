use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Deserializer};

use crate::model::{
    MULTIMODAL_MODEL, POLICY_VERSION, Policy, RoutePolicy, SandboxMode, SeatBinding, TEXT_MODEL,
};
use crate::util::{create_private_dir_all, read_json, write_json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompatiblePolicy {
    policy_version: String,
    #[serde(default)]
    seat: Option<SeatBinding>,
    roster: Vec<RoutePolicy>,
    #[serde(default, deserialize_with = "deserialize_present_route")]
    counterpart_arbiter: Option<RoutePolicy>,
    #[serde(default, deserialize_with = "deserialize_present_route")]
    auditor: Option<RoutePolicy>,
    #[serde(default)]
    primary_arbiter: Option<RoutePolicy>,
    #[serde(default)]
    auto_primary_arbiter: bool,
    text_model: String,
    multimodal_model: String,
    max_parallel_r1: usize,
    max_parallel_r2: usize,
    #[serde(default)]
    r2_parallel: bool,
    max_attempts: usize,
    timeout_seconds: u64,
    #[serde(default)]
    r1_timeout_seconds: Option<u64>,
    #[serde(default)]
    r2_timeout_seconds: Option<u64>,
    #[serde(default)]
    r3_timeout_seconds: Option<u64>,
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
    let seat = SeatBinding {
        seat_id: "seat-mimo".into(),
        family: "mimo".into(),
        provider: "xiaomi".into(),
        text_model: TEXT_MODEL.into(),
        multimodal_model: MULTIMODAL_MODEL.into(),
    };
    let perspectives = [
        "Formal specification, invariants, and boundary-condition audit.",
        "Failure-mode, counterexample, and adversarial assumption search.",
        "Evidence provenance, uncertainty, and claim-support audit.",
        "Operational implementation, observability, recovery, and rollback audit.",
        "Independent synthesis emphasizing omissions and decision boundaries.",
    ];
    Policy {
        legacy_v1_source: false,
        policy_version: POLICY_VERSION.to_string(),
        seat: seat.clone(),
        roster: ["A", "B", "C", "D", "E"]
            .into_iter()
            .zip(perspectives)
            .map(|(letter, perspective)| {
                production_route(
                    &format!("Party {letter}"),
                    &format!("mimo-{}", letter.to_ascii_lowercase()),
                    perspective,
                    &seat,
                )
            })
            .collect(),
        counterpart_arbiter: production_route(
            "Counterpart Arbiter",
            "mimo-counterpart",
            "Steel-man the strongest lane case, then identify unresolved contradictions.",
            &seat,
        ),
        primary_arbiter: production_route(
            "Primary Arbiter",
            "mimo-primary",
            "Issue the final same-family verdict while preserving evidence and dissent.",
            &seat,
        ),
        auto_primary_arbiter: true,
        text_model: TEXT_MODEL.to_string(),
        multimodal_model: MULTIMODAL_MODEL.to_string(),
        max_parallel_r1: 5,
        max_parallel_r2: 1,
        // Serial R2 with pacing remains the default; r2_parallel is opt-in.
        r2_parallel: false,
        max_attempts: 3,
        // Hang recovery: real R1 lanes typically finish in 1–4 min. 300s fails
        // stuck adapters faster without starving healthy long reviews; R2 stays
        // serial with fixed 10s pacing so this does not increase 429 pressure.
        timeout_seconds: 300,
        // Per-phase timeout overrides: None means use the global timeout_seconds.
        // R2/R3 reviews analyze existing typed outputs and may complete faster
        // than R1 first-pass reviews. Set to Some(value) to override per-phase.
        r1_timeout_seconds: None,
        r2_timeout_seconds: None,
        r3_timeout_seconds: None,
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

fn production_route(
    party_id: &str,
    route_id: &str,
    perspective: &str,
    seat: &SeatBinding,
) -> RoutePolicy {
    RoutePolicy {
        party_id: party_id.into(),
        route_id: route_id.into(),
        adapter: "mimo".into(),
        executable: "mimo".into(),
        required: true,
        family: seat.family.clone(),
        provider: seat.provider.clone(),
        text_model: seat.text_model.clone(),
        multimodal_model: seat.multimodal_model.clone(),
        perspective: perspective.into(),
    }
}

fn route(party_id: &str, route_id: &str, adapter: &str, executable: &str) -> RoutePolicy {
    RoutePolicy {
        party_id: party_id.to_string(),
        route_id: route_id.to_string(),
        adapter: adapter.to_string(),
        executable: executable.to_string(),
        required: true,
        family: "mimo".into(),
        provider: "xiaomi-token-plan-cn".into(),
        text_model: TEXT_MODEL.into(),
        multimodal_model: MULTIMODAL_MODEL.into(),
        perspective: String::new(),
    }
}

fn legacy_seat_binding() -> SeatBinding {
    SeatBinding {
        seat_id: "legacy-mimo".into(),
        family: "mimo".into(),
        provider: "xiaomi-token-plan-cn".into(),
        text_model: TEXT_MODEL.into(),
        multimodal_model: MULTIMODAL_MODEL.into(),
    }
}

pub fn load(path: &Path) -> anyhow::Result<Policy> {
    let policy = read_compatible(path)?;
    validate(&policy)?;
    Ok(policy)
}

pub fn load_for_runtime(path: &Path) -> anyhow::Result<Policy> {
    let policy = read_compatible(path)?;
    if policy.legacy_v1_source {
        bail!(
            "policy v1 is read-only compatible and cannot start a new run; back up policy.json, then run `quinte init --force` to install the production v2 policy"
        );
    }
    if !policy.auto_primary_arbiter {
        bail!("production policy v2 requires auto_primary_arbiter=true");
    }
    validate_for_runtime(&policy)?;
    Ok(policy)
}

fn read_compatible(path: &Path) -> anyhow::Result<Policy> {
    let raw: serde_json::Value = read_json(path)?;
    let legacy = raw
        .get("policy_version")
        .and_then(serde_json::Value::as_str)
        == Some("1.0");
    let mut compatible: CompatiblePolicy = serde_json::from_value(if legacy {
        normalize_v1_policy(raw)?
    } else {
        raw
    })
    .with_context(|| format!("invalid JSON in {}", path.display()))?;
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
        legacy_v1_source: legacy,
        policy_version: compatible.policy_version,
        seat: compatible.seat.take().expect("normalized policy has seat"),
        roster: compatible.roster,
        counterpart_arbiter,
        primary_arbiter: compatible
            .primary_arbiter
            .expect("normalized policy has primary arbiter"),
        auto_primary_arbiter: compatible.auto_primary_arbiter,
        text_model: compatible.text_model,
        multimodal_model: compatible.multimodal_model,
        max_parallel_r1: compatible.max_parallel_r1,
        max_parallel_r2: compatible.max_parallel_r2,
        r2_parallel: compatible.r2_parallel,
        max_attempts: compatible.max_attempts,
        timeout_seconds: compatible.timeout_seconds,
        r1_timeout_seconds: compatible.r1_timeout_seconds,
        r2_timeout_seconds: compatible.r2_timeout_seconds,
        r3_timeout_seconds: compatible.r3_timeout_seconds,
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

fn normalize_v1_policy(mut raw: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let object = raw
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("policy must be a JSON object"))?;
    let seat = legacy_seat_binding();
    object.insert("policy_version".into(), serde_json::json!(POLICY_VERSION));
    object.insert("seat".into(), serde_json::to_value(&seat)?);
    for name in ["roster", "counterpart_arbiter", "auditor"] {
        if let Some(routes) = object.get_mut(name) {
            if let Some(array) = routes.as_array_mut() {
                for route in array {
                    add_binding(route, &seat)?;
                }
            } else if routes.is_object() {
                add_binding(routes, &seat)?;
            }
        }
    }
    object.insert(
        "primary_arbiter".into(),
        serde_json::to_value(route("Primary Arbiter", "pa", "omp", "omp"))?,
    );
    object.insert("auto_primary_arbiter".into(), serde_json::json!(false));
    Ok(raw)
}

fn add_binding(value: &mut serde_json::Value, seat: &SeatBinding) -> anyhow::Result<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("route must be a JSON object"))?;
    object.insert("family".into(), serde_json::json!(seat.family));
    object.insert("provider".into(), serde_json::json!(seat.provider));
    object.insert("text_model".into(), serde_json::json!(seat.text_model));
    object.insert(
        "multimodal_model".into(),
        serde_json::json!(seat.multimodal_model),
    );
    Ok(())
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
    validate_seat_binding(&policy.seat)?;
    let expected_parties = ["Party A", "Party B", "Party C", "Party D", "Party E"];
    let mut route_ids = BTreeSet::new();
    for (index, route) in policy.roster.iter().enumerate() {
        let party_id = expected_parties[index];
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
                "fake" | "fake_mimo" | "fake_envelope" | "fake_codewhale"
            );
        validate_route_binding(route, &policy.seat, fake, policy.legacy_v1_source)?;
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
            "fake" | "fake_mimo" | "fake_envelope" | "fake_arbiter"
        );
    validate_route_binding(
        &policy.counterpart_arbiter,
        &policy.seat,
        fake_arbiter,
        policy.legacy_v1_source,
    )?;
    if fake_arbiter {
        validate_fake_executable(&policy.counterpart_arbiter.executable)?;
    }
    if policy.primary_arbiter.party_id != "Primary Arbiter" || !policy.primary_arbiter.required {
        bail!("policy must bind required Primary Arbiter");
    }
    validate_route_id(&policy.primary_arbiter.route_id)?;
    if !route_ids.insert(policy.primary_arbiter.route_id.as_str()) {
        bail!("route_id values must be globally unique");
    }
    let fake_primary = allow_fake
        && matches!(
            policy.primary_arbiter.adapter.as_str(),
            "fake" | "fake_mimo" | "fake_envelope" | "fake_arbiter"
        );
    validate_route_binding(
        &policy.primary_arbiter,
        &policy.seat,
        fake_primary,
        policy.legacy_v1_source,
    )?;
    if fake_primary {
        validate_fake_executable(&policy.primary_arbiter.executable)?;
    }
    if policy.text_model != policy.seat.text_model
        || policy.multimodal_model != policy.seat.multimodal_model
    {
        bail!("policy model aliases must match the seat binding");
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
    // Validate per-phase timeout overrides
    for (name, value) in [
        ("r1_timeout_seconds", policy.r1_timeout_seconds),
        ("r2_timeout_seconds", policy.r2_timeout_seconds),
        ("r3_timeout_seconds", policy.r3_timeout_seconds),
    ] {
        if let Some(t) = value {
            if t < 5 || t > 3600 {
                bail!("{name} must be between 5 and 3600 when set");
            }
        }
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

fn validate_seat_binding(seat: &SeatBinding) -> anyhow::Result<()> {
    for (name, value) in [
        ("seat_id", &seat.seat_id),
        ("family", &seat.family),
        ("provider", &seat.provider),
        ("text_model", &seat.text_model),
        ("multimodal_model", &seat.multimodal_model),
    ] {
        if !valid_binding_identifier(value) {
            bail!("seat {name} must be a safe non-empty identifier up to 128 characters");
        }
    }
    Ok(())
}

fn validate_route_binding(
    route: &RoutePolicy,
    seat: &SeatBinding,
    fake: bool,
    legacy_v1_source: bool,
) -> anyhow::Result<()> {
    if route.executable.trim().is_empty() || route.adapter.trim().is_empty() {
        bail!("{} has an empty adapter or executable", route.party_id);
    }
    if !fake
        && !legacy_v1_source
        && !matches!(route.adapter.as_str(), "mimo" | "reasonix" | "codex")
    {
        bail!(
            "{} uses unsupported adapter {}",
            route.party_id,
            route.adapter
        );
    }
    for (name, value) in [
        ("family", &route.family),
        ("provider", &route.provider),
        ("text_model", &route.text_model),
        ("multimodal_model", &route.multimodal_model),
    ] {
        if !valid_binding_identifier(value) {
            bail!(
                "{} {name} must be a safe non-empty identifier up to 128 characters",
                route.party_id
            );
        }
    }
    if route.family != seat.family
        || route.provider != seat.provider
        || route.text_model != seat.text_model
        || route.multimodal_model != seat.multimodal_model
    {
        bail!(
            "{} violates the single-family seat invariant",
            route.party_id
        );
    }
    if !fake {
        validate_adapter_capability(route, seat, legacy_v1_source)?;
    }
    Ok(())
}

fn validate_adapter_capability(
    route: &RoutePolicy,
    seat: &SeatBinding,
    legacy_v1_source: bool,
) -> anyhow::Result<()> {
    if legacy_v1_source {
        if seat.family != "mimo" || seat.provider != "xiaomi-token-plan-cn" {
            bail!("legacy seat binding must remain MiMo token-plan");
        }
        if !matches!(
            route.adapter.as_str(),
            "codewhale" | "opencode" | "kilo" | "mimo" | "omp" | "claude"
        ) {
            bail!(
                "{} uses an adapter incompatible with legacy MiMo",
                route.party_id
            );
        }
        return Ok(());
    }

    let (expected_provider, expected_adapter) = match seat.family.as_str() {
        // MiMo Code accepts complete config/auth documents through environment
        // variables, so it can run without copying host-managed state.
        "mimo" => ("xiaomi", "mimo"),
        "deepseek" => ("deepseek", "reasonix"),
        // Codex uses the Responses API. The seat relay must implement it; a
        // placeholder endpoint is rejected separately by provider binding.
        "openai" => ("openai-api", "codex"),
        other => bail!("unsupported production seat family {other}"),
    };
    if seat.provider != expected_provider {
        bail!(
            "production seat family {} requires provider {expected_provider}",
            seat.family
        );
    }
    if route.adapter != expected_adapter {
        bail!(
            "{} must use the isolated {expected_adapter} adapter for production family {}; {} is not a proven stateless binding for that family",
            route.party_id,
            seat.family,
            route.adapter
        );
    }
    Ok(())
}

fn valid_binding_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
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
