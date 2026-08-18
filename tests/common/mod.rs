use serde_json::{Value, json};

#[cfg_attr(not(test), allow(dead_code))]
use std::fs;
#[cfg_attr(not(test), allow(dead_code))]
use std::path::{Path, PathBuf};
#[cfg_attr(not(test), allow(dead_code))]
use std::process::Command;
#[cfg_attr(not(test), allow(dead_code))]
use std::sync::OnceLock;

/// A test-only temporary directory that can clean up QUINTE's read-only lane
/// input trees. Production adapter staging deliberately changes copied input
/// to 0400/0500 (or sets the Windows read-only attribute), which makes
/// `tempfile::TempDir`'s default Drop cleanup fail on some platforms.
#[allow(dead_code)]
pub struct TestTempDir {
    inner: Option<::tempfile::TempDir>,
}

#[allow(dead_code)]
impl TestTempDir {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            inner: Some(::tempfile::tempdir()?),
        })
    }

    pub fn path(&self) -> &Path {
        self.inner
            .as_ref()
            .expect("test temporary directory was already closed")
            .path()
    }
}

#[allow(dead_code)]
impl std::ops::Deref for TestTempDir {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

#[allow(dead_code)]
impl AsRef<Path> for TestTempDir {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

#[allow(dead_code)]
impl Drop for TestTempDir {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else {
            return;
        };
        let root = inner.path().to_path_buf();
        if root.exists() {
            make_tree_writable(&root);
        }
        if let Err(error) = inner.close() {
            eprintln!("warning: test temporary directory cleanup failed: {error}");
        }
    }
}

#[allow(dead_code)]
fn make_tree_writable(root: &Path) {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let Ok(entry) = entry else {
            continue;
        };
        if entry.file_type().is_symlink() {
            continue;
        }
        let Ok(metadata) = fs::metadata(entry.path()) else {
            continue;
        };
        let mut permissions = metadata.permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(if metadata.is_dir() { 0o700 } else { 0o600 });
        }
        #[cfg(windows)]
        permissions.set_readonly(false);
        let _ = fs::set_permissions(entry.path(), permissions);
    }
}

/// Namespace-compatible shim for existing `tempfile::tempdir()` calls in
/// `run_e2e.rs`; the wrapper's `.path()` API stays identical to `TempDir`.
#[allow(dead_code)]
pub mod tempfile {
    use super::TestTempDir;

    pub fn tempdir() -> std::io::Result<TestTempDir> {
        TestTempDir::new()
    }
}

#[allow(dead_code)]
pub fn valid_lane_output() -> Value {
    json!({
        "lane_output_version": "1.0",
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
        let temporary = ::tempfile::tempdir_in(output_dir)
            .expect("fake agent cache directory must be created");
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
