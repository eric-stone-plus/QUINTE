use serde::Serialize;

use crate::adapters;
use crate::contract::DOCTOR_VERSION;
use crate::model::{Policy, SandboxMode};
use crate::util::command_exists;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub doctor_version: &'static str,
    pub ok: bool,
    pub platform: String,
    pub checks: Vec<serde_json::Value>,
}

pub fn run(policy: &Policy) -> DoctorReport {
    let mut checks = adapters::doctor(policy);
    checks.push(serde_json::json!({
        "name": "process_group_supervision",
        "ok": cfg!(any(unix, windows)),
        "message": "per-lane process tree termination is available"
    }));
    checks.push(serde_json::json!({
        "name": "silent_child_launch",
        "ok": true,
        "message": if cfg!(windows) {
            "Windows CREATE_NO_WINDOW is applied to every non-interactive helper and lane process"
        } else {
            "Unix child processes do not create console windows; no CREATE_NO_WINDOW equivalent required"
        }
    }));
    checks.push(serde_json::json!({
        "name": "os_sandbox",
        "ok": false,
        "severity": "warning",
        "message": "process mode isolates cwd/HOME/state and tool permissions but does not provide a kernel-enforced filesystem/network sandbox"
    }));
    checks.push(serde_json::json!({
        "name": "git",
        "ok": command_exists("git"),
        "message": "optional snapshot provenance tool"
    }));
    let required_ok = checks
        .iter()
        .filter(|check| check.get("party_id").is_some())
        .all(|check| {
            check
                .get("ok")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        });
    let sandbox_ok = policy.sandbox_mode != SandboxMode::Strict;
    if !sandbox_ok {
        checks.push(serde_json::json!({
            "name": "strict_sandbox_policy",
            "ok": false,
            "severity": "error",
            "message": "strict mode is fail-closed because no supported kernel sandbox backend is available"
        }));
    }
    DoctorReport {
        doctor_version: DOCTOR_VERSION,
        ok: required_ok && sandbox_ok,
        platform: std::env::consts::OS.to_string(),
        checks,
    }
}
