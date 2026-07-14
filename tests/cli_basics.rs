use assert_cmd::Command;

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
