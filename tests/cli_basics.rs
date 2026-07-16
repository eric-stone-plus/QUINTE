use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn version_prints_to_stdout_and_exits_successfully() {
    let output = Command::cargo_bin("quinte")
        .unwrap()
        .arg("--version")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("quinte {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn help_prints_to_stdout_and_exits_successfully() {
    let output = Command::cargo_bin("quinte")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Protocol-enforcing QUINTE CLI"));
    assert!(stdout.contains("Usage: quinte"));
    assert!(output.stderr.is_empty());
}

#[test]
fn runtime_docs_do_not_bind_behavior_to_cargo_product_versions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "README.md",
        "skills/SKILL.md",
        "specs/CLI.md",
        "specs/PROTOCOL.md",
        "src/doctor.rs",
    ] {
        let text = std::fs::read_to_string(root.join(relative)).unwrap();
        assert!(
            !text.contains("v0.1"),
            "product version leaked into {relative}"
        );
    }
}

#[test]
fn run_commands_reject_malformed_ids_before_any_store_path_is_used() {
    let temporary = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temporary.path()).unwrap();
    std::fs::write(temporary.path().join("policy.json"), b"{}\n").unwrap();

    for run_id in ["../escape", "/absolute", r"..\escape", ".", "run-1"] {
        Command::cargo_bin("quinte")
            .unwrap()
            .env("QUINTE_HOME", temporary.path())
            .args(["status", run_id, "--json"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("run ID"));
    }
}

#[test]
fn runtime_sources_do_not_depend_on_the_cargo_product_version() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for entry in walkdir::WalkDir::new(root.join("src")) {
        let entry = entry.unwrap();
        if !entry.file_type().is_file() {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap();
        assert!(
            !text.contains("CARGO_PKG_VERSION"),
            "runtime source depends on the Cargo product version: {}",
            entry.path().display()
        );
    }
}
