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

    assert_eq!(output.lane_output_version, "0.1.1");
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
    assert_eq!(output.lane_output_version, "0.1.1");
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
    let child = common::compile_fake_agent(temporary.path());
    let entrypoint = temporary.path().join("node_modules/fake-agent/entry.js");
    fs::create_dir_all(entrypoint.parent().unwrap()).unwrap();
    fs::write(
        &entrypoint,
        r#"const { spawnSync } = require("node:child_process");
const result = spawnSync(process.env.FAKE_AGENT_RUNTIME_CHILD, process.argv.slice(2), {
  stdio: "inherit",
  env: { ...process.env, FAKE_AGENT_RUNTIME_CHILD: undefined },
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
"#,
    )
    .unwrap();
    fs::write(&shim, "#!/bin/sh\n").unwrap();
    fs::write(shim.with_extension("cmd"), "@exit /b 99\r\n").unwrap();
    fs::write(
        shim.with_extension("ps1"),
        r#"#!/usr/bin/env pwsh
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
    $input | & "$basedir/node$exe"  "$basedir/node_modules/fake-agent/entry.js" $args
  } else {
    & "$basedir/node$exe"  "$basedir/node_modules/fake-agent/entry.js" $args
  }
  $ret=$LASTEXITCODE
} else {
  # Support pipeline input
  if ($MyInvocation.ExpectingInput) {
    $input | & "node$exe"  "$basedir/node_modules/fake-agent/entry.js" $args
  } else {
    & "node$exe"  "$basedir/node_modules/fake-agent/entry.js" $args
  }
  $ret=$LASTEXITCODE
}
exit $ret
"#,
    )
    .unwrap();
    let run_dir = temporary.path().join("run-shim");
    let packet = create_run_packet(&run_dir);
    let lane_root = run_dir.join("lane");
    let special_args = ["line one\nline two & <review>", "quote\"value"];
    let mut route = fake_route(&shim);
    route.party_id = special_args[1].into();

    let mut invocation =
        build(&route, special_args[0], TEXT_MODEL, &packet, &lane_root, 30).unwrap();
    let child_probe = temporary.path().join("child-console-probe.txt");
    let args_probe = temporary.path().join("child-args-probe.txt");
    invocation.env.insert(
        "FAKE_AGENT_CONSOLE_PROBE".into(),
        child_probe.display().to_string(),
    );
    invocation.env.insert(
        "FAKE_AGENT_ARGS_PROBE".into(),
        args_probe.display().to_string(),
    );
    invocation.env.insert(
        "FAKE_AGENT_STDERR_SENTINEL".into(),
        "child-stderr-sentinel".into(),
    );
    invocation.env.insert(
        "FAKE_AGENT_RUNTIME_CHILD".into(),
        child.display().to_string(),
    );
    assert!(invocation.program.ends_with("node.exe"));
    assert_eq!(
        fs::canonicalize(&invocation.args[0]).unwrap(),
        fs::canonicalize(&entrypoint).unwrap()
    );
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
        "fake npm-style shim produced no output: status={} stderr={} args={:?} env_keys={:?} child_probe={:?}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
        invocation.args,
        invocation.env.keys().collect::<Vec<_>>(),
        fs::read_to_string(&child_probe)
    );
    let parsed = parse_output(invocation.output_kind, &output.stdout).unwrap();
    assert_eq!(parsed.confidence, 0.75);
    assert_eq!(fs::read_to_string(child_probe).unwrap(), "hidden");
    assert!(String::from_utf8_lossy(&output.stderr).contains("child-stderr-sentinel"));

    let forwarded = fs::read_to_string(args_probe).unwrap();
    let forwarded = forwarded.split('\0').collect::<Vec<_>>();
    assert_eq!(&forwarded[..2], &special_args);
    assert!(forwarded[2].ends_with(r"input\packet.json"));
}

#[cfg(windows)]
#[test]
fn fake_agent_runs_through_windows_bun_npm_style_shim() {
    let temporary = tempfile::tempdir().unwrap();
    let shim = temporary.path().join("quinte-fake-bun-shim");
    let child = common::compile_fake_agent(temporary.path());
    let entrypoint = temporary.path().join("node_modules/fake-agent/entry.js");
    fs::create_dir_all(entrypoint.parent().unwrap()).unwrap();
    fs::write(
        &entrypoint,
        r#"const { spawnSync } = require("node:child_process");
const result = spawnSync(process.env.FAKE_AGENT_RUNTIME_CHILD, process.argv.slice(2), {
  stdio: "inherit",
  env: { ...process.env, FAKE_AGENT_RUNTIME_CHILD: undefined },
});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
"#,
    )
    .unwrap();
    fs::write(&shim, "#!/bin/sh\n").unwrap();
    fs::write(shim.with_extension("cmd"), "@exit /b 99\r\n").unwrap();
    fs::write(
        shim.with_extension("ps1"),
        r#"#!/usr/bin/env pwsh
$basedir=Split-Path $MyInvocation.MyCommand.Definition -Parent

$exe=""
if ($PSVersionTable.PSVersion -lt "6.0" -or $IsWindows) {
  # Fix case when both the Windows and Linux builds of Node
  # are installed in the same directory
  $exe=".exe"
}
$ret=0
if (Test-Path "$basedir/bun$exe") {
  # Support pipeline input
  if ($MyInvocation.ExpectingInput) {
    $input | & "$basedir/bun$exe"  "$basedir/node_modules/fake-agent/entry.js" $args
  } else {
    & "$basedir/bun$exe"  "$basedir/node_modules/fake-agent/entry.js" $args
  }
  $ret=$LASTEXITCODE
} else {
  # Support pipeline input
  if ($MyInvocation.ExpectingInput) {
    $input | & "bun$exe"  "$basedir/node_modules/fake-agent/entry.js" $args
  } else {
    & "bun$exe"  "$basedir/node_modules/fake-agent/entry.js" $args
  }
  $ret=$LASTEXITCODE
}
exit $ret
"#,
    )
    .unwrap();
    let run_dir = temporary.path().join("run-bun-shim");
    let packet = create_run_packet(&run_dir);
    let lane_root = run_dir.join("lane");
    let route = fake_route(&shim);
    let mut invocation = build(&route, "R1", TEXT_MODEL, &packet, &lane_root, 30).unwrap();
    let child_probe = temporary.path().join("bun-child-console-probe.txt");
    invocation.env.insert(
        "FAKE_AGENT_CONSOLE_PROBE".into(),
        child_probe.display().to_string(),
    );
    invocation.env.insert(
        "FAKE_AGENT_RUNTIME_CHILD".into(),
        child.display().to_string(),
    );

    assert!(invocation.program.ends_with("bun.exe"));
    let output = spawn_command(&invocation).output().unwrap();
    assert!(
        output.status.success(),
        "fake Bun npm-style shim failed: status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed = parse_output(invocation.output_kind, &output.stdout).unwrap();
    assert_eq!(parsed.confidence, 0.75);
    assert_eq!(fs::read_to_string(child_probe).unwrap(), "hidden");
}

#[cfg(windows)]
#[test]
fn windows_native_adapter_runs_without_a_console_window() {
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let run_dir = temporary.path().join("run-hidden");
    let packet = create_run_packet(&run_dir);
    let lane_root = run_dir.join("lane");
    let route = fake_route(&executable);
    let probe = temporary.path().join("console-probe.txt");

    let mut invocation = build(&route, "R1", TEXT_MODEL, &packet, &lane_root, 30).unwrap();
    invocation.env.insert(
        "FAKE_AGENT_CONSOLE_PROBE".into(),
        probe.display().to_string(),
    );
    let output = spawn_command(&invocation).output().unwrap();

    assert!(
        output.status.success(),
        "hidden fake agent failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read_to_string(probe).unwrap(), "hidden");
    assert!(!output.stdout.is_empty());
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
