use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

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
    let mut file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
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
    fs::create_dir_all(parent)?;
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
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    replace_file(&temporary, path)?;
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
    fs::create_dir_all(parent)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
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
pub struct ResolvedCommand {
    pub program: PathBuf,
    pub prefix_args: Vec<String>,
    pub source: PathBuf,
}

/// Resolves a command exactly as QUINTE will launch it. On Windows, npm's
/// PowerShell shim avoids the unsafe argument boundary of `.cmd` wrappers.
pub fn resolve_command(command: &str) -> Option<ResolvedCommand> {
    resolve_command_with_path(command, std::env::var_os("PATH").as_deref())
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
        return resolve_command_candidate(command_path);
    }
    let search_path = search_path?;
    std::env::split_paths(search_path)
        .find_map(|directory| resolve_command_candidate(&directory.join(command_path)))
}

#[cfg(not(windows))]
fn resolve_command_candidate(candidate: &Path) -> Option<ResolvedCommand> {
    candidate.is_file().then(|| {
        let source = absolute_path(candidate);
        ResolvedCommand {
            program: source.clone(),
            prefix_args: Vec::new(),
            source,
        }
    })
}

#[cfg(windows)]
fn resolve_command_candidate(candidate: &Path) -> Option<ResolvedCommand> {
    if let Some(extension) = candidate.extension().and_then(|value| value.to_str()) {
        if ["exe", "com"]
            .iter()
            .any(|supported| extension.eq_ignore_ascii_case(supported))
        {
            return candidate.is_file().then(|| direct_command(candidate));
        }
        if extension.eq_ignore_ascii_case("ps1") {
            return resolve_powershell_shim(candidate);
        }
        if ["cmd", "bat"]
            .iter()
            .any(|supported| extension.eq_ignore_ascii_case(supported))
        {
            return resolve_powershell_shim(&candidate.with_extension("ps1"));
        }
        return None;
    }
    ["exe", "com"]
        .iter()
        .map(|extension| candidate.with_extension(extension))
        .find(|path| path.is_file())
        .map(|path| direct_command(&path))
        .or_else(|| resolve_powershell_shim(&candidate.with_extension("ps1")))
}

#[cfg(windows)]
fn direct_command(path: &Path) -> ResolvedCommand {
    let source = absolute_path(path);
    ResolvedCommand {
        program: source.clone(),
        prefix_args: Vec::new(),
        source,
    }
}

#[cfg(windows)]
fn resolve_powershell_shim(path: &Path) -> Option<ResolvedCommand> {
    if !path.is_file() {
        return None;
    }
    let source = absolute_path(path);
    Some(ResolvedCommand {
        program: system_powershell()?,
        prefix_args: vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-File".into(),
            source.display().to_string(),
        ],
        source,
    })
}

#[cfg(windows)]
fn system_powershell() -> Option<PathBuf> {
    for root_name in ["SYSTEMROOT", "WINDIR"] {
        if let Some(root) = std::env::var_os(root_name) {
            let candidate =
                PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe");
            if candidate.is_file() {
                return Some(absolute_path(&candidate));
            }
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join("powershell.exe"))
        .find(|candidate| candidate.is_file())
        .map(|candidate| absolute_path(&candidate))
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
    #[cfg(windows)]
    #[test]
    fn resolves_windows_npm_shim_without_batch_argument_parsing() {
        const SCRIPT: &str = r#"if ($args.Count -ne 2) { exit 90 }
if ($args[0] -cne "line one`nline two & <review>") { exit 91 }
if ($args[1] -cne 'quote"value') { exit 92 }
exit 0
"#;
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(temporary.path().join("agent"), "#!/bin/sh\n").unwrap();
        std::fs::write(temporary.path().join("agent.cmd"), "@exit /b 0\r\n").unwrap();
        std::fs::write(temporary.path().join("agent.ps1"), SCRIPT).unwrap();
        let search_path = std::env::join_paths([temporary.path()]).unwrap();

        let resolved = super::resolve_command_with_path("agent", Some(&search_path)).unwrap();

        assert_eq!(resolved.source, temporary.path().join("agent.ps1"));
        assert_eq!(
            resolved.program.file_name().unwrap().to_string_lossy(),
            "powershell.exe"
        );
        std::fs::remove_file(temporary.path().join("agent.ps1")).unwrap();
        assert!(super::resolve_command_with_path("agent", Some(&search_path)).is_none());
        std::fs::write(temporary.path().join("agent.ps1"), SCRIPT).unwrap();
        assert!(
            std::process::Command::new(resolved.program)
                .args(resolved.prefix_args)
                .arg("line one\nline two & <review>")
                .arg("quote\"value")
                .status()
                .unwrap()
                .success()
        );
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
