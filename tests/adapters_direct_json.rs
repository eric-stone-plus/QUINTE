mod common;

use std::fs;

use quinte::adapters::{
    OutputKind, build, cleanup_sensitive, parse_output, parse_output_with_limit, spawn_command,
};
use quinte::model::{RoutePolicy, TEXT_MODEL};

fn fake_route(executable: &std::path::Path) -> RoutePolicy {
    RoutePolicy {
        party_id: "Party A".into(),
        route_id: "fake-a".into(),
        adapter: "fake".into(),
        executable: executable.display().to_string(),
        required: true,
    }
}

fn create_run_packet(run_dir: &std::path::Path) -> std::path::PathBuf {
    fs::create_dir_all(run_dir.join("input/snapshot/root-0")).unwrap();
    fs::create_dir_all(run_dir.join("input/attachments")).unwrap();
    fs::write(
        run_dir.join("input/snapshot/root-0/evidence.txt"),
        b"evidence\n",
    )
    .unwrap();
    fs::write(run_dir.join("input/snapshot-manifest.json"), b"{}\n").unwrap();
    let packet = run_dir.join("packet.json");
    fs::write(&packet, b"{}\n").unwrap();
    packet
}

#[test]
fn direct_json_parses_valid_lane_output() {
    let bytes = serde_json::to_vec(&common::valid_lane_output()).unwrap();
    let output = parse_output(OutputKind::DirectJson, &bytes).unwrap();

    assert_eq!(output.lane_output_version, "1.0");
    assert_eq!(output.verdict, "The bounded review completed.");
}

#[test]
fn direct_json_rejects_unknown_fields_invalid_utf8_and_oversize() {
    let mut unknown = common::valid_lane_output();
    unknown["spawn_agent"] = serde_json::json!("Party F");
    let error = parse_output(
        OutputKind::DirectJson,
        &serde_json::to_vec(&unknown).unwrap(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("schema validation failed"));

    let error = parse_output(OutputKind::DirectJson, &[0xff]).unwrap_err();
    assert!(error.to_string().contains("payload is not strict UTF-8"));

    let error =
        parse_output(OutputKind::DirectJson, &vec![b' '; 16 * 1024 * 1024 + 1]).unwrap_err();
    assert!(error.to_string().contains("exceeds hard 16 MiB limit"));
}

#[test]
fn policy_output_limit_is_enforced_before_schema_parsing() {
    let bytes = serde_json::to_vec(&common::valid_lane_output()).unwrap();
    let error =
        parse_output_with_limit(OutputKind::DirectJson, &bytes, bytes.len() - 1).unwrap_err();
    assert!(error.to_string().contains("policy limit"));
}

#[test]
fn full_supported_policy_limit_is_not_shadowed_by_a_lower_parser_cap() {
    let bytes = serde_json::to_vec(&common::valid_lane_output()).unwrap();
    let output = parse_output_with_limit(OutputKind::DirectJson, &bytes, 16 * 1024 * 1024).unwrap();
    assert_eq!(output.lane_output_version, "1.0");
}

#[test]
fn fake_agent_runs_through_build_spawn_and_direct_json_parse() {
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let run_dir = temporary.path().join("run");
    let packet = create_run_packet(&run_dir);
    let lane_root = run_dir.join("lane");
    let route = fake_route(&executable);

    let invocation = build(&route, "R1", TEXT_MODEL, &packet, &lane_root, 30).unwrap();
    assert_eq!(invocation.output_kind, OutputKind::DirectJson);
    assert_eq!(invocation.cwd, lane_root);
    assert_eq!(invocation.args[0], "R1");
    assert_eq!(invocation.args[1], "Party A");
    assert_eq!(
        fs::canonicalize(std::path::PathBuf::from(&invocation.args[2])).unwrap(),
        fs::canonicalize(lane_root.join("input").join("packet.json")).unwrap()
    );
    assert_eq!(
        fs::read(lane_root.join("input/snapshot/root-0/evidence.txt")).unwrap(),
        b"evidence\n"
    );
    assert!(
        fs::metadata(lane_root.join("input/packet.json"))
            .unwrap()
            .permissions()
            .readonly()
    );

    let output = spawn_command(&invocation).output().unwrap();
    assert!(
        output.status.success(),
        "fake agent failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed = parse_output(invocation.output_kind, &output.stdout).unwrap();
    assert_eq!(parsed.confidence, 0.75);
    cleanup_sensitive(&invocation).unwrap();
}

#[cfg(windows)]
#[test]
fn fake_agent_runs_through_windows_npm_style_shim() {
    let temporary = tempfile::tempdir().unwrap();
    let shim = temporary.path().join("quinte-fake-shim");
    fs::write(&shim, "#!/bin/sh\n").unwrap();
    fs::write(shim.with_extension("cmd"), "@exit /b 99\r\n").unwrap();
    fs::write(
        shim.with_extension("ps1"),
        r#"if ($args.Count -ne 3) { exit 90 }
if ($args[0] -cne 'R1') { exit 91 }
if ($args[1] -cne 'Party A') { exit 92 }
if (-not $args[2].EndsWith('input\packet.json')) { exit 93 }
[Console]::Out.Write('{"lane_output_version":"1.0","task_restatement":"Review the supplied evidence packet.","verdict":"The bounded review completed.","confidence":0.75,"claims":[],"residuals":[],"uncertainties":[]}')
exit 0
"#,
    )
    .unwrap();
    let run_dir = temporary.path().join("run-shim");
    let packet = create_run_packet(&run_dir);
    let lane_root = run_dir.join("lane");
    let route = fake_route(&shim);

    let invocation = build(&route, "R1", TEXT_MODEL, &packet, &lane_root, 30).unwrap();

    assert!(invocation.program.ends_with("powershell.exe"));
    assert!(invocation.args.iter().any(|arg| arg.ends_with(".ps1")));
    let output = spawn_command(&invocation).output().unwrap();
    assert!(
        output.status.success(),
        "fake npm-style shim failed: status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !output.stdout.is_empty(),
        "fake npm-style shim produced no output: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed = parse_output(invocation.output_kind, &output.stdout).unwrap();
    assert_eq!(parsed.confidence, 0.75);
}

#[test]
fn fake_agent_fault_modes_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let run_dir = temporary.path().join("run");
    let packet = create_run_packet(&run_dir);
    let route = fake_route(&executable);

    for (mode, expected) in [
        ("invalid_utf8", "payload is not strict UTF-8"),
        ("unknown_field", "schema validation failed"),
    ] {
        let lane_root = run_dir.join(mode);
        let mut invocation = build(&route, "R1", TEXT_MODEL, &packet, &lane_root, 30).unwrap();
        invocation.env.insert("FAKE_AGENT_MODE".into(), mode.into());
        let output = spawn_command(&invocation).output().unwrap();
        assert!(output.status.success());
        let error = parse_output(invocation.output_kind, &output.stdout).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "mode {mode}: {error:#}"
        );
    }
}
