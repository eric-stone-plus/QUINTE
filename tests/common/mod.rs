use serde_json::{Value, json};

#[cfg_attr(not(test), allow(dead_code))]
use std::path::{Path, PathBuf};
#[cfg_attr(not(test), allow(dead_code))]
use std::process::Command;
#[cfg_attr(not(test), allow(dead_code))]
use std::sync::OnceLock;

#[allow(dead_code)]
pub fn valid_lane_output() -> Value {
    json!({
        "lane_output_version": "0.1.1",
        "task_restatement": "Review the supplied evidence packet.",
        "verdict": "The bounded review completed.",
        "confidence": 0.75,
        "claims": [{
            "id": "claim-1",
            "statement": "The packet was reviewed.",
            "evidence_refs": ["snapshot:file.txt#sha256:test"],
            "confidence": 0.8,
            "category": "coverage"
        }],
        "residuals": [{
            "id": "residual-1",
            "severity": "MEDIUM",
            "residual_type": "evidence_gap",
            "source": "R1/Party A",
            "finding": "One assertion lacks independent confirmation.",
            "evidence_refs": [],
            "disposition": "unresolved",
            "required_closure": "human_review",
            "closure_state": "open",
            "closure_evidence": [],
            "scope": "This review only"
        }],
        "uncertainties": ["The packet may be incomplete."]
    })
}

#[allow(dead_code)]
pub fn compile_fake_agent(output_dir: &Path) -> PathBuf {
    let executable = output_dir.join(if cfg!(windows) {
        "quinte-fake-agent.exe"
    } else {
        "quinte-fake-agent"
    });
    static COMPILED: OnceLock<Vec<u8>> = OnceLock::new();
    let compiled = COMPILED.get_or_init(|| {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_agent.rs");
        let temporary =
            tempfile::tempdir_in(output_dir).expect("fake agent cache directory must be created");
        let cached = temporary
            .path()
            .join(format!("fake-agent{}", std::env::consts::EXE_SUFFIX));
        let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let result = Command::new(rustc)
            .arg("--edition=2024")
            .arg(&source)
            .arg("-o")
            .arg(&cached)
            .output()
            .expect("rustc must be available to compile the fake agent fixture");
        assert!(
            result.status.success(),
            "fake agent compilation failed: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let compiled = std::fs::read(cached).expect("compiled fake agent must be readable");
        temporary
            .close()
            .expect("fake agent cache directory must be removed");
        compiled
    });
    std::fs::write(&executable, compiled).expect("cached fake agent must copy into the fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700))
            .expect("copied fake agent must be executable");
    }
    executable
}
