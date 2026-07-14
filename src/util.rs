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
            launcher: CommandLauncher::Native,
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
        launcher: CommandLauncher::Native,
    }
}

#[cfg(windows)]
fn resolve_powershell_shim(path: &Path) -> Option<ResolvedCommand> {
    let source = absolute_path(path);
    let (program, prefix_args) = parse_npm_powershell_shim(&source)?;
    Some(ResolvedCommand {
        program,
        prefix_args,
        source,
        launcher: CommandLauncher::NpmShim,
    })
}

#[cfg(windows)]
fn parse_npm_powershell_shim(path: &Path) -> Option<(PathBuf, Vec<String>)> {
    let script = fs::read_to_string(path).ok()?;
    parse_standard_npm_runtime_shim(path, &script)
        .or_else(|| parse_standard_npm_native_shim(path, &script))
}

#[cfg(windows)]
fn parse_standard_npm_runtime_shim(path: &Path, script: &str) -> Option<(PathBuf, Vec<String>)> {
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

    let directory = path.parent()?;
    let program = directory.join(format!("{runtime}.exe"));
    let program = if program.is_file() {
        program.canonicalize().ok()?
    } else {
        resolve_command_with_path(
            &format!("{runtime}.exe"),
            std::env::var_os("PATH").as_deref(),
        )?
        .program
    };
    let entrypoint = canonical_shim_child(directory, raw_entry)?;
    Some((program, vec![entrypoint.display().to_string()]))
}

#[cfg(windows)]
fn parse_standard_npm_native_shim(path: &Path, script: &str) -> Option<(PathBuf, Vec<String>)> {
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

    let program = canonical_shim_child(path.parent()?, raw_program)?;
    Some((program, Vec::new()))
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
            assert!(super::resolve_powershell_shim(&shim).is_none());
        }

        std::fs::write(
            &shim,
            "#!/usr/bin/env pwsh\n& \"$basedir/node$exe\" \"$basedir/node_modules/fake/entry.js\" $args\n& \"$basedir/bun$exe\" \"$basedir/node_modules/fake/entry.js\" $args\n",
        )
        .unwrap();
        assert!(super::resolve_powershell_shim(&shim).is_none());

        std::fs::write(
            &shim,
            "#!/usr/bin/env pwsh\n& \"$basedir/node$exe\" \"$basedir/../outside.js\" $args\n",
        )
        .unwrap();
        assert!(super::resolve_powershell_shim(&shim).is_none());

        std::fs::write(
            &shim,
            "Write-Output 'not an npm-generated PowerShell shim'\n",
        )
        .unwrap();
        assert!(super::resolve_powershell_shim(&shim).is_none());

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
        assert!(super::resolve_powershell_shim(&shim).is_none());
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

        let resolved = super::resolve_powershell_shim(&shim).unwrap();
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

        let resolved = super::resolve_powershell_shim(&shim).unwrap();
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
        assert!(super::resolve_powershell_shim(&shim).is_none());
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
