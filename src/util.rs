use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(windows)]
use std::path::Component;

use anyhow::{Context, anyhow, bail};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Applies platform process settings shared by every non-interactive helper.
pub fn configure_hidden_process(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

pub fn utc_now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
}

pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let io_path = filesystem_path(path)?;
    let mut file =
        File::open(&io_path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

/// Returns an OS-facing path that supports long absolute names on Windows.
/// Verbatim prefixes stay internal and are never serialized into run artifacts.
pub fn filesystem_path(path: &Path) -> anyhow::Result<PathBuf> {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::{OsStrExt, OsStringExt};

        let absolute = std::path::absolute(path)
            .with_context(|| format!("cannot make {} absolute", path.display()))?;
        let raw = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
        let verbatim = r"\\?\".encode_utf16().collect::<Vec<_>>();
        if raw.starts_with(&verbatim) {
            return Ok(absolute);
        }
        let mut extended = if raw.starts_with(&[b'\\' as u16, b'\\' as u16]) {
            let mut prefix = r"\\?\UNC\".encode_utf16().collect::<Vec<_>>();
            prefix.extend_from_slice(&raw[2..]);
            prefix
        } else {
            let mut prefix = verbatim;
            prefix.extend_from_slice(&raw);
            prefix
        };
        if extended.last() == Some(&0) {
            extended.pop();
        }
        return Ok(PathBuf::from(OsString::from_wide(&extended)));
    }
    #[cfg(not(windows))]
    Ok(path.to_path_buf())
}

pub fn canonical_existing(path: &Path) -> anyhow::Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("path does not exist: {}", path.display()))
}

pub fn ensure_within(root: &Path, candidate: &Path) -> anyhow::Result<()> {
    if !candidate.starts_with(root) {
        bail!(
            "path {} resolves outside allowed root {}",
            candidate.display(),
            root.display()
        );
    }
    Ok(())
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let text = std::str::from_utf8(&bytes)
        .with_context(|| format!("{} is not strict UTF-8", path.display()))?;
    serde_json::from_str(text).with_context(|| format!("invalid JSON in {}", path.display()))
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    create_private_dir_all(parent)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("invalid output path {}", path.display()))?;
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), sequence));
    {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options.open(&temporary)?;
        harden_private_file(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    replace_file(&temporary, path)?;
    harden_private_file(path)?;
    Ok(())
}

pub fn create_private_dir_all(path: &Path) -> anyhow::Result<()> {
    if path.file_name().is_none() {
        bail!("refusing to treat a filesystem root as a private directory");
    }

    let mut missing = Vec::new();
    let mut cursor = path;
    loop {
        match fs::symlink_metadata(cursor) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir() {
                    bail!(
                        "private directory path is not a directory: {}",
                        cursor.display()
                    );
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(cursor.to_path_buf());
                cursor = cursor.parent().ok_or_else(|| {
                    anyhow!(
                        "private directory has no existing ancestor: {}",
                        path.display()
                    )
                })?;
            }
            Err(error) => return Err(error.into()),
        }
    }

    for directory in missing.iter().rev() {
        match create_private_dir(directory) {
            Ok(()) => harden_private_dir(directory)?,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = fs::symlink_metadata(directory)?;
                if !metadata.file_type().is_dir() {
                    bail!(
                        "private directory path was replaced during creation: {}",
                        directory.display()
                    );
                }
            }
            Err(error) => return Err(error.into()),
        }
    }

    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        bail!(
            "private directory leaf is not a directory: {}",
            path.display()
        );
    }
    harden_private_dir(path)
}

fn create_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = fs::DirBuilder::new();
        builder.mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    fs::create_dir(path)
}

fn harden_private_dir(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    harden_windows_path(path, true)?;
    Ok(())
}

pub fn harden_private_file(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    harden_windows_path(path, false)?;
    Ok(())
}

pub fn verify_private_file(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        bail!("private authorization is not a regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            bail!("private authorization permissions must be 0600");
        }
    }
    Ok(())
}

#[cfg(windows)]
struct WindowsHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for WindowsHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
        }
    }
}

#[cfg(windows)]
struct LocalAllocation(*mut core::ffi::c_void);

#[cfg(windows)]
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { windows_sys::Win32::Foundation::LocalFree(self.0) };
        }
    }
}

#[cfg(windows)]
struct WindowsTokenUser {
    _token: WindowsHandle,
    buffer: Vec<usize>,
}

#[cfg(windows)]
impl WindowsTokenUser {
    fn current() -> anyhow::Result<Self> {
        use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, GetLastError};
        use windows_sys::Win32::Security::{
            GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let mut raw_token = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw_token) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("cannot open current process token for private ACL");
        }
        let token = WindowsHandle(raw_token);
        let mut required = 0_u32;
        let first = unsafe {
            GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut required)
        };
        if first != 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER || required == 0 {
            bail!("cannot determine current token SID buffer size");
        }
        let word = std::mem::size_of::<usize>();
        let mut buffer = vec![0_usize; (required as usize).div_ceil(word)];
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                required,
                &mut required,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("cannot read current token SID");
        }
        if required as usize > buffer.len() * word
            || buffer.len() * word < std::mem::size_of::<TOKEN_USER>()
        {
            bail!("current token returned an invalid user SID buffer");
        }
        let result = Self {
            _token: token,
            buffer,
        };
        if unsafe { windows_sys::Win32::Security::IsValidSid(result.sid()) } == 0 {
            bail!("current process token contains an invalid user SID");
        }
        Ok(result)
    }

    fn sid(&self) -> windows_sys::Win32::Security::PSID {
        let user = unsafe {
            &*self
                .buffer
                .as_ptr()
                .cast::<windows_sys::Win32::Security::TOKEN_USER>()
        };
        user.User.Sid
    }
}

#[cfg(windows)]
fn harden_windows_path(path: &Path, directory: bool) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW,
        TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE,
        PROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;

    let user = WindowsTokenUser::current()?;
    let inheritance = if directory {
        OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
    } else {
        0
    };
    let trustee = TRUSTEE_W {
        TrusteeForm: TRUSTEE_IS_SID,
        TrusteeType: TRUSTEE_IS_USER,
        ptstrName: user.sid().cast(),
        ..Default::default()
    };
    let access = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ALL_ACCESS,
        grfAccessMode: SET_ACCESS,
        grfInheritance: inheritance,
        Trustee: trustee,
    };
    let mut raw_acl = std::ptr::null_mut();
    let code = unsafe { SetEntriesInAclW(1, &access, std::ptr::null(), &mut raw_acl) };
    // Some Win32 APIs may return an allocation alongside an error; own any
    // non-null result before inspecting the status code.
    let acl = LocalAllocation(raw_acl.cast());
    if code != ERROR_SUCCESS {
        bail!("cannot build private Windows ACL: OS error {code}");
    }
    let mut wide_path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let code = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            acl.0.cast(),
            std::ptr::null(),
        )
    };
    if code != ERROR_SUCCESS {
        bail!("cannot apply private Windows ACL: OS error {code}");
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> anyhow::Result<()> {
    fs::rename(temporary, destination)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            source.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        return Err(std::io::Error::last_os_error()).with_context(|| {
            format!(
                "cannot atomically replace {} with {}",
                destination.display(),
                temporary.display()
            )
        });
    }
    Ok(())
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

pub fn append_jsonl<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent", path.display()))?;
    create_private_dir_all(parent)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    harden_private_file(path)?;
    let mut record = serde_json::to_vec(value)?;
    record.push(b'\n');
    file.write_all(&record)?;
    file.sync_data()?;
    Ok(())
}

pub fn user_home() -> anyhow::Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .ok_or_else(|| anyhow!("HOME/USERPROFILE is not set"))
}

pub fn command_exists(command: &str) -> bool {
    resolve_command(command).is_some()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandResolution {
    pub command: Option<ResolvedCommand>,
    pub code: CommandResolutionCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandResolutionCode {
    Available,
    NotFound,
    UnsupportedExecutable,
    UnsafeBatchShim,
    ShimUnreadable,
    UnsupportedShim,
    RuntimeMissing,
    EntrypointMissing,
    EntrypointUnsafe,
}

impl CommandResolutionCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::NotFound => "not_found",
            Self::UnsupportedExecutable => "unsupported_executable",
            Self::UnsafeBatchShim => "unsafe_batch_shim",
            Self::ShimUnreadable => "shim_unreadable",
            Self::UnsupportedShim => "unsupported_shim",
            Self::RuntimeMissing => "runtime_missing",
            Self::EntrypointMissing => "entrypoint_missing",
            Self::EntrypointUnsafe => "entrypoint_unsafe",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCommand {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
    pub source: PathBuf,
    pub launcher: CommandLauncher,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandLauncher {
    Native,
    NpmShim,
}

/// Resolves a command exactly as QUINTE will launch it. On Windows, a standard
/// npm PowerShell shim is reduced to its runtime and entrypoint so no shell
/// reparses QUINTE's arguments.
pub fn resolve_command(command: &str) -> Option<ResolvedCommand> {
    resolve_command_with_path(command, std::env::var_os("PATH").as_deref())
}

pub fn diagnose_command(command: &str) -> CommandResolution {
    diagnose_command_with_path(command, std::env::var_os("PATH").as_deref())
}

fn diagnose_command_with_path(
    command: &str,
    search_path: Option<&std::ffi::OsStr>,
) -> CommandResolution {
    if let Some(resolved) = resolve_command_with_path(command, search_path) {
        return CommandResolution {
            command: Some(resolved),
            code: CommandResolutionCode::Available,
            message: "available".into(),
        };
    }

    #[cfg(windows)]
    {
        if let Some((code, message)) = diagnose_windows_command(command, search_path) {
            return CommandResolution {
                command: None,
                code,
                message,
            };
        }
        let command_path = Path::new(command);
        let has_directory = command_path.is_absolute()
            || command_path
                .parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty());
        if has_directory {
            return CommandResolution {
                command: None,
                code: CommandResolutionCode::NotFound,
                message: format!("command path not found: {}", command_path.display()),
            };
        }
    }

    CommandResolution {
        command: None,
        code: CommandResolutionCode::NotFound,
        message: "not found on PATH".into(),
    }
}

fn resolve_command_with_path(
    command: &str,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<ResolvedCommand> {
    let command_path = Path::new(command);
    let has_directory = command_path.is_absolute()
        || command_path
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty());
    if has_directory {
        return resolve_command_candidate(command_path, search_path);
    }
    let search_path = search_path?;
    std::env::split_paths(search_path).find_map(|directory| {
        resolve_command_candidate(&directory.join(command_path), Some(search_path))
    })
}

#[cfg(not(windows))]
fn resolve_command_candidate(
    candidate: &Path,
    _search_path: Option<&std::ffi::OsStr>,
) -> Option<ResolvedCommand> {
    candidate.is_file().then(|| {
        let source = absolute_path(candidate);
        ResolvedCommand {
            program: source.clone(),
            prefix_args: Vec::new(),
            source,
            launcher: CommandLauncher::Native,
        }
    })
}

#[cfg(windows)]
fn resolve_command_candidate(
    candidate: &Path,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<ResolvedCommand> {
    if let Some(extension) = candidate.extension().and_then(|value| value.to_str()) {
        if ["exe", "com"]
            .iter()
            .any(|supported| extension.eq_ignore_ascii_case(supported))
        {
            return candidate.is_file().then(|| direct_command(candidate));
        }
        if extension.eq_ignore_ascii_case("ps1") {
            return resolve_powershell_shim(candidate, search_path);
        }
        if ["cmd", "bat"]
            .iter()
            .any(|supported| extension.eq_ignore_ascii_case(supported))
        {
            return resolve_powershell_shim(&candidate.with_extension("ps1"), search_path);
        }
        return None;
    }
    ["exe", "com"]
        .iter()
        .map(|extension| candidate.with_extension(extension))
        .find(|path| path.is_file())
        .map(|path| direct_command(&path))
        .or_else(|| resolve_powershell_shim(&candidate.with_extension("ps1"), search_path))
}

#[cfg(windows)]
fn diagnose_windows_command(
    command: &str,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<(CommandResolutionCode, String)> {
    let command_path = Path::new(command);
    let has_directory = command_path.is_absolute()
        || command_path
            .parent()
            .is_some_and(|parent| !parent.as_os_str().is_empty());
    if has_directory {
        return diagnose_windows_candidate(command_path, search_path);
    }
    let search_path = search_path?;
    let mut first_rejection = None;
    for directory in std::env::split_paths(search_path) {
        let candidate = directory.join(command_path);
        if resolve_command_candidate(&candidate, Some(search_path)).is_some() {
            return None;
        }
        if first_rejection.is_none() {
            first_rejection = diagnose_windows_candidate(&candidate, Some(search_path));
        }
    }
    first_rejection
}

#[cfg(windows)]
fn diagnose_windows_candidate(
    candidate: &Path,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<(CommandResolutionCode, String)> {
    let shim = match candidate.extension().and_then(|value| value.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("ps1") => candidate.to_path_buf(),
        Some(extension)
            if ["cmd", "bat"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported)) =>
        {
            candidate.with_extension("ps1")
        }
        Some(extension)
            if ["exe", "com"]
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported)) =>
        {
            return candidate.exists().then(|| {
                (
                    CommandResolutionCode::UnsupportedExecutable,
                    format!(
                        "native executable is not a regular file: {}",
                        candidate.display()
                    ),
                )
            });
        }
        Some(_) => {
            return candidate.exists().then(|| {
                (
                    CommandResolutionCode::UnsupportedExecutable,
                    "unsupported executable type".into(),
                )
            });
        }
        None => {
            if ["exe", "com"]
                .iter()
                .map(|extension| candidate.with_extension(extension))
                .any(|path| path.exists())
            {
                return Some((
                    CommandResolutionCode::UnsupportedExecutable,
                    "native executable is not a regular file".into(),
                ));
            }
            candidate.with_extension("ps1")
        }
    };

    if shim.exists() {
        return Some(diagnose_npm_powershell_shim(&shim, search_path));
    }
    ["cmd", "bat"]
        .iter()
        .map(|extension| candidate.with_extension(extension))
        .find(|path| path.exists())
        .map(|path| {
            (
                CommandResolutionCode::UnsafeBatchShim,
                format!(
                    "unsafe batch shim has no validated PowerShell sibling: {}",
                    path.display()
                ),
            )
        })
}

#[cfg(windows)]
fn direct_command(path: &Path) -> ResolvedCommand {
    let source = absolute_path(path);
    ResolvedCommand {
        program: source.clone(),
        prefix_args: Vec::new(),
        source,
        launcher: CommandLauncher::Native,
    }
}

#[cfg(windows)]
fn resolve_powershell_shim(
    path: &Path,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<ResolvedCommand> {
    let source = absolute_path(path);
    let (program, prefix_args) = parse_npm_powershell_shim(&source, search_path)?;
    Some(ResolvedCommand {
        program,
        prefix_args,
        source,
        launcher: CommandLauncher::NpmShim,
    })
}

#[cfg(windows)]
fn parse_npm_powershell_shim(
    path: &Path,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<(PathBuf, Vec<String>)> {
    let script = fs::read_to_string(path).ok()?;
    parse_standard_npm_runtime_shim(path, &script, search_path)
        .or_else(|| parse_standard_npm_native_shim(path, &script))
}

#[cfg(windows)]
fn diagnose_npm_powershell_shim(
    path: &Path,
    search_path: Option<&std::ffi::OsStr>,
) -> (CommandResolutionCode, String) {
    let script = match fs::read_to_string(path) {
        Ok(script) => script,
        Err(error) => {
            return (
                CommandResolutionCode::ShimUnreadable,
                format!("cannot read PowerShell shim {}: {error}", path.display()),
            );
        }
    };
    if let Some((runtime, entrypoint)) = npm_runtime_shim_contract(&script) {
        let directory = match path.parent() {
            Some(directory) => directory,
            None => {
                return (
                    CommandResolutionCode::EntrypointUnsafe,
                    format!(
                        "PowerShell shim has no parent directory: {}",
                        path.display()
                    ),
                );
            }
        };
        if let Some(issue) = diagnose_shim_child_safety(directory, entrypoint, "npm entrypoint") {
            return issue;
        }
        let local_runtime = directory.join(format!("{runtime}.exe"));
        if !local_runtime.is_file()
            && resolve_command_with_path(&format!("{runtime}.exe"), search_path).is_none()
        {
            return (
                CommandResolutionCode::RuntimeMissing,
                format!(
                    "validated npm shim requires {runtime}.exe, but no runtime was found beside the shim or on PATH"
                ),
            );
        }
        return diagnose_shim_child(directory, entrypoint, "npm entrypoint");
    }
    if let Some(program) = npm_native_shim_contract(&script) {
        let Some(directory) = path.parent() else {
            return (
                CommandResolutionCode::EntrypointUnsafe,
                format!(
                    "PowerShell shim has no parent directory: {}",
                    path.display()
                ),
            );
        };
        if let Some(issue) = diagnose_shim_child_safety(directory, program, "npm native entrypoint")
        {
            return issue;
        }
        return diagnose_shim_child(directory, program, "npm native entrypoint");
    }
    (
        CommandResolutionCode::UnsupportedShim,
        format!(
            "PowerShell shim is not a supported standard npm cmd-shim template: {}",
            path.display()
        ),
    )
}

#[cfg(windows)]
fn diagnose_shim_child_safety(
    directory: &Path,
    raw_path: &str,
    label: &str,
) -> Option<(CommandResolutionCode, String)> {
    let Some(relative) = raw_path.strip_prefix("$basedir/") else {
        return Some((
            CommandResolutionCode::EntrypointUnsafe,
            format!("{label} is not relative to the shim directory"),
        ));
    };
    if relative.contains('$') || relative.contains('`') {
        return Some((
            CommandResolutionCode::EntrypointUnsafe,
            format!("{label} contains unsupported PowerShell expansion"),
        ));
    }
    let relative = PathBuf::from(relative.replace('/', "\\"));
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Some((
            CommandResolutionCode::EntrypointUnsafe,
            format!("{label} escapes the shim directory"),
        ));
    }
    let candidate = directory.join(relative);
    if candidate.is_file()
        && let (Ok(directory_canonical), Ok(candidate_canonical)) =
            (directory.canonicalize(), candidate.canonicalize())
        && !candidate_canonical.starts_with(directory_canonical)
    {
        return Some((
            CommandResolutionCode::EntrypointUnsafe,
            format!("{label} resolves outside the shim directory"),
        ));
    }
    None
}

#[cfg(windows)]
fn diagnose_shim_child(
    directory: &Path,
    raw_path: &str,
    label: &str,
) -> (CommandResolutionCode, String) {
    if let Some(issue) = diagnose_shim_child_safety(directory, raw_path, label) {
        return issue;
    }
    let relative = raw_path.strip_prefix("$basedir/").unwrap();
    let relative = PathBuf::from(relative.replace('/', "\\"));
    let candidate = directory.join(relative);
    if !candidate.is_file() {
        return (
            CommandResolutionCode::EntrypointMissing,
            format!("{label} is missing: {}", candidate.display()),
        );
    }
    let Some(directory_canonical) = directory.canonicalize().ok() else {
        return (
            CommandResolutionCode::EntrypointUnsafe,
            format!("cannot resolve shim directory: {}", directory.display()),
        );
    };
    let Some(candidate_canonical) = candidate.canonicalize().ok() else {
        return (
            CommandResolutionCode::EntrypointMissing,
            format!("cannot resolve {label}: {}", candidate.display()),
        );
    };
    if !candidate_canonical.starts_with(directory_canonical) {
        return (
            CommandResolutionCode::EntrypointUnsafe,
            format!("{label} resolves outside the shim directory"),
        );
    }
    (
        CommandResolutionCode::EntrypointUnsafe,
        format!("{label} could not be resolved"),
    )
}

#[cfg(windows)]
fn parse_standard_npm_runtime_shim(
    path: &Path,
    script: &str,
    search_path: Option<&std::ffi::OsStr>,
) -> Option<(PathBuf, Vec<String>)> {
    let (runtime, raw_entry) = npm_runtime_shim_contract(script)?;
    let directory = path.parent()?;
    let program = directory.join(format!("{runtime}.exe"));
    let program = if program.is_file() {
        program.canonicalize().ok()?
    } else {
        resolve_command_with_path(&format!("{runtime}.exe"), search_path)?.program
    };
    let entrypoint = canonical_shim_child(directory, raw_entry)?;
    Some((program, vec![entrypoint.display().to_string()]))
}

#[cfg(windows)]
fn npm_runtime_shim_contract(script: &str) -> Option<(&str, &str)> {
    let pattern = regex::Regex::new(
        r#"(?m)^\s*(?:\$input\s*\|\s*)?&\s+"(?:(?:\$basedir/)?(?P<runtime>node|bun)\$exe)"\s+\s*"(?P<entry>\$basedir/[^"\r\n]+)"\s+\$args\s*$"#,
    )
    .ok()?;
    let captures = pattern.captures_iter(script).collect::<Vec<_>>();
    let first = captures.first()?;
    let runtime = first.name("runtime")?.as_str();
    let raw_entry = first.name("entry")?.as_str();
    if captures.len() != 4
        || captures.iter().any(|capture| {
            capture.name("runtime").map(|value| value.as_str()) != Some(runtime)
                || capture.name("entry").map(|value| value.as_str()) != Some(raw_entry)
        })
        || script != standard_npm_powershell_shim(runtime, raw_entry)?
    {
        return None;
    }
    Some((runtime, raw_entry))
}

#[cfg(windows)]
fn parse_standard_npm_native_shim(path: &Path, script: &str) -> Option<(PathBuf, Vec<String>)> {
    let raw_program = npm_native_shim_contract(script)?;
    let program = canonical_shim_child(path.parent()?, raw_program)?;
    Some((program, Vec::new()))
}

#[cfg(windows)]
fn npm_native_shim_contract(script: &str) -> Option<&str> {
    let pattern = regex::Regex::new(
        r#"(?m)^\s*(?:\$input\s*\|\s*)?&\s+"(?P<program>\$basedir/[^"\r\n]+\.(?:exe|com))"\s+\s*\$args\s*$"#,
    )
    .ok()?;
    let captures = pattern.captures_iter(script).collect::<Vec<_>>();
    let first = captures.first()?;
    let raw_program = first.name("program")?.as_str();
    if captures.len() != 2
        || captures
            .iter()
            .any(|capture| capture.name("program").map(|value| value.as_str()) != Some(raw_program))
        || script != standard_npm_native_powershell_shim(raw_program)?
    {
        return None;
    }
    Some(raw_program)
}

#[cfg(windows)]
fn canonical_shim_child(directory: &Path, raw_path: &str) -> Option<PathBuf> {
    let relative = raw_path.strip_prefix("$basedir/")?;
    if relative.contains('$') || relative.contains('`') {
        return None;
    }
    let relative = PathBuf::from(relative.replace('/', "\\"));
    if !relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let directory_canonical = directory.canonicalize().ok()?;
    let launch_path = absolute_path(&directory.join(relative));
    let canonical = launch_path.canonicalize().ok()?;
    if !canonical.is_file() || !canonical.starts_with(&directory_canonical) {
        return None;
    }
    Some(launch_path)
}

#[cfg(windows)]
fn standard_npm_powershell_shim(runtime: &str, entry: &str) -> Option<String> {
    matches!(runtime, "node" | "bun").then(|| {
        format!(
            r#"#!/usr/bin/env pwsh
$basedir=Split-Path $MyInvocation.MyCommand.Definition -Parent

$exe=""
if ($PSVersionTable.PSVersion -lt "6.0" -or $IsWindows) {{
  # Fix case when both the Windows and Linux builds of Node
  # are installed in the same directory
  $exe=".exe"
}}
$ret=0
if (Test-Path "$basedir/{runtime}$exe") {{
  # Support pipeline input
  if ($MyInvocation.ExpectingInput) {{
    $input | & "$basedir/{runtime}$exe"  "{entry}" $args
  }} else {{
    & "$basedir/{runtime}$exe"  "{entry}" $args
  }}
  $ret=$LASTEXITCODE
}} else {{
  # Support pipeline input
  if ($MyInvocation.ExpectingInput) {{
    $input | & "{runtime}$exe"  "{entry}" $args
  }} else {{
    & "{runtime}$exe"  "{entry}" $args
  }}
  $ret=$LASTEXITCODE
}}
exit $ret
"#
        )
    })
}

#[cfg(windows)]
fn standard_npm_native_powershell_shim(program: &str) -> Option<String> {
    let extension = Path::new(program)
        .extension()
        .and_then(|value| value.to_str())?;
    ["exe", "com"]
        .iter()
        .any(|supported| extension.eq_ignore_ascii_case(supported))
        .then(|| {
            format!(
                r#"#!/usr/bin/env pwsh
$basedir=Split-Path $MyInvocation.MyCommand.Definition -Parent

$exe=""
if ($PSVersionTable.PSVersion -lt "6.0" -or $IsWindows) {{
  # Fix case when both the Windows and Linux builds of Node
  # are installed in the same directory
  $exe=".exe"
}}
# Support pipeline input
if ($MyInvocation.ExpectingInput) {{
  $input | & "{program}"   $args
}} else {{
  & "{program}"   $args
}}
exit $LASTEXITCODE
"#
            )
        })
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

pub fn relative_slash(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn private_directory_creation_preserves_ancestors_and_hardens_new_components() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let temporary = tempfile::tempdir().unwrap();
        let existing = temporary.path().join("existing");
        std::fs::create_dir(&existing).unwrap();
        std::fs::set_permissions(&existing, std::fs::Permissions::from_mode(0o755)).unwrap();
        let nested = existing.join("new/deep/leaf");

        super::create_private_dir_all(&nested).unwrap();

        assert_eq!(std::fs::metadata(&existing).unwrap().mode() & 0o777, 0o755);
        for path in [
            existing.join("new"),
            existing.join("new/deep"),
            nested.clone(),
        ] {
            assert_eq!(
                std::fs::metadata(&path).unwrap().mode() & 0o777,
                0o700,
                "{} was not private",
                path.display()
            );
        }

        std::fs::set_permissions(&nested, std::fs::Permissions::from_mode(0o755)).unwrap();
        super::create_private_dir_all(&nested).unwrap();
        assert_eq!(std::fs::metadata(&nested).unwrap().mode() & 0o777, 0o700);
    }

    #[cfg(windows)]
    #[test]
    fn resolves_windows_npm_shim_without_batch_argument_parsing() {
        const SCRIPT: &str = r#"#!/usr/bin/env pwsh
$basedir=Split-Path $MyInvocation.MyCommand.Definition -Parent

$exe=""
if ($PSVersionTable.PSVersion -lt "6.0" -or $IsWindows) {
  # Fix case when both the Windows and Linux builds of Node
  # are installed in the same directory
  $exe=".exe"
}
$ret=0
if (Test-Path "$basedir/node$exe") {
  # Support pipeline input
  if ($MyInvocation.ExpectingInput) {
    $input | & "$basedir/node$exe"  "$basedir/node_modules/fake/entry.js" $args
  } else {
    & "$basedir/node$exe"  "$basedir/node_modules/fake/entry.js" $args
  }
  $ret=$LASTEXITCODE
} else {
  # Support pipeline input
  if ($MyInvocation.ExpectingInput) {
    $input | & "node$exe"  "$basedir/node_modules/fake/entry.js" $args
  } else {
    & "node$exe"  "$basedir/node_modules/fake/entry.js" $args
  }
  $ret=$LASTEXITCODE
}
exit $ret
"#;
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("agent"), "#!/bin/sh\n").unwrap();
        std::fs::write(temporary.path().join("agent.cmd"), "@exit /b 0\r\n").unwrap();
        std::fs::write(temporary.path().join("agent.ps1"), SCRIPT).unwrap();
        std::fs::write(temporary.path().join("node.exe"), b"runtime").unwrap();
        std::fs::create_dir_all(temporary.path().join("node_modules/fake")).unwrap();
        std::fs::write(
            temporary.path().join("node_modules/fake/entry.js"),
            b"entry",
        )
        .unwrap();
        let search_path = std::env::join_paths([temporary.path()]).unwrap();

        let resolved = super::resolve_command_with_path("agent", Some(&search_path)).unwrap();

        assert_eq!(resolved.source, temporary.path().join("agent.ps1"));
        assert_eq!(
            resolved.program.file_name().unwrap().to_string_lossy(),
            "node.exe"
        );
        assert_eq!(
            std::fs::canonicalize(&resolved.prefix_args[0]).unwrap(),
            std::fs::canonicalize(temporary.path().join("node_modules/fake/entry.js")).unwrap()
        );
        std::fs::remove_file(temporary.path().join("agent.ps1")).unwrap();
        assert!(super::resolve_command_with_path("agent", Some(&search_path)).is_none());
    }

    #[test]
    fn windows_acl_implementation_uses_the_current_token_sid() {
        let source = include_str!("util.rs");
        let implementation = source.split("#[cfg(test)]").next().unwrap();
        for required in [
            "OpenProcessToken",
            "GetCurrentProcess",
            "GetTokenInformation",
            "TokenUser",
            "TRUSTEE_IS_SID",
            "PROTECTED_DACL_SECURITY_INFORMATION",
        ] {
            assert!(
                implementation.contains(required),
                "missing Windows ACL primitive {required}"
            );
        }
        for forbidden in ["GetUserNameW", "TRUSTEE_IS_NAME"] {
            assert!(
                !implementation.contains(forbidden),
                "Windows ACL still depends on ambiguous name lookup {forbidden}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn native_private_acl_is_protected_and_grants_only_the_current_token_sid() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
        use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
        use windows_sys::Win32::Security::{
            ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
            CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
            GetSecurityDescriptorControl, OBJECT_INHERIT_ACE, PSECURITY_DESCRIPTOR, PSID,
            SE_DACL_PROTECTED,
        };
        use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;

        unsafe fn assert_acl(path: &std::path::Path, expected_flags: u32) {
            let mut path = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect::<Vec<_>>();
            let mut dacl: *mut ACL = std::ptr::null_mut();
            let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            let code = unsafe {
                GetNamedSecurityInfoW(
                    path.as_mut_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut dacl,
                    std::ptr::null_mut(),
                    &mut descriptor,
                )
            };
            assert_eq!(code, ERROR_SUCCESS);
            assert!(!descriptor.is_null());
            struct Descriptor(PSECURITY_DESCRIPTOR);
            impl Drop for Descriptor {
                fn drop(&mut self) {
                    unsafe { LocalFree(self.0) };
                }
            }
            let _descriptor = Descriptor(descriptor);

            let mut control = 0_u16;
            let mut revision = 0_u32;
            assert_ne!(
                unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) },
                0
            );
            assert_ne!(control & SE_DACL_PROTECTED, 0);
            assert!(!dacl.is_null());

            let mut info = ACL_SIZE_INFORMATION::default();
            assert_ne!(
                unsafe {
                    GetAclInformation(
                        dacl,
                        (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                        std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
                        AclSizeInformation,
                    )
                },
                0
            );
            assert_eq!(info.AceCount, 1);
            let mut raw_ace = std::ptr::null_mut();
            assert_ne!(unsafe { GetAce(dacl, 0, &mut raw_ace) }, 0);
            let ace = unsafe { &*raw_ace.cast::<ACCESS_ALLOWED_ACE>() };
            assert_eq!(ace.Header.AceType, 0, "expected ACCESS_ALLOWED_ACE");
            assert_eq!(u32::from(ace.Header.AceFlags), expected_flags);
            assert_eq!(ace.Mask & FILE_ALL_ACCESS, FILE_ALL_ACCESS);
            let current = super::WindowsTokenUser::current().unwrap();
            let ace_sid: PSID = (&ace.SidStart as *const u32).cast_mut().cast();
            assert_ne!(unsafe { EqualSid(current.sid(), ace_sid) }, 0);
        }

        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("private");
        super::create_private_dir_all(&directory).unwrap();
        let file = directory.join("secret");
        std::fs::write(&file, b"secret").unwrap();
        super::harden_private_file(&file).unwrap();

        unsafe {
            assert_acl(&directory, OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE);
            assert_acl(&file, 0);
        }
        assert_eq!(std::fs::read(&file).unwrap(), b"secret");
    }

    #[cfg(windows)]
    #[test]
    fn rejects_ambiguous_or_escaping_npm_powershell_shims() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("node.exe"), b"runtime").unwrap();
        std::fs::create_dir_all(temporary.path().join("node_modules/fake")).unwrap();
        std::fs::write(
            temporary.path().join("node_modules/fake/entry.js"),
            b"entry",
        )
        .unwrap();
        let shim = temporary.path().join("agent.ps1");
        let valid =
            super::standard_npm_powershell_shim("node", "$basedir/node_modules/fake/entry.js")
                .unwrap();

        for malicious in [
            format!("Write-Output 'unexpected side effect'\n{valid}"),
            format!("<#\n{valid}#>\n"),
            format!("@'\n{valid}'@\n"),
            format!("if ($false) {{\n{valid}}}\n"),
            valid.replacen("$ret=0\n", "$args=@('--changed') + $args\n$ret=0\n", 1),
            valid.replacen(
                "  } else {\n    & \"$basedir/node$exe\"",
                "  } elseif ($false) {\n    & \"$basedir/node$exe\"",
                1,
            ),
        ] {
            std::fs::write(&shim, malicious).unwrap();
            assert!(super::resolve_powershell_shim(&shim, None).is_none());
        }

        std::fs::write(
            &shim,
            "#!/usr/bin/env pwsh\n& \"$basedir/node$exe\" \"$basedir/node_modules/fake/entry.js\" $args\n& \"$basedir/bun$exe\" \"$basedir/node_modules/fake/entry.js\" $args\n",
        )
        .unwrap();
        assert!(super::resolve_powershell_shim(&shim, None).is_none());

        std::fs::write(
            &shim,
            "#!/usr/bin/env pwsh\n& \"$basedir/node$exe\" \"$basedir/../outside.js\" $args\n",
        )
        .unwrap();
        assert!(super::resolve_powershell_shim(&shim, None).is_none());

        std::fs::write(
            &shim,
            "Write-Output 'not an npm-generated PowerShell shim'\n",
        )
        .unwrap();
        assert!(super::resolve_powershell_shim(&shim, None).is_none());

        std::fs::write(
            &shim,
            super::standard_npm_powershell_shim("node", "$basedir/node_modules/fake/entry.js")
                .unwrap(),
        )
        .unwrap();
        std::fs::remove_file(temporary.path().join("node_modules/fake/entry.js")).unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::windows::fs::symlink_file(
            outside.path(),
            temporary.path().join("node_modules/fake/entry.js"),
        )
        .unwrap();
        assert!(super::resolve_powershell_shim(&shim, None).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn resolves_standard_bun_npm_powershell_shim() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("bun.exe"), b"runtime").unwrap();
        std::fs::create_dir_all(temporary.path().join("node_modules/fake")).unwrap();
        std::fs::write(
            temporary.path().join("node_modules/fake/entry.js"),
            b"entry",
        )
        .unwrap();
        let shim = temporary.path().join("agent.ps1");
        std::fs::write(
            &shim,
            super::standard_npm_powershell_shim("bun", "$basedir/node_modules/fake/entry.js")
                .unwrap(),
        )
        .unwrap();

        let resolved = super::resolve_powershell_shim(&shim, None).unwrap();
        assert!(resolved.program.ends_with("bun.exe"));
        assert_eq!(resolved.launcher, super::CommandLauncher::NpmShim);
    }

    #[cfg(windows)]
    #[test]
    fn resolves_standard_native_npm_powershell_shim() {
        let temporary = tempfile::tempdir().unwrap();
        let program = temporary
            .path()
            .join("node_modules/fake-agent/bin/agent.exe");
        std::fs::create_dir_all(program.parent().unwrap()).unwrap();
        std::fs::write(&program, b"native agent").unwrap();
        let shim = temporary.path().join("agent.ps1");
        let raw_program = "$basedir/node_modules/fake-agent/bin/agent.exe";
        std::fs::write(
            &shim,
            super::standard_npm_native_powershell_shim(raw_program).unwrap(),
        )
        .unwrap();

        let resolved = super::resolve_powershell_shim(&shim, None).unwrap();
        assert_eq!(
            std::fs::canonicalize(&resolved.program).unwrap(),
            std::fs::canonicalize(&program).unwrap()
        );
        assert!(resolved.prefix_args.is_empty());

        let malicious = super::standard_npm_native_powershell_shim(raw_program)
            .unwrap()
            .replacen(
                "# Support pipeline input\n",
                "$args=@('--changed') + $args\n# Support pipeline input\n",
                1,
            );
        std::fs::write(&shim, malicious).unwrap();
        assert!(super::resolve_powershell_shim(&shim, None).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn diagnoses_missing_command_invalid_shim_runtime_and_entrypoint() {
        let temporary = tempfile::tempdir().unwrap();
        let search_path = std::env::join_paths([temporary.path()]).unwrap();

        let missing = super::diagnose_command_with_path("absent", Some(&search_path));
        assert!(missing.command.is_none());
        assert_eq!(missing.code, super::CommandResolutionCode::NotFound);
        assert_eq!(missing.message, "not found on PATH");

        let shim = temporary.path().join("agent.ps1");
        std::fs::write(&shim, "#!/usr/bin/env pwsh\nWrite-Output 'custom'\n").unwrap();
        let invalid = super::diagnose_command_with_path("agent", Some(&search_path));
        assert!(invalid.command.is_none());
        assert_eq!(invalid.code, super::CommandResolutionCode::UnsupportedShim);
        assert!(invalid.message.contains("not a supported standard npm"));

        let entry = "$basedir/node_modules/fake/entry.js";
        std::fs::write(
            &shim,
            super::standard_npm_powershell_shim("node", entry).unwrap(),
        )
        .unwrap();
        let runtime = super::diagnose_command_with_path("agent", Some(&search_path));
        assert!(runtime.command.is_none());
        assert_eq!(runtime.code, super::CommandResolutionCode::RuntimeMissing);
        assert!(runtime.message.contains("requires node.exe"));

        std::fs::write(temporary.path().join("node.exe"), b"runtime").unwrap();
        let entrypoint = super::diagnose_command_with_path("agent", Some(&search_path));
        assert!(entrypoint.command.is_none());
        assert_eq!(
            entrypoint.code,
            super::CommandResolutionCode::EntrypointMissing
        );
        assert!(entrypoint.message.contains("npm entrypoint is missing"));
    }

    #[cfg(windows)]
    #[test]
    fn diagnoses_batch_shim_without_safe_powershell_sibling() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("agent.cmd"), "@exit /b 0\r\n").unwrap();
        let search_path = std::env::join_paths([temporary.path()]).unwrap();

        let diagnosis = super::diagnose_command_with_path("agent", Some(&search_path));
        assert!(diagnosis.command.is_none());
        assert_eq!(
            diagnosis.code,
            super::CommandResolutionCode::UnsafeBatchShim
        );
        assert!(diagnosis.message.contains("unsafe batch shim"));
    }

    #[cfg(windows)]
    #[test]
    fn diagnosis_uses_injected_path_and_prefers_unsafe_entrypoints() {
        let temporary = tempfile::tempdir().unwrap();
        let runtime_dir = temporary.path().join("runtime");
        let shim_dir = temporary.path().join("shim");
        std::fs::create_dir_all(&runtime_dir).unwrap();
        std::fs::create_dir_all(shim_dir.join("node_modules/fake")).unwrap();
        std::fs::write(runtime_dir.join("node.exe"), b"runtime").unwrap();
        std::fs::write(shim_dir.join("node_modules/fake/entry.js"), b"entry").unwrap();
        std::fs::write(
            shim_dir.join("agent.ps1"),
            super::standard_npm_powershell_shim("node", "$basedir/node_modules/fake/entry.js")
                .unwrap(),
        )
        .unwrap();
        let search_path = std::env::join_paths([&shim_dir, &runtime_dir]).unwrap();

        let available = super::diagnose_command_with_path("agent", Some(&search_path));
        assert_eq!(available.code, super::CommandResolutionCode::Available);
        assert!(available.command.is_some());

        std::fs::write(
            shim_dir.join("agent.ps1"),
            super::standard_npm_powershell_shim("node", "$basedir/../outside.js").unwrap(),
        )
        .unwrap();
        std::fs::remove_file(runtime_dir.join("node.exe")).unwrap();
        let unsafe_entry = super::diagnose_command_with_path("agent", Some(&search_path));
        assert_eq!(
            unsafe_entry.code,
            super::CommandResolutionCode::EntrypointUnsafe
        );
    }

    #[cfg(windows)]
    #[test]
    fn diagnosis_continues_past_broken_path_candidate_to_valid_executable() {
        let temporary = tempfile::tempdir().unwrap();
        let broken = temporary.path().join("broken");
        let valid = temporary.path().join("valid");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::write(broken.join("agent.ps1"), "Write-Output 'custom'\n").unwrap();
        std::fs::write(valid.join("agent.exe"), b"native").unwrap();
        let search_path = std::env::join_paths([broken, valid]).unwrap();

        let diagnosis = super::diagnose_command_with_path("agent", Some(&search_path));
        assert_eq!(diagnosis.code, super::CommandResolutionCode::Available);
        assert!(diagnosis.command.unwrap().program.ends_with("agent.exe"));
    }

    #[cfg(unix)]
    #[test]
    fn resolves_unix_extensionless_command_directly() {
        use std::os::unix::fs::PermissionsExt;

        let temporary = tempfile::tempdir().unwrap();
        let command = temporary.path().join("agent");
        std::fs::write(&command, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&command, std::fs::Permissions::from_mode(0o700)).unwrap();
        let search_path = std::env::join_paths([temporary.path()]).unwrap();

        let resolved = super::resolve_command_with_path("agent", Some(&search_path)).unwrap();

        assert_eq!(resolved.source, command);
        assert_eq!(resolved.program, command);
        assert!(resolved.prefix_args.is_empty());
    }
}
