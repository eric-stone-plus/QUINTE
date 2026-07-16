//! Platform credential store for R3 Claude Code isolation.
//!
//! Prefer OS protected storage (macOS Keychain / Windows Credential Manager).
//! `ANTHROPIC_API_KEY` remains a legacy fallback and is reported as non-isolated.

#[cfg(target_os = "macos")]
use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::{Command, Stdio};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

/// Default service / target name for the MiMo token-plan API key.
pub const DEFAULT_CLAUDE_SERVICE: &str = "xiaomi-mimo-token-plan-api-key";
pub const HELPER_AUTHORIZATION_FILE: &str = "credential-helper-authorization.json";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct HelperAuthorization {
    service: String,
    lane_root: PathBuf,
}

pub struct ProtectedSecret(String);

impl ProtectedSecret {
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl Drop for ProtectedSecret {
    fn drop(&mut self) {
        zeroize_string(&mut self.0);
    }
}

/// Where a credential was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    Keychain,
    WindowsCredentialManager,
    EnvironmentVariable,
}

impl CredentialSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keychain => "keychain",
            Self::WindowsCredentialManager => "windows_credential_manager",
            Self::EnvironmentVariable => "environment_variable",
        }
    }

    pub fn isolated(self) -> bool {
        !matches!(self, Self::EnvironmentVariable)
    }
}

impl fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Probe result for doctor and adapter preflight.
#[derive(Debug, Clone, Serialize)]
pub struct CredentialStatus {
    pub available: bool,
    pub source: Option<CredentialSource>,
    pub isolated: bool,
    pub message: String,
}

impl CredentialStatus {
    pub fn missing(message: impl Into<String>) -> Self {
        Self {
            available: false,
            source: None,
            isolated: false,
            message: message.into(),
        }
    }

    pub fn found(source: CredentialSource, message: impl Into<String>) -> Self {
        Self {
            available: true,
            source: Some(source),
            isolated: source.isolated(),
            message: message.into(),
        }
    }
}

/// Probe protected storage first, then legacy environment variable.
pub fn probe(service: &str) -> CredentialStatus {
    if let Some(status) = probe_protected(service) {
        return status;
    }
    if std::env::var_os("ANTHROPIC_API_KEY").is_some() {
        return CredentialStatus::found(
            CredentialSource::EnvironmentVariable,
            "ANTHROPIC_API_KEY available (not isolated; prefer protected store)",
        );
    }
    CredentialStatus::missing(format!(
        "credential missing for service {service:?} (protected store and ANTHROPIC_API_KEY)"
    ))
}

/// Read the secret. Prefers protected store over environment.
pub fn get(service: &str) -> anyhow::Result<String> {
    if let Ok(secret) = get_protected(service) {
        let trimmed = secret.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    if let Ok(value) = std::env::var("ANTHROPIC_API_KEY") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }
    bail!("Claude credential not found in protected store or ANTHROPIC_API_KEY")
}

/// Read only from the OS-protected store; lane helpers never inherit the
/// environment-variable fallback.
pub fn get_isolated(service: &str) -> anyhow::Result<ProtectedSecret> {
    validate_helper_service(service)?;
    normalize_secret(get_protected(service)?).map(ProtectedSecret)
}

pub fn create_helper_authorization(lane_root: &Path, service: &str) -> anyhow::Result<PathBuf> {
    validate_helper_service(service)?;
    let lane_root = lane_root
        .canonicalize()
        .with_context(|| format!("cannot canonicalize lane root {}", lane_root.display()))?;
    let config = lane_root.join("config");
    crate::util::create_private_dir_all(&config)?;
    let path = config.join(HELPER_AUTHORIZATION_FILE);
    let authorization = HelperAuthorization {
        service: service.into(),
        lane_root,
    };
    crate::util::write_json(&path, &authorization)?;
    crate::util::harden_private_file(&path)?;
    Ok(path)
}

pub fn authorize_helper(
    service: &str,
    lane_root: &Path,
    authorization_path: &Path,
) -> anyhow::Result<()> {
    validate_helper_service(service)?;
    let lane_root = lane_root
        .canonicalize()
        .with_context(|| format!("invalid helper lane root {}", lane_root.display()))?;
    let expected_path = lane_root.join("config").join(HELPER_AUTHORIZATION_FILE);
    let authorization_path = authorization_path
        .canonicalize()
        .with_context(|| "invalid helper authorization path")?;
    if authorization_path != expected_path {
        bail!("credential helper authorization is outside its canonical lane root");
    }
    crate::util::verify_private_file(&authorization_path)?;
    let authorization: HelperAuthorization = crate::util::read_json(&authorization_path)?;
    if authorization.lane_root != lane_root || authorization.service != service {
        bail!("credential helper authorization context does not match the lane");
    }
    Ok(())
}

fn validate_helper_service(service: &str) -> anyhow::Result<()> {
    if service != DEFAULT_CLAUDE_SERVICE {
        bail!("credential helper service is not allowed");
    }
    Ok(())
}

fn normalize_secret(mut secret: String) -> anyhow::Result<String> {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        zeroize_string(&mut secret);
        bail!("protected credential is empty");
    }
    if trimmed.len() == secret.len() {
        return Ok(secret);
    }
    let normalized = trimmed.to_string();
    zeroize_string(&mut secret);
    Ok(normalized)
}

fn zeroize_string(value: &mut String) {
    // Volatile writes keep credential erasure from being optimized away.
    for byte in unsafe { value.as_mut_vec() } {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    value.clear();
}

fn probe_protected(service: &str) -> Option<CredentialStatus> {
    #[cfg(target_os = "macos")]
    {
        if macos_keychain_available(service) {
            return Some(CredentialStatus::found(
                CredentialSource::Keychain,
                "Keychain credential available",
            ));
        }
        None
    }
    #[cfg(windows)]
    {
        match windows_cred_available(service) {
            Ok(true) => {
                return Some(CredentialStatus::found(
                    CredentialSource::WindowsCredentialManager,
                    "Windows Credential Manager entry available",
                ));
            }
            Ok(false) | Err(_) => return None,
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = service;
        None
    }
}

fn get_protected(service: &str) -> anyhow::Result<String> {
    #[cfg(target_os = "macos")]
    {
        macos_keychain_get(service)
    }
    #[cfg(windows)]
    {
        return windows_cred_get(service)?.context("Windows Credential Manager entry missing");
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = service;
        bail!("protected credential store is not implemented on this platform");
    }
}

#[cfg(target_os = "macos")]
fn macos_keychain_available(service: &str) -> bool {
    let Ok(identity) = macos_keychain_identity() else {
        return false;
    };
    let status = Command::new("/usr/bin/security")
        .arg("find-generic-password")
        .arg("-a")
        .arg(&identity.account)
        .arg("-s")
        .arg(service)
        .arg("-w")
        .arg(&identity.login_keychain)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    status.is_ok_and(|status| status.success())
}

#[cfg(target_os = "macos")]
fn macos_keychain_get(service: &str) -> anyhow::Result<String> {
    let identity = macos_keychain_identity()?;
    let output = Command::new("/usr/bin/security")
        .arg("find-generic-password")
        .arg("-a")
        .arg(&identity.account)
        .arg("-s")
        .arg(service)
        .arg("-w")
        .arg(&identity.login_keychain)
        .output()
        .context("failed to invoke /usr/bin/security")?;
    if !output.status.success() {
        bail!(
            "Keychain credential missing for service {service}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("Keychain secret is not valid UTF-8")
}

#[cfg(target_os = "macos")]
struct MacosKeychainIdentity {
    account: OsString,
    login_keychain: PathBuf,
}

#[cfg(target_os = "macos")]
fn macos_keychain_identity() -> anyhow::Result<MacosKeychainIdentity> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStringExt;

    let mut buffer_len = match unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) } {
        size if size > 0 => size as usize,
        _ => 16 * 1024,
    };
    loop {
        let mut entry = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut found = std::ptr::null_mut();
        let mut buffer = vec![0_u8; buffer_len];
        let code = unsafe {
            libc::getpwuid_r(
                libc::geteuid(),
                entry.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut found,
            )
        };
        if code == libc::ERANGE && buffer_len < 1024 * 1024 {
            buffer_len *= 2;
            continue;
        }
        if code != 0 {
            return Err(std::io::Error::from_raw_os_error(code))
                .context("failed to resolve the effective macOS user");
        }
        if found.is_null() {
            bail!("effective macOS user has no password database entry");
        }
        let entry = unsafe { entry.assume_init() };
        let account = unsafe { CStr::from_ptr(entry.pw_name) }.to_bytes();
        let home = unsafe { CStr::from_ptr(entry.pw_dir) }.to_bytes();
        if account.is_empty() || home.is_empty() {
            bail!("effective macOS user has an incomplete password database entry");
        }
        let home = PathBuf::from(OsString::from_vec(home.to_vec()));
        if !home.is_absolute() {
            bail!("effective macOS user home is not absolute");
        }
        return Ok(MacosKeychainIdentity {
            account: OsString::from_vec(account.to_vec()),
            login_keychain: home.join("Library/Keychains/login.keychain-db"),
        });
    }
}

#[cfg(windows)]
struct WindowsCredential(*mut windows_sys::Win32::Security::Credentials::CREDENTIALW);

#[cfg(windows)]
impl Drop for WindowsCredential {
    fn drop(&mut self) {
        use windows_sys::Win32::Security::Credentials::CredFree;

        if self.0.is_null() {
            return;
        }
        let credential = unsafe { &mut *self.0 };
        if !credential.CredentialBlob.is_null() {
            for index in 0..credential.CredentialBlobSize as usize {
                unsafe { std::ptr::write_volatile(credential.CredentialBlob.add(index), 0) };
            }
            std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
        }
        unsafe { CredFree(self.0.cast()) };
    }
}

#[cfg(windows)]
fn windows_credential(service: &str) -> anyhow::Result<Option<WindowsCredential>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{CRED_TYPE_GENERIC, CREDENTIALW, CredReadW};

    let target = windows_target(service)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut raw: *mut CREDENTIALW = std::ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut raw) } == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_NOT_FOUND {
            return Ok(None);
        }
        bail!("Windows Credential Manager read failed: OS error {code}");
    }
    if raw.is_null() {
        bail!("Windows Credential Manager returned a null credential");
    }
    Ok(Some(WindowsCredential(raw)))
}

#[cfg(windows)]
fn windows_cred_available(service: &str) -> anyhow::Result<bool> {
    Ok(windows_credential(service)?.is_some())
}

#[cfg(windows)]
fn windows_cred_get(service: &str) -> anyhow::Result<Option<String>> {
    let Some(raw) = windows_credential(service)? else {
        return Ok(None);
    };

    let credential = unsafe { &*raw.0 };
    if credential.CredentialBlobSize > 0 && credential.CredentialBlob.is_null() {
        bail!("Windows credential has a null blob");
    }
    let blob = unsafe {
        std::slice::from_raw_parts(
            credential.CredentialBlob,
            credential.CredentialBlobSize as usize,
        )
    };
    if blob.len() % 2 != 0 {
        bail!("Windows credential has an invalid byte length");
    }
    let mut utf16 = blob
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    let decoded = String::from_utf16(&utf16).context("Windows credential is not valid UTF-16");
    for unit in &mut utf16 {
        unsafe { std::ptr::write_volatile(unit, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    decoded.map(Some)
}

#[cfg(windows)]
fn windows_target(service: &str) -> std::ffi::OsString {
    std::ffi::OsString::from(format!("{service}.quinte"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_source_is_not_isolated() {
        assert!(!CredentialSource::EnvironmentVariable.isolated());
        assert!(CredentialSource::Keychain.isolated());
        assert!(CredentialSource::WindowsCredentialManager.isolated());
    }

    #[test]
    fn missing_status_shape() {
        let status = CredentialStatus::missing("none");
        assert!(!status.available);
        assert!(!status.isolated);
        assert!(status.source.is_none());
    }

    #[test]
    fn helper_authorization_binds_service_root_and_private_file_without_a_secret() {
        let temporary = tempfile::tempdir().unwrap();
        let lane = temporary.path().join("lane");
        crate::util::create_private_dir_all(&lane).unwrap();
        let authorization = create_helper_authorization(&lane, DEFAULT_CLAUDE_SERVICE).unwrap();

        authorize_helper(DEFAULT_CLAUDE_SERVICE, &lane, &authorization).unwrap();
        assert!(authorize_helper("other", &lane, &authorization).is_err());
        let other = temporary.path().join("other");
        crate::util::create_private_dir_all(&other).unwrap();
        assert!(authorize_helper(DEFAULT_CLAUDE_SERVICE, &other, &authorization).is_err());
        let authorization: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(authorization).unwrap()).unwrap();
        assert_eq!(
            authorization
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["lane_root", "service"]
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_windows_target_preserves_previous_keyring_identity() {
        assert_eq!(windows_target("service"), "service.quinte");
    }

    #[cfg(windows)]
    #[test]
    fn native_windows_store_is_visible_across_processes() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Security::Credentials::{
            CRED_PERSIST_SESSION, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredWriteW,
        };

        let service = format!("quinte-cross-process-test-{}", std::process::id());
        let expected = "cross-process-secret";
        let mut target = windows_target(&service)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut username = service
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut blob = expected
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_SESSION,
            UserName: username.as_mut_ptr(),
            ..Default::default()
        };
        assert_ne!(unsafe { CredWriteW(&credential, 0) }, 0);
        blob.fill(0);

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "credential::tests::windows_child_reads_native_store",
                "--ignored",
                "--nocapture",
            ])
            .env("QUINTE_WINDOWS_CREDENTIAL_TEST_SERVICE", &service)
            .output()
            .unwrap();
        unsafe {
            CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0);
        }
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let marker = "QUINTE_CREDENTIAL_TEST_SECRET=";
        let actual = stdout
            .lines()
            .find_map(|line| line.split_once(marker).map(|(_, secret)| secret))
            .expect("child credential marker missing from test harness output");
        assert_eq!(actual, expected);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "spawned only by native_windows_store_is_visible_across_processes"]
    fn windows_child_reads_native_store() {
        let service = std::env::var("QUINTE_WINDOWS_CREDENTIAL_TEST_SERVICE").unwrap();
        println!(
            "QUINTE_CREDENTIAL_TEST_SECRET={}",
            windows_cred_get(&service).unwrap().unwrap()
        );
    }
}
