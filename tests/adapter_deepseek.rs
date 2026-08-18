//! Contract tests for the native in-process DeepSeek adapter. The mock HTTP
//! endpoint binds 127.0.0.1 only; no test traffic leaves the loopback
//! interface.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use quinte::adapters::{
    ChatCompletionsCall, ChatProxy, Execution, OutputContract, OutputKind, build,
    execute_chat_completions, parse_typed_output_with_limit,
};
use quinte::model::RoutePolicy;
use serde_json::{Value, json};

struct CapturedRequest {
    request_line: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl CapturedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Serves exactly one HTTP response on a 127.0.0.1 ephemeral port and
/// reports the captured request over the returned channel.
fn spawn_mock(
    status: u16,
    reason: &str,
    extra_headers: &[(&str, &str)],
    body: String,
) -> (String, mpsc::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sender, receiver) = mpsc::channel();
    let reason = reason.to_string();
    let extra: Vec<(String, String)> = extra_headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line).unwrap();
        let mut content_length = 0usize;
        let mut headers = Vec::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some((name, value)) = trimmed.split_once(':') {
                if name.eq_ignore_ascii_case("content-length") {
                    content_length = value.trim().parse().unwrap();
                }
                headers.push((name.to_string(), value.trim().to_string()));
            }
        }
        let mut request_body = vec![0u8; content_length];
        reader.read_exact(&mut request_body).unwrap();
        sender
            .send(CapturedRequest {
                request_line,
                headers,
                body: serde_json::from_slice(&request_body).unwrap_or(Value::String(
                    String::from_utf8_lossy(&request_body).into_owned(),
                )),
            })
            .unwrap();
        let mut response = format!(
            "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
            body.len()
        );
        for (name, value) in &extra {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str("\r\n");
        response.push_str(&body);
        let mut stream = reader.into_inner();
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });
    (format!("http://127.0.0.1:{port}/v1"), receiver)
}

fn lane_call(base_url: &str) -> ChatCompletionsCall {
    ChatCompletionsCall {
        base_url: base_url.into(),
        key: "test-key".into(),
        model: "deepseek-v4-pro".into(),
        prompt: "test prompt".into(),
        images: Vec::new(),
        proxy: ChatProxy::Direct,
        timeout_seconds: 30,
    }
}

fn chat_envelope(content: &str) -> String {
    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 0,
        "model": "deepseek-v4-pro",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }]
    })
    .to_string()
}

#[test]
fn chat_completion_posts_bearer_auth_and_parses_lane_output() {
    let lane = common::valid_lane_output();
    let (base_url, requests) = spawn_mock(200, "OK", &[], chat_envelope(&lane.to_string()));
    let outcome = execute_chat_completions(&lane_call(&base_url), 1_048_576);

    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.stderr.is_empty());
    assert!(!outcome.timed_out);
    let output = parse_typed_output_with_limit(
        OutputKind::ChatCompletions,
        OutputContract::Lane,
        &outcome.stdout,
        1_048_576,
    )
    .unwrap()
    .into_lane()
    .unwrap();
    assert_eq!(output.lane_output_version, "1.0");
    assert_eq!(output.confidence, 0.75);

    let request = requests.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        request
            .request_line
            .starts_with("POST /v1/chat/completions "),
        "unexpected request line: {}",
        request.request_line
    );
    assert_eq!(request.header("authorization"), Some("Bearer test-key"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.body["model"], "deepseek-v4-pro");
    assert_eq!(request.body["temperature"], 0.0);
    assert_eq!(request.body["messages"][0]["role"], "user");
    assert_eq!(request.body["messages"][0]["content"], "test prompt");
}

#[test]
fn chat_completion_parses_arbiter_verdict_for_the_r3_contract() {
    let verdict = json!({
        "arbiter_verdict_version": "1.0",
        "summary": "Evidence converged on one decisive finding.",
        "recommendation": "Close the evidence gap before merging.",
        "residuals": []
    });
    let (base_url, _requests) = spawn_mock(200, "OK", &[], chat_envelope(&verdict.to_string()));
    let outcome = execute_chat_completions(&lane_call(&base_url), 1_048_576);

    assert_eq!(outcome.exit_code, Some(0));
    let output = parse_typed_output_with_limit(
        OutputKind::ChatCompletions,
        OutputContract::Arbiter,
        &outcome.stdout,
        1_048_576,
    )
    .unwrap()
    .into_arbiter()
    .unwrap();
    assert_eq!(output.arbiter_verdict_version, "1.0");
    assert!(output.residuals.is_empty());
}

#[test]
fn http_500_fails_closed_with_the_status_in_stderr() {
    let (base_url, _requests) = spawn_mock(
        500,
        "Internal Server Error",
        &[],
        json!({"error": {"message": "provider overloaded"}}).to_string(),
    );
    let outcome = execute_chat_completions(&lane_call(&base_url), 1_048_576);

    assert_eq!(outcome.exit_code, Some(1));
    assert!(!outcome.timed_out);
    let stderr = String::from_utf8(outcome.stderr).unwrap();
    assert!(stderr.contains("HTTP 500"), "stderr: {stderr}");
}

#[test]
fn http_429_reports_retry_after_for_rate_limit_classification() {
    let (base_url, _requests) = spawn_mock(
        429,
        "Too Many Requests",
        &[("retry-after", "7")],
        json!({"error": {"message": "rate limited", "type": "rate_limit_error"}}).to_string(),
    );
    let outcome = execute_chat_completions(&lane_call(&base_url), 1_048_576);

    assert_eq!(outcome.exit_code, Some(1));
    let stderr = String::from_utf8(outcome.stderr).unwrap();
    assert!(stderr.contains("HTTP 429"), "stderr: {stderr}");
    assert!(stderr.contains("Retry-After: 7"), "stderr: {stderr}");
    let structured: Value = serde_json::from_slice(&outcome.stdout).unwrap();
    assert_eq!(structured["error"]["type"], "rate_limit_error");
    assert_eq!(structured["error"]["retry_after"], 7);
}

#[test]
fn malformed_success_bodies_fail_closed() {
    for body in [
        "not json at all".to_string(),
        json!({"id": "chatcmpl-test", "choices": []}).to_string(),
        json!({"error": {"message": "quota exhausted"}}).to_string(),
        chat_envelope("the model answered with prose only"),
    ] {
        let (base_url, _requests) = spawn_mock(200, "OK", &[], body);
        let outcome = execute_chat_completions(&lane_call(&base_url), 1_048_576);
        assert_eq!(outcome.exit_code, Some(0));
        let error = parse_typed_output_with_limit(
            OutputKind::ChatCompletions,
            OutputContract::Lane,
            &outcome.stdout,
            1_048_576,
        )
        .unwrap_err();
        assert!(
            !error.to_string().is_empty(),
            "malformed body was accepted: {error}"
        );
    }
}

#[test]
fn refused_transport_fails_closed_without_a_panic() {
    // Loopback port 1 accepts no connections; no traffic leaves the host.
    let outcome = execute_chat_completions(&lane_call("http://127.0.0.1:1"), 1_048_576);

    assert_eq!(outcome.exit_code, Some(1));
    assert!(!outcome.timed_out);
    let stderr = String::from_utf8(outcome.stderr).unwrap();
    assert!(stderr.contains("transport failed"), "stderr: {stderr}");
}

#[test]
fn slow_endpoint_is_reported_as_a_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (_stream, _) = listener.accept().unwrap();
        std::thread::sleep(Duration::from_secs(3));
    });
    let mut call = lane_call(&format!("http://127.0.0.1:{port}/v1"));
    call.timeout_seconds = 1;

    let outcome = execute_chat_completions(&call, 1_048_576);

    assert!(outcome.timed_out, "expected a timeout outcome: {outcome:?}");
}

#[test]
fn missing_credentials_and_non_https_endpoints_fail_closed_at_build() {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    let _lock = LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let names = [
        "QUINTE_PROVIDER_KEY_ENV",
        "QUINTE_PROVIDER_BASE_URL_ENV",
        "DEEPSEEK_API_KEY",
        "DEEPSEEK_BASE_URL",
    ];
    let saved = names
        .iter()
        .map(|name| ((*name).to_string(), std::env::var_os(name)))
        .collect::<Vec<_>>();

    let temporary = tempfile::tempdir().unwrap();
    let run_dir = temporary.path().join("run");
    std::fs::create_dir_all(run_dir.join("input/snapshot")).unwrap();
    std::fs::write(run_dir.join("input/snapshot-manifest.json"), b"{}\n").unwrap();
    let packet = run_dir.join("packet.json");
    std::fs::write(&packet, b"{}\n").unwrap();
    let route = RoutePolicy {
        party_id: "Party A".into(),
        route_id: "deepseek-a".into(),
        adapter: "deepseek".into(),
        executable: "in-process".into(),
        required: true,
        family: "deepseek".into(),
        provider: "deepseek".into(),
        text_model: "deepseek-v4-pro".into(),
        multimodal_model: "deepseek-v4-pro".into(),
        perspective: String::new(),
    };

    unsafe {
        std::env::set_var("QUINTE_PROVIDER_KEY_ENV", "DEEPSEEK_API_KEY");
        std::env::set_var("QUINTE_PROVIDER_BASE_URL_ENV", "DEEPSEEK_BASE_URL");
        std::env::remove_var("DEEPSEEK_API_KEY");
        std::env::set_var("DEEPSEEK_BASE_URL", "https://relay.example.test/v1");
    }
    let error = build(
        &route,
        "R1",
        "deepseek-v4-pro",
        &packet,
        &run_dir.join("lane"),
        30,
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("DEEPSEEK_API_KEY is unavailable"),
        "missing key was not fail-closed: {error:#}"
    );

    unsafe {
        std::env::set_var("DEEPSEEK_API_KEY", "selected-key");
        std::env::set_var("DEEPSEEK_BASE_URL", "http://127.0.0.1:9/v1");
    }
    let error = build(
        &route,
        "R1",
        "deepseek-v4-pro",
        &packet,
        &run_dir.join("lane"),
        30,
    )
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("https"),
        "non-https endpoint was not fail-closed: {error:#}"
    );

    unsafe {
        std::env::set_var("DEEPSEEK_BASE_URL", "https://relay.example.test/v1");
    }
    let invocation = build(
        &route,
        "R1",
        "deepseek-v4-pro",
        &packet,
        &run_dir.join("lane"),
        30,
    )
    .unwrap();
    assert!(matches!(
        invocation.execution,
        Execution::ChatCompletions(_)
    ));

    unsafe {
        for (name, value) in saved {
            if let Some(value) = value {
                std::env::set_var(name, value);
            } else {
                std::env::remove_var(name);
            }
        }
    }
}
