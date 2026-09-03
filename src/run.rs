use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};
use base64::Engine;
use chrono::{Duration as ChronoDuration, Utc};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::adapters::{self, Invocation};
use crate::contract::{
    BRIEF_VERSION, EVIDENCE_PACKET_VERSION, PRIMARY_ARBITER_CHALLENGE_VERSION,
    PRIMARY_ARBITER_RESPONSE_VERSION, PRIMARY_ARBITER_SUBMISSION_VERSION, PROTOCOL_VERSION,
    R2_PACKET_VERSION, R3_INPUT_RECEIPT_VERSION, RATE_STATE_VERSION, RETRY_STATE_VERSION,
    RUN_MANIFEST_VERSION, SNAPSHOT_VERSION, TASK_PACKET_VERSION, TRIAL_MANIFEST_VERSION,
    brief_version_supported, contract,
};
use crate::model::{
    ArbiterVerdict, ArtifactBinding, AttachmentEntry, Brief, ClosureState, Disposition,
    ObservedContestation,
    LaneArtifactBinding, LaneOutput, Policy, PrimaryArbiterChallenge, PrimaryArbiterResponse,
    PrimaryArbiterSubmissionReceipt, PrimaryArbiterSubmissionState, R2Packet, R3InputReceipt,
    RESULT_VERSION, Residual, ResultEnvelope, RouteBinding, RunError, RunManifest, RunStatus,
    Severity, SnapshotEntry, SnapshotManifest, TrialManifest, TrialPerspective,
};
use crate::policy;
use crate::schema::{
    LANE_OUTPUT_SCHEMA, LEGACY_HM_RESPONSE_SCHEMA, PRIMARY_ARBITER_RESPONSE_SCHEMA,
    R3_INPUT_RECEIPT_SCHEMA, RESULT_SCHEMA, validate_file, validate_value,
};
use crate::store::{ActiveProcess, Store};
#[cfg(windows)]
use crate::util::configure_hidden_process;
use crate::util::{
    atomic_write, canonical_existing, create_private_dir_all, filesystem_path, harden_private_file,
    read_json, relative_slash, sha256_bytes, sha256_file, utc_now, write_json,
};

static INTERRUPTED: OnceLock<Arc<AtomicBool>> = OnceLock::new();
static RUNTIME_SHA256: OnceLock<anyhow::Result<String, String>> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct RunOptions {
    pub brief_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct RunCreated {
    pub run_id: String,
    pub status: RunStatus,
    pub run_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
#[error("wait interrupted")]
pub struct WaitInterrupted;

#[derive(Clone, Debug)]
struct LaneAccepted {
    party_id: String,
    route_id: String,
    output: LaneOutput,
    artifact_ref: String,
}

struct LaneJob {
    route: crate::model::RoutePolicy,
    attempt: usize,
    handle: thread::JoinHandle<anyhow::Result<AttemptOutcome>>,
}

struct ChildCleanup<'a> {
    child: Child,
    store: &'a Store,
    run_id: &'a str,
    registered: Option<ActiveProcess>,
    tree_cleaned: bool,
    /// Windows Job Object that owns the adapter process tree (kill-on-close).
    #[cfg(windows)]
    job: Option<WindowsJob>,
}

impl<'a> ChildCleanup<'a> {
    fn new(child: Child, store: &'a Store, run_id: &'a str) -> Self {
        Self {
            child,
            store,
            run_id,
            registered: None,
            tree_cleaned: false,
            #[cfg(windows)]
            job: None,
        }
    }

    #[cfg(windows)]
    fn attach_job(&mut self, job: WindowsJob) {
        self.job = Some(job);
    }

    fn mark_registered(&mut self, process: ActiveProcess) {
        self.registered = Some(process);
    }

    fn unregister(&mut self) -> anyhow::Result<()> {
        if let Some(process) = self.registered.as_ref() {
            self.store.remove_active_process(self.run_id, process)?;
            self.registered = None;
        }
        Ok(())
    }
}

impl Drop for ChildCleanup<'_> {
    fn drop(&mut self) {
        if !self.tree_cleaned {
            #[cfg(windows)]
            let job = self.job.as_ref();
            #[cfg(not(windows))]
            let job = Option::<&()>::None;
            let _ = terminate_child(&mut self.child, Duration::from_millis(500), job);
        }
        #[cfg(windows)]
        {
            // Closing the job with KILL_ON_JOB_CLOSE reaps any remaining descendants.
            self.job.take();
        }
        if let Some(process) = self.registered.take() {
            let _ = self.store.remove_active_process(self.run_id, &process);
        }
    }
}

/// Owned Windows Job Object used to contain an adapter process tree.
#[cfg(windows)]
struct WindowsJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl WindowsJob {
    fn create() -> anyhow::Result<Self> {
        use std::mem::zeroed;
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
        use windows_sys::Win32::System::JobObjects::{
            CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        };

        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            bail!("CreateJobObjectW failed: {}", unsafe { GetLastError() });
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                std::mem::size_of_val(&info) as u32,
            )
        };
        if ok == 0 {
            let error = unsafe { GetLastError() };
            unsafe {
                CloseHandle(handle);
            }
            bail!("SetInformationJobObject KILL_ON_JOB_CLOSE failed: {error}");
        }
        Ok(Self { handle })
    }

    fn assign_process_handle(
        &self,
        process: windows_sys::Win32::Foundation::HANDLE,
    ) -> anyhow::Result<()> {
        use windows_sys::Win32::Foundation::GetLastError;
        use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

        let ok = unsafe { AssignProcessToJobObject(self.handle, process) };
        if ok == 0 {
            bail!("AssignProcessToJobObject failed: {}", unsafe {
                GetLastError()
            });
        }
        Ok(())
    }

    fn terminate(&self) {
        use windows_sys::Win32::System::JobObjects::TerminateJobObject;
        let _ = unsafe { TerminateJobObject(self.handle, 1) };
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE terminates remaining members.
        unsafe {
            CloseHandle(self.handle);
        }
        self.handle = std::ptr::null_mut();
    }
}

/// Spawn an adapter with platform containment. On Windows the process is created
/// suspended, assigned to a kill-on-close Job Object, then resumed so children
/// cannot escape before assignment.
fn spawn_adapter_process(
    command: &mut std::process::Command,
) -> anyhow::Result<(Child, Option<WindowsJobPlaceholder>)> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{
            CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        };

        let job = WindowsJob::create()?;
        // Combine hidden + suspended + new process group. CREATE_SUSPENDED closes
        // the race between spawn and AssignProcessToJobObject.
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED | CREATE_NEW_PROCESS_GROUP);
        let mut child = command.spawn().context("cannot start adapter process")?;
        let process = child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
        if let Err(error) = job.assign_process_handle(process) {
            // Ensure a failed assignment cannot leave a suspended orphan.
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        if let Err(error) = resume_suspended_process(child.id()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok((child, Some(job)))
    }
    #[cfg(not(windows))]
    {
        let child = command.spawn().context("cannot start adapter process")?;
        Ok((child, None))
    }
}

/// Placeholder type so non-Windows builds can keep a uniform Option without
/// referencing WindowsJob.
#[cfg(not(windows))]
type WindowsJobPlaceholder = ();
#[cfg(windows)]
type WindowsJobPlaceholder = WindowsJob;

#[cfg(windows)]
fn resume_suspended_process(pid: u32) -> anyhow::Result<()> {
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::Threading::{
        OpenThread, ResumeThread, THREAD_QUERY_INFORMATION, THREAD_SUSPEND_RESUME,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        bail!(
            "CreateToolhelp32Snapshot failed while resuming adapter: {}",
            unsafe { GetLastError() }
        );
    }
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        cntUsage: 0,
        th32ThreadID: 0,
        th32OwnerProcessID: 0,
        tpBasePri: 0,
        tpDeltaPri: 0,
        dwFlags: 0,
    };
    let mut resumed = 0_u32;
    let mut ok = unsafe { Thread32First(snapshot, &mut entry) };
    while ok != 0 {
        if entry.th32OwnerProcessID == pid {
            let thread = unsafe {
                OpenThread(
                    THREAD_SUSPEND_RESUME | THREAD_QUERY_INFORMATION,
                    0,
                    entry.th32ThreadID,
                )
            };
            if !thread.is_null() {
                let previous = unsafe { ResumeThread(thread) };
                unsafe {
                    CloseHandle(thread);
                }
                if previous != u32::MAX {
                    resumed += 1;
                }
            }
        }
        ok = unsafe { Thread32Next(snapshot, &mut entry) };
    }
    unsafe {
        CloseHandle(snapshot);
    }
    if resumed == 0 {
        bail!("failed to resume any thread for suspended adapter pid {pid}");
    }
    Ok(())
}

#[derive(Debug)]
struct AttemptOutcome {
    output: Option<adapters::AdapterOutput>,
    error: Option<String>,
    cancelled: bool,
    retry: RetryClass,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetryClass {
    Never,
    TransientTimeout,
    TransientAdapter,
    RateLimited(RateLimitSignal),
}

impl RetryClass {
    fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::TransientTimeout | Self::TransientAdapter | Self::RateLimited(_)
        )
    }

    fn failure_class(self) -> &'static str {
        match self {
            Self::Never => "non_retryable",
            Self::TransientTimeout => "timeout",
            Self::TransientAdapter => "transient_adapter",
            Self::RateLimited(_) => "rate_limit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RateLimitSignal {
    source: &'static str,
    retry_after_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct R2RateState {
    rate_state_version: String,
    next_allowed_at: String,
    reason: String,
    route_id: String,
    attempt: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RetryDeadlineState {
    retry_state_version: String,
    phase: String,
    route_id: String,
    previous_attempt: usize,
    next_attempt: usize,
    due_at: String,
    failure_class: String,
    source: String,
}

fn retry_allowed(retry: RetryClass, attempt: usize, max_attempts: usize) -> bool {
    retry.is_retryable() && attempt < max_attempts
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RetrySchedule {
    source: &'static str,
    backoff_seconds: u64,
    retry_after_seconds: Option<u64>,
    jitter: Duration,
    delay: Duration,
}

fn retry_schedule(
    retry: RetryClass,
    policy: &Policy,
    run_id: &str,
    phase: &str,
    route_id: &str,
    attempt: usize,
) -> anyhow::Result<RetrySchedule> {
    let (source, retry_after_seconds) = match retry {
        RetryClass::Never => bail!("non-retryable failure has no retry schedule"),
        RetryClass::TransientTimeout => ("host_timeout", None),
        RetryClass::TransientAdapter => ("adapter_structured_error", None),
        RetryClass::RateLimited(signal) => (signal.source, signal.retry_after_seconds),
    };
    if retry_after_seconds.is_some_and(|delay| delay > policy.retry_backoff_max_seconds) {
        bail!("provider Retry-After exceeds the bounded retry wait policy");
    }
    let exponent = u32::try_from(attempt.saturating_sub(1)).unwrap_or(u32::MAX);
    let backoff_seconds = policy
        .retry_backoff_seconds
        .saturating_mul(2_u64.saturating_pow(exponent))
        .min(policy.retry_backoff_max_seconds);
    let seed = sha256_bytes(format!("{run_id}\0{phase}\0{route_id}\0{attempt}").as_bytes());
    let jitter_basis_points = u64::from_str_radix(&seed[7..11], 16).unwrap_or(0) % 2_001;
    let jitter_millis = backoff_seconds
        .saturating_mul(1_000)
        .saturating_mul(jitter_basis_points)
        / 10_000;
    let jitter = Duration::from_millis(jitter_millis);
    let bounded_backoff = Duration::from_secs(backoff_seconds)
        .saturating_add(jitter)
        .min(Duration::from_secs(policy.retry_backoff_max_seconds));
    let delay = bounded_backoff.max(Duration::from_secs(retry_after_seconds.unwrap_or(0)));
    Ok(RetrySchedule {
        source,
        backoff_seconds,
        retry_after_seconds,
        jitter,
        delay,
    })
}

fn r2_rate_state_path(run_dir: &Path) -> PathBuf {
    run_dir.join("diagnostics/r2-rate-state.json")
}

fn retry_deadline_path(run_dir: &Path, phase: &str, route_id: &str) -> PathBuf {
    run_dir
        .join("lanes")
        .join(phase)
        .join(route_id)
        .join("retry-deadline.json")
}

fn persist_retry_deadline(
    run_dir: &Path,
    phase: &str,
    route: &crate::model::RoutePolicy,
    previous_attempt: usize,
    due_at: chrono::DateTime<Utc>,
    retry: RetryClass,
    source: &str,
) -> anyhow::Result<()> {
    let next_attempt = previous_attempt
        .checked_add(1)
        .ok_or_else(|| anyhow!("attempt history overflow for {phase}/{}", route.route_id))?;
    write_json(
        &retry_deadline_path(run_dir, phase, &route.route_id),
        &RetryDeadlineState {
            retry_state_version: RETRY_STATE_VERSION.into(),
            phase: phase.into(),
            route_id: route.route_id.clone(),
            previous_attempt,
            next_attempt,
            due_at: due_at.to_rfc3339(),
            failure_class: retry.failure_class().into(),
            source: source.into(),
        },
    )
}

fn wait_for_retry_deadline(
    store: &Store,
    manifest: &mut RunManifest,
    phase: &str,
    route: &crate::model::RoutePolicy,
    attempt: usize,
) -> anyhow::Result<()> {
    let run_dir = store.run_dir(&manifest.run_id)?;
    let path = retry_deadline_path(&run_dir, phase, &route.route_id);
    if !path.exists() {
        return Ok(());
    }
    let state: RetryDeadlineState = read_json(&path)?;
    if state.retry_state_version != RETRY_STATE_VERSION
        || state.phase != phase
        || state.route_id != route.route_id
        || state.next_attempt != attempt
        || state.previous_attempt.checked_add(1) != Some(state.next_attempt)
    {
        bail!(
            "retry deadline does not match {phase}/{} attempt {attempt}",
            route.route_id
        );
    }
    let due_at = chrono::DateTime::parse_from_rfc3339(&state.due_at)?.with_timezone(&Utc);
    let delay = (due_at - Utc::now()).to_std().unwrap_or(Duration::ZERO);
    store.event(
        &manifest.run_id,
        "lane.retry_wait",
        Some(phase),
        Some(&route.party_id),
        Some(attempt),
        json!({
            "route_id": route.route_id,
            "previous_attempt": state.previous_attempt,
            "failure_class": state.failure_class,
            "source": state.source,
            "delay_milliseconds": delay.as_millis(),
            "due_at": due_at.to_rfc3339()
        }),
    )?;
    if !delay.is_zero() && wait_cancellable(&run_dir, delay) {
        cancel_run(store, manifest)?;
        bail!("run cancelled");
    }
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn persist_r2_pacing(
    store: &Store,
    manifest: &RunManifest,
    route: &crate::model::RoutePolicy,
    attempt: usize,
    interval_seconds: u64,
    reason: &str,
) -> anyhow::Result<()> {
    persist_r2_pacing_until(
        store,
        manifest,
        route,
        attempt,
        Utc::now() + ChronoDuration::seconds(interval_seconds as i64),
        reason,
    )
}

fn persist_r2_pacing_until(
    store: &Store,
    manifest: &RunManifest,
    route: &crate::model::RoutePolicy,
    attempt: usize,
    requested_due_at: chrono::DateTime<Utc>,
    reason: &str,
) -> anyhow::Result<()> {
    let path = r2_rate_state_path(&store.run_dir(&manifest.run_id)?);
    let due_at = if path.exists() {
        let existing: R2RateState = read_json(&path)?;
        let existing_due =
            chrono::DateTime::parse_from_rfc3339(&existing.next_allowed_at)?.with_timezone(&Utc);
        requested_due_at.max(existing_due)
    } else {
        requested_due_at
    };
    write_json(
        &path,
        &R2RateState {
            rate_state_version: RATE_STATE_VERSION.into(),
            next_allowed_at: due_at.to_rfc3339(),
            reason: reason.into(),
            route_id: route.route_id.clone(),
            attempt,
        },
    )
}

/// Inter-start delay for parallel lane fan-out (R1, and R2 under the
/// `r2_parallel` policy switch). Zero under fake-adapter tests.
fn parallel_start_stagger() -> Duration {
    if std::env::var_os("QUINTE_ALLOW_FAKE_ADAPTERS").is_some() {
        return Duration::ZERO;
    }
    match std::env::var("QUINTE_R1_STAGGER_MS") {
        Ok(raw) if raw.trim() == "0" => Duration::ZERO,
        Ok(raw) => {
            let ms = raw.trim().parse::<u64>().unwrap_or(2_000).min(10_000);
            Duration::from_millis(ms)
        }
        Err(_) => Duration::from_millis(2_000),
    }
}

fn wait_for_r2_pacing(
    store: &Store,
    manifest: &mut RunManifest,
    route: &crate::model::RoutePolicy,
    attempt: usize,
) -> anyhow::Result<()> {
    let run_id = &manifest.run_id;
    let run_dir = store.run_dir(run_id)?;
    let path = r2_rate_state_path(&run_dir);
    if !path.exists() {
        return Ok(());
    }
    let state: R2RateState = read_json(&path)?;
    if state.rate_state_version != RATE_STATE_VERSION {
        bail!("unsupported R2 rate state version");
    }
    let due_at = chrono::DateTime::parse_from_rfc3339(&state.next_allowed_at)?.with_timezone(&Utc);
    let delay = (due_at - Utc::now()).to_std().unwrap_or(Duration::ZERO);
    store.event(
        run_id,
        "r2.pacing_wait",
        Some("R2"),
        Some(&route.party_id),
        Some(attempt),
        json!({
            "route_id": route.route_id,
            "previous_route_id": state.route_id,
            "previous_attempt": state.attempt,
            "reason": state.reason,
            "delay_milliseconds": delay.as_millis(),
            "due_at": due_at.to_rfc3339()
        }),
    )?;
    if !delay.is_zero() && wait_cancellable(&run_dir, delay) {
        cancel_run(store, manifest)?;
        bail!("run cancelled");
    }
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn wait_cancellable(run_dir: &Path, duration: Duration) -> bool {
    let deadline = Instant::now() + duration;
    loop {
        if cancellation_requested(run_dir) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}

pub fn create(store: &Store, policy: &Policy, options: &RunOptions) -> anyhow::Result<RunCreated> {
    let brief_contract = contract("brief").expect("brief contract is registered");
    let brief_bytes = fs::read(&options.brief_path)
        .with_context(|| format!("cannot read {}", options.brief_path.display()))?;
    create_from_brief_bytes(store, policy, &brief_bytes, brief_contract)
}

pub(crate) fn create_from_brief_bytes(
    store: &Store,
    policy: &Policy,
    brief_bytes: &[u8],
    brief_contract: &'static crate::contract::ContractSpec,
) -> anyhow::Result<RunCreated> {
    let mut brief: Brief = crate::schema::parse_versioned(brief_bytes, brief_contract)?;
    if !brief_version_supported(&brief.brief_version) || brief.question.trim().is_empty() {
        bail!("brief contract revision is unsupported or question is empty");
    }
    // New runs normalize legacy briefs to the current contract before hashing.
    brief.brief_version = BRIEF_VERSION.into();
    // A brief without a binding (CLI wizard, handwritten, legacy) gets it
    // derived at intake so brief, result, and residual trace stay bound to
    // the same route request.
    if brief.action_binding_sha256.is_none() {
        brief.action_binding_sha256 = Some(crate::highball_carriers::intake_action_binding(
            &brief.question,
            &json!(brief.affected_paths),
        ));
    }
    let snapshot_ignore = snapshot_ignore_set(&brief.snapshot_ignore)?;
    policy::validate_for_runtime(policy)?;
    if !brief.attachments.is_empty() {
        adapters::validate_attachment_capability(policy)?;
    }

    let run_id = Uuid::now_v7().to_string();
    let run_dir = store.run_dir(&run_id)?;
    // `create_run_dirs` normally cannot fail after it has created the run
    // directory, but a permissions/filesystem error in one of the nested
    // components can leave a partial tree behind.  Remember whether this
    // exact UUID path existed before we started so cleanup can never remove a
    // pre-existing run after a UUID collision.
    let existed_before = match fs::symlink_metadata(&run_dir) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if let Err(error) = store.create_run_dirs(&run_id) {
        if !existed_before {
            if let Err(cleanup) = cleanup_unpublished_run_dir(store, &run_id, &run_dir) {
                return Err(error.context(format!("run directory cleanup failed: {cleanup:#}")));
            }
        }
        return Err(error);
    }

    // Everything below is creation-time state, not a published run yet.  If
    // evidence copying, hashing, manifest validation, or event publication
    // fails, remove this exact unpublished UUID tree before returning.  This
    // prevents a normal bad Brief/evidence error from poisoning the host's
    // fail-closed run scan on the next launch.
    let creation = (|| -> anyhow::Result<RunCreated> {
        let canonical_brief = serde_json::to_vec(&brief)?;
        let canonical_policy = serde_json::to_vec(policy)?;
        let brief_sha256 = sha256_bytes(&canonical_brief);
        let policy_sha256 = sha256_bytes(&canonical_policy);
        write_json(&run_dir.join("input/brief.json"), &brief)?;
        write_json(&run_dir.join("input/policy.json"), policy)?;

        let snapshot = build_snapshot_with_ignore(&run_dir, &brief, policy, &snapshot_ignore)?;
        let snapshot_bytes = serde_json::to_vec(&snapshot)?;
        let snapshot_sha256 = sha256_bytes(&snapshot_bytes);
        write_json(&run_dir.join("input/snapshot-manifest.json"), &snapshot)?;

        let effective_model = if snapshot.attachments.is_empty() {
            policy.text_model.as_str()
        } else {
            policy.multimodal_model.as_str()
        };
        let runtime_sha256 = runtime_sha256()?;
        let now = utc_now();
        let manifest = RunManifest {
            manifest_version: RUN_MANIFEST_VERSION.into(),
            run_id: run_id.clone(),
            created_at: now.clone(),
            updated_at: now,
            status: RunStatus::Queued,
            brief_sha256,
            policy_sha256,
            snapshot_sha256,
            runtime_sha256,
            protocol_version: PROTOCOL_VERSION.into(),
            effective_model: effective_model.to_string(),
            seat_binding: policy.seat.clone(),
            route_bindings: policy_route_bindings(policy),
            sandbox_mode: policy.sandbox_mode,
            current_phase: None,
            error: None,
            r3_input_receipt: None,
            primary_arbiter_challenge: None,
            primary_arbiter_submission: None,
            result_sha256: None,
        };
        store.save_manifest(&manifest)?;
        store.event(
            &run_id,
            "run.created",
            None,
            None,
            None,
            json!({"brief_sha256": manifest.brief_sha256, "policy_sha256": manifest.policy_sha256}),
        )?;
        Ok(RunCreated {
            run_id: run_id.clone(),
            status: RunStatus::Queued,
            run_dir: run_dir.clone(),
        })
    })();

    match creation {
        Ok(created) => Ok(created),
        Err(error) => {
            if let Err(cleanup) = cleanup_unpublished_run_dir(store, &run_id, &run_dir) {
                return Err(error.context(format!("run directory cleanup failed: {cleanup:#}")));
            }
            Err(error)
        }
    }
}

/// Remove only a run tree created by the current, not-yet-published creation
/// transaction.  The path is checked both lexically and via symlink metadata;
/// in particular, a replaced run directory is never followed or recursively
/// deleted.  A worker/active-process marker means the tree has escaped the
/// creation window and must be reconciled manually instead.
fn cleanup_unpublished_run_dir(store: &Store, run_id: &str, run_dir: &Path) -> anyhow::Result<()> {
    let expected = store.run_dir(run_id)?;
    if run_dir != expected {
        bail!("refusing to clean a run directory outside its canonical path");
    }
    let metadata = match fs::symlink_metadata(run_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_dir() {
        bail!("refusing to clean a run path that is not a directory");
    }
    for relative in [
        "active-pids.json",
        "diagnostics/worker.json",
        "diagnostics/worker-heartbeat",
        "diagnostics/worker-finished",
    ] {
        let marker = run_dir.join(relative);
        match fs::symlink_metadata(&marker) {
            Ok(_) => bail!(
                "refusing to clean run {run_id}: worker state was published ({relative})"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    fs::remove_dir_all(run_dir)
        .with_context(|| format!("cannot remove unpublished run directory {}", run_dir.display()))
}

fn policy_route_bindings(policy: &Policy) -> Vec<RouteBinding> {
    policy
        .roster
        .iter()
        .chain(std::iter::once(&policy.counterpart_arbiter))
        .chain(std::iter::once(&policy.primary_arbiter))
        .map(RouteBinding::from)
        .collect()
}

/// Starts the scheduler in a separate process so the creating CLI can return immediately.
pub(crate) fn spawn_worker(store: &Store, run_id: &str) -> anyhow::Result<u32> {
    #[cfg(feature = "test-adapters")]
    if std::env::var_os("QUINTE_TEST_FAIL_WORKER_LAUNCH").is_some() {
        bail!("fault-injected worker launch failure");
    }
    let run_dir = store.run_dir(run_id)?;
    let diagnostics_dir = run_dir.join("diagnostics");
    create_private_dir_all(&diagnostics_dir)?;
    let log_path = diagnostics_dir.join("worker.log");
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("cannot open worker log {}", log_path.display()))?;
    harden_private_file(&log_path)?;
    writeln!(log, "{} worker launch requested", utc_now())?;
    let stdout = OpenOptions::new().append(true).open(&log_path)?;
    let stderr = OpenOptions::new().append(true).open(&log_path)?;
    drop(log);

    let mut worker = spawn_background_worker(store.home(), run_id, stdout, stderr)
        .with_context(|| format!("cannot start background worker for run {run_id}"))?;
    let pid = worker.pid();
    if let Err(error) = write_json(
        &diagnostics_dir.join("worker.json"),
        &json!({"pid": pid, "started_at": utc_now()}),
    ) {
        worker.terminate();
        return Err(error).context("worker started but metadata could not be persisted");
    }
    Ok(pid)
}

fn worker_heartbeat_loop(run_dir: PathBuf, stopped: Arc<AtomicBool>) {
    while !stopped.load(Ordering::SeqCst) {
        let _ = atomic_write(
            &run_dir.join("diagnostics/worker-heartbeat"),
            utc_now().as_bytes(),
        );
        for _ in 0..10 {
            if stopped.load(Ordering::SeqCst) {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

pub struct WorkerHeartbeat {
    stopped: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    run_dir: PathBuf,
}

pub struct WorkerStdioGuard;

impl WorkerHeartbeat {
    pub fn start(store: &Store, run_id: &str) -> anyhow::Result<Self> {
        let stopped = Arc::new(AtomicBool::new(false));
        let run_dir = store.run_dir(run_id)?;
        let worker_stopped = stopped.clone();
        let worker_dir = run_dir.clone();
        let handle = thread::spawn(move || worker_heartbeat_loop(worker_dir, worker_stopped));
        Ok(Self {
            stopped,
            handle: Some(handle),
            run_dir,
        })
    }
}

impl Drop for WorkerHeartbeat {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        let _ = atomic_write(
            &self.run_dir.join("diagnostics/worker-finished"),
            utc_now().as_bytes(),
        );
    }
}

#[cfg(not(windows))]
fn spawn_background_worker(
    home: &Path,
    run_id: &str,
    stdout: File,
    stderr: File,
) -> std::io::Result<BackgroundWorker> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg("--home")
        .arg(home)
        .arg("__worker")
        .arg(run_id)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    #[cfg(unix)]
    use std::os::unix::process::CommandExt;
    #[cfg(unix)]
    command.process_group(0);

    command.spawn().map(BackgroundWorker)
}

#[cfg(not(windows))]
struct BackgroundWorker(Child);

#[cfg(not(windows))]
impl BackgroundWorker {
    fn pid(&self) -> u32 {
        self.0.id()
    }

    fn terminate(&mut self) {
        kill_process_tree(self.pid(), true);
    }
}

#[cfg(windows)]
struct BackgroundWorker {
    pid: u32,
    process: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl BackgroundWorker {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn terminate(&mut self) {
        use windows_sys::Win32::System::Threading::TerminateProcess;

        kill_process_tree(self.pid(), true);
        unsafe {
            TerminateProcess(self.process, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for BackgroundWorker {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.process);
        }
    }
}

#[cfg(windows)]
fn spawn_background_worker(
    home: &Path,
    run_id: &str,
    stdout: File,
    stderr: File,
) -> std::io::Result<BackgroundWorker> {
    use std::ffi::OsStr;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, HANDLE};
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
        DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
        InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
        PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, STARTF_USESTDHANDLES,
        STARTUPINFOEXW, STARTUPINFOW, UpdateProcThreadAttribute,
    };

    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn duplicate_inheritable(raw: HANDLE) -> std::io::Result<OwnedHandle> {
        use windows_sys::Win32::Foundation::{DuplicateHandle, HANDLE};
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        let process = unsafe { GetCurrentProcess() };
        let mut duplicated: HANDLE = null_mut();
        if unsafe {
            DuplicateHandle(
                process,
                raw,
                process,
                &mut duplicated,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(OwnedHandle(duplicated))
    }

    struct AttributeList {
        storage: Vec<usize>,
    }

    impl AttributeList {
        fn for_handles(handles: &[HANDLE]) -> std::io::Result<Self> {
            let mut required_size = 0;
            unsafe {
                InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut required_size);
            }
            if required_size == 0 {
                return Err(std::io::Error::last_os_error());
            }

            let words = required_size.div_ceil(size_of::<usize>());
            let mut storage = vec![0usize; words];
            let list = storage.as_mut_ptr().cast();
            if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut required_size) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            if unsafe {
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                    handles.as_ptr().cast(),
                    std::mem::size_of_val(handles),
                    null_mut(),
                    null(),
                )
            } == 0
            {
                let error = std::io::Error::last_os_error();
                unsafe {
                    DeleteProcThreadAttributeList(list);
                }
                return Err(error);
            }
            Ok(Self { storage })
        }

        fn as_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
            self.storage.as_mut_ptr().cast()
        }
    }

    impl Drop for AttributeList {
        fn drop(&mut self) {
            unsafe {
                DeleteProcThreadAttributeList(self.as_ptr());
            }
        }
    }

    fn push_quoted(target: &mut Vec<u16>, value: &OsStr) -> std::io::Result<()> {
        let units = value.encode_wide().collect::<Vec<_>>();
        if units.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "worker argument contains a null character",
            ));
        }

        target.push(b'"' as u16);
        let mut backslashes = 0;
        for unit in units {
            if unit == b'\\' as u16 {
                backslashes += 1;
                continue;
            }
            for _ in 0..backslashes {
                target.push(b'\\' as u16);
            }
            if unit == b'"' as u16 {
                for _ in 0..=backslashes {
                    target.push(b'\\' as u16);
                }
            }
            target.push(unit);
            backslashes = 0;
        }
        for _ in 0..backslashes {
            target.extend([b'\\' as u16, b'\\' as u16]);
        }
        target.push(b'"' as u16);
        Ok(())
    }

    let executable = std::env::current_exe()?;
    let args = [
        executable.as_os_str(),
        OsStr::new("--home"),
        home.as_os_str(),
        OsStr::new("__worker"),
        OsStr::new(run_id),
    ];
    let mut command_line = Vec::new();
    for (index, arg) in args.into_iter().enumerate() {
        if index != 0 {
            command_line.push(b' ' as u16);
        }
        push_quoted(&mut command_line, arg)?;
    }
    command_line.push(0);

    let mut application = executable.as_os_str().encode_wide().collect::<Vec<_>>();
    if application.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "worker executable path contains a null character",
        ));
    }
    application.push(0);

    let stdin = OpenOptions::new().read(true).write(true).open("NUL")?;
    let stdin_handle = duplicate_inheritable(stdin.as_raw_handle() as HANDLE)?;
    let stdout_handle = duplicate_inheritable(stdout.as_raw_handle() as HANDLE)?;
    let stderr_handle = duplicate_inheritable(stderr.as_raw_handle() as HANDLE)?;
    let handles = [stdin_handle.0, stdout_handle.0, stderr_handle.0];
    let mut attribute_list = AttributeList::for_handles(&handles)?;

    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = stdin_handle.0;
    startup.StartupInfo.hStdOutput = stdout_handle.0;
    startup.StartupInfo.hStdError = stderr_handle.0;
    startup.lpAttributeList = attribute_list.as_ptr();
    let mut process = PROCESS_INFORMATION::default();
    let flags = CREATE_NEW_PROCESS_GROUP
        | CREATE_NO_WINDOW
        | CREATE_UNICODE_ENVIRONMENT
        | EXTENDED_STARTUPINFO_PRESENT;
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            flags,
            null(),
            null(),
            (&startup as *const STARTUPINFOEXW).cast::<STARTUPINFOW>(),
            &mut process,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }

    unsafe {
        CloseHandle(process.hThread);
    }
    Ok(BackgroundWorker {
        pid: process.dwProcessId,
        process: process.hProcess,
    })
}

#[cfg(windows)]
fn clear_worker_stdio_inheritance(
    handles: &[windows_sys::Win32::Foundation::HANDLE],
) -> anyhow::Result<()> {
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

    let mut cleared = Vec::new();
    for &handle in handles {
        if handle.is_null() || cleared.contains(&handle) {
            continue;
        }
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
            bail!(
                "cannot disable worker standard-handle inheritance: {}",
                std::io::Error::last_os_error()
            );
        }
        cleared.push(handle);
    }
    Ok(())
}

#[cfg(windows)]
pub fn prepare_worker_stdio() -> anyhow::Result<WorkerStdioGuard> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;

    let handles = [
        std::io::stdin().as_raw_handle() as HANDLE,
        std::io::stdout().as_raw_handle() as HANDLE,
        std::io::stderr().as_raw_handle() as HANDLE,
    ];
    clear_worker_stdio_inheritance(&handles)?;
    Ok(WorkerStdioGuard)
}

#[cfg(not(windows))]
pub fn prepare_worker_stdio() -> anyhow::Result<WorkerStdioGuard> {
    Ok(WorkerStdioGuard)
}

/// Converts an unexpected scheduler error into a durable terminal state.
pub fn record_worker_failure(
    store: &Store,
    run_id: &str,
    message: &str,
) -> anyhow::Result<RunStatus> {
    #[cfg(feature = "test-adapters")]
    if std::env::var_os("QUINTE_TEST_FAIL_WORKER_FAILURE_RECORD").is_some() {
        bail!("fault-injected worker failure record error");
    }
    let _lock = store.lock(run_id)?;
    let mut manifest = store.load_manifest(run_id)?;
    if manifest.status.terminal()
        || manifest.status == RunStatus::WaitingPrimaryArbiter
        || manifest.status == RunStatus::Cancelling
    {
        return Ok(manifest.status);
    }
    fail_run(
        store,
        &mut manifest,
        RunStatus::Failed,
        "worker_failed",
        message,
        false,
    )
}

pub fn advance(store: &Store, run_id: &str) -> anyhow::Result<RunStatus> {
    let _lock = store.lock(run_id)?;
    reconcile_orphan_processes(store, run_id)?;
    let mut manifest = store.load_manifest(run_id)?;
    if manifest.status.terminal() {
        return Ok(manifest.status);
    }
    let run_dir = store.run_dir(run_id)?;
    let brief: Brief = read_json(&run_dir.join("input/brief.json"))?;
    let policy: Policy = read_json(&run_dir.join("input/policy.json"))?;
    if let Err(error) = verify_resume_integrity(&manifest, &brief, &policy, &run_dir) {
        return fail_run(
            store,
            &mut manifest,
            RunStatus::FailedPolicy,
            "integrity_drift",
            &error.to_string(),
            false,
        );
    }

    if manifest.status == RunStatus::Queued || manifest.status == RunStatus::Preflight {
        if store.transition(&mut manifest, RunStatus::Preflight, None, json!({}))?
            == RunStatus::Cancelled
        {
            return Ok(RunStatus::Cancelled);
        }
        if policy.sandbox_mode == crate::model::SandboxMode::Strict {
            return fail_run(
                store,
                &mut manifest,
                RunStatus::FailedPolicy,
                "strict_sandbox_unavailable",
                "strict mode is unavailable without a supported kernel sandbox backend",
                false,
            );
        }
        let missing = adapters::doctor(&policy)
            .into_iter()
            .filter(|entry| !entry["ok"].as_bool().unwrap_or(false))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return fail_run(
                store,
                &mut manifest,
                RunStatus::Failed,
                "preflight_failed",
                &format!("required adapters unavailable: {missing:?}"),
                false,
            );
        }
    }

    let r1 = if phase_complete(&run_dir, "R1", &policy) {
        load_phase(&run_dir, "R1", &policy)?
    } else {
        if store.transition(&mut manifest, RunStatus::R1Running, Some("R1"), json!({}))?
            == RunStatus::Cancelled
        {
            return Ok(RunStatus::Cancelled);
        }
        match run_phase(store, &mut manifest, &policy, &brief, "R1", None) {
            Ok(outputs) => outputs,
            Err(error) => {
                return fail_run(
                    store,
                    &mut manifest,
                    RunStatus::Failed,
                    "r1_failed",
                    &error.to_string(),
                    false,
                );
            }
        }
    };
    if matches!(
        manifest.status,
        RunStatus::Queued | RunStatus::Preflight | RunStatus::R1Running
    ) && store.transition(
        &mut manifest,
        RunStatus::R1Gate,
        Some("R1"),
        json!({"accepted": 5}),
    )? == RunStatus::Cancelled
    {
        return Ok(RunStatus::Cancelled);
    }

    // D3 adaptive rounds: R2 re-examines only the outputs R1 actually
    // contested; a unanimous R1 (k=0) skips R2 entirely. The skip is
    // recorded durably so resume replays identically.
    let contested = r1_contestation(&r1);
    let skip_path = run_dir.join("r2/skipped.json");
    let r2 = if skip_path.exists() || contested.is_empty() {
        if !skip_path.exists() {
            write_json(
                &skip_path,
                &json!({
                    "predicate": "r1-contestation-v1",
                    "contested_lane_indices": contested,
                    "reason": if contested.is_empty() { "no-contestation" } else { "skip-resumed" },
                }),
            )?;
        }
        Vec::new()
    } else {
        let r2_packet_path = run_dir.join("packets/r2.json");
        if !r2_packet_path.exists() {
            if store.transition(&mut manifest, RunStatus::R2Packet, Some("R2"), json!({}))?
                == RunStatus::Cancelled
            {
                return Ok(RunStatus::Cancelled);
            }
            let packet = build_r2_packet(&manifest, &brief, &r1, &policy)?;
            write_json(&r2_packet_path, &packet)?;
        }
        if phase_complete(&run_dir, "R2", &policy) {
            load_phase(&run_dir, "R2", &policy)?
        } else {
            if store.transition(&mut manifest, RunStatus::R2Running, Some("R2"), json!({}))?
                == RunStatus::Cancelled
            {
                return Ok(RunStatus::Cancelled);
            }
            match run_phase(
                store,
                &mut manifest,
                &policy,
                &brief,
                "R2",
                Some(&r2_packet_path),
            ) {
                Ok(outputs) => outputs,
                Err(error) => {
                    return fail_run(
                        store,
                        &mut manifest,
                        RunStatus::Failed,
                        "r2_failed",
                        &error.to_string(),
                        false,
                    );
                }
            }
        }
    };
    if matches!(
        manifest.status,
        RunStatus::R1Gate
            | RunStatus::R2Packet
            | RunStatus::R2Running
            | RunStatus::Queued
            | RunStatus::Preflight
            | RunStatus::R1Running
    ) && store.transition(
        &mut manifest,
        RunStatus::R2Gate,
        Some("R2"),
        // The event ledger is the authority: a skipped (unanimous-R1) R2
        // records zero accepted lanes, a contested R2 records the lanes
        // that actually ran — never a hardcoded five.
        json!({"accepted": r2.len()}),
    )? == RunStatus::Cancelled
    {
        return Ok(RunStatus::Cancelled);
    }

    let evidence_packet_path = run_dir.join("r3/evidence-packet.json");
    if !evidence_packet_path.exists() {
        let evidence = json!({
            "evidence_packet_version": EVIDENCE_PACKET_VERSION,
            "run_id": run_id,
            "question": brief.question,
            "r1": r1.iter().map(|lane| (&lane.party_id, &lane.output)).collect::<BTreeMap<_, _>>(),
            "r2": r2.iter().map(|lane| (&lane.party_id, &lane.output)).collect::<BTreeMap<_, _>>(),
            "snapshot_sha256": manifest.snapshot_sha256
        });
        write_json(&evidence_packet_path, &evidence)?;
    }

    let cc_path = run_dir.join("r3/cc-response.json");
    if !cc_path.exists() {
        if store.transition(&mut manifest, RunStatus::R3Cc, Some("R3"), json!({}))?
            == RunStatus::Cancelled
        {
            return Ok(RunStatus::Cancelled);
        }
        let lane_result = (|| {
            let route = &policy.counterpart_arbiter;
            let attempt = next_attempt(&run_dir, "R3", &route.route_id, policy.max_attempts)?;
            wait_for_retry_deadline(store, &mut manifest, "R3", route, attempt)?;
            let lane_root = run_dir.join(format!("lanes/R3/{}/attempt-{attempt}", route.route_id));
            let r3_timeout = policy.timeout_for_phase("R3");
            let invocation = adapters::build(
                route,
                "R3",
                &manifest.effective_model,
                &evidence_packet_path,
                &lane_root,
                r3_timeout,
            )?;
            let outcome = run_attempt(
                store,
                &manifest,
                &route.party_id,
                &route.adapter,
                "R3",
                attempt,
                invocation,
                r3_timeout,
                policy.max_output_bytes,
                policy.max_attempts,
            )?;
            await_arbiter_output(
                store,
                &mut manifest,
                &policy,
                "R3",
                &evidence_packet_path,
                &run_dir,
                route,
                attempt,
                outcome,
            )
        })();
        let verdict = match lane_result {
            Err(error) => {
                if manifest.status == RunStatus::Cancelled {
                    return Ok(RunStatus::Cancelled);
                }
                return fail_run(
                    store,
                    &mut manifest,
                    RunStatus::Failed,
                    "r3_cc_failed",
                    &error.to_string(),
                    false,
                );
            }
            Ok(Some(verdict)) => verdict,
            Ok(None) => return cancel_run(store, &mut manifest),
        };
        if let Err(error) = write_json(&cc_path, &verdict) {
            return fail_run(
                store,
                &mut manifest,
                RunStatus::Failed,
                "r3_cc_failed",
                &format!("cannot persist Counterpart Arbiter verdict: {error:#}"),
                false,
            );
        }
    }

    if let Err(error) = ensure_r3_input_receipt(
        &mut manifest,
        &policy,
        &r1,
        &r2,
        &evidence_packet_path,
        &cc_path,
        &run_dir,
    ) {
        return fail_run(
            store,
            &mut manifest,
            RunStatus::FailedPolicy,
            "r3_input_drift",
            &error.to_string(),
            false,
        );
    }

    if policy.auto_primary_arbiter && manifest.primary_arbiter_submission.is_none() {
        if manifest.primary_arbiter_challenge.is_none() {
            let challenge =
                match create_primary_arbiter_challenge(&manifest, &brief, &evidence_packet_path) {
                    Ok(challenge) => challenge,
                    Err(error) => {
                        return fail_run(
                            store,
                            &mut manifest,
                            RunStatus::FailedPolicy,
                            "r3_primary_arbiter_challenge_failed",
                            &error.to_string(),
                            false,
                        );
                    }
                };
            manifest.primary_arbiter_challenge = Some(challenge);
            store.save_manifest(&manifest)?;
        }
        let primary_packet_path = run_dir.join("r3/primary-arbiter-packet.json");
        if let Err(error) = ensure_primary_arbiter_packet(
            &manifest,
            &evidence_packet_path,
            &cc_path,
            &primary_packet_path,
        ) {
            return fail_run(
                store,
                &mut manifest,
                RunStatus::FailedPolicy,
                "r3_primary_arbiter_input_drift",
                &error.to_string(),
                false,
            );
        }
        let lane_result = (|| {
            let route = &policy.primary_arbiter;
            let attempt = next_attempt(&run_dir, "R3", &route.route_id, policy.max_attempts)?;
            wait_for_retry_deadline(store, &mut manifest, "R3", route, attempt)?;
            let lane_root = run_dir.join(format!("lanes/R3/{}/attempt-{attempt}", route.route_id));
            let r3_timeout = policy.timeout_for_phase("R3");
            let invocation = adapters::build(
                route,
                "R3",
                &manifest.effective_model,
                &primary_packet_path,
                &lane_root,
                r3_timeout,
            )?;
            let outcome = run_attempt(
                store,
                &manifest,
                &route.party_id,
                &route.adapter,
                "R3",
                attempt,
                invocation,
                r3_timeout,
                policy.max_output_bytes,
                policy.max_attempts,
            )?;
            await_arbiter_output(
                store,
                &mut manifest,
                &policy,
                "R3",
                &primary_packet_path,
                &run_dir,
                route,
                attempt,
                outcome,
            )
        })();
        let verdict = match lane_result {
            Err(error) => {
                return fail_run(
                    store,
                    &mut manifest,
                    RunStatus::Failed,
                    "r3_primary_arbiter_failed",
                    &error.to_string(),
                    false,
                );
            }
            Ok(Some(verdict)) => verdict,
            Ok(None) => return cancel_run(store, &mut manifest),
        };
        let response = match bound_primary_response(&manifest, verdict) {
            Ok(response) => response,
            Err(error) => {
                return fail_run(
                    store,
                    &mut manifest,
                    RunStatus::FailedPolicy,
                    "r3_primary_arbiter_response_invalid",
                    &error.to_string(),
                    false,
                );
            }
        };
        if let Err(error) = stage_primary_response(store, &mut manifest, &run_dir, &brief, response)
        {
            return fail_run(
                store,
                &mut manifest,
                RunStatus::FailedPolicy,
                "r3_primary_arbiter_response_invalid",
                &error.to_string(),
                false,
            );
        }
    }

    if manifest.primary_arbiter_submission.is_none() {
        if manifest.primary_arbiter_challenge.is_none() {
            let challenge =
                create_primary_arbiter_challenge(&manifest, &brief, &evidence_packet_path)?;
            manifest.primary_arbiter_challenge = Some(challenge);
            // Persist scheduler ownership before publishing the request. A crash can then
            // regenerate the same request instead of exposing an orphan challenge.
            store.save_manifest(&manifest)?;
        }
        write_json(
            &run_dir.join("r3/primary-arbiter-request.json"),
            manifest.primary_arbiter_challenge.as_ref().unwrap(),
        )?;
        let status = store.transition(
            &mut manifest,
            RunStatus::WaitingPrimaryArbiter,
            Some("R3"),
            json!({
                "primary_arbiter_request": "r3/primary-arbiter-request.json"
            }),
        )?;
        return Ok(status);
    }

    let submission_ready = match recover_primary_arbiter_submission(&mut manifest, &brief, &run_dir)
    {
        Ok(ready) => ready,
        Err(error) => {
            return fail_run(
                store,
                &mut manifest,
                RunStatus::FailedPolicy,
                "primary_arbiter_submission_drift",
                &error.to_string(),
                false,
            );
        }
    };
    if !submission_ready {
        return store.transition(
            &mut manifest,
            RunStatus::WaitingPrimaryArbiter,
            Some("R3"),
            json!({"primary_arbiter_request": "r3/primary-arbiter-request.json", "submission": "staging"}),
        );
    }

    let merge_detail = if policy.auto_primary_arbiter {
        json!({"submission": "automatic"})
    } else {
        json!({})
    };
    if store.transition(&mut manifest, RunStatus::Merging, Some("R3"), merge_detail)?
        == RunStatus::Cancelled
    {
        return Ok(RunStatus::Cancelled);
    }
    let mut primary_arbiter_response = read_owned_primary_arbiter_response(&manifest, &run_dir)?;
    validate_primary_arbiter_response_binding(
        &manifest,
        &primary_arbiter_response,
        &brief,
        &evidence_packet_path,
    )?;
    // Challenge binding alone is insufficient: residual evidence references must
    // resolve to the exact snapshot before an external verdict can be merged.
    validate_arbiter_semantics(&mut primary_arbiter_response.verdict, &run_dir)?;
    let finalization = store.finalization_guard(run_id)?;
    if finalization.cancellation_requested() {
        drop(finalization);
        return cancel_run(store, &mut manifest);
    }
    let result = write_run_result(
        &manifest,
        &brief,
        &policy,
        &r1,
        &r2,
        &primary_arbiter_response.verdict,
        &run_dir,
    )?;
    manifest.result_sha256 = Some(sha256_file(&run_dir.join("result.json"))?);
    let status = store.transition(
        &mut manifest,
        result.status,
        Some("R3"),
        json!({"result": "result.json"}),
    )?;
    // Run-level telemetry summary
    let run_started = manifest.created_at.parse::<chrono::DateTime<chrono::Utc>>()
        .map(|t| t.timestamp_millis())
        .unwrap_or(0);
    let run_finished = chrono::Utc::now().timestamp_millis();
    let total_duration_ms = run_finished.saturating_sub(run_started);
    store.event(
        run_id,
        "run.completed",
        None,
        None,
        None,
        json!({
            "status": format!("{:?}", status),
            "total_duration_ms": total_duration_ms,
            "timeout_seconds": policy.timeout_seconds,
            "r2_parallel": policy.r2_parallel,
            "max_attempts": policy.max_attempts,
            "retry_backoff_seconds": policy.retry_backoff_seconds,
            "retry_backoff_max_seconds": policy.retry_backoff_max_seconds,
            "r2_min_interval_seconds": policy.r2_min_interval_seconds,
        }),
    )?;
    drop(finalization);
    Ok(status)
}

fn ensure_primary_arbiter_packet(
    manifest: &RunManifest,
    evidence_packet_path: &Path,
    cc_path: &Path,
    packet_path: &Path,
) -> anyhow::Result<()> {
    let receipt = manifest
        .r3_input_receipt
        .as_ref()
        .ok_or_else(|| anyhow!("R3 input receipt is missing"))?;
    let packet = json!({
        "primary_arbiter_packet_version": "1.0",
        "run_id": manifest.run_id,
        "input_receipt_sha256": receipt.sha256,
        "evidence_packet": read_json::<Value>(evidence_packet_path)?,
        "counterpart_arbiter_verdict": read_json::<ArbiterVerdict>(cc_path)?,
    });
    if packet_path.exists() {
        let existing: Value = read_json(packet_path)?;
        if existing != packet {
            bail!("primary arbiter packet changed after creation");
        }
        return Ok(());
    }
    write_json(packet_path, &packet)
}

fn verify_primary_arbiter_packet(manifest: &RunManifest, run_dir: &Path) -> anyhow::Result<()> {
    let packet_path = run_dir.join("r3/primary-arbiter-packet.json");
    if !packet_path.exists() {
        return Ok(());
    }
    ensure_primary_arbiter_packet(
        manifest,
        &run_dir.join("r3/evidence-packet.json"),
        &run_dir.join("r3/cc-response.json"),
        &packet_path,
    )
}

fn bound_primary_response(
    manifest: &RunManifest,
    verdict: ArbiterVerdict,
) -> anyhow::Result<PrimaryArbiterResponse> {
    let challenge = manifest
        .primary_arbiter_challenge
        .as_ref()
        .ok_or_else(|| anyhow!("primary arbiter challenge is missing"))?;
    Ok(PrimaryArbiterResponse {
        primary_arbiter_response_version: PRIMARY_ARBITER_RESPONSE_VERSION.into(),
        run_id: challenge.run_id.clone(),
        nonce: challenge.nonce.clone(),
        policy_sha256: challenge.policy_sha256.clone(),
        evidence_packet_sha256: challenge.evidence_packet_sha256.clone(),
        input_receipt_sha256: challenge.input_receipt_sha256.clone(),
        action_scope: challenge.action_scope.clone(),
        verdict,
    })
}

fn stage_primary_response(
    store: &Store,
    manifest: &mut RunManifest,
    run_dir: &Path,
    brief: &Brief,
    mut response: PrimaryArbiterResponse,
) -> anyhow::Result<()> {
    validate_new_primary_arbiter_response(
        manifest,
        &response,
        brief,
        &run_dir.join("r3/evidence-packet.json"),
    )?;
    validate_arbiter_semantics(&mut response.verdict, run_dir)?;
    let response_bytes = serde_json::to_vec_pretty(&response)?;
    let response_sha256 = sha256_bytes(&response_bytes_with_newline(response_bytes));
    let input_receipt_sha256 = manifest
        .r3_input_receipt
        .as_ref()
        .ok_or_else(|| anyhow!("R3 input receipt is missing"))?
        .sha256
        .clone();
    manifest.primary_arbiter_submission = Some(PrimaryArbiterSubmissionReceipt {
        submission_receipt_version: PRIMARY_ARBITER_SUBMISSION_VERSION.into(),
        state: PrimaryArbiterSubmissionState::Staging,
        response_ref: "r3/primary-arbiter-response.json".into(),
        response_sha256,
        input_receipt_sha256,
        staged_at: utc_now(),
        accepted_at: None,
    });
    store.save_manifest(manifest)?;
    write_json(&run_dir.join("r3/primary-arbiter-response.json"), &response)?;
    if !recover_primary_arbiter_submission(manifest, brief, run_dir)? {
        bail!("primary-arbiter response could not be durably accepted");
    }
    store.save_manifest(manifest)
}

pub fn submit_primary_arbiter(
    store: &Store,
    run_id: &str,
    response_path: &Path,
) -> anyhow::Result<RunStatus> {
    let internal = store
        .run_dir(run_id)?
        .join("r3/primary-arbiter-response.json");
    if response_path
        .canonicalize()
        .ok()
        .is_some_and(|path| path == internal)
    {
        bail!("primary-arbiter response input must be outside the scheduler-owned run directory");
    }
    let response: PrimaryArbiterResponse =
        validate_file(response_path, PRIMARY_ARBITER_RESPONSE_SCHEMA)?;
    submit_primary_arbiter_response(store, run_id, response)
}

/// Text floor for the degenerate-verdict guardrail (counted in characters,
/// CJK-friendly): with empty residuals, a verdict whose summary or
/// recommendation falls below this is almost certainly a stub, and submitting
/// it would finalize the run in one shot — so it is rejected by default.
const MIN_VERDICT_TEXT_CHARS: usize = 20;

fn verdict_looks_degenerate(verdict: &ArbiterVerdict) -> bool {
    verdict.residuals.is_empty()
        && (verdict.summary.chars().count() < MIN_VERDICT_TEXT_CHARS
            || verdict.recommendation.chars().count() < MIN_VERDICT_TEXT_CHARS)
}

fn ensure_verdict_not_degenerate(verdict: &ArbiterVerdict, force: bool) -> anyhow::Result<()> {
    if !force && verdict_looks_degenerate(verdict) {
        bail!(
            "verdict looks degenerate (empty residuals and trivial summary/recommendation); pass --force to proceed anyway"
        );
    }
    Ok(())
}

pub fn submit_primary_arbiter_verdict(
    store: &Store,
    run_id: &str,
    verdict_path: &Path,
    force: bool,
) -> anyhow::Result<RunStatus> {
    let run_dir = store.run_dir(run_id)?;
    if verdict_path
        .canonicalize()
        .ok()
        .is_some_and(|path| path.starts_with(&run_dir))
    {
        bail!("primary-arbiter verdict input must be outside the scheduler-owned run directory");
    }
    let verdict: ArbiterVerdict = read_json(verdict_path)?;
    ensure_verdict_not_degenerate(&verdict, force)?;
    // Fail fast, before any staging side effects: an invalid verdict
    // (e.g. unresolvable evidence_refs/closure_evidence) must bail here.
    // Previously the failure surfaced only inside the durable-acceptance
    // step AFTER the receipt and r3/primary-arbiter-response.json were
    // already written, poisoning the challenge for every later attempt.
    let mut verdict = verdict;
    validate_arbiter_semantics(&mut verdict, &run_dir)?;
    let manifest = store.load_manifest(run_id)?;
    let challenge = manifest
        .primary_arbiter_challenge
        .ok_or_else(|| anyhow!("primary arbiter challenge is not ready"))?;
    let response = PrimaryArbiterResponse {
        primary_arbiter_response_version: PRIMARY_ARBITER_RESPONSE_VERSION.into(),
        run_id: challenge.run_id,
        nonce: challenge.nonce,
        policy_sha256: challenge.policy_sha256,
        evidence_packet_sha256: challenge.evidence_packet_sha256,
        input_receipt_sha256: challenge.input_receipt_sha256,
        action_scope: challenge.action_scope,
        verdict,
    };
    validate_value(
        &serde_json::to_value(&response)?,
        PRIMARY_ARBITER_RESPONSE_SCHEMA,
    )?;
    submit_primary_arbiter_response(store, run_id, response)
}

fn submit_primary_arbiter_response(
    store: &Store,
    run_id: &str,
    response: PrimaryArbiterResponse,
) -> anyhow::Result<RunStatus> {
    let _lock = store.lock(run_id)?;
    let mut manifest = store.load_manifest(run_id)?;
    let run_dir = store.run_dir(run_id)?;
    let brief: Brief = read_json(&run_dir.join("input/brief.json"))?;
    let policy: Policy = read_json(&run_dir.join("input/policy.json"))?;
    if let Err(error) = verify_resume_integrity(&manifest, &brief, &policy, &run_dir) {
        return fail_run(
            store,
            &mut manifest,
            RunStatus::FailedPolicy,
            "integrity_drift",
            &error.to_string(),
            false,
        );
    }
    let response_bytes = serde_json::to_vec_pretty(&response)?;
    let response_sha256 = sha256_bytes(&response_bytes_with_newline(response_bytes));

    let already_accepted = manifest
        .primary_arbiter_submission
        .as_ref()
        .is_some_and(|receipt| receipt.state == PrimaryArbiterSubmissionState::Accepted);
    if let Some(receipt) = &manifest.primary_arbiter_submission {
        if receipt.response_sha256 != response_sha256 {
            bail!("primary arbiter challenge already has a different scheduler-owned submission");
        }
    } else {
        if manifest.status != RunStatus::WaitingPrimaryArbiter {
            bail!("run {run_id} is not waiting for Primary Arbiter");
        }
        if manifest
            .primary_arbiter_challenge
            .as_ref()
            .is_some_and(|challenge| challenge.consumed)
        {
            bail!("primary arbiter challenge was already consumed");
        }
        validate_new_primary_arbiter_response(
            &manifest,
            &response,
            &brief,
            &run_dir.join("r3/evidence-packet.json"),
        )?;
        let input_receipt_sha256 = manifest
            .r3_input_receipt
            .as_ref()
            .ok_or_else(|| anyhow!("R3 input receipt is missing"))?
            .sha256
            .clone();
        manifest.primary_arbiter_submission = Some(PrimaryArbiterSubmissionReceipt {
            submission_receipt_version: PRIMARY_ARBITER_SUBMISSION_VERSION.into(),
            state: PrimaryArbiterSubmissionState::Staging,
            response_ref: "r3/primary-arbiter-response.json".into(),
            response_sha256: response_sha256.clone(),
            input_receipt_sha256,
            staged_at: utc_now(),
            accepted_at: None,
        });
        store.save_manifest(&manifest)?;
    }

    write_json(&run_dir.join("r3/primary-arbiter-response.json"), &response)?;
    if !recover_primary_arbiter_submission(&mut manifest, &brief, &run_dir)? {
        bail!("primary-arbiter response could not be durably accepted");
    }
    store.save_manifest(&manifest)?;
    if !already_accepted {
        store.event(
            run_id,
            "primary_arbiter.accepted",
            Some("R3"),
            None,
            None,
            json!({"response_sha256": response_sha256}),
        )?;
    }
    drop(_lock);
    advance(store, run_id)
}

/// Rewrite a completed run's result.json with a replacement verdict. The R3
/// directory and lane artifacts are read-only and no party is re-run; only
/// result/report and the manifest's result_sha256 are updated, plus one audit
/// event is appended. Refused while the run is still active (the submit
/// handshake is the entry point for that).
pub fn amend_primary_arbiter_verdict(
    store: &Store,
    run_id: &str,
    verdict_path: &Path,
    force: bool,
) -> anyhow::Result<RunStatus> {
    let run_dir = store.run_dir(run_id)?;
    if verdict_path
        .canonicalize()
        .ok()
        .is_some_and(|path| path.starts_with(&run_dir))
    {
        bail!("primary-arbiter verdict input must be outside the scheduler-owned run directory");
    }
    let verdict: ArbiterVerdict = read_json(verdict_path)?;
    ensure_verdict_not_degenerate(&verdict, force)?;
    let _lock = store.lock(run_id)?;
    let mut manifest = store.load_manifest(run_id)?;
    if manifest.status != RunStatus::Completed {
        bail!(
            "run {run_id} is {:?}, not completed; `primary-arbiter amend` only applies to completed runs (use `primary-arbiter submit` for a run in the arbiter handshake)",
            manifest.status
        );
    }
    let brief: Brief = read_json(&run_dir.join("input/brief.json"))?;
    let policy: Policy = read_json(&run_dir.join("input/policy.json"))?;
    // The recorded inputs must be untampered before re-merging. The runtime
    // hash is deliberately not re-checked: the typical amend scenario is
    // repairing an old run after a CLI upgrade.
    if sha256_bytes(&serde_json::to_vec(&brief)?) != manifest.brief_sha256 {
        bail!("brief changed since run creation");
    }
    if sha256_bytes(&serde_json::to_vec(&policy)?) != manifest.policy_sha256 {
        bail!("policy changed since run creation");
    }
    if manifest.r3_input_receipt.is_some() {
        verify_r3_input_receipt(&manifest, &policy, &run_dir)?;
    }
    let r1 = load_phase(&run_dir, "R1", &policy)?;
    let r2 = load_phase(&run_dir, "R2", &policy)?;
    // Same semantic validation as finalize: residual ids unique, evidence
    // references resolvable to the snapshot.
    let mut verdict = verdict;
    validate_arbiter_semantics(&mut verdict, &run_dir)?;
    write_run_result(&manifest, &brief, &policy, &r1, &r2, &verdict, &run_dir)?;
    manifest.result_sha256 = Some(sha256_file(&run_dir.join("result.json"))?);
    store.save_manifest_after_terminal(&manifest)?;
    store.event(
        run_id,
        "primary_arbiter.amended",
        Some("R3"),
        None,
        None,
        json!({
            "verdict_sha256": sha256_file(verdict_path)?,
            "result": "result.json",
            "force": force,
        }),
    )?;
    Ok(manifest.status)
}

pub fn cancel(store: &Store, run_id: &str) -> anyhow::Result<RunStatus> {
    let mut manifest = store.load_manifest(run_id)?;
    if manifest.status.terminal() {
        return Ok(manifest.status);
    }
    let snapshot = store.request_cancellation(run_id)?;
    if snapshot.status.terminal() {
        return Ok(snapshot.status);
    }
    manifest = store.load_manifest(run_id)?;
    let current_phase = manifest.current_phase.clone();
    let status = store.transition(
        &mut manifest,
        RunStatus::Cancelling,
        current_phase.as_deref(),
        json!({}),
    )?;
    if status.terminal() {
        return Ok(status);
    }
    let had_active_processes = !snapshot.processes.is_empty();
    if had_active_processes {
        for process in &snapshot.processes {
            kill_verified_process_tree(process, false);
        }
        thread::sleep(Duration::from_millis(500));
        for process in &snapshot.processes {
            kill_verified_process_tree(process, true);
        }
    }
    if had_active_processes {
        return Ok(RunStatus::Cancelling);
    }
    let current_phase = manifest.current_phase.clone();
    store.transition(
        &mut manifest,
        RunStatus::Cancelled,
        current_phase.as_deref(),
        json!({}),
    )
}

#[cfg(test)]
fn build_snapshot(
    run_dir: &Path,
    brief: &Brief,
    policy: &Policy,
) -> anyhow::Result<SnapshotManifest> {
    let ignore = snapshot_ignore_set(&brief.snapshot_ignore)?;
    build_snapshot_with_ignore(run_dir, brief, policy, &ignore)
}

fn build_snapshot_with_ignore(
    run_dir: &Path,
    brief: &Brief,
    policy: &Policy,
    ignore: &GlobSet,
) -> anyhow::Result<SnapshotManifest> {
    let snapshot_dir = run_dir.join("input/snapshot");
    let attachments_dir = run_dir.join("input/attachments");
    let mut entries = Vec::new();
    let mut attachments = Vec::new();
    let mut total_bytes = 0_u64;
    for (root_index, root) in brief.evidence_roots.iter().enumerate() {
        let source_root = canonical_existing(root)?;
        if source_root.is_file() {
            let relative = source_root.file_name().unwrap_or_default();
            if !ignore.is_match(Path::new(relative)) {
                copy_snapshot_file(
                    &source_root,
                    &source_root,
                    root_index,
                    &snapshot_dir,
                    &mut entries,
                    &mut total_bytes,
                    policy,
                )?;
            }
        } else {
            for item in WalkDir::new(&source_root)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| snapshot_entry_allowed(entry, &source_root, ignore))
            {
                let item = item?;
                let file_type = item.file_type();
                if file_type.is_symlink() || !file_type.is_file() {
                    continue;
                }
                copy_snapshot_file(
                    &source_root,
                    item.path(),
                    root_index,
                    &snapshot_dir,
                    &mut entries,
                    &mut total_bytes,
                    policy,
                )?;
            }
        }
    }

    for (index, path) in brief.attachments.iter().enumerate() {
        let source = canonical_existing(path)?;
        let metadata = fs::metadata(&source)?;
        let next_total = total_bytes
            .checked_add(metadata.len())
            .ok_or_else(|| anyhow!("snapshot attachment byte count overflow"))?;
        if !metadata.is_file()
            || metadata.len() > policy.max_attachment_bytes
            || next_total > policy.max_snapshot_bytes + policy.max_attachment_bytes * 10
        {
            bail!(
                "attachment {} is not a permitted regular file",
                source.display()
            );
        }
        let prefix = read_prefix(&source, 16 * 1024)?;
        let media_type = infer::get(&prefix)
            .map(|kind| kind.mime_type().to_string())
            .ok_or_else(|| anyhow!("cannot detect attachment type for {}", source.display()))?;
        if !matches!(
            media_type.as_str(),
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        ) {
            bail!("unsupported attachment media type {media_type}");
        }
        let extension = infer::get(&prefix).unwrap().extension();
        let name = format!("attachment-{index}.{extension}");
        let target = attachments_dir.join(&name);
        copy_file_streaming(&source, &target, metadata.len())?;
        attachments.push(AttachmentEntry {
            attachment_ref: format!("attachment://{name}"),
            source_name: source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            sha256: sha256_file(&target)?,
            bytes: metadata.len(),
            media_type,
        });
        total_bytes = next_total;
    }

    entries.sort_by(|left, right| left.snapshot_ref.cmp(&right.snapshot_ref));
    Ok(SnapshotManifest {
        snapshot_version: SNAPSHOT_VERSION.into(),
        created_at: utc_now(),
        entries,
        attachments,
        total_bytes,
    })
}

fn snapshot_ignore_set(patterns: &[String]) -> anyhow::Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        if pattern.starts_with('/') || pattern.ends_with('/') || pattern.contains('\\') {
            bail!("snapshot_ignore pattern must be a relative '/'-separated path: {pattern}");
        }
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .case_insensitive(cfg!(windows))
            .build()
            .with_context(|| format!("invalid snapshot_ignore pattern: {pattern}"))?;
        builder.add(glob);
    }
    builder
        .build()
        .context("cannot compile snapshot_ignore patterns")
}

fn snapshot_entry_allowed(entry: &DirEntry, root: &Path, ignore: &GlobSet) -> bool {
    let name = entry.file_name().to_string_lossy();
    let allowed = !matches!(
        name.as_ref(),
        ".git" | "node_modules" | "target" | ".quinte" | ".env"
    ) && !name.ends_with(".key")
        && !name.ends_with(".pem");
    if entry.depth() == 0 {
        return true;
    }
    if !allowed {
        return false;
    }
    entry
        .path()
        .strip_prefix(root)
        .map(relative_slash)
        .map(|relative| !ignore.is_match(relative))
        .unwrap_or(false)
}

fn copy_snapshot_file(
    source_root: &Path,
    source: &Path,
    root_index: usize,
    snapshot_dir: &Path,
    entries: &mut Vec<SnapshotEntry>,
    total_bytes: &mut u64,
    policy: &Policy,
) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if !metadata.file_type().is_file() {
        bail!(
            "snapshot source is not a regular file: {}",
            source.display()
        );
    }
    let next_file_count = entries.len().saturating_add(1);
    let next_total = total_bytes
        .checked_add(metadata.len())
        .ok_or_else(|| anyhow!("evidence snapshot byte count overflow"))?;
    if next_file_count > policy.max_snapshot_files || next_total > policy.max_snapshot_bytes {
        bail!("evidence snapshot exceeds policy file/byte limit");
    }
    let relative = if source_root.is_file() {
        PathBuf::from(source.file_name().unwrap_or_default())
    } else {
        source.strip_prefix(source_root)?.to_path_buf()
    };
    let target_relative = PathBuf::from(format!("root-{root_index}")).join(&relative);
    let target = snapshot_dir.join(&target_relative);
    copy_file_streaming(source, &target, metadata.len())?;
    let prefix = read_prefix(source, 16 * 1024)?;
    let media_type = infer::get(&prefix)
        .map(|kind| kind.mime_type().to_string())
        .unwrap_or_else(|| "text/plain".to_string());
    entries.push(SnapshotEntry {
        snapshot_ref: format!("snapshot://{}", relative_slash(&target_relative)),
        source_name: source
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        sha256: sha256_file(&target)?,
        bytes: metadata.len(),
        media_type,
    });
    *total_bytes = next_total;
    Ok(())
}

fn copy_file_streaming(source: &Path, target: &Path, expected_bytes: u64) -> anyhow::Result<()> {
    let io_source = filesystem_path(source)?;
    let io_target = filesystem_path(target)?;
    let io_parent = io_target
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent", target.display()))?;
    create_private_dir_all(io_parent)?;
    let temporary = tempfile::NamedTempFile::new_in(io_parent)?;
    harden_private_file(temporary.path())?;
    let mut input = fs::File::open(&io_source)?;
    let mut output = temporary.as_file();
    let copied = std::io::copy(&mut input, &mut output)?;
    if copied != expected_bytes {
        bail!(
            "snapshot source changed while copying: {}",
            source.display()
        );
    }
    output.sync_all()?;
    temporary
        .persist(&io_target)
        .map_err(|error| anyhow!("cannot persist {}: {}", target.display(), error.error))?;
    harden_private_file(&io_target)?;
    Ok(())
}

fn read_prefix(path: &Path, max_bytes: u64) -> anyhow::Result<Vec<u8>> {
    let mut file = fs::File::open(filesystem_path(path)?)?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(max_bytes)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn verify_resume_integrity(
    manifest: &RunManifest,
    brief: &Brief,
    policy: &Policy,
    run_dir: &Path,
) -> anyhow::Result<()> {
    policy::validate_for_runtime(policy)?;
    if manifest.seat_binding != policy.seat
        || manifest.route_bindings != policy_route_bindings(policy)
    {
        bail!("seat or route binding changed since run creation");
    }
    if runtime_sha256()? != manifest.runtime_sha256 {
        bail!("QUINTE runtime changed since run creation");
    }
    if sha256_bytes(&serde_json::to_vec(brief)?) != manifest.brief_sha256 {
        bail!("brief changed since run creation");
    }
    if sha256_bytes(&serde_json::to_vec(policy)?) != manifest.policy_sha256 {
        bail!("policy changed since run creation");
    }
    let snapshot: SnapshotManifest = read_json(&run_dir.join("input/snapshot-manifest.json"))?;
    if sha256_bytes(&serde_json::to_vec(&snapshot)?) != manifest.snapshot_sha256 {
        bail!("snapshot manifest changed since run creation");
    }
    for entry in &snapshot.entries {
        let relative = entry.snapshot_ref.strip_prefix("snapshot://").unwrap();
        if sha256_file(&run_dir.join("input/snapshot").join(relative))? != entry.sha256 {
            bail!("snapshot artifact changed: {}", entry.snapshot_ref);
        }
    }
    for attachment in &snapshot.attachments {
        let relative = attachment
            .attachment_ref
            .strip_prefix("attachment://")
            .ok_or_else(|| anyhow!("invalid attachment reference {}", attachment.attachment_ref))?;
        if sha256_file(&run_dir.join("input/attachments").join(relative))? != attachment.sha256 {
            bail!("attachment artifact changed: {}", attachment.attachment_ref);
        }
    }
    if manifest.r3_input_receipt.is_some() {
        verify_r3_input_receipt(manifest, policy, run_dir)?;
    }
    verify_primary_arbiter_packet(manifest, run_dir)?;
    if manifest.primary_arbiter_submission.is_some() {
        let mut recovered = manifest.clone();
        recover_primary_arbiter_submission(&mut recovered, brief, run_dir)?;
    }
    Ok(())
}

fn runtime_sha256() -> anyhow::Result<String> {
    RUNTIME_SHA256
        .get_or_init(|| {
            std::env::current_exe()
                .map_err(|error| error.to_string())
                .and_then(|path| sha256_file(&path).map_err(|error| error.to_string()))
        })
        .clone()
        .map_err(anyhow::Error::msg)
}

fn phase_complete(run_dir: &Path, phase: &str, policy: &Policy) -> bool {
    policy.roster.iter().all(|route| {
        run_dir
            .join(format!("lanes/{phase}/{}/accepted.json", route.route_id))
            .exists()
    })
}

fn next_attempt(
    run_dir: &Path,
    phase: &str,
    route_id: &str,
    max_attempts: usize,
) -> anyhow::Result<usize> {
    if max_attempts == 0 {
        bail!("attempt budget exhausted for {phase}/{route_id}: maximum is zero");
    }
    let route_dir = run_dir.join("lanes").join(phase).join(route_id);
    let mut highest = 0_usize;
    match fs::read_dir(&route_dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.with_context(|| {
                    format!("cannot inspect attempt history entry for {phase}/{route_id}")
                })?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    continue;
                };
                let Some(number) = name.strip_prefix("attempt-") else {
                    continue;
                };
                let Ok(number) = number.parse::<usize>() else {
                    continue;
                };
                if number > 0 {
                    highest = highest.max(number);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot inspect attempt history for {phase}/{route_id}"));
        }
    }
    let attempt = highest
        .checked_add(1)
        .ok_or_else(|| anyhow!("attempt history overflow for {phase}/{route_id}"))?;
    if attempt > max_attempts {
        bail!(
            "attempt budget exhausted for {phase}/{route_id}: {highest} of {max_attempts} attempts already consumed"
        );
    }
    Ok(attempt)
}

fn load_phase(run_dir: &Path, phase: &str, policy: &Policy) -> anyhow::Result<Vec<LaneAccepted>> {
    if phase == "R2" && run_dir.join("r2/skipped.json").exists() {
        return Ok(Vec::new());
    }
    policy
        .roster
        .iter()
        .map(|route| {
            let path = run_dir.join(format!("lanes/{phase}/{}/accepted.json", route.route_id));
            let output: LaneOutput = validate_file(&path, LANE_OUTPUT_SCHEMA)?;
            Ok(LaneAccepted {
                party_id: route.party_id.clone(),
                route_id: route.route_id.clone(),
                output,
                artifact_ref: format!("lanes/{phase}/{}/accepted.json", route.route_id),
            })
        })
        .collect()
}

fn run_phase(
    store: &Store,
    manifest: &mut RunManifest,
    policy: &Policy,
    brief: &Brief,
    phase: &str,
    packet_override: Option<&Path>,
) -> anyhow::Result<Vec<LaneAccepted>> {
    let phase_started = Instant::now();
    let run_dir = store.run_dir(&manifest.run_id)?;
    let packet_path = packet_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| run_dir.join("input/task-packet.json"));
    if packet_override.is_none() && !packet_path.exists() {
        let packet = json!({
            "task_packet_version": TASK_PACKET_VERSION,
            "run_id": manifest.run_id,
            "phase": phase,
            "question": brief.question,
            "context": brief.context,
            "snapshot_manifest": "snapshot-manifest.json",
            "allowed_evidence_prefix": "snapshot://",
            "allowed_attachment_prefix": "attachment://",
            "instructions_are_data": true
        });
        write_json(&packet_path, &packet)?;
    }

    let mut accepted = load_existing_phase_outputs(&run_dir, phase, policy)?;
    let missing = policy
        .roster
        .iter()
        .filter(|route| !accepted.iter().any(|lane| lane.route_id == route.route_id))
        .cloned()
        .collect::<Vec<_>>();

    // R1 always fans out; R2 fans out only under the opt-in `r2_parallel`
    // policy switch. Both share this scheduler. The switch changes only the
    // rate-limit profile (serial inter-call pacing vs. parallel soft-stagger),
    // never a lane's information set: every R2 lane sees only the anonymized
    // R1 packet, and no lane ever reads another same-phase lane's output.
    let parallel_fan_out = phase == "R1" || (phase == "R2" && policy.r2_parallel);
    if parallel_fan_out {
        // Build every invocation before spawning any lane. A later build failure can
        // therefore never detach already-running children from scheduler ownership.
        let pending = missing
            .into_iter()
            .map(|route| {
                let attempt = next_attempt(&run_dir, phase, &route.route_id, policy.max_attempts)?;
                Ok((route, attempt))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let mut prepared = Vec::new();
        for (route, attempt) in pending {
            wait_for_retry_deadline(store, manifest, phase, &route, attempt)?;
            let lane_root = run_dir.join(format!(
                "lanes/{phase}/{}/attempt-{attempt}",
                route.route_id
            ));
            let invocation = adapters::build(
                &route,
                phase,
                &manifest.effective_model,
                &packet_path,
                &lane_root,
                policy.timeout_for_phase(phase),
            )?;
            prepared.push((route, attempt, invocation));
        }

        // Soft-stagger parallel starts so five same-family model routes do not
        // all hit the provider in the same second (primary 429 source). Total
        // lag is (n-1)*stagger and is recovered many times over when a single
        // 429 retry is avoided. Fake-adapter tests set QUINTE_ALLOW_FAKE_ADAPTERS
        // and skip the sleep so e2e stays fast.
        let stagger = parallel_start_stagger();
        let mut jobs = Vec::new();
        let mut spawn_cancelled = false;
        for (index, (route, attempt, invocation)) in prepared.into_iter().enumerate() {
            if index > 0 && !stagger.is_zero() && wait_cancellable(&run_dir, stagger) {
                let _ = cancel_run(store, manifest);
                spawn_cancelled = true;
                break;
            }
            let job_store = Store::new(store.home().to_path_buf());
            let job_manifest = manifest.clone();
            let party_id = route.party_id.clone();
            let adapter = route.adapter.clone();
            let phase_owned = phase.to_string();
            let timeout = policy.timeout_for_phase(phase);
            let max_output_bytes = policy.max_output_bytes;
            let max_attempts = policy.max_attempts;
            jobs.push(LaneJob {
                route,
                attempt,
                handle: thread::spawn(move || {
                    run_attempt(
                        &job_store,
                        &job_manifest,
                        &party_id,
                        &adapter,
                        &phase_owned,
                        attempt,
                        invocation,
                        timeout,
                        max_output_bytes,
                        max_attempts,
                    )
                }),
            });
        }

        // Always join every spawned lane before interpreting any output. In
        // particular, an invalid early lane must not orphan remaining children.
        let mut finished = Vec::new();
        let mut join_errors = Vec::new();
        for job in jobs {
            match job.handle.join() {
                Ok(Ok(outcome)) => finished.push((job.route, job.attempt, outcome)),
                Ok(Err(error)) => join_errors.push(format!("{}: {error:#}", job.route.party_id)),
                Err(_) => join_errors.push(format!("{}: lane worker panicked", job.route.party_id)),
            }
        }
        if spawn_cancelled {
            bail!("run cancelled");
        }
        if !join_errors.is_empty() {
            bail!("{phase} lane execution failed: {}", join_errors.join("; "));
        }

        let mut acceptance_errors = Vec::new();
        for (route, attempt, outcome) in finished {
            if let Err(error) = accept_or_retry_lane(
                store,
                manifest,
                policy,
                phase,
                &packet_path,
                &run_dir,
                &route,
                attempt,
                outcome,
                &mut accepted,
            ) {
                acceptance_errors.push(format!("{}: {error:#}", route.party_id));
            }
        }
        if !acceptance_errors.is_empty() {
            bail!(
                "{phase} acceptance failed: {}",
                acceptance_errors.join("; ")
            );
        }
    } else {
        // Serial R2 loop (the default): one lane at a time with persisted
        // inter-call pacing — the anti-429 rate-limit profile.
        for route in missing {
            let attempt = next_attempt(&run_dir, phase, &route.route_id, policy.max_attempts)?;
            wait_for_retry_deadline(store, manifest, phase, &route, attempt)?;
            wait_for_r2_pacing(store, manifest, &route, attempt)?;
            let lane_root = run_dir.join(format!(
                "lanes/{phase}/{}/attempt-{attempt}",
                route.route_id
            ));
            let phase_timeout = policy.timeout_for_phase(phase);
            let invocation = adapters::build(
                &route,
                phase,
                &manifest.effective_model,
                &packet_path,
                &lane_root,
                phase_timeout,
            )?;
            let outcome = run_attempt(
                store,
                manifest,
                &route.party_id,
                &route.adapter,
                phase,
                attempt,
                invocation,
                phase_timeout,
                policy.max_output_bytes,
                policy.max_attempts,
            )?;
            persist_r2_pacing(
                store,
                manifest,
                &route,
                attempt,
                policy.r2_min_interval_seconds,
                "inter_call_pacing",
            )?;
            accept_or_retry_lane(
                store,
                manifest,
                policy,
                phase,
                &packet_path,
                &run_dir,
                &route,
                attempt,
                outcome,
                &mut accepted,
            )?;
        }
    }
    if accepted.len() != 5 {
        bail!("{phase} five-party gate failed");
    }
    let phase_duration_ms = phase_started.elapsed().as_millis();
    let total_attempts: usize = accepted.iter().map(|l| {
        // Count attempts from the lane directory
        let lane_dir = run_dir.join(format!("lanes/{phase}/{}", l.route_id));
        std::fs::read_dir(&lane_dir)
            .map(|entries| entries.filter_map(|e| e.ok()).filter(|e| {
                e.file_name().to_string_lossy().starts_with("attempt-")
            }).count())
            .unwrap_or(0)
    }).sum();
    store.event(
        &manifest.run_id,
        "phase.completed",
        Some(phase),
        None,
        None,
        json!({
            "phase": phase,
            "duration_ms": phase_duration_ms,
            "lanes_completed": accepted.len(),
            "total_attempts": total_attempts,
            "timeout_seconds": policy.timeout_for_phase(phase),
            "r2_parallel": policy.r2_parallel,
            "parallel_fan_out": phase == "R1" || (phase == "R2" && policy.r2_parallel),
        }),
    )?;
    Ok(accepted)
}

fn load_existing_phase_outputs(
    run_dir: &Path,
    phase: &str,
    policy: &Policy,
) -> anyhow::Result<Vec<LaneAccepted>> {
    let mut outputs = Vec::new();
    for route in &policy.roster {
        let path = run_dir.join(format!("lanes/{phase}/{}/accepted.json", route.route_id));
        if path.exists() {
            outputs.push(LaneAccepted {
                party_id: route.party_id.clone(),
                route_id: route.route_id.clone(),
                output: validate_file(&path, LANE_OUTPUT_SCHEMA)?,
                artifact_ref: format!("lanes/{phase}/{}/accepted.json", route.route_id),
            });
        }
    }
    Ok(outputs)
}

#[allow(clippy::too_many_arguments)]
fn accept_or_retry_lane(
    store: &Store,
    manifest: &mut RunManifest,
    policy: &Policy,
    phase: &str,
    packet_path: &Path,
    run_dir: &Path,
    route: &crate::model::RoutePolicy,
    mut attempt: usize,
    mut outcome: AttemptOutcome,
    accepted: &mut Vec<LaneAccepted>,
) -> anyhow::Result<()> {
    let lane_started = Instant::now();
    let first_attempt = attempt;
    loop {
        if outcome.cancelled || cancellation_requested(run_dir) {
            cancel_run(store, manifest)?;
            bail!("run cancelled");
        }
        if matches!(outcome.output, Some(adapters::AdapterOutput::Arbiter(_))) {
            bail!(
                "internal adapter contract mismatch: {} returned ArbiterVerdict in {phase}",
                route.party_id
            );
        }
        if let Some(adapters::AdapterOutput::Lane(mut output)) = outcome.output {
            let remapped = validate_evidence_refs(&mut output, run_dir)?;
            if !remapped.is_empty() {
                store.event(
                    &manifest.run_id,
                    "evidence.refs_remapped",
                    Some(phase),
                    Some(&route.party_id),
                    Some(attempt),
                    json!({ "remapped": remapped }),
                )?;
            }
            let accepted_path =
                run_dir.join(format!("lanes/{phase}/{}/accepted.json", route.route_id));
            write_json(&accepted_path, &output)?;
            let duration_ms = lane_started.elapsed().as_millis();
            let retries = attempt.saturating_sub(first_attempt);
            store.event(
                &manifest.run_id,
                "lane.accepted",
                Some(phase),
                Some(&route.party_id),
                Some(attempt),
                json!({
                    "route_id": route.route_id,
                    "artifact": relative_slash(accepted_path.strip_prefix(run_dir)?),
                    "duration_ms": duration_ms,
                    "attempts": attempt,
                    "retries": retries,
                    "phase": phase,
                    "timeout_seconds": policy.timeout_for_phase(phase),
                }),
            )?;
            accepted.push(LaneAccepted {
                party_id: route.party_id.clone(),
                route_id: route.route_id.clone(),
                output,
                artifact_ref: format!("lanes/{phase}/{}/accepted.json", route.route_id),
            });
            return Ok(());
        }
        let error = outcome
            .error
            .take()
            .unwrap_or_else(|| "invalid adapter output".to_string());
        if !retry_allowed(outcome.retry, attempt, policy.max_attempts) {
            bail!("{} failed in {phase}: {error}", route.party_id);
        }
        (attempt, outcome) = run_retry_attempt(
            store,
            manifest,
            policy,
            phase,
            packet_path,
            run_dir,
            route,
            attempt,
            outcome.retry,
        )?;
    }
}

#[allow(clippy::too_many_arguments)]
fn await_arbiter_output(
    store: &Store,
    manifest: &mut RunManifest,
    policy: &Policy,
    phase: &str,
    packet_path: &Path,
    run_dir: &Path,
    route: &crate::model::RoutePolicy,
    mut attempt: usize,
    mut outcome: AttemptOutcome,
) -> anyhow::Result<Option<ArbiterVerdict>> {
    loop {
        if outcome.cancelled || cancellation_requested(run_dir) {
            return Ok(None);
        }
        if matches!(outcome.output, Some(adapters::AdapterOutput::Lane(_))) {
            bail!(
                "internal adapter contract mismatch: {} returned LaneOutput in {phase}",
                route.party_id
            );
        }
        if let Some(adapters::AdapterOutput::Arbiter(mut output)) = outcome.output {
            validate_arbiter_semantics(&mut output, run_dir)?;
            return Ok(Some(output));
        }
        let error = outcome
            .error
            .take()
            .unwrap_or_else(|| "invalid arbiter output".to_string());
        if !retry_allowed(outcome.retry, attempt, policy.max_attempts) {
            bail!("{} failed in {phase}: {error}", route.party_id);
        }
        (attempt, outcome) = run_retry_attempt(
            store,
            manifest,
            policy,
            phase,
            packet_path,
            run_dir,
            route,
            attempt,
            outcome.retry,
        )?;
    }
}

#[allow(clippy::too_many_arguments)]
fn run_retry_attempt(
    store: &Store,
    manifest: &mut RunManifest,
    policy: &Policy,
    phase: &str,
    packet_path: &Path,
    run_dir: &Path,
    route: &crate::model::RoutePolicy,
    attempt: usize,
    retry: RetryClass,
) -> anyhow::Result<(usize, AttemptOutcome)> {
    let schedule = retry_schedule(
        retry,
        policy,
        &manifest.run_id,
        phase,
        &route.route_id,
        attempt,
    )?;
    let due_at = Utc::now()
        + ChronoDuration::from_std(schedule.delay)
            .context("retry delay exceeds supported duration")?;
    persist_retry_deadline(
        run_dir,
        phase,
        route,
        attempt,
        due_at,
        retry,
        schedule.source,
    )?;
    // R2 inter-call pacing exists only in the default serial mode; parallel
    // R2 (r2_parallel) replaces pacing with the fan-out soft-stagger, so
    // retries skip the shared pacing state as well.
    let r2_serial_pacing = phase == "R2" && !policy.r2_parallel;
    if r2_serial_pacing {
        let reason = if matches!(retry, RetryClass::RateLimited(_)) {
            "rate_limit_backoff"
        } else {
            "retry_backoff"
        };
        persist_r2_pacing_until(store, manifest, route, attempt, due_at, reason)?;
    }
    store.event(
        &manifest.run_id,
        "lane.retry_scheduled",
        Some(phase),
        Some(&route.party_id),
        Some(attempt),
        json!({
            "route_id": route.route_id,
            "failure_class": retry.failure_class(),
            "source": schedule.source,
            "base_backoff_seconds": policy.retry_backoff_seconds,
            "exponential_backoff_seconds": schedule.backoff_seconds,
            "jitter_milliseconds": schedule.jitter.as_millis(),
            "retry_after_seconds": schedule.retry_after_seconds,
            "delay_milliseconds": schedule.delay.as_millis(),
            "due_at": due_at.to_rfc3339()
        }),
    )?;
    let attempt = attempt
        .checked_add(1)
        .ok_or_else(|| anyhow!("attempt history overflow for {phase}/{}", route.route_id))?;
    wait_for_retry_deadline(store, manifest, phase, route, attempt)?;
    if r2_serial_pacing {
        wait_for_r2_pacing(store, manifest, route, attempt)?;
    }
    store.event(
        &manifest.run_id,
        "lane.retry_started",
        Some(phase),
        Some(&route.party_id),
        Some(attempt),
        json!({"route_id": route.route_id}),
    )?;
    let lane_root = run_dir.join(format!(
        "lanes/{phase}/{}/attempt-{attempt}",
        route.route_id
    ));
    let invocation = adapters::build(
        route,
        phase,
        &manifest.effective_model,
        packet_path,
        &lane_root,
        policy.timeout_seconds,
    )?;
    let outcome = run_attempt(
        store,
        manifest,
        &route.party_id,
        &route.adapter,
        phase,
        attempt,
        invocation,
        policy.timeout_seconds,
        policy.max_output_bytes,
        policy.max_attempts,
    )?;
    if r2_serial_pacing {
        persist_r2_pacing(
            store,
            manifest,
            route,
            attempt,
            policy.r2_min_interval_seconds,
            "inter_call_pacing",
        )?;
    }
    Ok((attempt, outcome))
}

#[allow(clippy::too_many_arguments)]
fn run_attempt(
    store: &Store,
    manifest: &RunManifest,
    party_id: &str,
    adapter: &str,
    phase: &str,
    attempt: usize,
    invocation: Invocation,
    timeout_seconds: u64,
    max_output_bytes: usize,
    max_attempts: usize,
) -> anyhow::Result<AttemptOutcome> {
    let sensitive_cleanup = adapters::SensitiveCleanup::new(&invocation);
    if cancellation_requested(&store.run_dir(&manifest.run_id)?) {
        return Ok(AttemptOutcome {
            output: None,
            error: Some("cancelled".into()),
            cancelled: true,
            retry: RetryClass::Never,
        });
    }
    let lane_root = invocation.cwd.clone();
    create_private_dir_all(&lane_root)?;
    write_json(
        &lane_root.join("invocation.json"),
        &json!({
            "program": invocation.program,
            "args": invocation.args,
            "env_keys": invocation.env.keys().collect::<Vec<_>>(),
            "cwd": invocation.cwd,
            "output_kind": format!("{:?}", invocation.output_kind)
        }),
    )?;
    store.event(
        &manifest.run_id,
        "lane.started",
        Some(phase),
        Some(party_id),
        Some(attempt),
        json!({"adapter": invocation.program}),
    )?;
    let started = Instant::now();
    let (stdout, stderr, exit_code, timed_out, cancelled, output_limit_exceeded) = match &invocation
        .execution
    {
        adapters::Execution::Process => execute_subprocess_attempt(
            &invocation,
            store,
            manifest,
            timeout_seconds,
            max_output_bytes,
        )?,
        adapters::Execution::ChatCompletions(call) => {
            execute_in_process_attempt(call, store, manifest, timeout_seconds, max_output_bytes)?
        }
        adapters::Execution::A2a(call) => {
            execute_a2a_attempt(call, store, manifest, timeout_seconds, max_output_bytes)?
        }
    };
    atomic_write(&lane_root.join("stdout.bin"), &stdout)?;
    atomic_write(&lane_root.join("stderr.bin"), &stderr)?;
    let (mut output, mut error, mut retry) = evaluate_attempt_output(
        adapter,
        invocation.output_kind,
        invocation.contract,
        &stdout,
        &stderr,
        exit_code,
        timed_out,
        cancelled,
        output_limit_exceeded,
        max_output_bytes,
    );
    let mut output_recovered_after_timeout = timed_out && output.is_some();
    if let Some(candidate) = output.as_mut() {
        let validation = match candidate {
            adapters::AdapterOutput::Lane(output) => {
                validate_evidence_refs(output, &store.run_dir(&manifest.run_id)?).map(|_| ())
            }
            adapters::AdapterOutput::Arbiter(verdict) => {
                validate_arbiter_semantics(verdict, &store.run_dir(&manifest.run_id)?).map(|_| ())
            }
        };
        if let Err(validation_error) = validation {
            output = None;
            if timed_out {
                error = Some(format!(
                    "timeout; captured adapter output failed evidence validation: {validation_error}"
                ));
                retry = RetryClass::TransientTimeout;
            } else {
                error = Some(format!(
                    "adapter output failed evidence validation: {validation_error}"
                ));
                retry = RetryClass::Never;
            }
            output_recovered_after_timeout = false;
        }
    }
    let cleanup_error = sensitive_cleanup.finish().err();
    if let Some(cleanup_error) = cleanup_error {
        output = None;
        error = Some(format!(
            "temporary credential cleanup failed: {cleanup_error:#}"
        ));
        retry = RetryClass::Never;
        output_recovered_after_timeout = false;
    }
    store.event(
        &manifest.run_id,
        "lane.finished",
        Some(phase),
        Some(party_id),
        Some(attempt),
        json!({
            "exit_code": exit_code, "timed_out": timed_out, "cancelled": cancelled,
            "accepted": output.is_some(), "error": error,
            "output_recovered_after_timeout": output_recovered_after_timeout,
            "failure_class": output.is_none().then(|| retry.failure_class()),
            "retryable": output.is_none() && retry_allowed(retry, attempt, max_attempts),
            "stdout_bytes": stdout.len(), "stderr_bytes": stderr.len(),
            "duration_ms": started.elapsed().as_millis()
        }),
    )?;
    Ok(AttemptOutcome {
        output,
        error,
        cancelled,
        retry,
    })
}

/// Runs the subprocess half of `run_attempt`: spawn, register, capture, and
/// wait, returning the raw streams and terminal flags for evaluation.
#[allow(clippy::type_complexity)]
fn execute_subprocess_attempt(
    invocation: &Invocation,
    store: &Store,
    manifest: &RunManifest,
    timeout_seconds: u64,
    max_output_bytes: usize,
) -> anyhow::Result<(Vec<u8>, Vec<u8>, Option<i32>, bool, bool, bool)> {
    let mut command = adapters::spawn_command(invocation);
    let registration = store.process_registration(&manifest.run_id)?;
    if registration.cancellation_requested() {
        return Ok((Vec::new(), Vec::new(), None, false, true, false));
    }
    let (spawned, job) = spawn_adapter_process(&mut command)
        .with_context(|| format!("cannot start {}", invocation.program))?;
    let mut child = ChildCleanup::new(spawned, store, &manifest.run_id);
    #[cfg(windows)]
    if let Some(job) = job {
        child.attach_job(job);
    }
    #[cfg(not(windows))]
    let _ = job;
    let pid = child.child.id();
    if let Some(identity) = process_identity(pid) {
        let active_process = ActiveProcess {
            pid,
            identity,
            program: invocation.program.clone(),
        };
        registration.add_process(active_process.clone())?;
        child.mark_registered(active_process);
    } else if child_exited_without_reaping(&mut child.child)? {
        #[cfg(windows)]
        kill_residual_process_group(pid, child.job.as_ref());
        #[cfg(not(windows))]
        kill_residual_process_group(pid);
    } else {
        bail!("cannot identify spawned adapter process");
    }
    drop(registration);
    let mut stdout_reader = child
        .child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("stdout pipe missing"))?;
    let mut stderr_reader = child
        .child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("stderr pipe missing"))?;
    let total_output = Arc::new(AtomicUsize::new(0));
    let output_limited = Arc::new(AtomicBool::new(false));
    let stdout_total = total_output.clone();
    let stdout_limited = output_limited.clone();
    let (stdout_sender, stdout_receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = stdout_sender.send(read_capped(
            &mut stdout_reader,
            max_output_bytes,
            &stdout_total,
            &stdout_limited,
        ));
    });
    let stderr_total = total_output.clone();
    let stderr_limited = output_limited.clone();
    let (stderr_sender, stderr_receiver) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let _ = stderr_sender.send(read_capped(
            &mut stderr_reader,
            max_output_bytes,
            &stderr_total,
            &stderr_limited,
        ));
    });
    let (status, timed_out, cancelled, output_limit_exceeded) = wait_child(
        &mut child.child,
        timeout_seconds,
        &store.run_dir(&manifest.run_id)?,
        &output_limited,
        #[cfg(windows)]
        child.job.as_ref(),
        #[cfg(not(windows))]
        None,
    )?;
    child.tree_cleaned = true;
    // Drop the job after the leader has been waited so kill-on-close reaps any
    // descendants that outlived the leader.
    #[cfg(windows)]
    child.job.take();
    let stdout = receive_captured_output(stdout_receiver, "stdout")?;
    let stderr = receive_captured_output(stderr_receiver, "stderr")?;
    child.unregister()?;
    let output_limit_exceeded = output_limit_exceeded || output_limited.load(Ordering::SeqCst);
    let exit_code = status.and_then(|status| status.code());
    Ok((
        stdout,
        stderr,
        exit_code,
        timed_out,
        cancelled,
        output_limit_exceeded,
    ))
}

/// Runs the in-process half of `run_attempt`: the provider call executes on a
/// worker thread so cancellation and the phase deadline are still observed;
/// a timed-out or cancelled worker is bounded by its own request timeout.
#[allow(clippy::type_complexity)]
fn execute_a2a_attempt(
    call: &adapters::A2aCall,
    store: &Store,
    manifest: &RunManifest,
    timeout_seconds: u64,
    max_output_bytes: usize,
) -> anyhow::Result<(Vec<u8>, Vec<u8>, Option<i32>, bool, bool, bool)> {
    let run_dir = store.run_dir(&manifest.run_id)?;
    let (sender, receiver) = mpsc::channel();
    let (gate_signal, gate_acquired) = mpsc::channel();
    let call = call.clone();
    thread::spawn(move || {
        let _ = sender.send(adapters::execute_a2a_call_signaled(
            &call,
            max_output_bytes,
            Some(gate_signal),
        ));
    });
    await_a2a_worker(&receiver, &gate_acquired, &run_dir, timeout_seconds)
}

/// Wait loop shared by the A2A seat attempt: cancellation stays responsive,
/// and the lane deadline starts only when the worker reports it acquired the
/// bounded process-wide seat gate — a few seat calls run concurrently (shared
/// key), so a queued lane must not burn its timeout budget before its
/// request is on the wire.
#[allow(clippy::type_complexity)]
fn await_a2a_worker(
    receiver: &mpsc::Receiver<adapters::ChatCompletionsOutcome>,
    gate_acquired: &mpsc::Receiver<()>,
    run_dir: &std::path::Path,
    timeout_seconds: u64,
) -> anyhow::Result<(Vec<u8>, Vec<u8>, Option<i32>, bool, bool, bool)> {
    let mut deadline: Option<Instant> = None;
    let outcome = loop {
        match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(outcome) => break outcome,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancellation_requested(run_dir) {
                    return Ok((
                        Vec::new(),
                        b"cancelled while awaiting the A2A seat".to_vec(),
                        None,
                        false,
                        true,
                        false,
                    ));
                }
                if deadline.is_none() && gate_acquired.try_recv().is_ok() {
                    deadline = Some(Instant::now() + Duration::from_secs(timeout_seconds.max(1)));
                }
                if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    return Ok((Vec::new(), Vec::new(), None, true, false, false));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("A2A seat worker terminated unexpectedly")
            }
        }
    };
    Ok((
        outcome.stdout,
        outcome.stderr,
        outcome.exit_code,
        outcome.timed_out,
        false,
        outcome.output_limit_exceeded,
    ))
}

fn execute_in_process_attempt(
    call: &adapters::ChatCompletionsCall,
    store: &Store,
    manifest: &RunManifest,
    timeout_seconds: u64,
    max_output_bytes: usize,
) -> anyhow::Result<(Vec<u8>, Vec<u8>, Option<i32>, bool, bool, bool)> {
    let run_dir = store.run_dir(&manifest.run_id)?;
    let (sender, receiver) = mpsc::channel();
    let call = call.clone();
    thread::spawn(move || {
        let _ = sender.send(adapters::execute_chat_completions(&call, max_output_bytes));
    });
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    let outcome = loop {
        match receiver.recv_timeout(Duration::from_millis(500)) {
            Ok(outcome) => break outcome,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancellation_requested(&run_dir) {
                    return Ok((
                        Vec::new(),
                        b"cancelled while awaiting the provider response".to_vec(),
                        None,
                        false,
                        true,
                        false,
                    ));
                }
                if Instant::now() >= deadline {
                    return Ok((Vec::new(), Vec::new(), None, true, false, false));
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                bail!("in-process adapter worker terminated unexpectedly")
            }
        }
    };
    Ok((
        outcome.stdout,
        outcome.stderr,
        outcome.exit_code,
        outcome.timed_out,
        false,
        outcome.output_limit_exceeded,
    ))
}

#[allow(clippy::too_many_arguments)]
fn evaluate_attempt_output(
    adapter: &str,
    output_kind: adapters::OutputKind,
    output_contract: adapters::OutputContract,
    stdout: &[u8],
    stderr: &[u8],
    exit_code: Option<i32>,
    timed_out: bool,
    cancelled: bool,
    output_limit_exceeded: bool,
    max_output_bytes: usize,
) -> (Option<adapters::AdapterOutput>, Option<String>, RetryClass) {
    if cancelled {
        return (None, Some("cancelled".into()), RetryClass::Never);
    }
    if output_limit_exceeded {
        return (
            None,
            Some(format!(
                "adapter output exceeds policy limit of {max_output_bytes} bytes"
            )),
            RetryClass::Never,
        );
    }
    if timed_out {
        return match adapters::parse_typed_output_with_limit(
            output_kind,
            output_contract,
            stdout,
            max_output_bytes,
        ) {
            Ok(output) => (Some(output), None, RetryClass::Never),
            Err(_) => (None, Some("timeout".into()), RetryClass::TransientTimeout),
        };
    }
    if exit_code != Some(0) {
        return if let Some(signal) = classify_rate_limit(adapter, stdout, stderr) {
            (
                None,
                Some(format!(
                    "adapter transport was rate limited ({})",
                    signal.source
                )),
                RetryClass::RateLimited(signal),
            )
        } else {
            // A failed a2a seat task — contract violation, provider
            // transport blip, seat-side error — has produced no evidence;
            // regeneration faces the same closed-schema acceptance, so a
            // bounded retry weakens nothing. Any other non-zero exit
            // stays fail-closed.
            let seat_task_failed = std::str::from_utf8(stderr)
                .map(|s| s.contains("a2a seat task"))
                .unwrap_or(false);
            let retry = if seat_task_failed {
                RetryClass::TransientAdapter
            } else {
                RetryClass::Never
            };
            (
                None,
                Some(format!("adapter exited {:?}", exit_code)),
                retry,
            )
        };
    }
    match adapters::parse_typed_output_with_limit(
        output_kind,
        output_contract,
        stdout,
        max_output_bytes,
    ) {
        Ok(output) => (Some(output), None, RetryClass::Never),
        Err(parse_error) => {
            let event_stream_unusable = output_kind == adapters::OutputKind::JsonEvents
                && adapters::events_completed_with_unusable_final_candidate(stdout);
            let envelope_unusable = output_kind == adapters::OutputKind::EnvelopeJson
                && envelope_completed_with_unusable_result(stdout);
            let truncated_completion = matches!(adapter, "codewhale" | "fake_codewhale")
                && adapters::codewhale_completed_with_retryable_content(stdout)
                || event_stream_unusable
                || envelope_unusable;
            let retry = if truncated_completion {
                RetryClass::TransientAdapter
            } else {
                RetryClass::Never
            };
            (None, Some(parse_error.to_string()), retry)
        }
    }
}

fn envelope_completed_with_unusable_result(stdout: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(stdout) else {
        return false;
    };
    if value.get("is_error").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    match value
        .get("structured_output")
        .or_else(|| value.get("result"))
    {
        None | Some(Value::Null) => true,
        Some(Value::String(text)) => {
            text.trim().is_empty()
                || !text.contains('{')
                || adapters::events_completed_with_unusable_final_candidate(text.as_bytes())
        }
        Some(_) => false,
    }
}

fn classify_rate_limit(adapter: &str, stdout: &[u8], stderr: &[u8]) -> Option<RateLimitSignal> {
    let known_adapter = adapter == "deepseek" || adapter == "qwen" || adapter == "a2a";
    #[cfg(feature = "test-adapters")]
    let known_adapter = known_adapter || adapter == "fake";
    if !known_adapter {
        return None;
    }
    if let Some(retry_after_seconds) = structured_rate_limit(stdout) {
        return Some(RateLimitSignal {
            source: "adapter_structured_error",
            retry_after_seconds,
        });
    }
    let stderr = std::str::from_utf8(stderr).ok()?;
    let normalized = stderr.to_ascii_lowercase();
    let marker = [
        "http 429",
        "http/1.1 429",
        "http/2 429",
        "status=429",
        "status_code=429",
        "status code: 429",
        "too many requests",
        // The A2A seat adapter surfaces the seat's provider verdict
        // verbatim ("provider returned 429 after 5 attempts: ...",
        // Chinese rate-limit wording included).
        "provider returned 429",
        "rate limit",
        "速率限制",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));
    marker.then(|| RateLimitSignal {
        source: "adapter_stderr_marker",
        retry_after_seconds: parse_retry_after(stderr),
    })
}

fn structured_rate_limit(stdout: &[u8]) -> Option<Option<u64>> {
    let text = std::str::from_utf8(stdout).ok()?;
    let values = if let Ok(value) = serde_json::from_str::<Value>(text) {
        vec![value]
    } else {
        text.lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .collect()
    };
    values.iter().find_map(find_structured_rate_limit)
}

fn find_structured_rate_limit(value: &Value) -> Option<Option<u64>> {
    if let Some(values) = value.as_array() {
        return values.iter().find_map(find_structured_rate_limit);
    }
    let object = value.as_object()?;
    let numeric_429 = ["status", "status_code", "http_status"]
        .iter()
        .any(|key| object.get(*key).and_then(Value::as_u64) == Some(429));
    let typed_429 = ["code", "type", "error_code"]
        .iter()
        .filter_map(|key| object.get(*key).and_then(Value::as_str))
        .any(|code| {
            matches!(
                code,
                "rate_limit_error" | "rate_limited" | "too_many_requests" | "resource_exhausted"
            )
        });
    if numeric_429 || typed_429 {
        let retry_after = ["retry_after", "retry_after_seconds"]
            .iter()
            .find_map(|key| object.get(*key).and_then(Value::as_u64));
        return Some(retry_after);
    }
    object.values().find_map(find_structured_rate_limit)
}

fn parse_retry_after(stderr: &str) -> Option<u64> {
    stderr.lines().find_map(|line| {
        let (name, value) = line.trim().split_once(':')?;
        name.eq_ignore_ascii_case("retry-after")
            .then(|| value.trim().parse::<u64>().ok())
            .flatten()
    })
}

fn wait_child(
    child: &mut Child,
    timeout_seconds: u64,
    run_dir: &Path,
    output_limited: &AtomicBool,
    #[cfg(windows)] job: Option<&WindowsJob>,
    #[cfg(not(windows))] job: Option<&()>,
) -> anyhow::Result<(Option<ExitStatus>, bool, bool, bool)> {
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    loop {
        if child_exited_without_reaping(child)? {
            kill_residual_process_group_with_job(child.id(), job);
            let status = child.wait()?;
            return Ok((
                Some(status),
                false,
                false,
                output_limited.load(Ordering::SeqCst),
            ));
        }
        if cancellation_requested(run_dir) {
            let status = terminate_child(child, Duration::from_millis(500), job)
                .ok_or_else(|| anyhow!("cannot terminate cancelled adapter process"))?;
            return Ok((Some(status), false, true, false));
        }
        if output_limited.load(Ordering::SeqCst) {
            let status = terminate_child(child, Duration::from_millis(100), job)
                .ok_or_else(|| anyhow!("cannot terminate output-limited adapter process"))?;
            return Ok((Some(status), false, false, true));
        }
        if Instant::now() >= deadline {
            let status = terminate_child(child, Duration::from_millis(500), job)
                .ok_or_else(|| anyhow!("cannot terminate timed-out adapter process"))?;
            return Ok((Some(status), true, false, false));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn terminate_child(
    child: &mut Child,
    grace: Duration,
    #[cfg(windows)] job: Option<&WindowsJob>,
    #[cfg(not(windows))] job: Option<&()>,
) -> Option<ExitStatus> {
    let pid = child.id();
    kill_process_tree_with_job(pid, false, job);
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if child_exited_without_reaping(child).ok()? {
            kill_residual_process_group_with_job(pid, job);
            return child.wait().ok();
        }
        thread::sleep(Duration::from_millis(25));
    }
    kill_process_tree_with_job(pid, true, job);
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child_exited_without_reaping(child).ok()? {
            kill_residual_process_group_with_job(pid, job);
            return child.wait().ok();
        }
        thread::sleep(Duration::from_millis(25));
    }
    // A failed process-group signal must never turn a bounded lane timeout into
    // an unbounded scheduler wait.
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if child_exited_without_reaping(child).ok()? {
            kill_residual_process_group_with_job(pid, job);
            return child.wait().ok();
        }
        thread::sleep(Duration::from_millis(25));
    }
    kill_residual_process_group_with_job(pid, job);
    None
}

#[cfg(windows)]
fn kill_process_tree_with_job(pid: u32, force: bool, job: Option<&WindowsJob>) {
    if let Some(job) = job {
        // Prefer job-wide termination so grandchildren without the leader PID die.
        job.terminate();
        if force {
            kill_process_tree(pid, true);
        }
        return;
    }
    kill_process_tree(pid, force);
}

#[cfg(not(windows))]
fn kill_process_tree_with_job(pid: u32, force: bool, _job: Option<&()>) {
    kill_process_tree(pid, force);
}

#[cfg(windows)]
fn kill_residual_process_group_with_job(pid: u32, job: Option<&WindowsJob>) {
    if let Some(job) = job {
        job.terminate();
        return;
    }
    // Fallback when no owned job is available (e.g. orphan recovery).
    kill_process_tree(pid, true);
}

#[cfg(not(windows))]
fn kill_residual_process_group_with_job(pid: u32, _job: Option<&()>) {
    kill_residual_process_group(pid);
}

#[cfg(windows)]
fn kill_residual_process_group(pid: u32, job: Option<&WindowsJob>) {
    kill_residual_process_group_with_job(pid, job);
}

#[cfg(unix)]
fn child_exited_without_reaping(child: &mut Child) -> std::io::Result<bool> {
    let mut siginfo = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id() as libc::id_t,
            siginfo.as_mut_ptr(),
            libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let siginfo = unsafe { siginfo.assume_init() };
    match siginfo.si_signo {
        libc::SIGCHLD => Ok(true),
        0 => Ok(false),
        signal => Err(std::io::Error::other(format!(
            "waitid returned unexpected signal {signal}"
        ))),
    }
}

#[cfg(windows)]
fn child_exited_without_reaping(child: &mut Child) -> std::io::Result<bool> {
    child.try_wait().map(|status| status.is_some())
}

fn read_capped(
    reader: &mut impl Read,
    limit: usize,
    total: &AtomicUsize,
    exceeded: &AtomicBool,
) -> std::io::Result<Vec<u8>> {
    let mut captured = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let count = reader.read(&mut chunk)?;
        if count == 0 {
            return Ok(captured);
        }
        let previous = total.fetch_add(count, Ordering::SeqCst);
        if previous < limit {
            let retained = count.min(limit - previous);
            captured.extend_from_slice(&chunk[..retained]);
        }
        if previous.saturating_add(count) > limit {
            exceeded.store(true, Ordering::SeqCst);
        }
    }
}

fn receive_captured_output(
    receiver: mpsc::Receiver<std::io::Result<Vec<u8>>>,
    stream: &str,
) -> anyhow::Result<Vec<u8>> {
    receiver
        .recv_timeout(Duration::from_secs(5))
        .with_context(|| format!("{stream} reader did not close after child termination"))?
        .with_context(|| format!("cannot read adapter {stream}"))
}

/// R1 contestation predicate — pure code over lane outputs, never model
/// judgment (RASHOMON Shomon: the divergence gate decides what gets
/// re-examined; Kyomon's mechanical backbone is the attestation rule).
///
/// A lane index is contested when:
/// 1. two lanes disagree on the normalized verdict/recommendation, or
/// 2. two lanes disagree on the disposition of the same residual id, or
/// 3. a lane's confidence is below the unsupported floor, or
/// 4. (attestation symmetry) a residual id is attested by a strict
///    majority of lanes but omitted by others — the omitting lanes
///    likely misread the packet, and the attesting lanes must be
///    re-examined too so the residual itself faces cross-examination.
///    School-specific findings (attested by a minority) are NOT
///    contested: different lenses legitimately raise different
///    residuals.
const UNSUPPORTED_CONFIDENCE_FLOOR: f64 = 0.5;

fn r1_contestation(r1: &[LaneAccepted]) -> Vec<usize> {
    let n = r1.len();
    if n == 0 {
        return Vec::new();
    }
    let mut contested = vec![false; n];

    // Rule 1: verdict disagreement.
    for i in 0..n {
        for j in (i + 1)..n {
            if r1[i].output.verdict.trim() != r1[j].output.verdict.trim() {
                contested[i] = true;
                contested[j] = true;
            }
        }
    }

    // Rule 2: disposition conflict on the same residual id.
    for i in 0..n {
        for j in (i + 1)..n {
            let mut by_id = std::collections::BTreeMap::<&str, (Disposition, Disposition)>::new();
            for residual in &r1[i].output.residuals {
                by_id.insert(residual.id.as_str(), (residual.disposition, residual.disposition));
            }
            for residual in &r1[j].output.residuals {
                if let Some(entry) = by_id.get_mut(residual.id.as_str()) {
                    entry.1 = residual.disposition;
                }
            }
            for (_, (a, b)) in by_id {
                if a != b {
                    contested[i] = true;
                    contested[j] = true;
                    break;
                }
            }
        }
    }

    // Rule 3: unsupported confidence.
    for (i, lane) in r1.iter().enumerate() {
        if lane.output.confidence < UNSUPPORTED_CONFIDENCE_FLOOR {
            contested[i] = true;
        }
    }

    // Rule 4: attestation symmetry (majority-attested, minority-omitted).
    let mut attestations: std::collections::BTreeMap<&str, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, lane) in r1.iter().enumerate() {
        for residual in &lane.output.residuals {
            attestations
                .entry(residual.id.as_str())
                .or_default()
                .push(i);
        }
    }
    let majority = n / 2 + 1;
    for lanes in attestations.values() {
        if lanes.len() >= majority && lanes.len() < n {
            for &i in lanes {
                contested[i] = true;
            }
            for i in 0..n {
                if !lanes.contains(&i) {
                    contested[i] = true;
                }
            }
        }
    }

    contested
        .into_iter()
        .enumerate()
        .filter_map(|(i, c)| c.then_some(i))
        .collect()
}

/// Per-lane identity tokens that must never survive into an R2 packet.
/// Route ids, party ids, and perspective texts are unique per lane, so any
/// of them appearing in lane-authored prose would undo the label rotation;
/// model names are shared across the roster under the single-vendor
/// doctrine but are withheld from the packet all the same. Shared tokens
/// (adapter, executable, family, provider, seat) name the vendor, not a
/// participant, and are left to the packet's structural omissions.
fn r2_identity_replacements(policy: &Policy) -> Vec<(String, &'static str)> {
    let mut tokens: Vec<(String, &'static str)> = Vec::new();
    let mut routes: Vec<&crate::model::RoutePolicy> = policy.roster.iter().collect();
    routes.push(&policy.counterpart_arbiter);
    routes.push(&policy.primary_arbiter);
    for route in routes {
        tokens.push((route.route_id.clone(), "[route]"));
        tokens.push((route.party_id.clone(), "[party]"));
        if !route.perspective.is_empty() {
            tokens.push((route.perspective.clone(), "[perspective]"));
        }
    }
    tokens.push((policy.seat.text_model.clone(), "[model]"));
    tokens.push((policy.seat.multimodal_model.clone(), "[model]"));
    tokens.retain(|(token, _)| token.chars().count() >= 3);
    tokens
}

/// Replace one identity token anywhere in the text, case-insensitively for
/// ASCII tokens (ASCII lowercasing preserves byte offsets, so slicing stays
/// valid inside prose of any language).
fn replace_identity_token(text: &str, token: &str, replacement: &str) -> String {
    if token.is_empty() {
        return text.to_string();
    }
    if token.is_ascii() {
        let token_lower = token.to_ascii_lowercase();
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(index) = rest.to_ascii_lowercase().find(&token_lower) {
            out.push_str(&rest[..index]);
            out.push_str(replacement);
            rest = &rest[index + token.len()..];
        }
        out.push_str(rest);
        out
    } else {
        text.replace(token, replacement)
    }
}

/// Walk every string in a serialized lane output and scrub the identity
/// tokens. Numeric and structural fields pass through unchanged.
fn scrub_identity_markers(value: &Value, tokens: &[(String, &'static str)]) -> Value {
    match value {
        Value::String(text) => {
            let mut scrubbed = text.clone();
            for (token, replacement) in tokens {
                scrubbed = replace_identity_token(&scrubbed, token, replacement);
            }
            Value::String(scrubbed)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| scrub_identity_markers(item, tokens))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), scrub_identity_markers(item, tokens)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn scrub_lane_output(
    output: &LaneOutput,
    tokens: &[(String, &'static str)],
) -> anyhow::Result<LaneOutput> {
    let value = serde_json::to_value(output)?;
    Ok(serde_json::from_value(scrub_identity_markers(
        &value, tokens,
    ))?)
}

fn build_r2_packet(
    manifest: &RunManifest,
    brief: &Brief,
    r1: &[LaneAccepted],
    policy: &Policy,
) -> anyhow::Result<R2Packet> {
    let mut labels = vec![
        "Participant A",
        "Participant B",
        "Participant C",
        "Participant D",
        "Participant E",
    ];
    // The rotation must not be derivable from anything inside the packet:
    // the run_id is part of the wire payload, so seeding from it would let
    // every reader invert the participant-to-lane mapping deterministically.
    // A random rotation is computed once here and the packet is persisted
    // (packets/r2.json) before any R2 lane starts, so restarts reuse it.
    // The rotation is pseudonymity, never content anonymization: lane
    // prose is carried verbatim except for per-lane identity markers,
    // which are scrubbed below (see the trial_manifest controls).
    let rotation = rand::rng().random_range(0..labels.len());
    labels.rotate_left(rotation);
    let contested = r1_contestation(r1);
    let tokens = r2_identity_replacements(policy);
    let participants = labels
        .into_iter()
        .zip(r1.iter().enumerate())
        .filter(|(_, (index, _))| contested.contains(index))
        .map(|(label, (_, lane))| {
            Ok::<(String, LaneOutput), anyhow::Error>((
                label.to_string(),
                scrub_lane_output(&lane.output, &tokens)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(R2Packet {
        packet_version: R2_PACKET_VERSION.into(),
        run_id: manifest.run_id.clone(),
        question: brief.question.clone(),
        participants,
        evidence_manifest_sha256: manifest.snapshot_sha256.clone(),
    })
}

fn create_primary_arbiter_challenge(
    manifest: &RunManifest,
    brief: &Brief,
    evidence_packet_path: &Path,
) -> anyhow::Result<PrimaryArbiterChallenge> {
    let input_receipt_sha256 = manifest
        .r3_input_receipt
        .as_ref()
        .ok_or_else(|| anyhow!("R3 input receipt is missing"))?
        .sha256
        .clone();
    let mut nonce = [0_u8; 32];
    rand::rng().fill_bytes(&mut nonce);
    Ok(PrimaryArbiterChallenge {
        challenge_version: PRIMARY_ARBITER_CHALLENGE_VERSION.into(),
        run_id: manifest.run_id.clone(),
        nonce: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce),
        policy_sha256: manifest.policy_sha256.clone(),
        evidence_packet_sha256: sha256_file(evidence_packet_path)?,
        input_receipt_sha256,
        action_scope: brief.action_scope.clone(),
        issued_at: utc_now(),
        expires_at: (Utc::now() + ChronoDuration::hours(24)).to_rfc3339(),
        consumed: false,
    })
}

fn validate_primary_arbiter_response_binding(
    manifest: &RunManifest,
    response: &PrimaryArbiterResponse,
    brief: &Brief,
    evidence_packet_path: &Path,
) -> anyhow::Result<()> {
    let challenge = manifest
        .primary_arbiter_challenge
        .as_ref()
        .ok_or_else(|| anyhow!("primary arbiter challenge is missing"))?;
    let expected = (
        &challenge.run_id,
        &challenge.nonce,
        &challenge.policy_sha256,
        &challenge.evidence_packet_sha256,
        &challenge.input_receipt_sha256,
        &challenge.action_scope,
    );
    let actual = (
        &response.run_id,
        &response.nonce,
        &response.policy_sha256,
        &response.evidence_packet_sha256,
        &response.input_receipt_sha256,
        &response.action_scope,
    );
    if expected != actual
        || response.run_id != manifest.run_id
        || response.policy_sha256 != manifest.policy_sha256
        || response.evidence_packet_sha256 != sha256_file(evidence_packet_path)?
        || manifest
            .r3_input_receipt
            .as_ref()
            .is_none_or(|binding| response.input_receipt_sha256 != binding.sha256)
        || response.action_scope != brief.action_scope
    {
        bail!("primary-arbiter response does not bind the active challenge");
    }
    Ok(())
}

fn validate_new_primary_arbiter_response(
    manifest: &RunManifest,
    response: &PrimaryArbiterResponse,
    brief: &Brief,
    evidence_packet_path: &Path,
) -> anyhow::Result<()> {
    validate_primary_arbiter_response_binding(manifest, response, brief, evidence_packet_path)?;
    let challenge = manifest.primary_arbiter_challenge.as_ref().unwrap();
    if challenge.consumed {
        bail!("primary arbiter challenge was already consumed");
    }
    let expiry = chrono::DateTime::parse_from_rfc3339(&challenge.expires_at)?;
    if expiry < Utc::now() {
        bail!("primary arbiter challenge expired");
    }
    Ok(())
}

fn ensure_r3_input_receipt(
    manifest: &mut RunManifest,
    policy: &Policy,
    r1: &[LaneAccepted],
    r2: &[LaneAccepted],
    evidence_packet_path: &Path,
    cc_path: &Path,
    run_dir: &Path,
) -> anyhow::Result<()> {
    if manifest.r3_input_receipt.is_some() {
        return verify_r3_input_receipt(manifest, policy, run_dir);
    }

    let receipt = R3InputReceipt {
        input_receipt_version: R3_INPUT_RECEIPT_VERSION.into(),
        run_id: manifest.run_id.clone(),
        issued_at: utc_now(),
        r1: phase_artifact_bindings(r1, run_dir)?,
        r2: phase_artifact_bindings(r2, run_dir)?,
        evidence_packet: artifact_binding(evidence_packet_path, run_dir)?,
        cc_response: artifact_binding(cc_path, run_dir)?,
    };
    validate_value(&serde_json::to_value(&receipt)?, R3_INPUT_RECEIPT_SCHEMA)?;
    let receipt_path = run_dir.join("r3/input-receipt.json");
    write_json(&receipt_path, &receipt)?;
    manifest.r3_input_receipt = Some(ArtifactBinding {
        artifact_ref: "r3/input-receipt.json".into(),
        sha256: sha256_file(&receipt_path)?,
    });
    verify_r3_input_receipt(manifest, policy, run_dir)
}

fn phase_artifact_bindings(
    lanes: &[LaneAccepted],
    run_dir: &Path,
) -> anyhow::Result<Vec<LaneArtifactBinding>> {
    lanes
        .iter()
        .map(|lane| {
            let path = run_dir.join(&lane.artifact_ref);
            Ok(LaneArtifactBinding {
                party_id: lane.party_id.clone(),
                route_id: lane.route_id.clone(),
                artifact_ref: lane.artifact_ref.clone(),
                sha256: sha256_file(&path)?,
            })
        })
        .collect()
}

fn artifact_binding(path: &Path, run_dir: &Path) -> anyhow::Result<ArtifactBinding> {
    Ok(ArtifactBinding {
        artifact_ref: relative_slash(path.strip_prefix(run_dir)?),
        sha256: sha256_file(path)?,
    })
}

fn verify_r3_input_receipt(
    manifest: &RunManifest,
    policy: &Policy,
    run_dir: &Path,
) -> anyhow::Result<()> {
    let binding = manifest
        .r3_input_receipt
        .as_ref()
        .ok_or_else(|| anyhow!("R3 input receipt binding is missing"))?;
    if binding.artifact_ref != "r3/input-receipt.json" {
        bail!("R3 input receipt has an invalid artifact reference");
    }
    let receipt_path = run_dir.join(&binding.artifact_ref);
    if sha256_file(&receipt_path)? != binding.sha256 {
        bail!("R3 input receipt changed after scheduler acceptance");
    }
    let receipt: R3InputReceipt = validate_file(&receipt_path, R3_INPUT_RECEIPT_SCHEMA)?;
    if receipt.run_id != manifest.run_id {
        bail!("R3 input receipt belongs to another run");
    }
    verify_phase_bindings(&receipt.r1, "R1", policy, run_dir)?;
    verify_phase_bindings(&receipt.r2, "R2", policy, run_dir)?;
    verify_artifact_binding(&receipt.evidence_packet, "r3/evidence-packet.json", run_dir)?;
    verify_artifact_binding(&receipt.cc_response, "r3/cc-response.json", run_dir)?;
    Ok(())
}

fn verify_phase_bindings(
    bindings: &[LaneArtifactBinding],
    phase: &str,
    policy: &Policy,
    run_dir: &Path,
) -> anyhow::Result<()> {
    if phase == "R2" && run_dir.join("r2/skipped.json").exists() {
        if !bindings.is_empty() {
            bail!("R2 was skipped but the input receipt still lists R2 lanes");
        }
        return Ok(());
    }
    if bindings.len() != policy.roster.len() {
        bail!("{phase} input receipt has the wrong lane count");
    }
    for route in &policy.roster {
        let binding = bindings
            .iter()
            .find(|binding| binding.route_id == route.route_id)
            .ok_or_else(|| anyhow!("{phase} input receipt is missing {}", route.route_id))?;
        let expected_ref = format!("lanes/{phase}/{}/accepted.json", route.route_id);
        if binding.party_id != route.party_id || binding.artifact_ref != expected_ref {
            bail!(
                "{phase} input receipt route binding changed for {}",
                route.route_id
            );
        }
        verify_artifact_binding(
            &ArtifactBinding {
                artifact_ref: binding.artifact_ref.clone(),
                sha256: binding.sha256.clone(),
            },
            &expected_ref,
            run_dir,
        )?;
    }
    Ok(())
}

fn verify_artifact_binding(
    binding: &ArtifactBinding,
    expected_ref: &str,
    run_dir: &Path,
) -> anyhow::Result<()> {
    if binding.artifact_ref != expected_ref {
        bail!("R3 artifact binding changed: expected {expected_ref}");
    }
    if sha256_file(&run_dir.join(expected_ref))? != binding.sha256 {
        bail!("R3 accepted artifact changed: {expected_ref}");
    }
    Ok(())
}

fn recover_primary_arbiter_submission(
    manifest: &mut RunManifest,
    brief: &Brief,
    run_dir: &Path,
) -> anyhow::Result<bool> {
    let Some(receipt) = manifest.primary_arbiter_submission.as_ref() else {
        return Ok(false);
    };
    let response_path = run_dir.join(&receipt.response_ref);
    if !response_path.exists() {
        if receipt.state == PrimaryArbiterSubmissionState::Staging {
            return Ok(false);
        }
        bail!("accepted primary-arbiter response artifact is missing");
    }
    if sha256_file(&response_path)? != receipt.response_sha256 {
        bail!("primary-arbiter response changed after scheduler staging");
    }
    let input_receipt = manifest
        .r3_input_receipt
        .as_ref()
        .ok_or_else(|| anyhow!("R3 input receipt is missing"))?;
    if receipt.input_receipt_sha256 != input_receipt.sha256 {
        bail!("primary arbiter submission does not bind the accepted R3 inputs");
    }
    let response = read_owned_primary_arbiter_response(manifest, run_dir)?;
    validate_primary_arbiter_response_binding(
        manifest,
        &response,
        brief,
        &run_dir.join("r3/evidence-packet.json"),
    )?;

    if receipt.state == PrimaryArbiterSubmissionState::Staging {
        let receipt = manifest.primary_arbiter_submission.as_mut().unwrap();
        receipt.state = PrimaryArbiterSubmissionState::Accepted;
        receipt.accepted_at = Some(utc_now());
        manifest
            .primary_arbiter_challenge
            .as_mut()
            .unwrap()
            .consumed = true;
    } else if !manifest
        .primary_arbiter_challenge
        .as_ref()
        .is_some_and(|challenge| challenge.consumed)
    {
        bail!("accepted primary arbiter receipt has an unconsumed challenge");
    }
    Ok(true)
}

fn read_owned_primary_arbiter_response(
    manifest: &RunManifest,
    run_dir: &Path,
) -> anyhow::Result<PrimaryArbiterResponse> {
    let receipt = manifest
        .primary_arbiter_submission
        .as_ref()
        .ok_or_else(|| anyhow!("primary-arbiter submission receipt is missing"))?;
    let (schema, unexpected_ref) = match receipt.response_ref.as_str() {
        "r3/primary-arbiter-response.json" => {
            (PRIMARY_ARBITER_RESPONSE_SCHEMA, "r3/hm-response.json")
        }
        "r3/hm-response.json" => (
            LEGACY_HM_RESPONSE_SCHEMA,
            "r3/primary-arbiter-response.json",
        ),
        _ => bail!("primary-arbiter submission has an invalid artifact reference"),
    };
    if run_dir.join(unexpected_ref).exists() {
        bail!("primary-arbiter submission has ambiguous response artifacts");
    }
    validate_file(&run_dir.join(&receipt.response_ref), schema)
}

fn response_bytes_with_newline(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.push(b'\n');
    bytes
}

fn residual_fields_conflict(left: &Residual, right: &Residual) -> bool {
    left.disposition != right.disposition
        || left.closure_state != right.closure_state
        || left.finding != right.finding
        || left.severity != right.severity
        || left.residual_type != right.residual_type
        || left.source != right.source
        || left.evidence_refs != right.evidence_refs
        || left.required_closure != right.required_closure
        || left.scope != right.scope
        || left.closure_evidence != right.closure_evidence
}

fn is_high_risk_severity(severity: Severity) -> bool {
    matches!(severity, Severity::High | Severity::Critical | Severity::P0)
}

fn conservative_unresolved(mut residual: Residual) -> Residual {
    residual.disposition = Disposition::Unresolved;
    residual.closure_state = ClosureState::Open;
    residual.closure_evidence = Vec::new();
    residual
}

/// R1 divergence observables for the trial manifest
/// (ASSUMPTIONS-REVIEW measurement plan).
fn observed_contestation(r1: &[LaneAccepted], r2: &[LaneAccepted]) -> ObservedContestation {
    let contested = r1_contestation(r1);
    let mut verdict_distribution = std::collections::BTreeMap::new();
    for lane in r1 {
        *verdict_distribution
            .entry(lane.output.verdict.trim().to_string())
            .or_insert(0) += 1;
    }
    let mut residual_attestations = std::collections::BTreeMap::new();
    for lane in r1 {
        for residual in &lane.output.residuals {
            *residual_attestations
                .entry(residual.id.clone())
                .or_insert(0) += 1;
        }
    }
    ObservedContestation {
        r1_lane_count: r1.len(),
        contested_lane_count: contested.len(),
        contested_rate: contested.len() as f64 / r1.len().max(1) as f64,
        verdict_distribution,
        residual_attestations,
        r2_skipped: r2.is_empty(),
    }
}

fn merge_verdicts(
    manifest: &RunManifest,
    brief: &Brief,
    policy: &Policy,
    r1: &[LaneAccepted],
    r2: &[LaneAccepted],
    primary_arbiter: &ArbiterVerdict,
    counterpart_arbiter: &ArbiterVerdict,
) -> ResultEnvelope {
    // Model-relation derivation from the actual roster: one model
    // everywhere = same_model; one family = same_family; anything
    // wider = different_family (the honest label for mixed-vendor
    // deployments — HIGHBALL treats it as strong evidence).
    let mut roster_families = std::collections::BTreeSet::new();
    let mut roster_models = std::collections::BTreeSet::new();
    for route in policy
        .roster
        .iter()
        .chain(std::iter::once(&policy.counterpart_arbiter))
        .chain(std::iter::once(&policy.primary_arbiter))
    {
        roster_families.insert(route.family.as_str());
        roster_models.insert(route.text_model.as_str());
    }
    let base_model_relation: &str = if roster_models.len() == 1 {
        "same_model"
    } else if roster_families.len() == 1 {
        "same_family"
    } else {
        "different_family"
    };
    let correlation_risk = if roster_models.len() == 1 {
        "same_model_error_correlation"
    } else {
        "cross_family_residual_correlation"
    };
    let mut residuals = BTreeMap::<String, Residual>::new();
    let mut dissent = Vec::new();
    for residual in primary_arbiter
        .residuals
        .iter()
        .chain(counterpart_arbiter.residuals.iter())
    {
        if let Some(existing) = residuals.get(&residual.id) {
            if residual_fields_conflict(existing, residual) {
                dissent.push(format!(
                    "Residual {} differs between primary arbiter and counterpart arbiter (finding, disposition, closure, severity, type, source, evidence, required_closure, or scope); retained as unresolved/open.",
                    residual.id
                ));
                let merged = conservative_unresolved(existing.clone());
                residuals.insert(residual.id.clone(), merged);
            }
            continue;
        }
        residuals.insert(residual.id.clone(), residual.clone());
    }

    // Conservative R1/R2 preservation: high-risk residuals accepted in earlier
    // phases must not disappear solely because both R3 arbiters omitted them.
    let mut earlier = BTreeMap::<String, Residual>::new();
    for lane in r1.iter().chain(r2.iter()) {
        for residual in &lane.output.residuals {
            if !is_high_risk_severity(residual.severity) {
                continue;
            }
            if let Some(existing) = earlier.get(&residual.id) {
                if residual_fields_conflict(existing, residual) {
                    dissent.push(format!(
                        "Residual {} high-risk variants conflict across R1/R2; preserving as unresolved/open.",
                        residual.id
                    ));
                    earlier.insert(
                        residual.id.clone(),
                        conservative_unresolved(existing.clone()),
                    );
                }
            } else {
                earlier.insert(residual.id.clone(), residual.clone());
            }
        }
    }
    for (id, residual) in earlier {
        if residuals.contains_key(&id) {
            continue;
        }
        dissent.push(format!(
            "Residual {id} was high-risk in R1/R2 but omitted by both R3 arbiters; preserved as unresolved/open."
        ));
        residuals.insert(id, conservative_unresolved(residual));
    }

    if primary_arbiter.recommendation != counterpart_arbiter.recommendation {
        dissent.push(format!(
            "primary arbiter: {}
counterpart arbiter: {}",
            primary_arbiter.recommendation, counterpart_arbiter.recommendation
        ));
    }
    let perspectives = policy
        .roster
        .iter()
        .map(|route| TrialPerspective {
            party_id: route.party_id.clone(),
            route_id: route.route_id.clone(),
            r1_artifact: r1
                .iter()
                .find(|lane| lane.route_id == route.route_id)
                .unwrap()
                .artifact_ref
                .clone(),
            r2_artifact: r2
                .iter()
                .find(|lane| lane.route_id == route.route_id)
                .map(|lane| lane.artifact_ref.clone())
                .unwrap_or_else(|| "r2/skipped.json".into()),
            independent_first_pass: true,
        })
        .collect();
    ResultEnvelope {
        result_version: RESULT_VERSION.into(),
        run_id: manifest.run_id.clone(),
        status: RunStatus::Completed,
        brief_sha256: manifest.brief_sha256.clone(),
        question: brief.question.clone(),
        action_scope: brief.action_scope.clone(),
        affected_paths: brief.affected_paths.clone(),
        action_binding_sha256: brief.action_binding_sha256.clone(),
        seat_binding: manifest.seat_binding.clone(),
        route_bindings: manifest.route_bindings.clone(),
        summary: primary_arbiter.summary.clone(),
        recommendation: primary_arbiter.recommendation.clone(),
        dissent,
        residuals: residuals.into_values().collect(),
        trial_manifest: TrialManifest {
            manifest_version: TRIAL_MANIFEST_VERSION.into(),
            base_model_relation: base_model_relation.into(),
            perspective_count: 5,
            perspectives,
            perturbation_axes: vec![
                "role".into(),
                "reviewer_position".into(),
                "evidence_budget".into(),
            ],
            independence_controls: vec![
                "per_lane_workdir".into(),
                // The R2 controls in force are label rotation plus scrubbing
                // of per-lane identity markers from the verbatim lane
                // output; that is still not full content anonymization, so
                // claim exactly that.
                "participant_label_rotation".into(),
                "identity_marker_scrubbing".into(),
                "scheduler_captured_output".into(),
                "closed_schema".into(),
            ],
            contamination_risks: vec![
                correlation_risk.into(),
                "process_isolation_is_not_an_os_sandbox".into(),
                "label_rotation_is_not_content_anonymization".into(),
            ],
            wall_time_seconds: None,
            observed_contestation: Some(observed_contestation(r1, r2)),
        },
    }
}

/// Merge the two verdicts, validate against RESULT_SCHEMA, and write
/// result.json + report.md. R3 finalize and primary-arbiter amend share this
/// path, guaranteeing both produce structurally identical results.
fn write_run_result(
    manifest: &RunManifest,
    brief: &Brief,
    policy: &Policy,
    r1: &[LaneAccepted],
    r2: &[LaneAccepted],
    primary_arbiter: &ArbiterVerdict,
    run_dir: &Path,
) -> anyhow::Result<ResultEnvelope> {
    let counterpart_arbiter: ArbiterVerdict = read_json(&run_dir.join("r3/cc-response.json"))?;
    validate_unique_residual_ids(&counterpart_arbiter.residuals, "counterpart arbiter")?;
    let result = merge_verdicts(
        manifest,
        brief,
        policy,
        r1,
        r2,
        primary_arbiter,
        &counterpart_arbiter,
    );
    validate_value(&serde_json::to_value(&result)?, RESULT_SCHEMA)?;
    write_json(&run_dir.join("result.json"), &result)?;
    atomic_write(
        &run_dir.join("report.md"),
        render_report(&result).as_bytes(),
    )?;
    Ok(result)
}

fn render_report(result: &ResultEnvelope) -> String {
    let mut report = format!(
        "# QUINTE Verdict\n\nRun: `{}`\n\n## Summary\n\n{}\n\n## Recommendation\n\n{}\n",
        result.run_id, result.summary, result.recommendation
    );
    if !result.dissent.is_empty() {
        report.push_str("\n## Dissent\n");
        for item in &result.dissent {
            report.push_str(&format!("\n- {item}\n"));
        }
    }
    report.push_str("\n## Residuals\n");
    for residual in &result.residuals {
        report.push_str(&format!(
            "\n### {} ({:?})\n\n{}\n\n- Disposition: `{:?}`\n- Closure: `{:?}`\n- Scope: {}\n",
            residual.id,
            residual.severity,
            residual.finding,
            residual.disposition,
            residual.closure_state,
            residual.scope
        ));
    }
    report
}

fn validate_unique_ids<'a, I>(ids: I, kind: &str) -> anyhow::Result<()>
where
    I: IntoIterator<Item = &'a str>,
{
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            bail!("duplicate {kind} id: {id}");
        }
    }
    Ok(())
}

fn validate_unique_claim_ids(claims: &[crate::model::Claim]) -> anyhow::Result<()> {
    validate_unique_ids(claims.iter().map(|claim| claim.id.as_str()), "claim")
}

fn validate_unique_residual_ids(residuals: &[Residual], context: &str) -> anyhow::Result<()> {
    validate_unique_ids(
        residuals.iter().map(|residual| residual.id.as_str()),
        &format!("{context} residual"),
    )
}

fn allowed_evidence_refs(run_dir: &Path) -> anyhow::Result<BTreeSet<String>> {
    let snapshot: SnapshotManifest = read_json(&run_dir.join("input/snapshot-manifest.json"))?;
    Ok(snapshot
        .entries
        .into_iter()
        .map(|entry| entry.snapshot_ref)
        .chain(
            snapshot
                .attachments
                .into_iter()
                .map(|attachment| attachment.attachment_ref),
        )
        .collect())
}

fn validate_evidence_reference(reference: &str, valid: &BTreeSet<String>) -> anyhow::Result<()> {
    if reference.is_empty() {
        return Ok(());
    }
    if !valid.contains(reference) {
        bail!("unresolvable evidence reference: {reference}");
    }
    Ok(())
}

/// Production 2026-08-09 (0.2.4 follow-up, ROSCO LEMON R1 production loss): the model
/// cites a well-formed `snapshot://` URI with a 1–3 char typo (one dropped
/// char in a long CJK filename).  That is a near-miss deformation of a real
/// manifest entry, not a hallucinated source — remap it, but only when the
/// match is unambiguous (best Levenshtein distance ≤ 3 with a ≥3 margin over
/// the runner-up).  Ambiguous or farther references keep their original text
/// and still fail validation below: fail-closed on genuine hallucinations.
fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            curr[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(curr[j] + 1);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

fn best_ref_remap<'a>(reference: &str, valid: &'a BTreeSet<String>) -> Option<&'a str> {
    if valid.contains(reference) {
        return None;
    }
    let mut best: Option<(&str, usize)> = None;
    let mut second = usize::MAX;
    for candidate in valid {
        let distance = levenshtein(reference, candidate);
        match best {
            Some((_, best_distance)) if distance >= best_distance => {
                if distance < second {
                    second = distance;
                }
            }
            _ => {
                second = best.map_or(second, |(_, best_distance)| best_distance);
                best = Some((candidate, distance));
            }
        }
    }
    match best {
        Some((candidate, distance)) if distance <= 3 && second >= distance + 3 => Some(candidate),
        _ => None,
    }
}

fn remap_evidence_refs(
    refs: &mut [String],
    valid: &BTreeSet<String>,
    remapped: &mut Vec<(String, String)>,
) {
    for reference in refs.iter_mut() {
        if reference.is_empty()
            || !(reference.starts_with("snapshot://") || reference.starts_with("attachment://"))
        {
            continue;
        }
        if let Some(found) = best_ref_remap(reference, valid) {
            let found = found.to_string();
            let old = std::mem::replace(reference, found.clone());
            remapped.push((old, found));
        }
    }
}

fn validate_residual_evidence_refs(
    residuals: &mut [Residual],
    run_dir: &Path,
) -> anyhow::Result<Vec<(String, String)>> {
    let valid = allowed_evidence_refs(run_dir)?;
    let mut remapped = Vec::new();
    for residual in residuals.iter_mut() {
        remap_evidence_refs(&mut residual.evidence_refs, &valid, &mut remapped);
        remap_evidence_refs(&mut residual.closure_evidence, &valid, &mut remapped);
    }
    for residual in residuals.iter() {
        for reference in residual
            .evidence_refs
            .iter()
            .chain(residual.closure_evidence.iter())
        {
            validate_evidence_reference(reference, &valid)?;
        }
    }
    Ok(remapped)
}

fn validate_arbiter_semantics(
    verdict: &mut ArbiterVerdict,
    run_dir: &Path,
) -> anyhow::Result<Vec<(String, String)>> {
    validate_unique_residual_ids(&verdict.residuals, "arbiter")?;
    validate_residual_evidence_refs(&mut verdict.residuals, run_dir)
}

fn validate_evidence_refs(
    output: &mut LaneOutput,
    run_dir: &Path,
) -> anyhow::Result<Vec<(String, String)>> {
    validate_unique_claim_ids(&output.claims)?;
    validate_unique_residual_ids(&output.residuals, "lane")?;
    let valid = allowed_evidence_refs(run_dir)?;
    let mut remapped = Vec::new();
    for claim in output.claims.iter_mut() {
        remap_evidence_refs(&mut claim.evidence_refs, &valid, &mut remapped);
    }
    for residual in output.residuals.iter_mut() {
        remap_evidence_refs(&mut residual.evidence_refs, &valid, &mut remapped);
        remap_evidence_refs(&mut residual.closure_evidence, &valid, &mut remapped);
    }
    for reference in output
        .claims
        .iter()
        .flat_map(|claim| claim.evidence_refs.iter())
        .chain(
            output
                .residuals
                .iter()
                .flat_map(|residual| residual.evidence_refs.iter()),
        )
        .chain(
            output
                .residuals
                .iter()
                .flat_map(|residual| residual.closure_evidence.iter()),
        )
    {
        validate_evidence_reference(reference, &valid)?;
    }
    Ok(remapped)
}

fn fail_run(
    store: &Store,
    manifest: &mut RunManifest,
    status: RunStatus,
    code: &str,
    message: &str,
    retryable: bool,
) -> anyhow::Result<RunStatus> {
    manifest.error = Some(RunError {
        code: code.into(),
        message: message.into(),
        retryable,
    });
    store.transition(
        manifest,
        status,
        manifest.current_phase.clone().as_deref(),
        json!({"error": message}),
    )
}

fn cancel_run(store: &Store, manifest: &mut RunManifest) -> anyhow::Result<RunStatus> {
    manifest.error = Some(RunError {
        code: "cancelled".into(),
        message: "run cancelled by explicit request".into(),
        retryable: false,
    });
    store.transition(
        manifest,
        RunStatus::Cancelled,
        manifest.current_phase.clone().as_deref(),
        json!({}),
    )
}

fn cancellation_requested(run_dir: &Path) -> bool {
    run_dir.join("cancel.requested").exists()
}

fn reconcile_orphan_processes(store: &Store, run_id: &str) -> anyhow::Result<()> {
    for process in store.active_processes(run_id)? {
        if process_matches(&process) {
            kill_verified_process_tree(&process, false);
            let deadline = Instant::now() + Duration::from_secs(2);
            while process_matches(&process) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
            }
            if process_matches(&process) {
                kill_verified_process_tree(&process, true);
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while process_matches(&process) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
            }
            if process_matches(&process) {
                bail!(
                    "cannot recover run while verified orphan process {} is alive",
                    process.pid
                );
            }
        }
        store.remove_active_process(run_id, &process)?;
    }
    Ok(())
}

fn kill_verified_process_tree(process: &ActiveProcess, force: bool) {
    if process_matches(process) {
        kill_process_tree(process.pid, force);
    }
}

fn process_matches(process: &ActiveProcess) -> bool {
    process_identity(process.pid).as_deref() == Some(process.identity.as_str())
}

#[cfg(unix)]
fn process_identity(pid: u32) -> Option<String> {
    let output = std::process::Command::new("ps")
        .args(["-o", "lstart=", "-o", "command=", "-p", &pid.to_string()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let started = String::from_utf8(output.stdout).ok()?;
    let started = started.trim();
    (!started.is_empty()).then(|| started.to_string())
}

#[cfg(windows)]
fn process_identity(pid: u32) -> Option<String> {
    let script =
        format!("(Get-Process -Id {pid} -ErrorAction Stop).StartTime.ToUniversalTime().Ticks");
    let mut command = std::process::Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_hidden_process(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let started = String::from_utf8(output.stdout).ok()?;
    let started = started.trim();
    (!started.is_empty()).then(|| started.to_string())
}

#[cfg(unix)]
fn kill_process_tree(pid: u32, force: bool) {
    use core::ffi::c_int;

    unsafe extern "C" {
        fn kill(pid: c_int, signal: c_int) -> c_int;
    }

    let Ok(pid) = c_int::try_from(pid) else {
        return;
    };
    let signal = if force { 9 } else { 15 };
    // A negative PID targets the process group created for every adapter.
    if unsafe { kill(-pid, signal) } != 0 {
        let _ = unsafe { kill(pid, signal) };
    }
}

#[cfg(unix)]
fn kill_residual_process_group(pid: u32) {
    use core::ffi::c_int;

    unsafe extern "C" {
        fn kill(pid: c_int, signal: c_int) -> c_int;
    }

    let Ok(group_id) = c_int::try_from(pid) else {
        return;
    };
    // Keep the exited group leader unreaped while draining its descendants so
    // the PGID cannot be recycled underneath us. SIGKILL delivery is
    // asynchronous, and one fire-and-forget signal can return while a
    // grandchild still owns stdout/stderr. Linux can observe live group
    // members precisely; other Unix targets get a short grace and a second
    // best-effort signal without mistaking the zombie leader for a live child.
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        let _ = unsafe { kill(-group_id, 9) };
        thread::sleep(Duration::from_millis(10));
        let _ = unsafe { kill(-group_id, 9) };
        return;
    }

    #[cfg(target_os = "linux")]
    let deadline = Instant::now() + Duration::from_millis(500);
    #[cfg(target_os = "linux")]
    let mut empty_observations = 0;
    #[cfg(target_os = "linux")]
    loop {
        let _ = unsafe { kill(-group_id, 9) };
        if process_group_has_live_member(group_id) {
            empty_observations = 0;
        } else {
            empty_observations += 1;
            if empty_observations == 2 {
                return;
            }
        }
        if Instant::now() >= deadline {
            let _ = unsafe { kill(-group_id, 9) };
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
fn process_group_has_live_member(group_id: core::ffi::c_int) -> bool {
    process_group_has_live_member_in(Path::new("/proc"), group_id)
        .unwrap_or_else(|_| process_group_exists(group_id))
}

#[cfg(target_os = "linux")]
fn process_group_has_live_member_in(
    proc_root: &Path,
    group_id: core::ffi::c_int,
) -> std::io::Result<bool> {
    let entries = std::fs::read_dir(proc_root)?;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return Ok(true),
        };
        let Some(_pid) = entry.file_name().to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        let process_dir = entry.path();
        let stat = match std::fs::read(process_dir.join("stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let Some((state, process_group)) = linux_stat_state_and_process_group(&stat) else {
            return Ok(true);
        };
        if process_group != group_id {
            continue;
        }
        if state != b'Z' {
            return Ok(true);
        }
        // `/proc` enumerates thread-group leaders only. A leader may have
        // called pthread_exit and remain a zombie while another thread in the
        // same process is alive and still owns an inherited pipe.
        let Ok(tasks) = std::fs::read_dir(process_dir.join("task")) else {
            return Ok(true);
        };
        for task in tasks {
            let task = match task {
                Ok(task) => task,
                Err(_) => return Ok(true),
            };
            let stat = match std::fs::read(task.path().join("stat")) {
                Ok(stat) => stat,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Ok(true),
            };
            match linux_stat_state_and_process_group(&stat) {
                Some((state, process_group)) if state != b'Z' && process_group == group_id => {
                    return Ok(true);
                }
                Some(_) => {}
                None => return Ok(true),
            }
        }
    }
    Ok(false)
}

#[cfg(target_os = "linux")]
fn linux_stat_state_and_process_group(stat: &[u8]) -> Option<(u8, core::ffi::c_int)> {
    // `comm` is parenthesized, may contain spaces and arbitrary non-NUL bytes,
    // and is therefore not necessarily UTF-8. Split after its final `) `; the
    // following ASCII fields are state, ppid, pgrp.
    let fields_start = stat.windows(2).rposition(|bytes| bytes == b") ")? + 2;
    let mut fields = stat[fields_start..]
        .split(u8::is_ascii_whitespace)
        .filter(|field| !field.is_empty());
    let state = fields.next()?;
    if state.len() != 1 {
        return None;
    }
    let _parent_pid = fields.next()?;
    let process_group = std::str::from_utf8(fields.next()?).ok()?.parse().ok()?;
    Some((state[0], process_group))
}

#[cfg(target_os = "linux")]
fn process_group_exists(group_id: core::ffi::c_int) -> bool {
    unsafe extern "C" {
        fn kill(pid: core::ffi::c_int, signal: core::ffi::c_int) -> core::ffi::c_int;
    }

    if unsafe { kill(-group_id, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(windows)]
fn kill_process_tree(pid: u32, force: bool) {
    // taskkill remains a fallback for processes not assigned to an owned job
    // (for example recovered orphans recorded only by PID/identity).
    let mut args = vec!["/PID".to_string(), pid.to_string(), "/T".to_string()];
    if force {
        args.push("/F".to_string());
    }
    let mut command = std::process::Command::new("taskkill");
    command
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_hidden_process(&mut command);
    let _ = command.status();
}

fn install_interrupt_handler() {
    let flag = interrupt_flag().clone();
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| {
        ctrlc::set_handler(move || {
            flag.store(true, Ordering::SeqCst);
        })
        .expect("install Ctrl-C handler");
    });
}

fn interrupt_flag() -> &'static Arc<AtomicBool> {
    INTERRUPTED.get_or_init(|| Arc::new(AtomicBool::new(false)))
}

pub fn wait(store: &Store, run_id: &str, poll_interval: Duration) -> anyhow::Result<RunStatus> {
    let interrupted = interrupt_flag().clone();
    interrupted.store(false, Ordering::SeqCst);
    install_interrupt_handler();
    #[cfg(feature = "test-adapters")]
    if std::env::var_os("QUINTE_TEST_WAIT_READY").is_some() {
        atomic_write(
            &store
                .run_dir(run_id)?
                .join("diagnostics/wait-handler-ready"),
            b"ready\n",
        )?;
    }
    loop {
        if interrupted.load(Ordering::SeqCst) {
            return Err(WaitInterrupted.into());
        }
        let status = store.load_manifest(run_id)?.status;
        if status.terminal() {
            verify_result_integrity(store, run_id)?;
            return Ok(status);
        }
        if status == RunStatus::WaitingPrimaryArbiter {
            let policy: Value = read_json(&store.run_dir(run_id)?.join("input/policy.json"))?;
            if policy.get("auto_primary_arbiter").and_then(Value::as_bool) == Some(true) {
                bail!(
                    "automatic Primary Arbiter run entered the manual handoff state; run `quinte resume {run_id}` to recover"
                );
            }
            return Ok(status);
        }
        ensure_worker_liveness(store, run_id)?;
        thread::sleep(poll_interval);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultIntegrity {
    pub contract_version: &'static str,
    pub actionable: bool,
}

pub fn verify_result_integrity(
    store: &Store,
    run_id: &str,
) -> anyhow::Result<Option<ResultIntegrity>> {
    let manifest = store.load_manifest(run_id)?;
    if !matches!(manifest.status, RunStatus::Completed | RunStatus::Degraded) {
        return Ok(None);
    }
    let expected = manifest
        .result_sha256
        .as_deref()
        .ok_or_else(|| anyhow!("completed run is missing its result digest"))?;
    let path = store.run_dir(run_id)?.join("result.json");
    // Read the result exactly once.  Hashing the path and parsing it via two
    // independent opens creates a TOCTOU window in which a concurrent amend
    // (or an attacker with write access to the state root) can swap the bytes
    // between the integrity check and semantic validation.
    let io_path = filesystem_path(&path)?;
    let result_bytes = fs::read(&io_path).context("completed run result is missing")?;
    let actual = sha256_bytes(&result_bytes);
    if actual != expected {
        bail!("completed run result integrity check failed");
    }
    let result_contract = contract("result").expect("result contract is registered");
    let result_text = std::str::from_utf8(&result_bytes)
        .with_context(|| format!("{} is not strict UTF-8", path.display()))?;
    let result: Value = serde_json::from_str(result_text)
        .with_context(|| format!("invalid JSON syntax in {}", path.display()))?;
    let revision = crate::schema::validate_versioned_value(&result, result_contract)?;
    if result.get("run_id").and_then(Value::as_str) != Some(run_id) {
        bail!("completed run result identity does not match its manifest");
    }
    if result.get("status").and_then(Value::as_str)
        != serde_json::to_value(manifest.status)?.as_str()
    {
        bail!("completed run result status does not match its manifest");
    }
    if result.get("brief_sha256").is_some()
        && result.get("brief_sha256").and_then(Value::as_str)
            != Some(manifest.brief_sha256.as_str())
    {
        bail!("completed run result brief digest does not match its manifest");
    }
    if result.get("seat_binding").is_some()
        && result.get("seat_binding") != Some(&serde_json::to_value(&manifest.seat_binding)?)
    {
        bail!("completed run result seat binding does not match its manifest");
    }
    if result.get("route_bindings").is_some()
        && result.get("route_bindings")
            != Some(&serde_json::to_value(&manifest.route_bindings)?)
    {
        bail!("completed run result bindings do not match its manifest");
    }
    Ok(Some(ResultIntegrity {
        contract_version: revision.version,
        actionable: revision.version == RESULT_VERSION,
    }))
}

fn ensure_worker_liveness(store: &Store, run_id: &str) -> anyhow::Result<()> {
    let run_dir = store.run_dir(run_id)?;
    let worker: serde_json::Value = match read_json(&run_dir.join("diagnostics/worker.json")) {
        Ok(worker) => worker,
        Err(_) => return Ok(()),
    };
    if run_dir.join("diagnostics/worker-finished").exists() {
        return Ok(());
    }
    let heartbeat = run_dir.join("diagnostics/worker-heartbeat");
    let stale = heartbeat
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed > Duration::from_secs(15));
    let pid = worker
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok());
    if stale || pid.is_none_or(|pid| !process_alive(pid)) {
        // The worker may publish a durable stop state between the waiter's
        // manifest read and this process check.
        let status = store.load_manifest(run_id)?.status;
        if status.terminal() || status == RunStatus::WaitingPrimaryArbiter {
            return Ok(());
        }
        bail!("worker is not alive; run `quinte resume {run_id}` to recover the run");
    }
    Ok(())
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let mut exit_code = 0;
    let ok = unsafe { GetExitCodeProcess(process, &mut exit_code) } != 0;
    unsafe {
        CloseHandle(process);
    }
    ok && exit_code == STILL_ACTIVE as u32
}

#[cfg(test)]
mod retry_tests {
    use super::{
        LaneAccepted, RateLimitSignal, RetryClass, build_r2_packet, build_snapshot,
        classify_rate_limit, ensure_verdict_not_degenerate, evaluate_attempt_output,
        merge_verdicts, next_attempt, r1_contestation, residual_fields_conflict, retry_allowed,
        retry_schedule, validate_arbiter_semantics, validate_evidence_refs,
        validate_unique_claim_ids, validate_unique_residual_ids, verdict_looks_degenerate,
    };
    use crate::adapters::{self, OutputKind};
    use crate::contract::BRIEF_VERSION;
    use crate::model::{
        ArbiterVerdict, AttachmentEntry, Brief, ClosureState, Disposition, LaneOutput, Residual,
        RunManifest, RunStatus, SandboxMode, Severity, SnapshotEntry, SnapshotManifest,
    };
    use crate::policy::default_policy;

    #[cfg(unix)]
    use std::os::unix::process::CommandExt;
    #[cfg(unix)]
    use std::process::{Command, Stdio};
    #[cfg(unix)]
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    #[cfg(unix)]
    use std::sync::mpsc;
    #[cfg(unix)]
    use std::thread;
    #[cfg(unix)]
    use std::time::{Duration, Instant};

    const VALID_SNAPSHOT_REF: &str = "snapshot://evidence.txt";

    #[test]
    fn snapshot_ignore_prunes_relative_files_and_directories() {
        let temporary = tempfile::tempdir().unwrap();
        let evidence = temporary.path().join("evidence");
        std::fs::create_dir_all(evidence.join(".firecrawl/nested")).unwrap();
        std::fs::create_dir_all(evidence.join("tools/r4se-packages/pkg")).unwrap();
        std::fs::create_dir_all(evidence.join("kept")).unwrap();
        std::fs::write(evidence.join(".firecrawl/nested/cache.json"), b"ignored").unwrap();
        std::fs::write(
            evidence.join("tools/r4se-packages/pkg/archive.ipk"),
            b"ignored",
        )
        .unwrap();
        std::fs::write(evidence.join("kept/report.txt"), b"kept").unwrap();
        std::fs::write(evidence.join("kept/scratch.tmp"), b"ignored").unwrap();
        let brief = Brief {
            brief_version: "1.0".into(),
            question: "What remains?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: vec![
                ".firecrawl".into(),
                "tools/r4se-packages".into(),
                "**/*.tmp".into(),
            ],
            attachments: Vec::new(),
            action_scope: None,
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        };

        let snapshot = build_snapshot(temporary.path(), &brief, &default_policy()).unwrap();

        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(
            snapshot.entries[0].snapshot_ref,
            "snapshot://root-0/kept/report.txt"
        );
    }

    #[test]
    fn snapshot_ignore_applies_to_a_single_file_evidence_root() {
        let temporary = tempfile::tempdir().unwrap();
        let evidence = temporary.path().join("ignored.txt");
        std::fs::write(&evidence, b"ignored").unwrap();
        let brief = Brief {
            brief_version: "1.0".into(),
            question: "What remains?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: vec!["ignored.txt".into()],
            attachments: Vec::new(),
            action_scope: None,
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        };

        let snapshot = build_snapshot(temporary.path(), &brief, &default_policy()).unwrap();

        assert!(snapshot.entries.is_empty());
        assert_eq!(snapshot.total_bytes, 0);
    }

    #[test]
    fn snapshot_accepts_all_contract_image_attachment_types_by_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let sources = [
            ("source.bin", b"\x89PNG\r\n\x1a\n".as_slice()),
            (
                "source.data",
                b"\xff\xd8\xff\xe0\x00\x10JFIF\x00".as_slice(),
            ),
            ("source.raw", b"RIFF\x04\x00\x00\x00WEBP".as_slice()),
            ("source.image", b"GIF89a".as_slice()),
        ];
        let mut attachments = Vec::new();
        for (index, (name, bytes)) in sources.iter().enumerate() {
            let path = temporary.path().join(format!("{index}-{name}"));
            std::fs::write(&path, bytes).unwrap();
            attachments.push(path);
        }
        let brief = Brief {
            brief_version: "1.1".into(),
            question: "Inspect images".into(),
            context: None,
            evidence_roots: Vec::new(),
            snapshot_ignore: Vec::new(),
            attachments,
            action_scope: None,
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        };

        let snapshot = build_snapshot(temporary.path(), &brief, &default_policy()).unwrap();
        let media_types = snapshot
            .attachments
            .iter()
            .map(|attachment| attachment.media_type.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            media_types,
            ["image/png", "image/jpeg", "image/webp", "image/gif"]
        );
        assert!(
            snapshot
                .attachments
                .iter()
                .all(|attachment| attachment.attachment_ref.starts_with("attachment://"))
        );
    }

    #[test]
    fn snapshot_ignore_rejects_platform_specific_or_absolute_patterns() {
        for pattern in [r"tools\\cache", "/cache", "cache/"] {
            let brief = Brief {
                brief_version: "1.0".into(),
                question: "What remains?".into(),
                context: None,
                evidence_roots: Vec::new(),
                snapshot_ignore: vec![pattern.into()],
                attachments: Vec::new(),
                action_scope: None,
                affected_paths: Vec::new(),
                action_binding_sha256: None,
            };
            let temporary = tempfile::tempdir().unwrap();

            let error = build_snapshot(temporary.path(), &brief, &default_policy()).unwrap_err();
            assert!(error.to_string().contains("snapshot_ignore pattern"));
        }
    }

    #[cfg(windows)]
    #[test]
    fn snapshot_persists_and_hashes_paths_beyond_max_path() {
        let temporary = tempfile::tempdir().unwrap();
        let evidence = temporary.path().join("evidence");
        let mut deep = evidence.clone();
        while deep.as_os_str().len() < 280 {
            deep.push("segment-with-a-deliberately-long-name");
        }
        let io_deep = crate::util::filesystem_path(&deep).unwrap();
        std::fs::create_dir_all(&io_deep).unwrap();
        let source = deep.join("long-evidence-name.txt");
        std::fs::write(crate::util::filesystem_path(&source).unwrap(), b"long path").unwrap();
        let brief = Brief {
            brief_version: "1.0".into(),
            question: "What remains?".into(),
            context: None,
            evidence_roots: vec![evidence],
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: None,
            affected_paths: Vec::new(),
            action_binding_sha256: None,
        };

        let snapshot = build_snapshot(temporary.path(), &brief, &default_policy()).unwrap();

        assert_eq!(snapshot.entries.len(), 1);
        let relative = snapshot.entries[0]
            .snapshot_ref
            .strip_prefix("snapshot://")
            .unwrap();
        let copied = temporary.path().join("input/snapshot").join(relative);
        assert!(copied.as_os_str().len() > 260);
        assert_eq!(
            crate::util::sha256_file(&copied).unwrap(),
            snapshot.entries[0].sha256
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_verbatim_paths_preserve_non_unicode_names() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        let temporary = tempfile::tempdir().unwrap();
        let name = std::ffi::OsString::from_wide(&[b'n' as u16, 0xd800, b'x' as u16]);
        let path = temporary.path().join(name);

        let converted = crate::util::filesystem_path(&path).unwrap();
        let converted_wide = converted.as_os_str().encode_wide().collect::<Vec<_>>();

        assert!(
            converted_wide
                .windows(3)
                .any(|window| window == [b'n' as u16, 0xd800, b'x' as u16])
        );
    }

    #[cfg(unix)]
    #[test]
    fn stubborn_process_group_is_reaped_within_the_hard_bound() {
        let mut command = Command::new("sh");
        command
            .args(["-c", "trap '' TERM; while :; do sleep 60; done"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        let mut child = command.spawn().unwrap();
        let started = Instant::now();

        let status = super::terminate_child(&mut child, Duration::from_millis(50), None);

        assert!(status.is_some());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn exited_group_leader_does_not_leave_a_pipe_holding_grandchild() {
        use core::ffi::c_int;

        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                "(trap '' TERM; while :; do sleep 60; done) & printf '%s\\n' \"$!\"; exit 0",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0);
        let mut child = command.spawn().unwrap();
        let group_id = child.id();
        let mut stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let _ = sender.send(super::read_capped(
                &mut stdout,
                1024,
                &AtomicUsize::new(0),
                &AtomicBool::new(false),
            ));
        });

        let status = super::wait_child(
            &mut child,
            2,
            tempfile::tempdir().unwrap().path(),
            &AtomicBool::new(false),
            None,
        )
        .unwrap()
        .0;
        assert!(status.is_some_and(|status| status.success()));

        let output = super::receive_captured_output(receiver, "stdout").unwrap();
        let grandchild_pid: c_int = std::str::from_utf8(&output)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let group_id = c_int::try_from(group_id).unwrap();
        assert!(!process_is_running(grandchild_pid));
        assert!(!process_group_has_running_member(group_id));
    }

    #[cfg(target_os = "linux")]
    fn process_is_running(pid: core::ffi::c_int) -> bool {
        let status = std::fs::read_to_string(format!("/proc/{pid}/stat"));
        status.is_ok_and(|status| status.split_whitespace().nth(2) != Some("Z"))
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn process_group_scan_handles_non_utf8_names_and_live_threads_under_zombie_leaders() {
        fn stat(pid: u32, name: &[u8], state: u8, parent: u32, group: i32) -> Vec<u8> {
            let mut value = format!("{pid} (").into_bytes();
            value.extend_from_slice(name);
            value.extend_from_slice(format!(") {} {parent} {group} 0\n", state as char).as_bytes());
            value
        }

        let temporary = tempfile::tempdir().unwrap();
        let proc_root = temporary.path();
        let group_id = 4242;

        let own_leader = proc_root.join(group_id.to_string());
        std::fs::create_dir_all(own_leader.join("task").join(group_id.to_string())).unwrap();
        let own_stat = stat(group_id as u32, b"leader", b'Z', 1, group_id);
        std::fs::write(own_leader.join("stat"), &own_stat).unwrap();
        std::fs::write(
            own_leader
                .join("task")
                .join(group_id.to_string())
                .join("stat"),
            &own_stat,
        )
        .unwrap();
        assert!(!super::process_group_has_live_member_in(proc_root, group_id).unwrap());

        let threaded_pid = 5252;
        let threaded = proc_root.join(threaded_pid.to_string());
        std::fs::create_dir_all(threaded.join("task").join("5253")).unwrap();
        let mut non_utf8_name = b"worker ) ".to_vec();
        non_utf8_name.push(0xff);
        std::fs::write(
            threaded.join("stat"),
            stat(threaded_pid, &non_utf8_name, b'Z', 1, group_id),
        )
        .unwrap();
        std::fs::write(
            threaded.join("task").join("5253").join("stat"),
            stat(5253, &non_utf8_name, b'S', 1, group_id),
        )
        .unwrap();
        assert!(super::process_group_has_live_member_in(proc_root, group_id).unwrap());

        std::fs::write(
            threaded.join("task").join("5253").join("stat"),
            stat(5253, &non_utf8_name, b'Z', 1, group_id),
        )
        .unwrap();
        assert!(!super::process_group_has_live_member_in(proc_root, group_id).unwrap());

        let malformed = proc_root.join("6262");
        std::fs::create_dir(&malformed).unwrap();
        std::fs::write(malformed.join("stat"), b"malformed").unwrap();
        assert!(super::process_group_has_live_member_in(proc_root, group_id).unwrap());
    }

    #[cfg(target_os = "linux")]
    fn process_group_has_running_member(group_id: core::ffi::c_int) -> bool {
        std::fs::read_dir("/proc").is_ok_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                    return false;
                };
                let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                    return false;
                };
                let fields = status.split_whitespace().collect::<Vec<_>>();
                fields.get(2) != Some(&"Z")
                    && fields
                        .get(4)
                        .and_then(|value| value.parse::<core::ffi::c_int>().ok())
                        == Some(group_id)
            })
        })
    }

    #[cfg(not(target_os = "linux"))]
    fn process_is_running(pid: core::ffi::c_int) -> bool {
        unsafe extern "C" {
            fn kill(pid: core::ffi::c_int, signal: core::ffi::c_int) -> core::ffi::c_int;
        }
        (unsafe { kill(pid, 0) }) == 0
    }

    #[cfg(not(target_os = "linux"))]
    fn process_group_has_running_member(group_id: core::ffi::c_int) -> bool {
        unsafe extern "C" {
            fn kill(pid: core::ffi::c_int, signal: core::ffi::c_int) -> core::ffi::c_int;
        }
        (unsafe { kill(-group_id, 0) }) == 0
    }

    #[test]
    fn next_attempt_uses_persisted_directories_and_ignores_malformed_entries() {
        let temporary = tempfile::tempdir().unwrap();
        let route_dir = temporary.path().join("lanes/R1/party-a");
        std::fs::create_dir_all(route_dir.join("attempt-1")).unwrap();
        std::fs::create_dir_all(route_dir.join("attempt-not-a-number")).unwrap();
        std::fs::create_dir_all(route_dir.join("attempt-0")).unwrap();
        std::fs::create_dir_all(route_dir.join("unrelated")).unwrap();
        std::fs::write(route_dir.join("attempt-99"), b"not a directory").unwrap();

        assert_eq!(
            next_attempt(temporary.path(), "R1", "party-a", 3).unwrap(),
            2
        );
        assert_eq!(
            next_attempt(temporary.path(), "R2", "party-a", 3).unwrap(),
            1
        );
    }

    #[test]
    fn next_attempt_fails_closed_after_budget_is_consumed() {
        let temporary = tempfile::tempdir().unwrap();
        let route_dir = temporary.path().join("lanes/R3/cc");
        for attempt in 1..=3 {
            std::fs::create_dir_all(route_dir.join(format!("attempt-{attempt}"))).unwrap();
        }

        let error = next_attempt(temporary.path(), "R3", "cc", 3).unwrap_err();
        assert!(error.to_string().contains("attempt budget exhausted"));
        assert!(!route_dir.join("attempt-4").exists());
    }

    fn evidence_test_run_dir() -> tempfile::TempDir {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temporary.path().join("input")).unwrap();
        let manifest = SnapshotManifest {
            snapshot_version: "1.0".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            entries: vec![SnapshotEntry {
                snapshot_ref: VALID_SNAPSHOT_REF.into(),
                source_name: "evidence.txt".into(),
                sha256: "sha256:test".into(),
                bytes: 1,
                media_type: "text/plain".into(),
            }],
            attachments: Vec::new(),
            total_bytes: 1,
        };
        std::fs::write(
            temporary.path().join("input/snapshot-manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        temporary
    }

    fn attachment_evidence_test_run_dir() -> tempfile::TempDir {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temporary.path().join("input")).unwrap();
        let manifest = SnapshotManifest {
            snapshot_version: "1.0".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            entries: Vec::new(),
            attachments: vec![AttachmentEntry {
                attachment_ref: "attachment://attachment-0.png".into(),
                source_name: "source.png".into(),
                sha256: "sha256:test".into(),
                bytes: 1,
                media_type: "image/png".into(),
            }],
            total_bytes: 1,
        };
        std::fs::write(
            temporary.path().join("input/snapshot-manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        temporary
    }

    fn lane_output_with_evidence(claim_ref: &str, closure_ref: &str) -> LaneOutput {
        serde_json::from_value(serde_json::json!({
            "lane_output_version": "1.0",
            "task_restatement": "test evidence references",
            "verdict": "test verdict",
            "confidence": 0.9,
            "claims": [{
                "id": "claim-1",
                "statement": "test claim",
                "evidence_refs": if claim_ref.is_empty() { vec![] } else { vec![claim_ref] },
                "confidence": 0.9,
                "category": "test"
            }],
            "residuals": [{
                "id": "residual-1",
                "severity": "MEDIUM",
                "residual_type": "test",
                "source": "test",
                "finding": "test residual",
                "evidence_refs": [],
                "disposition": "unresolved",
                "required_closure": "provide evidence",
                "closure_state": "open",
                "closure_evidence": if closure_ref.is_empty() {
                    vec![]
                } else {
                    vec![closure_ref]
                },
                "scope": "test"
            }],
            "uncertainties": []
        }))
        .unwrap()
    }

    #[test]
    fn exact_snapshot_evidence_reference_is_accepted() {
        let temporary = evidence_test_run_dir();
        let mut output = lane_output_with_evidence(VALID_SNAPSHOT_REF, VALID_SNAPSHOT_REF);

        validate_evidence_refs(&mut output, temporary.path()).unwrap();
    }

    #[test]
    fn exact_attachment_evidence_reference_is_accepted() {
        let temporary = attachment_evidence_test_run_dir();
        let reference = "attachment://attachment-0.png";
        let mut output = lane_output_with_evidence(reference, reference);

        validate_evidence_refs(&mut output, temporary.path()).unwrap();
    }

    #[test]
    fn attachment_reference_with_arbitrary_fragment_is_rejected() {
        let temporary = attachment_evidence_test_run_dir();
        let mut output = lane_output_with_evidence(
            "attachment://attachment-0.png#arbitrary",
            "attachment://attachment-0.png",
        );

        let error = validate_evidence_refs(&mut output, temporary.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unresolvable evidence reference")
        );
    }

    #[test]
    fn attachment_reference_missing_from_manifest_is_rejected() {
        let temporary = attachment_evidence_test_run_dir();
        let mut output =
            lane_output_with_evidence("attachment://missing.png", "attachment://attachment-0.png");

        let error = validate_evidence_refs(&mut output, temporary.path()).unwrap_err();
        assert!(error.to_string().contains("attachment://missing.png"));
    }

    #[test]
    fn snapshot_reference_with_arbitrary_fragment_is_rejected() {
        let temporary = evidence_test_run_dir();
        let mut output = lane_output_with_evidence(
            &format!("{VALID_SNAPSHOT_REF}#arbitrary"),
            VALID_SNAPSHOT_REF,
        );

        let error = validate_evidence_refs(&mut output, temporary.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unresolvable evidence reference")
        );
    }

    #[test]
    fn invalid_closure_evidence_reference_is_rejected() {
        let temporary = evidence_test_run_dir();
        let mut output = lane_output_with_evidence(VALID_SNAPSHOT_REF, "snapshot://missing.txt");

        let error = validate_evidence_refs(&mut output, temporary.path()).unwrap_err();
        assert!(error.to_string().contains("snapshot://missing.txt"));
    }

    fn evidence_run_dir_with_refs(refs: &[&str]) -> tempfile::TempDir {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temporary.path().join("input")).unwrap();
        let manifest = SnapshotManifest {
            snapshot_version: "1.0".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            entries: refs
                .iter()
                .map(|reference| SnapshotEntry {
                    snapshot_ref: (*reference).into(),
                    source_name: "source".into(),
                    sha256: "sha256:test".into(),
                    bytes: 1,
                    media_type: "text/plain".into(),
                })
                .collect(),
            attachments: Vec::new(),
            total_bytes: 1,
        };
        std::fs::write(
            temporary.path().join("input/snapshot-manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        temporary
    }

    #[test]
    fn near_miss_snapshot_reference_is_remapped_to_the_unique_manifest_entry() {
        // Production 2026-08-09, ROSCO LEMON R1: the model dropped one
        // character ("DN260424A" -> "D260424A") from a long CJK filename.
        let manifest_ref = "snapshot://root-2/DN260424A-ROSCO LEMON_20260424_德运_ROSCO_LEMON.csv";
        let temporary = evidence_run_dir_with_refs(&[
            "snapshot://root-0/HP-2 FN-7万-珠电燃料-德运（1.1-5）.txt",
            manifest_ref,
        ]);
        let mut output = lane_output_with_evidence(
            "snapshot://root-2/D260424A-ROSCO LEMON_20260424_德运_ROSCO_LEMON.csv",
            "",
        );

        let remapped = validate_evidence_refs(&mut output, temporary.path()).unwrap();
        assert_eq!(remapped.len(), 1);
        assert_eq!(remapped[0].1, manifest_ref);
        assert_eq!(output.claims[0].evidence_refs, [manifest_ref]);
    }

    #[test]
    fn near_miss_reference_with_two_close_candidates_stays_rejected() {
        // Ambiguity is not a deformation to guess at: two manifest entries at
        // the same small distance must keep the hard failure.
        let temporary = evidence_run_dir_with_refs(&[
            "snapshot://root-0/SOF(1).txt",
            "snapshot://root-0/SOF(2).txt",
        ]);
        let mut output = lane_output_with_evidence("snapshot://root-0/SOF(3).txt", "");

        let error = validate_evidence_refs(&mut output, temporary.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unresolvable evidence reference")
        );
        assert_eq!(
            output.claims[0].evidence_refs,
            ["snapshot://root-0/SOF(3).txt"]
        );
    }

    #[test]
    fn arbiter_verdict_near_miss_reference_is_remapped() {
        let manifest_ref = "snapshot://root-1/MV DIMITRA M - DP LAYTIME.csv";
        let temporary = evidence_run_dir_with_refs(&[manifest_ref]);
        let mut residual = sample_residual("primary-r1", Severity::High, "risk");
        residual.evidence_refs = vec!["snapshot://root-1/MV DIMITRA M - DP LAYTIME.csvx".into()];
        let mut verdict = ArbiterVerdict {
            arbiter_verdict_version: "1.0".into(),
            summary: "s".into(),
            recommendation: "r".into(),
            residuals: vec![residual],
        };

        let remapped = validate_arbiter_semantics(&mut verdict, temporary.path()).unwrap();
        assert_eq!(remapped.len(), 1);
        assert_eq!(verdict.residuals[0].evidence_refs, [manifest_ref]);
    }

    fn sample_residual(id: &str, severity: Severity, finding: &str) -> Residual {
        Residual {
            id: id.into(),
            severity,
            residual_type: "generic".into(),
            source: "test".into(),
            finding: finding.into(),
            evidence_refs: vec![],
            disposition: Disposition::Unresolved,
            required_closure: "close with evidence".into(),
            closure_state: ClosureState::Open,
            closure_evidence: vec![],
            scope: "test-scope".into(),
        }
    }

    fn sample_lane(id: &str, residual: Residual) -> LaneAccepted {
        LaneAccepted {
            party_id: "Party A".into(),
            route_id: "party-a".into(),
            output: LaneOutput {
                lane_output_version: "1.0".into(),
                task_restatement: "task".into(),
                verdict: "verdict".into(),
                confidence: 0.5,
                claims: vec![],
                residuals: vec![residual],
                uncertainties: vec![],
                limitations: vec![],
            },
            artifact_ref: format!("lanes/R1/{id}/accepted.json"),
        }
    }

    fn sample_manifest() -> RunManifest {
        RunManifest {
            manifest_version: "1.0".into(),
            run_id: "run-residual-test".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            status: RunStatus::Merging,
            brief_sha256: format!("sha256:{}", "a".repeat(64)),
            policy_sha256: format!("sha256:{}", "b".repeat(64)),
            snapshot_sha256: format!("sha256:{}", "c".repeat(64)),
            runtime_sha256: format!("sha256:{}", "d".repeat(64)),
            protocol_version: "1.0".into(),
            effective_model: "mimo-v2.5-pro".into(),
            seat_binding: Default::default(),
            route_bindings: crate::model::legacy_route_bindings(),
            sandbox_mode: SandboxMode::Process,
            current_phase: Some("R3".into()),
            error: None,
            r3_input_receipt: None,
            primary_arbiter_challenge: None,
            primary_arbiter_submission: None,
            result_sha256: None,
        }
    }

    fn sample_brief() -> Brief {
        Brief {
            brief_version: BRIEF_VERSION.into(),
            question: "What remains?".into(),
            context: None,
            evidence_roots: Vec::new(),
            snapshot_ignore: Vec::new(),
            attachments: Vec::new(),
            action_scope: Some("test-scope".into()),
            affected_paths: vec!["README.md".into()],
            action_binding_sha256: Some(format!("sha256:{}", "e".repeat(64))),
        }
    }

    fn empty_roster_lanes(policy: &crate::model::Policy) -> (Vec<LaneAccepted>, Vec<LaneAccepted>) {
        let r1: Vec<LaneAccepted> = policy
            .roster
            .iter()
            .map(|route| LaneAccepted {
                party_id: route.party_id.clone(),
                route_id: route.route_id.clone(),
                output: LaneOutput {
                    lane_output_version: "1.0".into(),
                    task_restatement: "task".into(),
                    verdict: "ok".into(),
                    confidence: 0.5,
                    claims: vec![],
                    residuals: vec![],
                    uncertainties: vec![],
                    limitations: vec![],
                },
                artifact_ref: format!("lanes/R1/{}/accepted.json", route.route_id),
            })
            .collect();
        let r2 = r1
            .iter()
            .map(|lane| {
                let mut clone = lane.clone();
                clone.artifact_ref = format!("lanes/R2/{}/accepted.json", lane.route_id);
                clone
            })
            .collect();
        (r1, r2)
    }

    #[test]
    fn degenerate_verdict_guardrail_counts_chars_not_bytes() {
        let verdict =
            |summary: String, recommendation: String, residuals: Vec<Residual>| ArbiterVerdict {
                arbiter_verdict_version: "1.0".into(),
                summary,
                recommendation,
                residuals,
            };

        // Empty residuals + both texts short → degenerate; --force waives
        let stub = verdict("test".into(), "ok".into(), vec![]);
        assert!(verdict_looks_degenerate(&stub));
        let error = ensure_verdict_not_degenerate(&stub, false).unwrap_err();
        assert!(error.to_string().contains("degenerate"), "{error}");
        ensure_verdict_not_degenerate(&stub, true).unwrap();

        // Either text being too short is degenerate (OR semantics)
        let long = "x".repeat(20);
        assert!(verdict_looks_degenerate(&verdict(
            long.clone(),
            "short".into(),
            vec![]
        )));
        assert!(!verdict_looks_degenerate(&verdict(
            long.clone(),
            long,
            vec![]
        )));

        // 20 hanzi (60 bytes) count as non-trivial text by character count
        let cjk = "中文裁决摘要".repeat(4);
        assert_eq!(cjk.chars().count(), 24);
        assert!(!verdict_looks_degenerate(&verdict(
            cjk.clone(),
            cjk,
            vec![]
        )));

        // Non-empty residuals never count as degenerate, however short the text
        let with_residual = verdict(
            "t".into(),
            "r".into(),
            vec![sample_residual("r1", Severity::Low, "finding")],
        );
        assert!(!verdict_looks_degenerate(&with_residual));
    }

    #[test]
    fn duplicate_claim_ids_are_rejected() {
        let mut output = lane_output_with_evidence(VALID_SNAPSHOT_REF, "");
        output.claims.push(output.claims[0].clone());
        let error = validate_unique_claim_ids(&output.claims).unwrap_err();
        assert!(error.to_string().contains("duplicate claim id"));
        let temporary = evidence_test_run_dir();
        let error = validate_evidence_refs(&mut output, temporary.path()).unwrap_err();
        assert!(error.to_string().contains("duplicate claim id"));
    }

    #[test]
    fn duplicate_residual_ids_are_rejected_for_lanes_and_arbiters() {
        let residual = sample_residual("r-dup", Severity::Medium, "finding");
        let error = validate_unique_residual_ids(&[residual.clone(), residual.clone()], "lane")
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate lane residual id: r-dup")
        );

        let temporary = evidence_test_run_dir();
        let mut output = lane_output_with_evidence(VALID_SNAPSHOT_REF, "");
        output.residuals.push(output.residuals[0].clone());
        let error = validate_evidence_refs(&mut output, temporary.path()).unwrap_err();
        assert!(error.to_string().contains("duplicate lane residual id"));

        let mut verdict = ArbiterVerdict {
            arbiter_verdict_version: "1.0".into(),
            summary: "s".into(),
            recommendation: "r".into(),
            residuals: vec![residual.clone(), residual],
        };
        let error = validate_arbiter_semantics(&mut verdict, temporary.path()).unwrap_err();
        assert!(error.to_string().contains("duplicate arbiter residual id"));
    }

    #[test]
    fn primary_arbiter_residual_evidence_refs_must_match_snapshot() {
        let temporary = evidence_test_run_dir();
        let mut residual = sample_residual("primary-r1", Severity::High, "risk");
        residual.evidence_refs = vec!["snapshot://missing-from-snapshot.txt".into()];
        let mut verdict = ArbiterVerdict {
            arbiter_verdict_version: "1.0".into(),
            summary: "s".into(),
            recommendation: "r".into(),
            residuals: vec![residual],
        };
        let error = validate_arbiter_semantics(&mut verdict, temporary.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unresolvable evidence reference: snapshot://missing-from-snapshot.txt")
        );

        let mut ok = sample_residual("primary-r1", Severity::High, "risk");
        ok.evidence_refs = vec![VALID_SNAPSHOT_REF.into()];
        let mut verdict = ArbiterVerdict {
            arbiter_verdict_version: "1.0".into(),
            summary: "s".into(),
            recommendation: "r".into(),
            residuals: vec![ok],
        };
        validate_arbiter_semantics(&mut verdict, temporary.path()).unwrap();
    }

    #[test]
    fn residual_merge_detects_severity_type_source_evidence_scope_conflicts() {
        let left = sample_residual("conflict-1", Severity::High, "same finding");
        let mut right = left.clone();
        right.severity = Severity::Low;
        assert!(residual_fields_conflict(&left, &right));
        right = left.clone();
        right.residual_type = "other".into();
        assert!(residual_fields_conflict(&left, &right));
        right = left.clone();
        right.source = "other-source".into();
        assert!(residual_fields_conflict(&left, &right));
        right = left.clone();
        right.evidence_refs = vec![VALID_SNAPSHOT_REF.into()];
        assert!(residual_fields_conflict(&left, &right));
        right = left.clone();
        right.required_closure = "different closure".into();
        assert!(residual_fields_conflict(&left, &right));
        right = left.clone();
        right.scope = "other-scope".into();
        assert!(residual_fields_conflict(&left, &right));
        right = left.clone();
        assert!(!residual_fields_conflict(&left, &right));

        let policy = default_policy();
        let (r1, r2) = empty_roster_lanes(&policy);
        let primary = ArbiterVerdict {
            arbiter_verdict_version: "1.0".into(),
            summary: "primary".into(),
            recommendation: "proceed carefully".into(),
            residuals: vec![sample_residual(
                "conflict-1",
                Severity::High,
                "same finding",
            )],
        };
        let mut cc_residual = sample_residual("conflict-1", Severity::High, "same finding");
        cc_residual.severity = Severity::Critical;
        cc_residual.scope = "cc-scope".into();
        let counterpart = ArbiterVerdict {
            arbiter_verdict_version: "1.0".into(),
            summary: "counterpart".into(),
            recommendation: "proceed carefully".into(),
            residuals: vec![cc_residual],
        };
        let result = merge_verdicts(
            &sample_manifest(),
            &sample_brief(),
            &policy,
            &r1,
            &r2,
            &primary,
            &counterpart,
        );
        let merged = result
            .residuals
            .iter()
            .find(|residual| residual.id == "conflict-1")
            .unwrap();
        assert_eq!(merged.disposition, Disposition::Unresolved);
        assert_eq!(merged.closure_state, ClosureState::Open);
        assert!(
            result.dissent.iter().any(
                |item| item.contains("differs between primary arbiter and counterpart arbiter")
            )
        );
    }

    #[test]
    fn result_carries_r1_contestation_observables() {
        use crate::run::observed_contestation;
        let shared = sample_residual("r-shared", Severity::High, "shared");
        let lanes = vec![
            {
                let mut l = lane_with("proceed", 0.9, vec![shared.clone()]);
                l.party_id = "Party A".into();
                l
            },
            {
                let mut l = lane_with("proceed", 0.9, vec![shared.clone()]);
                l.party_id = "Party B".into();
                l
            },
            {
                let mut l = lane_with("block", 0.9, vec![]);
                l.party_id = "Party C".into();
                l
            },
            {
                let mut l = lane_with("proceed", 0.9, vec![]);
                l.party_id = "Party D".into();
                l
            },
            {
                let mut l = lane_with("proceed", 0.9, vec![shared.clone()]);
                l.party_id = "Party E".into();
                l
            },
        ];
        let oc = observed_contestation(&lanes, &[]);
        assert_eq!(oc.r1_lane_count, 5);
        assert_eq!(oc.contested_lane_count, 5); // dissenter contests every pair; attestation asymmetry too
        assert_eq!(oc.contested_rate, 1.0);
        assert_eq!(oc.verdict_distribution.get("proceed"), Some(&4));
        assert_eq!(oc.verdict_distribution.get("block"), Some(&1));
        assert_eq!(oc.residual_attestations.get("r-shared"), Some(&3));
        assert!(oc.r2_skipped); // empty r2
    }

    #[test]
    fn high_risk_r1_r2_residual_is_preserved_when_both_arbiters_omit_it() {
        let policy = default_policy();
        let (mut r1, r2) = empty_roster_lanes(&policy);
        let high_risk = sample_residual(
            "r1-high-risk",
            Severity::Critical,
            "critical earlier finding",
        );
        r1[0].output.residuals.push(high_risk.clone());
        // Also cover a medium residual that must NOT be auto-preserved.
        r1[0].output.residuals.push(sample_residual(
            "r1-medium",
            Severity::Medium,
            "medium only",
        ));

        let primary = ArbiterVerdict {
            arbiter_verdict_version: "1.0".into(),
            summary: "primary".into(),
            recommendation: "ok".into(),
            residuals: vec![],
        };
        let counterpart = ArbiterVerdict {
            arbiter_verdict_version: "1.0".into(),
            summary: "counterpart".into(),
            recommendation: "ok".into(),
            residuals: vec![],
        };
        let result = merge_verdicts(
            &sample_manifest(),
            &sample_brief(),
            &policy,
            &r1,
            &r2,
            &primary,
            &counterpart,
        );
        assert!(
            result
                .residuals
                .iter()
                .any(|residual| residual.id == "r1-high-risk"
                    && residual.disposition == Disposition::Unresolved
                    && residual.closure_state == ClosureState::Open)
        );
        assert!(
            result
                .residuals
                .iter()
                .all(|residual| residual.id != "r1-medium")
        );
        assert!(
            result.dissent.iter().any(|item| item
                .contains("was high-risk in R1/R2 but omitted by both R3 arbiters"))
        );
        // silence unused helper warning if compiler optimizes oddly
        let _ = sample_lane("x", high_risk);
    }

    #[test]
    fn windows_job_object_containment_is_wired_in_source() {
        // Portable structural proof: the shipped adapter lifecycle owns a
        // kill-on-close Job Object, assigns processes before resume, and no
        // longer treats post-leader residual cleanup as a Windows no-op.
        let source = include_str!("run.rs");
        assert!(source.contains("JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE"));
        assert!(source.contains("CreateJobObjectW"));
        assert!(source.contains("AssignProcessToJobObject"));
        assert!(source.contains("CREATE_SUSPENDED"));
        assert!(source.contains("fn spawn_adapter_process"));
        assert!(source.contains("fn kill_residual_process_group_with_job"));
        assert!(
            source.contains("job.terminate()"),
            "Windows residual cleanup must terminate the owned job"
        );
        // Production Windows residual path must use the job-aware helper rather
        // than an empty stub body on the residual API.
        assert!(
            source.contains("fn kill_residual_process_group(pid: u32, job: Option<&WindowsJob>)"),
            "Windows residual process cleanup must accept the owned job"
        );
    }

    #[test]
    fn only_host_observed_timeout_is_retryable() {
        assert!(retry_allowed(RetryClass::TransientTimeout, 1, 2));
        assert!(!retry_allowed(RetryClass::TransientTimeout, 2, 2));
        assert!(!retry_allowed(RetryClass::Never, 1, 2));
    }

    #[test]
    fn timeout_accepts_only_a_complete_schema_valid_lane_output() {
        let valid = serde_json::json!({
            "lane_output_version": "1.0",
            "task_restatement": "bounded task",
            "verdict": "complete before timeout",
            "confidence": 0.9,
            "claims": [],
            "residuals": [],
            "uncertainties": []
        });
        let bytes = serde_json::to_vec(&valid).unwrap();
        let (output, error, retry) = evaluate_attempt_output(
            "fake",
            OutputKind::DirectJson,
            adapters::OutputContract::Lane,
            &bytes,
            b"",
            None,
            true,
            false,
            false,
            bytes.len(),
        );
        assert_eq!(
            output.unwrap().into_lane().unwrap().verdict,
            "complete before timeout"
        );
        assert_eq!(error, None);
        assert_eq!(retry, RetryClass::Never);

        let (output, error, retry) = evaluate_attempt_output(
            "fake",
            OutputKind::DirectJson,
            adapters::OutputContract::Lane,
            br#"{"lane_output_version":"1.0""#,
            b"",
            None,
            true,
            false,
            false,
            1024,
        );
        assert!(output.is_none());
        assert_eq!(error.as_deref(), Some("timeout"));
        assert_eq!(retry, RetryClass::TransientTimeout);
    }

    #[test]
    fn seat_contract_violation_retries_as_transient_adapter() {
        let (_, error, retry) = evaluate_attempt_output(
            "a2a",
            OutputKind::DirectJson,
            adapters::OutputContract::Lane,
            b"",
            b"a2a seat transport failed: a2a seat task t-1 failed: lane-output.schema.json contract violated by seat 'c-r1': \"verdict\" is a required property",
            Some(1),
            false,
            false,
            false,
            1024,
        );
        assert!(error.unwrap().contains("adapter exited"));
        assert_eq!(retry, RetryClass::TransientAdapter);

        // A provider transport blip inside the seat task retries too.
        let (_, _, retry) = evaluate_attempt_output(
            "a2a",
            OutputKind::DirectJson,
            adapters::OutputContract::Lane,
            b"",
            b"a2a seat transport failed: a2a seat task t-2 failed: provider body read failed",
            Some(1),
            false,
            false,
            false,
            1024,
        );
        assert_eq!(retry, RetryClass::TransientAdapter);

        // Any other non-zero adapter exit stays fail-closed.
        let (_, _, retry) = evaluate_attempt_output(
            "a2a",
            OutputKind::DirectJson,
            adapters::OutputContract::Lane,
            b"",
            b"process crashed",
            Some(1),
            false,
            false,
            false,
            1024,
        );
        assert_eq!(retry, RetryClass::Never);
    }

    #[test]
    fn completed_codewhale_with_retryable_content_is_bounded() {
        let stdout = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({"type": "content", "content": "analysis without final JSON"}),
            serde_json::json!({"type": "metadata", "meta": {"status": "completed"}}),
            serde_json::json!({"type": "done"})
        );
        let (output, error, retry) = evaluate_attempt_output(
            "codewhale",
            OutputKind::CodewhaleStream,
            adapters::OutputContract::Lane,
            stdout.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert!(error.unwrap().contains("no valid LaneOutput"));
        assert_eq!(retry, RetryClass::TransientAdapter);

        let truncated = serde_json::json!({
            "type": "content",
            "content": "analysis without final JSON"
        });
        let (_, _, retry) = evaluate_attempt_output(
            "codewhale",
            OutputKind::CodewhaleStream,
            adapters::OutputContract::Lane,
            truncated.to_string().as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert_eq!(retry, RetryClass::Never);
    }

    #[test]
    fn truncated_opencode_final_text_is_transient_adapter() {
        let stdout = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({"type": "text", "part": {"text": "Now I have all the evidence."}}),
            serde_json::json!({"type": "text", "part": {"text": "```json\n{\"lane_output_version\":\"1.0\",\"verdict\":\"cut off mid sent"}}),
            serde_json::json!({"type": "step_finish", "part": {"reason": "stop"}})
        );
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            stdout.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert!(error.unwrap().contains("no valid LaneOutput"));
        assert_eq!(retry, RetryClass::TransientAdapter);

        // Without a terminal stop step the same truncated payload is permanent.
        let no_terminal =
            serde_json::json!({"type": "text", "part": {"text": "{\"lane_output_vers"}});
        let (_, _, retry) = evaluate_attempt_output(
            "opencode",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            no_terminal.to_string().as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert_eq!(retry, RetryClass::Never);

        // Brace-complete but unparseable payload (unescaped inner quote) is
        // also transient generation corruption, not a contract violation.
        let malformed = format!(
            "{}\n{}\n",
            serde_json::json!({"type": "text", "part": {"text": "{\"lane_output_version\":\"1.0\",\"verdict\":\"方案将\"单模型分析\"改造为流水线\",\"confidence\":0.8}"}}),
            serde_json::json!({"type": "step_finish", "part": {"reason": "stop"}})
        );
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            malformed.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert!(error.unwrap().contains("no valid LaneOutput"));
        assert_eq!(retry, RetryClass::TransientAdapter);

        // A closed Markdown fence still carries malformed JSON and therefore
        // remains transient generation corruption rather than a permanent
        // schema violation.
        let malformed_closed_fence = format!(
            "{}\n{}\n",
            serde_json::json!({
                "type": "text",
                "part": {"text": "```json\n{\"lane_output_version\":\"1.0\",\"task_restatement\":\"x\" \"verdict\":\"y\"}\n```"}
            }),
            serde_json::json!({"type": "step_finish", "part": {"reason": "stop"}})
        );
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            malformed_closed_fence.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert!(error.unwrap().contains("no valid LaneOutput"));
        assert_eq!(retry, RetryClass::TransientAdapter);

        // Complete but schema-invalid JSON stays permanent.
        let schema_invalid = format!(
            "{}\n{}\n",
            serde_json::json!({"type": "text", "part": {"text": "{\"lane_output_version\":\"1.0\",\"task_restatement\":\"missing fields\"}"}}),
            serde_json::json!({"type": "step_finish", "part": {"reason": "stop"}})
        );
        let (_, error, retry) = evaluate_attempt_output(
            "opencode",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            schema_invalid.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(error.unwrap().contains("no valid LaneOutput"));
        assert_eq!(retry, RetryClass::Never);
    }

    #[test]
    fn completed_empty_text_stream_is_transient_adapter() {
        // (a) Terminal stop with zero text output is an empty completion: the
        // model rolled a no-output turn, which is transient across runs.
        let empty_text = format!(
            "{}\n{}\n",
            serde_json::json!({"type": "text", "part": {"text": ""}}),
            serde_json::json!({"type": "step_finish", "part": {"reason": "stop"}})
        );
        assert!(adapters::events_completed_with_unusable_final_candidate(
            empty_text.as_bytes()
        ));
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            empty_text.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert!(error.unwrap().contains("no valid LaneOutput"));
        assert_eq!(retry, RetryClass::TransientAdapter);

        // Same empty completion with no text events at all (tool calls only).
        let no_text = format!(
            "{}\n{}\n",
            serde_json::json!({"type": "tool_use", "part": {"tool": "read"}}),
            serde_json::json!({"type": "step_finish", "part": {"reason": "stop"}})
        );
        let (_, _, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            no_text.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert_eq!(retry, RetryClass::TransientAdapter);

        // (b) Terminal stop with complete but schema-invalid content stays a
        // permanent contract failure: never retried.
        let schema_invalid = format!(
            "{}\n{}\n",
            serde_json::json!({"type": "text", "part": {"text": "{\"lane_output_version\":\"1.0\",\"task_restatement\":\"missing fields\"}"}}),
            serde_json::json!({"type": "step_finish", "part": {"reason": "stop"}})
        );
        assert!(!adapters::events_completed_with_unusable_final_candidate(
            schema_invalid.as_bytes()
        ));
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            schema_invalid.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert!(error.unwrap().contains("no valid LaneOutput"));
        assert_eq!(retry, RetryClass::Never);

        // (c) Terminal stop with a valid LaneOutput payload parses cleanly.
        let valid = serde_json::json!({
            "lane_output_version": "1.0",
            "task_restatement": "bounded task",
            "verdict": "all good",
            "confidence": 0.9,
            "claims": [],
            "residuals": [],
            "uncertainties": []
        });
        let valid_stream = format!(
            "{}\n{}\n",
            serde_json::json!({"type": "text", "part": {"text": valid.to_string()}}),
            serde_json::json!({"type": "step_finish", "part": {"reason": "stop"}})
        );
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            adapters::OutputContract::Lane,
            valid_stream.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert_eq!(output.unwrap().into_lane().unwrap().verdict, "all good");
        assert_eq!(error, None);
        assert_eq!(retry, RetryClass::Never);
    }

    #[test]
    fn completed_codewhale_with_empty_content_is_transient_adapter() {
        // Completed codewhale stream whose assembled content is empty (no
        // JSON candidate at all) is the same transient empty completion.
        let stdout = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({"type": "content", "content": ""}),
            serde_json::json!({"type": "metadata", "meta": {"status": "completed"}}),
            serde_json::json!({"type": "done"})
        );
        assert!(adapters::codewhale_completed_with_retryable_content(
            stdout.as_bytes()
        ));
        let (output, error, retry) = evaluate_attempt_output(
            "codewhale",
            OutputKind::CodewhaleStream,
            adapters::OutputContract::Lane,
            stdout.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert!(error.unwrap().contains("no valid LaneOutput"));
        assert_eq!(retry, RetryClass::TransientAdapter);

        // No content events at all (tool calls only) behaves identically.
        let no_content = format!(
            "{}\n{}\n",
            serde_json::json!({"type": "metadata", "meta": {"status": "completed"}}),
            serde_json::json!({"type": "done"})
        );
        let (_, _, retry) = evaluate_attempt_output(
            "codewhale",
            OutputKind::CodewhaleStream,
            adapters::OutputContract::Lane,
            no_content.as_bytes(),
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert_eq!(retry, RetryClass::TransientAdapter);
    }

    #[test]
    fn rate_limit_classification_requires_failed_transport_evidence() {
        let structured = br#"{"error":{"type":"rate_limit_error","retry_after":7}}"#;
        assert_eq!(
            classify_rate_limit("deepseek", structured, b""),
            Some(RateLimitSignal {
                source: "adapter_structured_error",
                retry_after_seconds: Some(7),
            }),
            "deepseek did not classify a typed 429"
        );
        assert_eq!(
            classify_rate_limit(
                "deepseek",
                b"ordinary output",
                b"HTTP 429 Too Many Requests\nRetry-After: 9\n",
            ),
            Some(RateLimitSignal {
                source: "adapter_stderr_marker",
                retry_after_seconds: Some(9),
            }),
            "deepseek did not classify a transport 429"
        );
        assert_eq!(
            classify_rate_limit("deepseek", b"a model discussed 429", b""),
            None
        );
        assert_eq!(classify_rate_limit("omp", structured, b""), None);
    }

    #[test]
    fn rate_limit_backoff_is_bounded_and_deterministic() {
        let policy = default_policy();
        let retry = RetryClass::RateLimited(RateLimitSignal {
            source: "adapter_structured_error",
            retry_after_seconds: Some(30),
        });
        let first = retry_schedule(retry, &policy, "run", "R2", "deepseek", 1).unwrap();
        let same = retry_schedule(retry, &policy, "run", "R2", "deepseek", 1).unwrap();
        assert_eq!(first, same);
        assert!(first.delay >= std::time::Duration::from_secs(30));
        assert!(first.delay <= std::time::Duration::from_secs(policy.retry_backoff_max_seconds));
    }

    #[cfg(windows)]
    #[test]
    fn worker_stdio_handles_stop_inheriting_before_adapter_spawn() {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::Foundation::{
            DUPLICATE_SAME_ACCESS, DuplicateHandle, GetHandleInformation, HANDLE,
            HANDLE_FLAG_INHERIT,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        let null = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("NUL")
            .unwrap();
        let process = unsafe { GetCurrentProcess() };
        let mut duplicated: HANDLE = std::ptr::null_mut();
        assert_ne!(
            unsafe {
                DuplicateHandle(
                    process,
                    null.as_raw_handle() as HANDLE,
                    process,
                    &mut duplicated,
                    0,
                    1,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            0
        );
        let duplicated = unsafe { OwnedHandle::from_raw_handle(duplicated) };
        let mut flags = 0;
        assert_ne!(
            unsafe { GetHandleInformation(duplicated.as_raw_handle() as HANDLE, &mut flags) },
            0
        );
        assert_ne!(flags & HANDLE_FLAG_INHERIT, 0);

        super::clear_worker_stdio_inheritance(&[duplicated.as_raw_handle() as HANDLE]).unwrap();

        assert_ne!(
            unsafe { GetHandleInformation(duplicated.as_raw_handle() as HANDLE, &mut flags) },
            0
        );
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
    }

    // --- r1_contestation predicate (RASHOMON Shomon divergence gate) ---

    fn lane_with(verdict: &str, confidence: f64, residuals: Vec<Residual>) -> LaneAccepted {
        let mut lane = sample_lane("x", sample_residual("r-unused", Severity::Low, "unused"));
        lane.output.verdict = verdict.into();
        lane.output.confidence = confidence;
        lane.output.residuals = residuals;
        lane
    }

    #[test]
    fn contestation_unanimous_r1_is_empty() {
        let lanes = (0..5)
            .map(|_| lane_with("proceed", 0.9, vec![]))
            .collect::<Vec<_>>();
        assert!(r1_contestation(&lanes).is_empty());
    }

    #[test]
    fn contestation_verdict_disagreement_contests_both() {
        // One dissenter disagrees with every other lane, so every pair
        // containing it contests both members — the whole roster faces
        // re-examination with full context.
        let lanes = vec![
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("block", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
        ];
        assert_eq!(r1_contestation(&lanes), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn contestation_disposition_conflict_contests_both() {
        let mut r_a = sample_residual("r1", Severity::High, "same finding");
        r_a.disposition = Disposition::Verified;
        let mut r_b = sample_residual("r1", Severity::High, "same finding");
        r_b.disposition = Disposition::Unresolved;
        let lanes = vec![
            lane_with("proceed", 0.9, vec![r_a.clone()]),
            lane_with("proceed", 0.9, vec![r_b.clone()]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
        ];
        assert_eq!(r1_contestation(&lanes), vec![0, 1]);
    }

    #[test]
    fn contestation_low_confidence_contests_that_lane() {
        let lanes = vec![
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.4, vec![]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
        ];
        assert_eq!(r1_contestation(&lanes), vec![1]);
    }

    #[test]
    fn contestation_attestation_symmetry_majority_rule() {
        let shared = sample_residual("r-shared", Severity::High, "shared finding");
        // 3 lanes attest r-shared, 2 omit → all five contested.
        let lanes = vec![
            lane_with("proceed", 0.9, vec![shared.clone()]),
            lane_with("proceed", 0.9, vec![shared.clone()]),
            lane_with("proceed", 0.9, vec![shared.clone()]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
        ];
        let mut contested = r1_contestation(&lanes);
        contested.sort();
        assert_eq!(contested, vec![0, 1, 2, 3, 4]);

        // A school-specific finding (single attester) is not contested.
        let solo = sample_residual("r-solo", Severity::High, "solo finding");
        let lanes = vec![
            lane_with("proceed", 0.9, vec![solo.clone()]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
        ];
        assert!(r1_contestation(&lanes).is_empty());
    }

    // --- R2 packet labeling discipline (PROTOCOL.md pseudonymous recheck) ---

    fn contested_roster() -> Vec<LaneAccepted> {
        let mut r_a = sample_residual("r-conflict", Severity::High, "same finding");
        r_a.disposition = Disposition::Verified;
        let mut r_b = sample_residual("r-conflict", Severity::High, "same finding");
        r_b.disposition = Disposition::Unresolved;
        // Verdicts stay unanimous (a verdict split would contest the whole
        // roster); the restatement is what tells the two outputs apart —
        // it is not a contestation input.
        let mut lane_zero = lane_with("proceed", 0.9, vec![r_a]);
        lane_zero.output.task_restatement = "restatement-lane-0".into();
        let mut lane_one = lane_with("proceed", 0.9, vec![r_b]);
        lane_one.output.task_restatement = "restatement-lane-1".into();
        let lanes = vec![
            lane_zero,
            lane_one,
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
            lane_with("proceed", 0.9, vec![]),
        ];
        assert_eq!(r1_contestation(&lanes), vec![0, 1]);
        lanes
    }

    #[test]
    fn r2_packet_labels_only_contested_lanes_with_canonical_names() {
        let lanes = contested_roster();
        let packet = build_r2_packet(
            &sample_manifest(),
            &sample_brief(),
            &lanes,
            &default_policy(),
        )
        .unwrap();
        assert_eq!(packet.participants.len(), 2, "uncontested lanes stay out");
        let canonical = [
            "Participant A",
            "Participant B",
            "Participant C",
            "Participant D",
            "Participant E",
        ];
        for label in packet.participants.keys() {
            assert!(
                canonical.contains(&label.as_str()),
                "non-canonical participant label: {label}"
            );
        }
        // Each contested lane's output appears exactly once, under some
        // participant label.
        let restatements: Vec<&str> = packet
            .participants
            .values()
            .map(|output| output.task_restatement.as_str())
            .collect();
        assert!(restatements.contains(&"restatement-lane-0"));
        assert!(restatements.contains(&"restatement-lane-1"));
    }

    #[test]
    fn r2_packet_rotation_is_not_derived_from_the_run_id() {
        // The run_id is inside the packet, so a run_id-derived rotation would
        // be invertible by every reader. Across repeated builds of the very
        // same inputs the mapping must vary (probability that a uniform
        // 5-way rotation repeats one value 12 times is ~2e-8 — never flaky).
        let lanes = contested_roster();
        let manifest = sample_manifest();
        let brief = sample_brief();
        let mut mappings = std::collections::BTreeSet::new();
        for _ in 0..12 {
            let packet = build_r2_packet(&manifest, &brief, &lanes, &default_policy()).unwrap();
            let mapping: Vec<(&String, &str)> = packet
                .participants
                .iter()
                .map(|(label, output)| (label, output.task_restatement.as_str()))
                .collect();
            mappings.insert(format!("{mapping:?}"));
        }
        assert!(
            mappings.len() > 1,
            "label rotation must vary across runs, not follow the run_id"
        );
    }

    #[test]
    fn r2_packet_scrubs_identity_markers_from_lane_prose() {
        let policy = default_policy();
        let mut lanes = contested_roster();
        let route_id = policy.roster[0].route_id.clone();
        let party_id = policy.roster[1].party_id.clone();
        let model = policy.seat.text_model.clone();
        let perspective = policy.roster[2].perspective.clone();
        lanes[0].output.task_restatement =
            format!("As {route_id} analyzing alongside {party_id} on {model}: {perspective}");
        lanes[1].output.task_restatement =
            "clean prose with DEEPSEEK-A in shouting case".to_string();
        let packet = build_r2_packet(&sample_manifest(), &sample_brief(), &lanes, &policy).unwrap();
        let prose: Vec<&str> = packet
            .participants
            .values()
            .map(|output| output.task_restatement.as_str())
            .collect();
        for text in &prose {
            assert!(!text.contains(&route_id), "route id leaked: {text}");
            assert!(!text.contains(&party_id), "party id leaked: {text}");
            assert!(!text.contains(&model), "model name leaked: {text}");
            assert!(
                !text.contains(&perspective),
                "perspective text leaked: {text}"
            );
        }
        assert!(
            prose.iter().any(|text| text.contains("[route]")
                && text.contains("[party]")
                && text.contains("[model]")
                && text.contains("[perspective]")),
            "scrubbed prose should carry the neutral markers: {prose:?}"
        );
        // Case-insensitive: the shouting-case route id must be scrubbed too.
        assert!(
            prose.iter().any(|text| !text.contains("DEEPSEEK-A")),
            "uppercase route id must not survive: {prose:?}"
        );
    }

    #[test]
    fn r2_scrubbing_preserves_non_identity_prose_verbatim() {
        let policy = default_policy();
        let mut lanes = contested_roster();
        lanes[0].output.task_restatement =
            "通胀路径与估值压缩的交叉风险，以及对 2026 年下半年盈利预测的影响。".to_string();
        lanes[1].output.task_restatement = "plain analysis with no markers".to_string();
        let packet = build_r2_packet(&sample_manifest(), &sample_brief(), &lanes, &policy).unwrap();
        let prose: Vec<&str> = packet
            .participants
            .values()
            .map(|output| output.task_restatement.as_str())
            .collect();
        assert!(
            prose.contains(&"通胀路径与估值压缩的交叉风险，以及对 2026 年下半年盈利预测的影响。"),
            "non-identity prose must pass through verbatim: {prose:?}"
        );
        assert!(
            prose.contains(&"plain analysis with no markers"),
            "clean prose must be untouched: {prose:?}"
        );
    }
}

#[cfg(test)]
mod rate_limit_classification_tests {
    use super::classify_rate_limit;

    #[test]
    fn a2a_seat_stderr_is_retryable_rate_limit() {
        // Verbatim shape observed live: the seat's provider verdict
        // surfaces on the adapter stderr after in-seat backoff exhausts.
        let stderr = b"a2a seat transport failed: a2a seat task \
01a01cc9-085a-7571-b0d9-269416ae044d failed: provider returned 429 after 5 attempts: \
\xe6\x82\xa8\xe7\x9a\x84\xe8\xb4\xa6\xe6\x88\xb7\xe5\xb7\xb2\xe8\xbe\xbe\xe5\x88\xb0\xe9\x80\x9f\xe7\x8e\x87\xe9\x99\x90\xe5\x88\xb6";
        let signal = classify_rate_limit("a2a", b"", stderr);
        assert!(signal.is_some(), "a2a seat 429 must classify as rate limit");
        let plain = b"... provider returned 429 after 5 attempts: too many requests";
        assert!(classify_rate_limit("a2a", b"", plain).is_some());
    }

    #[test]
    fn unknown_adapters_stay_non_retryable() {
        assert!(classify_rate_limit("codewhale", b"", b"http 429").is_none());
    }
}

#[cfg(test)]
mod a2a_attempt_tests {
    use super::await_a2a_worker;
    use crate::adapters::ChatCompletionsOutcome;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn outcome() -> ChatCompletionsOutcome {
        ChatCompletionsOutcome {
            stdout: b"{}".to_vec(),
            stderr: Vec::new(),
            exit_code: Some(0),
            timed_out: false,
            output_limit_exceeded: false,
        }
    }

    #[test]
    fn lane_deadline_starts_when_the_seat_gate_is_acquired() {
        // The worker is queued behind the process-wide seat gate for 600ms,
        // then runs 600ms. With a 1s lane budget, a deadline started at
        // attempt entry would fire while the lane is still queued; starting
        // at gate acquisition it must complete.
        let run_dir = tempfile::tempdir().unwrap();
        let (outcome_tx, outcome_rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(600));
            let _ = gate_tx.send(());
            thread::sleep(Duration::from_millis(600));
            let _ = outcome_tx.send(outcome());
        });
        let (stdout, stderr, exit_code, timed_out, cancelled, limited) =
            await_a2a_worker(&outcome_rx, &gate_rx, run_dir.path(), 1).unwrap();
        assert!(!timed_out, "a queued lane must not burn its timeout budget");
        assert!(!cancelled);
        assert!(!limited);
        assert_eq!(exit_code, Some(0));
        assert_eq!(stdout, b"{}");
        assert!(stderr.is_empty());
    }

    #[test]
    fn lane_deadline_still_fires_after_gate_acquisition() {
        // Once the gate is acquired, the lane gets exactly its budget: a
        // worker that never reports back times out.
        let run_dir = tempfile::tempdir().unwrap();
        let (outcome_tx, outcome_rx) = mpsc::channel::<ChatCompletionsOutcome>();
        let (gate_tx, gate_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = gate_tx.send(());
            // The seat round trip outlives the 1s lane budget by far more
            // than the 500ms poll granularity, so the deadline check can
            // never race the outcome arrival.
            thread::sleep(Duration::from_millis(4000));
            let _ = outcome_tx.send(outcome());
        });
        let started = std::time::Instant::now();
        let (_, _, exit_code, timed_out, _, _) =
            await_a2a_worker(&outcome_rx, &gate_rx, run_dir.path(), 1).unwrap();
        assert!(timed_out);
        assert_eq!(exit_code, None);
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "the deadline must start at gate acquisition, not attempt entry"
        );
    }
}
