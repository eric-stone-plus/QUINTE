//! Platform credential store for R3 Claude Code isolation.
//!
//! Prefer OS protected storage (macOS Keychain / Windows Credential Manager).
//! `ANTHROPIC_API_KEY` remains a legacy fallback and is reported as non-isolated.

use std::fmt;
use std::process::{Command, Stdio};

use anyhow::{Context, bail};
use serde::Serialize;

/// Default service / target name for the MiMo token-plan API key.
pub const DEFAULT_CLAUDE_SERVICE: &str = "xiaomi-mimo-token-plan-api-key";

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

/// Store a secret in the platform protected store.
pub fn set(service: &str, secret: &str) -> anyhow::Result<()> {
    let secret = secret.trim();
    if secret.is_empty() {
        bail!("credential value must be non-empty");
    }
    set_protected(service, secret)
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
        return None;
    }
    #[cfg(windows)]
    {
        match windows_cred_get(service) {
            Ok(Some(_)) => {
                return Some(CredentialStatus::found(
                    CredentialSource::WindowsCredentialManager,
                    "Windows Credential Manager entry available",
                ));
            }
            Ok(None) | Err(_) => return None,
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
        return macos_keychain_get(service);
    }
    #[cfg(windows)]
    {
        return windows_cred_get(service)?
            .context("Windows Credential Manager entry missing");
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = service;
        bail!("protected credential store is not implemented on this platform");
    }
}

fn set_protected(service: &str, secret: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        return macos_keychain_set(service, secret);
    }
    #[cfg(windows)]
    {
        return windows_cred_set(service, secret);
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let _ = (service, secret);
        bail!("protected credential store is not implemented on this platform");
    }
}

#[cfg(target_os = "macos")]
fn macos_keychain_available(service: &str) -> bool {
    let status = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-a",
            &std::env::var("USER").unwrap_or_default(),
            "-s",
            service,
            "-w",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    status.is_ok_and(|status| status.success())
}

#[cfg(target_os = "macos")]
fn macos_keychain_get(service: &str) -> anyhow::Result<String> {
    let output = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-a",
            &std::env::var("USER").unwrap_or_default(),
            "-s",
            service,
            "-w",
        ])
        .output()
        .context("failed to invoke /usr/bin/security")?;
    if !output.status.success() {
        bail!(
            "Keychain credential missing for service {service}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let secret = String::from_utf8(output.stdout).context("Keychain secret is not valid UTF-8")?;
    Ok(secret.trim().to_string())
}

#[cfg(target_os = "macos")]
fn macos_keychain_set(service: &str, secret: &str) -> anyhow::Result<()> {
    let account = std::env::var("USER").unwrap_or_default();
    // Delete existing entry if present; ignore failure.
    let _ = Command::new("/usr/bin/security")
        .args([
            "delete-generic-password",
            "-a",
            &account,
            "-s",
            service,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let status = Command::new("/usr/bin/security")
        .args([
            "add-generic-password",
            "-a",
            &account,
            "-s",
            service,
            "-w",
            secret,
            "-U",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .context("failed to invoke /usr/bin/security add-generic-password")?;
    if !status.success() {
        bail!("failed to store Keychain credential for service {service}");
    }
    Ok(())
}

#[cfg(windows)]
fn windows_cred_get(service: &str) -> anyhow::Result<Option<String>> {
    let entry = keyring::Entry::new("quinte", service)
        .context("cannot open Windows Credential Manager entry")?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("Windows Credential Manager read failed"),
    }
}

#[cfg(windows)]
fn windows_cred_set(service: &str, secret: &str) -> anyhow::Result<()> {
    let entry = keyring::Entry::new("quinte", service)
        .context("cannot open Windows Credential Manager entry")?;
    entry
        .set_password(secret)
        .context("Windows Credential Manager write failed")
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
}
