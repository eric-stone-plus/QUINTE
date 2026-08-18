//! Host-wire tests for the shipped A2A front door. These drive the real
//! HTTP handlers and the HOST.md §8 map onto `host start`/`status`/`inspect`
//! — they do not re-implement envelopes or task projection.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use quinte::model::{MULTIMODAL_MODEL, Policy, RunStatus, SandboxMode, TEXT_MODEL};
use quinte::store::Store;
use quinte::util::write_json;
use serde_json::{Value, json};

struct HostFixture {
    _temporary: tempfile::TempDir,
    home: PathBuf,
    _executable: PathBuf,
}

fn fake_policy(executable: &std::path::Path) -> Policy {
    let seat = quinte::model::SeatBinding {
        seat_id: "seat-deepseek".into(),
        family: "deepseek".into(),
        provider: "deepseek".into(),
        text_model: TEXT_MODEL.into(),
        multimodal_model: MULTIMODAL_MODEL.into(),
    };
    let route = |party_id: &str, route_id: &str| quinte::model::RoutePolicy {
        party_id: party_id.into(),
        route_id: route_id.into(),
        adapter: "fake".into(),
        executable: executable.display().to_string(),
        required: true,
        family: seat.family.clone(),
        provider: seat.provider.clone(),
        text_model: seat.text_model.clone(),
        multimodal_model: seat.multimodal_model.clone(),
        perspective: String::new(),
    };
    Policy {
        legacy_v1_source: false,
        policy_version: "2.0".into(),
        seat: seat.clone(),
        roster: ["A", "B", "C", "D", "E"]
            .into_iter()
            .map(|party| {
                route(
                    &format!("Party {party}"),
                    &format!("fake-{}", party.to_ascii_lowercase()),
                )
            })
            .collect(),
        counterpart_arbiter: route("Counterpart Arbiter", "fake-counterpart"),
        primary_arbiter: route("Primary Arbiter", "fake-primary"),
        auto_primary_arbiter: true,
        text_model: TEXT_MODEL.into(),
        multimodal_model: MULTIMODAL_MODEL.into(),
        max_parallel_r1: 5,
        max_parallel_r2: 1,
        r2_parallel: false,
        max_attempts: 1,
        timeout_seconds: 30,
        r1_timeout_seconds: None,
        r2_timeout_seconds: None,
        r3_timeout_seconds: None,
        retry_backoff_seconds: 0,
        retry_backoff_max_seconds: 0,
        r2_min_interval_seconds: 0,
        max_output_bytes: 1_048_576,
        max_snapshot_files: 100,
        max_snapshot_bytes: 1_048_576,
        max_attachment_bytes: 1_048_576,
        sandbox_mode: SandboxMode::Process,
    }
}

fn fixture() -> HostFixture {
    let temporary = tempfile::tempdir().unwrap();
    let executable = common::compile_fake_agent(temporary.path());
    let home = temporary.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    write_json(&home.join("policy.json"), &fake_policy(&executable)).unwrap();
    HostFixture {
        _temporary: temporary,
        home,
        _executable: executable,
    }
}

/// The shipped `quinte host serve` process. Using the CLI binary (not the
/// test harness as current_exe) is what makes `host start` spawn a real
/// `__worker` that can finish a fake-adapter review.
struct LiveServer {
    endpoint: String,
    card_url: String,
    child: Child,
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        if let Ok(body) = serde_json::to_string(&json!({
            "jsonrpc": "2.0", "id": 99, "method": "ListTasks", "params": {}
        })) {
            let (status, text) = http("POST", &self.endpoint, Some(&body));
            if status == 200 {
                if let Ok(env) = serde_json::from_str::<Value>(&text) {
                    if let Some(tasks) = env["result"]["tasks"].as_array() {
                        for task in tasks {
                            if let Some(id) = task["id"].as_str() {
                                let cancel = json!({
                                    "jsonrpc": "2.0", "id": 98,
                                    "method": "CancelTask",
                                    "params": { "id": id }
                                })
                                .to_string();
                                let _ = http("POST", &self.endpoint, Some(&cancel));
                            }
                        }
                    }
                }
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn serve(fixture: &HostFixture) -> LiveServer {
    let mut child = Command::new(assert_cmd::cargo::cargo_bin("quinte"))
        .env("QUINTE_HOME", &fixture.home)
        .env("QUINTE_ALLOW_FAKE_ADAPTERS", "1")
        .args(["host", "serve", "--bind", "127.0.0.1:0", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn quinte host serve");
    let stdout = child.stdout.take().expect("serve stdout");
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .expect("read serve bind envelope");
    let envelope: Value = serde_json::from_str(line.trim()).unwrap_or_else(|_| {
        panic!("serve did not print a JSON bind envelope: {line:?}")
    });
    assert_eq!(envelope["ok"], true, "{envelope}");
    LiveServer {
        endpoint: envelope["data"]["endpoint"]
            .as_str()
            .expect("endpoint")
            .to_string(),
        card_url: envelope["data"]["card_url"]
            .as_str()
            .expect("card_url")
            .to_string(),
        child,
    }
}

fn http(method: &str, url: &str, body: Option<&str>) -> (u16, String) {
    let url = url.strip_prefix("http://").unwrap();
    let (authority, path) = url.split_once('/').unwrap();
    let path = format!("/{path}");
    let mut stream = TcpStream::connect(authority).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let payload = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {authority}\r\n\
         A2A-Version: 1.0\r\nAccept: application/json\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{payload}",
        payload.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (header, rest) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let status = header
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (status, rest.to_string())
}

fn rpc(endpoint: &str, id: u64, method: &str, params: Value) -> Value {
    let body = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    })
    .to_string();
    let (status, body) = http("POST", endpoint, Some(&body));
    assert_eq!(status, 200, "rpc {method} HTTP {status}: {body}");
    let envelope: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(envelope["jsonrpc"], "2.0");
    assert_eq!(envelope["id"], id);
    envelope
}

fn quinte_brief() -> Value {
    json!({
        "brief_version": "1.1",
        "question": "Which material risks remain in the supplied evidence?",
        "context": "Perform the full fixed QUINTE protocol.",
        "evidence_roots": [],
        "snapshot_ignore": [],
        "attachments": [],
        "action_scope": "decision support only",
        "affected_paths": [],
        "action_binding_sha256": null
    })
}

fn stammtisch_send_params(brief: Value, context_id: &str) -> Value {
    json!({
        "message": {
            "messageId": format!("m-{context_id}"),
            "contextId": context_id,
            "role": "ROLE_USER",
            "parts": [
                {"text": format!(
                    "STAMMTISCH pipeline 'quant-research-daily' run '{context_id}' stage 'review': \
                     review the attached evidence artifact(s) and return the product's contract artifacts."
                )},
                {
                    "data": brief,
                    "filename": "brief.json",
                    "mediaType": "application/json"
                }
            ],
            "metadata": { "pipeline": "quant-research-daily", "stage": "review" }
        },
        "configuration": {
            "acceptedOutputModes": ["application/json"],
            "historyLength": 10,
            "returnImmediately": true
        }
    })
}

fn wait_completed(endpoint: &str, task_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let envelope = rpc(endpoint, 90, "GetTask", json!({ "id": task_id }));
        let task = envelope.get("result").cloned().unwrap_or(Value::Null);
        let state = task["status"]["state"].as_str().unwrap_or("");
        if state == "TASK_STATE_COMPLETED" {
            return task;
        }
        if matches!(
            state,
            "TASK_STATE_FAILED" | "TASK_STATE_CANCELED" | "TASK_STATE_REJECTED"
        ) {
            panic!("task {task_id} ended in {state}: {task}");
        }
        assert!(
            Instant::now() < deadline,
            "task {task_id} did not complete; last={task}"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn card_is_served_at_well_known() {
    let fixture = fixture();
    let server = serve(&fixture);
    let (status, body) = http("GET", &server.card_url, None);
    assert_eq!(status, 200);
    let card: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(card["name"], "quinte");
    assert_eq!(
        card["supportedInterfaces"][0]["protocolBinding"],
        "JSONRPC"
    );
    assert_eq!(card["supportedInterfaces"][0]["protocolVersion"], "1.0");
    assert_eq!(card["supportedInterfaces"][0]["url"], server.endpoint);
}

#[test]
fn send_message_creates_one_task_and_stammtisch_body_is_accepted() {
    let fixture = fixture();
    let server = serve(&fixture);
    let envelope = rpc(
        &server.endpoint,
        1,
        "SendMessage",
        stammtisch_send_params(quinte_brief(), "run-a"),
    );
    let task = envelope["result"]["task"].clone();
    assert!(
        envelope["result"].get("task").is_some(),
        "SendMessage must return {{task: …}}, got {envelope}"
    );
    assert!(task["id"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(task["contextId"], "run-a");
    assert!(matches!(
        task["status"]["state"].as_str(),
        Some("TASK_STATE_SUBMITTED" | "TASK_STATE_WORKING" | "TASK_STATE_COMPLETED")
    ));
}

#[test]
fn invalid_brief_is_minus_32011() {
    let fixture = fixture();
    let server = serve(&fixture);
    let envelope = rpc(
        &server.endpoint,
        2,
        "SendMessage",
        json!({
            "message": {
                "messageId": "m-bad",
                "role": "ROLE_USER",
                "parts": [{"text": "no brief here"}],
            },
            "configuration": { "returnImmediately": true, "historyLength": 10 }
        }),
    );
    assert_eq!(envelope["error"]["code"], -32011);
    assert!(envelope.get("result").is_none());
}

#[test]
fn second_non_terminal_send_is_minus_32010() {
    let fixture = fixture();
    let server = serve(&fixture);
    let first = rpc(
        &server.endpoint,
        3,
        "SendMessage",
        stammtisch_send_params(quinte_brief(), "run-busy-1"),
    );
    assert!(first["result"]["task"]["id"].is_string(), "{first}");
    let second = rpc(
        &server.endpoint,
        4,
        "SendMessage",
        stammtisch_send_params(quinte_brief(), "run-busy-2"),
    );
    assert_eq!(second["error"]["code"], -32010, "{second}");
}

#[test]
fn completed_get_task_has_exactly_one_review_result() {
    let fixture = fixture();
    let server = serve(&fixture);
    let started = rpc(
        &server.endpoint,
        5,
        "SendMessage",
        stammtisch_send_params(quinte_brief(), "run-complete"),
    );
    let task_id = started["result"]["task"]["id"].as_str().unwrap().to_string();
    let task = wait_completed(&server.endpoint, &task_id);
    assert_eq!(task["id"], task_id);
    let artifacts = task["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1, "{task}");
    assert_eq!(artifacts[0]["name"], "review.result");
    let data = &artifacts[0]["parts"][0]["data"];
    assert!(data.is_object());
    assert!(data.get("status").is_some(), "{data}");
    assert_eq!(data["run_id"], task_id);
    assert!(
        data.get("status").and_then(Value::as_str) == Some("completed")
            || data.get("status").and_then(Value::as_str) == Some("degraded"),
        "{data}"
    );

    // GetTask returns a bare Task, not {task: …}.
    let envelope = rpc(&server.endpoint, 6, "GetTask", json!({ "id": task_id }));
    assert!(envelope["result"].get("id").is_some());
    assert!(envelope["result"].get("task").is_none());
}

#[test]
fn task_survives_process_restart() {
    let fixture = fixture();
    let first = serve(&fixture);
    let started = rpc(
        &first.endpoint,
        7,
        "SendMessage",
        stammtisch_send_params(quinte_brief(), "run-restart"),
    );
    let task_id = started["result"]["task"]["id"].as_str().unwrap().to_string();
    let completed = wait_completed(&first.endpoint, &task_id);
    let artifact_id = completed["artifacts"][0]["artifactId"]
        .as_str()
        .unwrap()
        .to_string();
    let state = completed["status"]["state"].as_str().unwrap().to_string();
    drop(first);

    let second = serve(&fixture);
    let task = rpc(&second.endpoint, 8, "GetTask", json!({ "id": task_id }))["result"].clone();
    assert_eq!(task["id"], task_id);
    assert_eq!(task["status"]["state"], state);
    assert_eq!(task["artifacts"][0]["name"], "review.result");
    assert_eq!(task["artifacts"][0]["artifactId"], artifact_id);
    assert_eq!(task["artifacts"][0]["parts"][0]["data"]["run_id"], task_id);

    let listed = rpc(&second.endpoint, 9, "ListTasks", json!({}));
    let ids: Vec<_> = listed["result"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["id"].as_str().map(str::to_owned))
        .collect();
    assert!(ids.contains(&task_id), "{listed}");
}

#[test]
fn galahad_brief_from_stammtisch_starts_a_run() {
    let fixture = fixture();
    let server = serve(&fixture);
    let galahad = json!({
        "schema": "galahad.brief.v0",
        "title": "Daily quant research brief",
        "pipeline": "quant-research-daily",
        "run_id": "run-galahad",
        "pack_sha256": format!("sha256:{}", "b".repeat(64)),
        "objectives": ["Evaluate one candidate strategy against the walkforward doctrine"],
        "acceptance_gates": ["quinte_result_21"]
    });
    let envelope = rpc(
        &server.endpoint,
        10,
        "SendMessage",
        stammtisch_send_params(galahad, "run-galahad"),
    );
    assert!(
        envelope["result"]["task"]["id"].is_string(),
        "GALAHAD brief must be accepted: {envelope}"
    );
}

#[test]
fn unknown_a2a_version_is_a_version_error() {
    let fixture = fixture();
    let server = serve(&fixture);
    let url = server.endpoint.strip_prefix("http://").unwrap();
    let (authority, path) = url.split_once('/').unwrap();
    let mut stream = TcpStream::connect(authority).unwrap();
    let body = json!({"jsonrpc":"2.0","id":11,"method":"GetTask","params":{"id":"x"}}).to_string();
    let request = format!(
        "POST /{path} HTTP/1.1\r\nHost: {authority}\r\nA2A-Version: 9.9\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let rest = raw.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
    let envelope: Value = serde_json::from_str(rest).unwrap();
    assert_eq!(envelope["error"]["code"], -32000);
}

#[test]
fn host_cli_commands_still_work_beside_the_front_door() {
    let fixture = fixture();
    let _server = serve(&fixture);
    let output = Command::new(assert_cmd::cargo::cargo_bin("quinte"))
        .env("QUINTE_HOME", &fixture.home)
        .env("QUINTE_ALLOW_FAKE_ADAPTERS", "1")
        .args(["host", "preflight", "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "host preflight failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["ok"], true);
    assert!(envelope["data"]["state"]["code"].is_string());
    let _ = RunStatus::Queued;
    let _ = Store::new(fixture.home.clone());
}
