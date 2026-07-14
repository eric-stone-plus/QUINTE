use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, anyhow, bail};
use base64::Engine;
use chrono::{Duration as ChronoDuration, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;
use walkdir::{DirEntry, WalkDir};

use crate::adapters::{self, Invocation};
use crate::model::{
    ArbiterVerdict, ArtifactBinding, AttachmentEntry, Brief, ClosureState, Disposition,
    HmChallenge, HmResponse, HmSubmissionReceipt, HmSubmissionState, LaneArtifactBinding,
    LaneOutput, MULTIMODAL_MODEL, Policy, R2Packet, R3InputReceipt, RESULT_VERSION, Residual,
    ResultEnvelope, RunError, RunManifest, RunStatus, SnapshotEntry, SnapshotManifest, TEXT_MODEL,
    TrialManifest, TrialPerspective,
};
use crate::policy;
use crate::schema::{
    BRIEF_SCHEMA, HM_RESPONSE_SCHEMA, LANE_OUTPUT_SCHEMA, R3_INPUT_RECEIPT_SCHEMA, RESULT_SCHEMA,
    validate_file, validate_value,
};
use crate::store::{ActiveProcess, Store};
#[cfg(windows)]
use crate::util::configure_hidden_process;
use crate::util::{
    atomic_write, canonical_existing, read_json, relative_slash, sha256_bytes, sha256_file,
    utc_now, write_json,
};

static INTERRUPTED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

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
}

impl<'a> ChildCleanup<'a> {
    fn new(child: Child, store: &'a Store, run_id: &'a str) -> Self {
        Self {
            child,
            store,
            run_id,
            registered: None,
        }
    }

    fn mark_registered(&mut self, process: ActiveProcess) {
        self.registered = Some(process);
    }

    fn unregister(&mut self) -> anyhow::Result<()> {
        if let Some(process) = self.registered.take() {
            self.store.remove_active_process(self.run_id, &process)?;
        }
        Ok(())
    }
}

impl Drop for ChildCleanup<'_> {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            kill_process_tree(self.child.id(), false);
            thread::sleep(Duration::from_millis(500));
            if self.child.try_wait().ok().flatten().is_none() {
                kill_process_tree(self.child.id(), true);
            }
            let _ = self.child.wait();
        }
        if let Some(process) = self.registered.take() {
            let _ = self.store.remove_active_process(self.run_id, &process);
        }
    }
}

#[derive(Debug)]
struct AttemptOutcome {
    output: Option<LaneOutput>,
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
    let lane_route_id = if phase == "R3" { "cc" } else { route_id };
    run_dir
        .join("lanes")
        .join(phase)
        .join(lane_route_id)
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
            retry_state_version: "1.0".into(),
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
    let run_dir = store.run_dir(&manifest.run_id);
    let path = retry_deadline_path(&run_dir, phase, &route.route_id);
    if !path.exists() {
        return Ok(());
    }
    let state: RetryDeadlineState = read_json(&path)?;
    if state.retry_state_version != "1.0"
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
    let path = r2_rate_state_path(&store.run_dir(&manifest.run_id));
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
            rate_state_version: "1.0".into(),
            next_allowed_at: due_at.to_rfc3339(),
            reason: reason.into(),
            route_id: route.route_id.clone(),
            attempt,
        },
    )
}

fn wait_for_r2_pacing(
    store: &Store,
    manifest: &mut RunManifest,
    route: &crate::model::RoutePolicy,
    attempt: usize,
) -> anyhow::Result<()> {
    let run_id = &manifest.run_id;
    let run_dir = store.run_dir(run_id);
    let path = r2_rate_state_path(&run_dir);
    if !path.exists() {
        return Ok(());
    }
    let state: R2RateState = read_json(&path)?;
    if state.rate_state_version != "1.0" {
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
    let brief: Brief = validate_file(&options.brief_path, BRIEF_SCHEMA)?;
    if brief.brief_version != "1.0" || brief.question.trim().is_empty() {
        bail!("brief_version must be 1.0 and question must not be empty");
    }
    policy::validate_for_runtime(policy)?;

    let run_id = Uuid::now_v7().to_string();
    let run_dir = store.create_run_dirs(&run_id)?;
    let canonical_brief = serde_json::to_vec(&brief)?;
    let canonical_policy = serde_json::to_vec(policy)?;
    let brief_sha256 = sha256_bytes(&canonical_brief);
    let policy_sha256 = sha256_bytes(&canonical_policy);
    write_json(&run_dir.join("input/brief.json"), &brief)?;
    write_json(&run_dir.join("input/policy.json"), policy)?;

    let snapshot = build_snapshot(&run_dir, &brief, policy)?;
    let snapshot_bytes = serde_json::to_vec(&snapshot)?;
    let snapshot_sha256 = sha256_bytes(&snapshot_bytes);
    write_json(&run_dir.join("input/snapshot-manifest.json"), &snapshot)?;

    let effective_model = if snapshot.attachments.is_empty() {
        TEXT_MODEL
    } else {
        MULTIMODAL_MODEL
    };
    let runtime_sha256 = runtime_sha256()?;
    let now = utc_now();
    let manifest = RunManifest {
        manifest_version: "1.0".to_string(),
        run_id: run_id.clone(),
        created_at: now.clone(),
        updated_at: now,
        status: RunStatus::Queued,
        brief_sha256,
        policy_sha256,
        snapshot_sha256,
        runtime_sha256,
        protocol_version: "1.0".to_string(),
        effective_model: effective_model.to_string(),
        sandbox_mode: policy.sandbox_mode,
        current_phase: None,
        error: None,
        r3_input_receipt: None,
        hm_challenge: None,
        hm_submission: None,
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
        run_id,
        status: RunStatus::Queued,
        run_dir,
    })
}

/// Starts the scheduler in a separate process so the creating CLI can return immediately.
pub(crate) fn spawn_worker(store: &Store, run_id: &str) -> anyhow::Result<u32> {
    let run_dir = store.run_dir(run_id);
    let diagnostics_dir = run_dir.join("diagnostics");
    fs::create_dir_all(&diagnostics_dir)?;
    let log_path = diagnostics_dir.join("worker.log");
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("cannot open worker log {}", log_path.display()))?;
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
    pub fn start(store: &Store, run_id: &str) -> Self {
        let stopped = Arc::new(AtomicBool::new(false));
        let run_dir = store.run_dir(run_id);
        let worker_stopped = stopped.clone();
        let worker_dir = run_dir.clone();
        let handle = thread::spawn(move || worker_heartbeat_loop(worker_dir, worker_stopped));
        Self {
            stopped,
            handle: Some(handle),
            run_dir,
        }
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
    let _lock = store.lock(run_id)?;
    let mut manifest = store.load_manifest(run_id)?;
    if manifest.status.terminal()
        || manifest.status == RunStatus::WaitingHm
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
    let run_dir = store.run_dir(run_id);
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

    let r2_packet_path = run_dir.join("packets/r2.json");
    if !r2_packet_path.exists() {
        if store.transition(&mut manifest, RunStatus::R2Packet, Some("R2"), json!({}))?
            == RunStatus::Cancelled
        {
            return Ok(RunStatus::Cancelled);
        }
        let packet = build_r2_packet(&manifest, &brief, &r1)?;
        write_json(&r2_packet_path, &packet)?;
    }
    let r2 = if phase_complete(&run_dir, "R2", &policy) {
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
        json!({"accepted": 5}),
    )? == RunStatus::Cancelled
    {
        return Ok(RunStatus::Cancelled);
    }

    let evidence_packet_path = run_dir.join("r3/evidence-packet.json");
    if !evidence_packet_path.exists() {
        let evidence = json!({
            "evidence_packet_version": "1.0",
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
            let attempt = next_attempt(&run_dir, "R3", "cc", policy.max_attempts)?;
            wait_for_retry_deadline(store, &mut manifest, "R3", &policy.auditor, attempt)?;
            let lane_root = run_dir.join(format!("lanes/R3/cc/attempt-{attempt}"));
            let invocation = adapters::build(
                &policy.auditor,
                "R3",
                &manifest.effective_model,
                &evidence_packet_path,
                &lane_root,
                policy.timeout_seconds,
            )?;
            let outcome = run_attempt(
                store,
                &manifest,
                &policy.auditor.party_id,
                &policy.auditor.adapter,
                "R3",
                attempt,
                invocation,
                policy.timeout_seconds,
                policy.max_output_bytes,
                policy.max_attempts,
            )?;
            await_lane_output(
                store,
                &mut manifest,
                &policy,
                "R3",
                &evidence_packet_path,
                &run_dir,
                &policy.auditor,
                attempt,
                outcome,
            )
        })();
        let lane = match lane_result {
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
            Ok(Some(lane)) => lane,
            Ok(None) => return cancel_run(store, &mut manifest),
        };
        let verdict = arbiter_from_lane(lane);
        if let Err(error) = write_json(&cc_path, &verdict) {
            return fail_run(
                store,
                &mut manifest,
                RunStatus::Failed,
                "r3_cc_failed",
                &format!("cannot persist Auditor B verdict: {error:#}"),
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

    let hm_path = run_dir.join("r3/hm-response.json");
    if manifest.hm_submission.is_none() {
        if manifest.hm_challenge.is_none() {
            let challenge = create_hm_challenge(&manifest, &brief, &evidence_packet_path)?;
            manifest.hm_challenge = Some(challenge);
            // Persist scheduler ownership before publishing the request. A crash can then
            // regenerate the same request instead of exposing an orphan challenge.
            store.save_manifest(&manifest)?;
        }
        write_json(
            &run_dir.join("r3/hm-request.json"),
            manifest.hm_challenge.as_ref().unwrap(),
        )?;
        let status = store.transition(
            &mut manifest,
            RunStatus::WaitingHm,
            Some("R3"),
            json!({
                "hm_request": "r3/hm-request.json"
            }),
        )?;
        return Ok(status);
    }

    let submission_ready = match recover_hm_submission(&mut manifest, &brief, &run_dir) {
        Ok(ready) => ready,
        Err(error) => {
            return fail_run(
                store,
                &mut manifest,
                RunStatus::FailedPolicy,
                "hm_submission_drift",
                &error.to_string(),
                false,
            );
        }
    };
    if !submission_ready {
        return store.transition(
            &mut manifest,
            RunStatus::WaitingHm,
            Some("R3"),
            json!({"hm_request": "r3/hm-request.json", "submission": "staging"}),
        );
    }

    if store.transition(&mut manifest, RunStatus::Merging, Some("R3"), json!({}))?
        == RunStatus::Cancelled
    {
        return Ok(RunStatus::Cancelled);
    }
    let hm: HmResponse = validate_file(&hm_path, HM_RESPONSE_SCHEMA)?;
    validate_hm_response_binding(&manifest, &hm, &brief, &evidence_packet_path)?;
    let cc: ArbiterVerdict = read_json(&cc_path)?;
    let result = merge_verdicts(&manifest, &policy, &r1, &r2, &hm.verdict, &cc);
    validate_value(&serde_json::to_value(&result)?, RESULT_SCHEMA)?;
    let finalization = store.finalization_guard(run_id)?;
    if finalization.cancellation_requested() {
        drop(finalization);
        return cancel_run(store, &mut manifest);
    }
    write_json(&run_dir.join("result.json"), &result)?;
    atomic_write(
        &run_dir.join("report.md"),
        render_report(&result).as_bytes(),
    )?;
    manifest.result_sha256 = Some(sha256_file(&run_dir.join("result.json"))?);
    let status = store.transition(
        &mut manifest,
        result.status,
        Some("R3"),
        json!({"result": "result.json"}),
    )?;
    drop(finalization);
    Ok(status)
}

pub fn submit_hm(store: &Store, run_id: &str, response_path: &Path) -> anyhow::Result<RunStatus> {
    let internal = store.run_dir(run_id).join("r3/hm-response.json");
    if response_path
        .canonicalize()
        .ok()
        .is_some_and(|path| path == internal)
    {
        bail!("hm response input must be outside the scheduler-owned run directory");
    }
    let response: HmResponse = validate_file(response_path, HM_RESPONSE_SCHEMA)?;
    submit_hm_response(store, run_id, response)
}

pub fn submit_hm_verdict(
    store: &Store,
    run_id: &str,
    verdict_path: &Path,
) -> anyhow::Result<RunStatus> {
    if verdict_path
        .canonicalize()
        .ok()
        .is_some_and(|path| path.starts_with(store.run_dir(run_id)))
    {
        bail!("hm verdict input must be outside the scheduler-owned run directory");
    }
    let verdict: ArbiterVerdict = read_json(verdict_path)?;
    let manifest = store.load_manifest(run_id)?;
    let challenge = manifest
        .hm_challenge
        .ok_or_else(|| anyhow!("hm challenge is not ready"))?;
    let response = HmResponse {
        hm_response_version: "1.0".into(),
        run_id: challenge.run_id,
        nonce: challenge.nonce,
        policy_sha256: challenge.policy_sha256,
        evidence_packet_sha256: challenge.evidence_packet_sha256,
        input_receipt_sha256: challenge.input_receipt_sha256,
        action_scope: challenge.action_scope,
        verdict,
    };
    validate_value(&serde_json::to_value(&response)?, HM_RESPONSE_SCHEMA)?;
    submit_hm_response(store, run_id, response)
}

fn submit_hm_response(
    store: &Store,
    run_id: &str,
    response: HmResponse,
) -> anyhow::Result<RunStatus> {
    let _lock = store.lock(run_id)?;
    let mut manifest = store.load_manifest(run_id)?;
    let run_dir = store.run_dir(run_id);
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
        .hm_submission
        .as_ref()
        .is_some_and(|receipt| receipt.state == HmSubmissionState::Accepted);
    if let Some(receipt) = &manifest.hm_submission {
        if receipt.response_sha256 != response_sha256 {
            bail!("hm challenge already has a different scheduler-owned submission");
        }
    } else {
        if manifest.status != RunStatus::WaitingHm {
            bail!("run {run_id} is not waiting for Hermes hm");
        }
        if manifest
            .hm_challenge
            .as_ref()
            .is_some_and(|challenge| challenge.consumed)
        {
            bail!("hm challenge was already consumed");
        }
        validate_new_hm_response(
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
        manifest.hm_submission = Some(HmSubmissionReceipt {
            submission_receipt_version: "1.0".into(),
            state: HmSubmissionState::Staging,
            response_ref: "r3/hm-response.json".into(),
            response_sha256: response_sha256.clone(),
            input_receipt_sha256,
            staged_at: utc_now(),
            accepted_at: None,
        });
        store.save_manifest(&manifest)?;
    }

    write_json(&run_dir.join("r3/hm-response.json"), &response)?;
    if !recover_hm_submission(&mut manifest, &brief, &run_dir)? {
        bail!("hm response could not be durably accepted");
    }
    store.save_manifest(&manifest)?;
    if !already_accepted {
        store.event(
            run_id,
            "hm.accepted",
            Some("R3"),
            None,
            None,
            json!({"response_sha256": response_sha256}),
        )?;
    }
    drop(_lock);
    advance(store, run_id)
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

fn build_snapshot(
    run_dir: &Path,
    brief: &Brief,
    policy: &Policy,
) -> anyhow::Result<SnapshotManifest> {
    let snapshot_dir = run_dir.join("input/snapshot");
    let attachments_dir = run_dir.join("input/attachments");
    let mut entries = Vec::new();
    let mut attachments = Vec::new();
    let mut total_bytes = 0_u64;
    for (root_index, root) in brief.evidence_roots.iter().enumerate() {
        let source_root = canonical_existing(root)?;
        if source_root.is_file() {
            copy_snapshot_file(
                &source_root,
                &source_root,
                root_index,
                &snapshot_dir,
                &mut entries,
                &mut total_bytes,
                policy,
            )?;
        } else {
            for item in WalkDir::new(&source_root)
                .follow_links(false)
                .into_iter()
                .filter_entry(snapshot_entry_allowed)
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
        snapshot_version: "1.0".into(),
        created_at: utc_now(),
        entries,
        attachments,
        total_bytes,
    })
}

fn snapshot_entry_allowed(entry: &DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    !matches!(
        name.as_ref(),
        ".git" | "node_modules" | "target" | ".quinte" | ".env"
    ) && !name.ends_with(".key")
        && !name.ends_with(".pem")
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
    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent", target.display()))?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    let mut input = fs::File::open(source)?;
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
        .persist(target)
        .map_err(|error| anyhow!("cannot persist {}: {}", target.display(), error.error))?;
    Ok(())
}

fn read_prefix(path: &Path, max_bytes: u64) -> anyhow::Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
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
    if manifest.hm_submission.is_some() {
        let mut recovered = manifest.clone();
        recover_hm_submission(&mut recovered, brief, run_dir)?;
    }
    Ok(())
}

fn runtime_sha256() -> anyhow::Result<String> {
    sha256_file(&std::env::current_exe()?)
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
    let run_dir = store.run_dir(&manifest.run_id);
    let packet_path = packet_override
        .map(Path::to_path_buf)
        .unwrap_or_else(|| run_dir.join("input/task-packet.json"));
    if packet_override.is_none() && !packet_path.exists() {
        let packet = json!({
            "task_packet_version": "1.0",
            "run_id": manifest.run_id,
            "phase": phase,
            "question": brief.question,
            "context": brief.context,
            "snapshot_manifest": "snapshot-manifest.json",
            "allowed_evidence_prefix": "snapshot://",
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

    if phase == "R1" {
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
                policy.timeout_seconds,
            )?;
            prepared.push((route, attempt, invocation));
        }

        let mut jobs = Vec::new();
        for (route, attempt, invocation) in prepared {
            let job_store = Store::new(store.home().to_path_buf());
            let job_manifest = manifest.clone();
            let party_id = route.party_id.clone();
            let adapter = route.adapter.clone();
            let phase_owned = phase.to_string();
            let timeout = policy.timeout_seconds;
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

        // Always join every R1 lane before interpreting any output. In particular, an
        // invalid early lane must not orphan the remaining children or their credentials.
        let mut finished = Vec::new();
        let mut join_errors = Vec::new();
        for job in jobs {
            match job.handle.join() {
                Ok(Ok(outcome)) => finished.push((job.route, job.attempt, outcome)),
                Ok(Err(error)) => join_errors.push(format!("{}: {error:#}", job.route.party_id)),
                Err(_) => join_errors.push(format!("{}: lane worker panicked", job.route.party_id)),
            }
        }
        if !join_errors.is_empty() {
            bail!("R1 lane execution failed: {}", join_errors.join("; "));
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
            bail!("R1 acceptance failed: {}", acceptance_errors.join("; "));
        }
    } else {
        for route in missing {
            let attempt = next_attempt(&run_dir, phase, &route.route_id, policy.max_attempts)?;
            wait_for_retry_deadline(store, manifest, phase, &route, attempt)?;
            wait_for_r2_pacing(store, manifest, &route, attempt)?;
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
    loop {
        if outcome.cancelled || cancellation_requested(run_dir) {
            cancel_run(store, manifest)?;
            bail!("run cancelled");
        }
        if let Some(output) = outcome.output {
            validate_evidence_refs(&output, run_dir)?;
            let accepted_path =
                run_dir.join(format!("lanes/{phase}/{}/accepted.json", route.route_id));
            write_json(&accepted_path, &output)?;
            store.event(
                &manifest.run_id,
                "lane.accepted",
                Some(phase),
                Some(&route.party_id),
                Some(attempt),
                json!({"route_id": route.route_id, "artifact": relative_slash(accepted_path.strip_prefix(run_dir)?)}),
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
fn await_lane_output(
    store: &Store,
    manifest: &mut RunManifest,
    policy: &Policy,
    phase: &str,
    packet_path: &Path,
    run_dir: &Path,
    route: &crate::model::RoutePolicy,
    mut attempt: usize,
    mut outcome: AttemptOutcome,
) -> anyhow::Result<Option<LaneOutput>> {
    loop {
        if outcome.cancelled || cancellation_requested(run_dir) {
            return Ok(None);
        }
        if let Some(output) = outcome.output {
            validate_evidence_refs(&output, run_dir)?;
            return Ok(Some(output));
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
    if phase == "R2" {
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
    if phase == "R2" {
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
    let lane_route_id = if phase == "R3" { "cc" } else { &route.route_id };
    let lane_root = run_dir.join(format!("lanes/{phase}/{lane_route_id}/attempt-{attempt}"));
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
    if phase == "R2" {
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
    if cancellation_requested(&store.run_dir(&manifest.run_id)) {
        return Ok(AttemptOutcome {
            output: None,
            error: Some("cancelled".into()),
            cancelled: true,
            retry: RetryClass::Never,
        });
    }
    let lane_root = invocation.cwd.clone();
    fs::create_dir_all(&lane_root)?;
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
    let mut command = adapters::spawn_command(&invocation);
    let registration = store.process_registration(&manifest.run_id)?;
    if registration.cancellation_requested() {
        return Ok(AttemptOutcome {
            output: None,
            error: Some("cancelled".into()),
            cancelled: true,
            retry: RetryClass::Never,
        });
    }
    let child = command
        .spawn()
        .with_context(|| format!("cannot start {}", invocation.program))?;
    let mut child = ChildCleanup::new(child, store, &manifest.run_id);
    let pid = child.child.id();
    if let Some(identity) = process_identity(pid) {
        let active_process = ActiveProcess {
            pid,
            identity,
            program: invocation.program.clone(),
        };
        registration.add_process(active_process.clone())?;
        child.mark_registered(active_process);
    } else if child.child.try_wait()?.is_none() {
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
    let stdout_thread = thread::spawn(move || {
        read_capped(
            &mut stdout_reader,
            max_output_bytes,
            &stdout_total,
            &stdout_limited,
        )
    });
    let stderr_total = total_output.clone();
    let stderr_limited = output_limited.clone();
    let stderr_thread = thread::spawn(move || {
        read_capped(
            &mut stderr_reader,
            max_output_bytes,
            &stderr_total,
            &stderr_limited,
        )
    });
    let (status, timed_out, cancelled, output_limit_exceeded) = wait_child(
        &mut child.child,
        timeout_seconds,
        &store.run_dir(&manifest.run_id),
        &output_limited,
    )?;
    child.unregister()?;
    let stdout = stdout_thread
        .join()
        .map_err(|_| anyhow!("stdout reader panicked"))??;
    let stderr = stderr_thread
        .join()
        .map_err(|_| anyhow!("stderr reader panicked"))??;
    let output_limit_exceeded = output_limit_exceeded || output_limited.load(Ordering::SeqCst);
    atomic_write(&lane_root.join("stdout.bin"), &stdout)?;
    atomic_write(&lane_root.join("stderr.bin"), &stderr)?;
    let exit_code = status.and_then(|status| status.code());
    let (mut output, mut error, mut retry) = evaluate_attempt_output(
        adapter,
        invocation.output_kind,
        &stdout,
        &stderr,
        exit_code,
        timed_out,
        cancelled,
        output_limit_exceeded,
        max_output_bytes,
    );
    let mut output_recovered_after_timeout = timed_out && output.is_some();
    if let Some(candidate) = output.as_ref()
        && let Err(validation_error) =
            validate_evidence_refs(candidate, &store.run_dir(&manifest.run_id))
    {
        output = None;
        if timed_out {
            error = Some(format!(
                "timeout; captured LaneOutput failed evidence validation: {validation_error}"
            ));
            retry = RetryClass::TransientTimeout;
        } else {
            error = Some(format!(
                "LaneOutput failed evidence validation: {validation_error}"
            ));
            retry = RetryClass::Never;
        }
        output_recovered_after_timeout = false;
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

#[cfg(test)]
const MIMO_REPETITION_ERROR: &str =
    "Text repetition detected: repeated n-grams after 2 recovery attempts. Session terminated.";

#[allow(clippy::too_many_arguments)]
fn evaluate_attempt_output(
    adapter: &str,
    output_kind: adapters::OutputKind,
    stdout: &[u8],
    stderr: &[u8],
    exit_code: Option<i32>,
    timed_out: bool,
    cancelled: bool,
    output_limit_exceeded: bool,
    max_output_bytes: usize,
) -> (Option<LaneOutput>, Option<String>, RetryClass) {
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
    if matches!(adapter, "mimo" | "fake_mimo")
        && let Some(adapter_error) = adapters::structured_stream_error(output_kind, stdout)
    {
        if is_mimo_repetition_error(&adapter_error.message) {
            return (
                None,
                Some(adapter_error.message),
                RetryClass::TransientAdapter,
            );
        }
        if exit_code != Some(0)
            && let Some(signal) = classify_rate_limit(adapter, stdout, stderr)
        {
            return (
                None,
                Some(format!(
                    "adapter transport was rate limited ({})",
                    signal.source
                )),
                RetryClass::RateLimited(signal),
            );
        }
        return (None, Some(adapter_error.message), RetryClass::Never);
    }
    if timed_out {
        return match adapters::parse_output_with_limit(output_kind, stdout, max_output_bytes) {
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
            (
                None,
                Some(format!("adapter exited {:?}", exit_code)),
                RetryClass::Never,
            )
        };
    }
    match adapters::parse_output_with_limit(output_kind, stdout, max_output_bytes) {
        Ok(output) => (Some(output), None, RetryClass::Never),
        Err(parse_error) => {
            let retry = if matches!(adapter, "codewhale" | "fake_codewhale")
                && adapters::codewhale_completed_without_json_candidate(stdout)
            {
                RetryClass::TransientAdapter
            } else {
                RetryClass::Never
            };
            (None, Some(parse_error.to_string()), retry)
        }
    }
}

fn is_mimo_repetition_error(message: &str) -> bool {
    message.starts_with("Text repetition detected: repeated n-grams after ")
        && message.contains(" recovery attempts")
}

fn classify_rate_limit(adapter: &str, stdout: &[u8], stderr: &[u8]) -> Option<RateLimitSignal> {
    let known_adapter = matches!(
        adapter,
        "codewhale" | "opencode" | "kilo" | "mimo" | "omp" | "claude"
    );
    #[cfg(feature = "test-adapters")]
    let known_adapter = known_adapter || adapter == "fake";
    if !known_adapter {
        return None;
    }
    if adapter != "omp"
        && let Some(retry_after_seconds) = structured_rate_limit(stdout)
    {
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
) -> anyhow::Result<(Option<ExitStatus>, bool, bool, bool)> {
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((
                Some(status),
                false,
                false,
                output_limited.load(Ordering::SeqCst),
            ));
        }
        if cancellation_requested(run_dir) {
            kill_process_tree(child.id(), false);
            thread::sleep(Duration::from_millis(500));
            kill_process_tree(child.id(), true);
            return Ok((child.wait().ok(), false, true, false));
        }
        if output_limited.load(Ordering::SeqCst) {
            kill_process_tree(child.id(), false);
            thread::sleep(Duration::from_millis(100));
            kill_process_tree(child.id(), true);
            return Ok((child.wait().ok(), false, false, true));
        }
        if Instant::now() >= deadline {
            kill_process_tree(child.id(), false);
            thread::sleep(Duration::from_millis(500));
            kill_process_tree(child.id(), true);
            return Ok((child.wait().ok(), true, false, false));
        }
        thread::sleep(Duration::from_millis(100));
    }
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

fn build_r2_packet(
    manifest: &RunManifest,
    brief: &Brief,
    r1: &[LaneAccepted],
) -> anyhow::Result<R2Packet> {
    let mut labels = vec![
        "Participant A",
        "Participant B",
        "Participant C",
        "Participant D",
        "Participant E",
    ];
    let seed = manifest
        .run_id
        .as_bytes()
        .iter()
        .fold(0_usize, |acc, byte| acc + *byte as usize);
    let label_count = labels.len();
    labels.rotate_left(seed % label_count);
    let participants = labels
        .into_iter()
        .zip(r1.iter())
        .map(|(label, lane)| (label.to_string(), lane.output.clone()))
        .collect();
    Ok(R2Packet {
        packet_version: "1.0".into(),
        run_id: manifest.run_id.clone(),
        question: brief.question.clone(),
        participants,
        evidence_manifest_sha256: manifest.snapshot_sha256.clone(),
    })
}

fn create_hm_challenge(
    manifest: &RunManifest,
    brief: &Brief,
    evidence_packet_path: &Path,
) -> anyhow::Result<HmChallenge> {
    let input_receipt_sha256 = manifest
        .r3_input_receipt
        .as_ref()
        .ok_or_else(|| anyhow!("R3 input receipt is missing"))?
        .sha256
        .clone();
    let mut nonce = [0_u8; 32];
    rand::rng().fill_bytes(&mut nonce);
    Ok(HmChallenge {
        challenge_version: "1.0".into(),
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

fn validate_hm_response_binding(
    manifest: &RunManifest,
    response: &HmResponse,
    brief: &Brief,
    evidence_packet_path: &Path,
) -> anyhow::Result<()> {
    let challenge = manifest
        .hm_challenge
        .as_ref()
        .ok_or_else(|| anyhow!("hm challenge is missing"))?;
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
        bail!("hm response does not bind the active challenge");
    }
    Ok(())
}

fn validate_new_hm_response(
    manifest: &RunManifest,
    response: &HmResponse,
    brief: &Brief,
    evidence_packet_path: &Path,
) -> anyhow::Result<()> {
    validate_hm_response_binding(manifest, response, brief, evidence_packet_path)?;
    let challenge = manifest.hm_challenge.as_ref().unwrap();
    if challenge.consumed {
        bail!("hm challenge was already consumed");
    }
    let expiry = chrono::DateTime::parse_from_rfc3339(&challenge.expires_at)?;
    if expiry < Utc::now() {
        bail!("hm challenge expired");
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
        input_receipt_version: "1.0".into(),
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

fn recover_hm_submission(
    manifest: &mut RunManifest,
    brief: &Brief,
    run_dir: &Path,
) -> anyhow::Result<bool> {
    let Some(receipt) = manifest.hm_submission.as_ref() else {
        return Ok(false);
    };
    let response_path = run_dir.join(&receipt.response_ref);
    if !response_path.exists() {
        if receipt.state == HmSubmissionState::Staging {
            return Ok(false);
        }
        bail!("accepted hm response artifact is missing");
    }
    if sha256_file(&response_path)? != receipt.response_sha256 {
        bail!("hm response changed after scheduler staging");
    }
    let input_receipt = manifest
        .r3_input_receipt
        .as_ref()
        .ok_or_else(|| anyhow!("R3 input receipt is missing"))?;
    if receipt.input_receipt_sha256 != input_receipt.sha256 {
        bail!("hm submission does not bind the accepted R3 inputs");
    }
    let response: HmResponse = validate_file(&response_path, HM_RESPONSE_SCHEMA)?;
    validate_hm_response_binding(
        manifest,
        &response,
        brief,
        &run_dir.join("r3/evidence-packet.json"),
    )?;

    if receipt.state == HmSubmissionState::Staging {
        let receipt = manifest.hm_submission.as_mut().unwrap();
        receipt.state = HmSubmissionState::Accepted;
        receipt.accepted_at = Some(utc_now());
        manifest.hm_challenge.as_mut().unwrap().consumed = true;
    } else if !manifest
        .hm_challenge
        .as_ref()
        .is_some_and(|challenge| challenge.consumed)
    {
        bail!("accepted hm receipt has an unconsumed challenge");
    }
    Ok(true)
}

fn response_bytes_with_newline(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.push(b'\n');
    bytes
}

fn arbiter_from_lane(output: LaneOutput) -> ArbiterVerdict {
    ArbiterVerdict {
        arbiter_verdict_version: "1.0".into(),
        summary: output.verdict.clone(),
        recommendation: output.verdict,
        residuals: output.residuals,
    }
}

fn merge_verdicts(
    manifest: &RunManifest,
    policy: &Policy,
    r1: &[LaneAccepted],
    r2: &[LaneAccepted],
    hm: &ArbiterVerdict,
    cc: &ArbiterVerdict,
) -> ResultEnvelope {
    let mut residuals = BTreeMap::<String, Residual>::new();
    let mut dissent = Vec::new();
    for residual in hm.residuals.iter().chain(cc.residuals.iter()) {
        if let Some(existing) = residuals.get(&residual.id)
            && (existing.disposition != residual.disposition
                || existing.closure_state != residual.closure_state
                || existing.finding != residual.finding)
        {
            dissent.push(format!(
                "Residual {} differs between hm and cc; retained as unresolved/open.",
                residual.id
            ));
            let mut merged = existing.clone();
            merged.disposition = Disposition::Unresolved;
            merged.closure_state = ClosureState::Open;
            merged.closure_evidence = Vec::new();
            residuals.insert(residual.id.clone(), merged);
            continue;
        }
        residuals
            .entry(residual.id.clone())
            .or_insert_with(|| residual.clone());
    }
    if hm.recommendation != cc.recommendation {
        dissent.push(format!(
            "hm: {}\ncc: {}",
            hm.recommendation, cc.recommendation
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
                .unwrap()
                .artifact_ref
                .clone(),
            independent_first_pass: true,
        })
        .collect();
    ResultEnvelope {
        result_version: RESULT_VERSION.into(),
        run_id: manifest.run_id.clone(),
        status: RunStatus::Completed,
        summary: hm.summary.clone(),
        recommendation: hm.recommendation.clone(),
        dissent,
        residuals: residuals.into_values().collect(),
        trial_manifest: TrialManifest {
            manifest_version: "1.0".into(),
            base_model_relation: "same_model".into(),
            perspective_count: 5,
            perspectives,
            perturbation_axes: vec![
                "role".into(),
                "reviewer_position".into(),
                "evidence_budget".into(),
            ],
            independence_controls: vec![
                "per_lane_workdir".into(),
                "anonymous_cross_review".into(),
                "scheduler_captured_output".into(),
                "closed_schema".into(),
            ],
            contamination_risks: vec![
                "same_model_error_correlation".into(),
                "process_isolation_is_not_an_os_sandbox".into(),
            ],
            wall_time_seconds: None,
        },
    }
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

fn validate_evidence_refs(output: &LaneOutput, run_dir: &Path) -> anyhow::Result<()> {
    let snapshot: SnapshotManifest = read_json(&run_dir.join("input/snapshot-manifest.json"))?;
    let valid = snapshot
        .entries
        .iter()
        .map(|entry| entry.snapshot_ref.as_str())
        .collect::<Vec<_>>();
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
        if reference.is_empty() {
            continue;
        }
        if !reference.starts_with("snapshot://") || !valid.contains(&reference.as_str()) {
            bail!("unresolvable evidence reference: {reference}");
        }
    }
    Ok(())
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
    let signal = if force { "KILL" } else { "TERM" };
    let group_status = std::process::Command::new("kill")
        .args([format!("-{signal}"), format!("-{pid}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if !group_status.is_ok_and(|status| status.success()) {
        let _ = std::process::Command::new("kill")
            .args([format!("-{signal}"), pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[cfg(windows)]
fn kill_process_tree(pid: u32, force: bool) {
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
            &store.run_dir(run_id).join("diagnostics/wait-handler-ready"),
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
        if status == RunStatus::WaitingHm {
            return Ok(status);
        }
        ensure_worker_liveness(store, run_id)?;
        thread::sleep(poll_interval);
    }
}

pub fn verify_result_integrity(store: &Store, run_id: &str) -> anyhow::Result<()> {
    let manifest = store.load_manifest(run_id)?;
    if !matches!(manifest.status, RunStatus::Completed | RunStatus::Degraded) {
        return Ok(());
    }
    let expected = manifest
        .result_sha256
        .as_deref()
        .ok_or_else(|| anyhow!("completed run is missing its result digest"))?;
    let path = store.run_dir(run_id).join("result.json");
    let actual = sha256_file(&path).context("completed run result is missing")?;
    if actual != expected {
        bail!("completed run result integrity check failed");
    }
    Ok(())
}

fn ensure_worker_liveness(store: &Store, run_id: &str) -> anyhow::Result<()> {
    let run_dir = store.run_dir(run_id);
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
        if status.terminal() || status == RunStatus::WaitingHm {
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
        MIMO_REPETITION_ERROR, RateLimitSignal, RetryClass, classify_rate_limit,
        evaluate_attempt_output, next_attempt, retry_allowed, retry_schedule,
        validate_evidence_refs,
    };
    use crate::adapters::OutputKind;
    use crate::model::{LaneOutput, SnapshotEntry, SnapshotManifest};
    use crate::policy::default_policy;

    const VALID_SNAPSHOT_REF: &str = "snapshot://evidence.txt";

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
        let output = lane_output_with_evidence(VALID_SNAPSHOT_REF, VALID_SNAPSHOT_REF);

        validate_evidence_refs(&output, temporary.path()).unwrap();
    }

    #[test]
    fn snapshot_reference_with_arbitrary_fragment_is_rejected() {
        let temporary = evidence_test_run_dir();
        let output = lane_output_with_evidence(
            &format!("{VALID_SNAPSHOT_REF}#arbitrary"),
            VALID_SNAPSHOT_REF,
        );

        let error = validate_evidence_refs(&output, temporary.path()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unresolvable evidence reference")
        );
    }

    #[test]
    fn invalid_closure_evidence_reference_is_rejected() {
        let temporary = evidence_test_run_dir();
        let output = lane_output_with_evidence(VALID_SNAPSHOT_REF, "snapshot://missing.txt");

        let error = validate_evidence_refs(&output, temporary.path()).unwrap_err();
        assert!(error.to_string().contains("snapshot://missing.txt"));
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
            &bytes,
            b"",
            None,
            true,
            false,
            false,
            bytes.len(),
        );
        assert_eq!(output.unwrap().verdict, "complete before timeout");
        assert_eq!(error, None);
        assert_eq!(retry, RetryClass::Never);

        let (output, error, retry) = evaluate_attempt_output(
            "fake",
            OutputKind::DirectJson,
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
    fn mimo_repetition_event_is_retryable_without_matching_model_prose() {
        let event = serde_json::json!({
            "type": "error",
            "error": {"name": "UnknownError", "data": {"message": MIMO_REPETITION_ERROR}}
        });
        let bytes = serde_json::to_vec(&event).unwrap();
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            &bytes,
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert_eq!(error.as_deref(), Some(MIMO_REPETITION_ERROR));
        assert_eq!(retry, RetryClass::TransientAdapter);

        let prose = serde_json::json!({"type": "content", "part": {"text": MIMO_REPETITION_ERROR}});
        let bytes = serde_json::to_vec(&prose).unwrap();
        let (_, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            &bytes,
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
    fn noncanonical_mimo_error_preserves_message_without_retrying() {
        let event = serde_json::json!({
            "type": "error",
            "error": {"name": "UnknownError", "data": {"message": "permanent model failure"}}
        });
        let bytes = serde_json::to_vec(&event).unwrap();
        let (_, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            &bytes,
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert_eq!(error.as_deref(), Some("permanent model failure"));
        assert_eq!(retry, RetryClass::Never);
    }

    #[test]
    fn completed_codewhale_without_lane_output_is_bounded_retryable() {
        let stdout = format!(
            "{}\n{}\n{}\n",
            serde_json::json!({"type": "content", "content": "analysis without final JSON"}),
            serde_json::json!({"type": "metadata", "meta": {"status": "completed"}}),
            serde_json::json!({"type": "done"})
        );
        let (output, error, retry) = evaluate_attempt_output(
            "codewhale",
            OutputKind::CodewhaleStream,
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
    fn typed_mimo_error_wins_over_an_earlier_valid_candidate() {
        let valid = serde_json::json!({
            "lane_output_version": "1.0",
            "task_restatement": "bounded task",
            "verdict": "partial output",
            "confidence": 0.9,
            "claims": [],
            "residuals": [],
            "uncertainties": []
        });
        let content = serde_json::json!({"type": "content", "part": {"text": valid.to_string()}});
        let error_event = serde_json::json!({
            "type": "error",
            "error": {"name": "UnknownError", "data": {"message": MIMO_REPETITION_ERROR}}
        });
        let bytes = format!("{content}\n{error_event}\n").into_bytes();
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            &bytes,
            b"",
            Some(0),
            false,
            false,
            false,
            4096,
        );
        assert!(output.is_none());
        assert_eq!(error.as_deref(), Some(MIMO_REPETITION_ERROR));
        assert_eq!(retry, RetryClass::TransientAdapter);
    }

    #[test]
    fn typed_mimo_error_wins_when_transport_times_out_or_exits_nonzero() {
        let error_event = serde_json::json!({
            "type": "error",
            "error": {
                "name": "UnknownError",
                "data": {
                    "message": "Text repetition detected: repeated n-grams after 2 recovery attempts; session terminated by detector."
                }
            }
        });
        for (exit_code, timed_out) in [(None, true), (Some(1), false)] {
            let (output, error, retry) = evaluate_attempt_output(
                "mimo",
                OutputKind::JsonEvents,
                error_event.to_string().as_bytes(),
                b"",
                exit_code,
                timed_out,
                false,
                false,
                4096,
            );
            assert!(output.is_none());
            assert!(error.unwrap().starts_with("Text repetition detected"));
            assert_eq!(retry, RetryClass::TransientAdapter);
        }
    }

    #[test]
    fn typed_mimo_rate_limit_keeps_rate_limit_retry_precedence() {
        let event = serde_json::json!({
            "type": "error",
            "error": {
                "name": "RateLimitError",
                "data": {
                    "message": "Too Many Requests",
                    "status": 429,
                    "retry_after": 7
                }
            }
        });
        let (output, error, retry) = evaluate_attempt_output(
            "mimo",
            OutputKind::JsonEvents,
            event.to_string().as_bytes(),
            b"",
            Some(1),
            false,
            false,
            false,
            4096,
        );

        assert!(output.is_none());
        assert_eq!(
            error.as_deref(),
            Some("adapter transport was rate limited (adapter_structured_error)")
        );
        assert_eq!(
            retry,
            RetryClass::RateLimited(RateLimitSignal {
                source: "adapter_structured_error",
                retry_after_seconds: Some(7),
            })
        );
    }

    #[test]
    fn rate_limit_classification_requires_failed_transport_evidence() {
        let structured = br#"{"error":{"type":"rate_limit_error","retry_after":7}}"#;
        assert_eq!(
            classify_rate_limit("mimo", structured, b""),
            Some(RateLimitSignal {
                source: "adapter_structured_error",
                retry_after_seconds: Some(7),
            })
        );
        assert_eq!(
            classify_rate_limit(
                "codewhale",
                b"ordinary output",
                b"HTTP 429 Too Many Requests\nRetry-After: 9\n",
            ),
            Some(RateLimitSignal {
                source: "adapter_stderr_marker",
                retry_after_seconds: Some(9),
            })
        );
        assert_eq!(
            classify_rate_limit("mimo", b"a model discussed 429", b""),
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
        let first = retry_schedule(retry, &policy, "run", "R2", "mimo", 1).unwrap();
        let same = retry_schedule(retry, &policy, "run", "R2", "mimo", 1).unwrap();
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
}
