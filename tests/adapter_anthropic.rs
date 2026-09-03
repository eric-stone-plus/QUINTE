//! Contract tests for the in-process provider adapter speaking the
//! Anthropic Messages protocol face (base URLs ending in ``/anthropic``
//! or ``/anthropic/v1``). The mock HTTP endpoint binds 127.0.0.1 only; no
//! test traffic leaves the loopback interface.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use quinte::adapters::{
    ChatCompletionsCall, ChatProxy, OutputContract, OutputKind, execute_chat_completions,
    parse_typed_output_with_limit,
};
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

/// Serves exactly one HTTP response on a 127.0.0.1 ephemeral port under
/// an ``/apps/anthropic`` base path and reports the captured request over
/// the returned channel. ``versioned`` adds the trailing ``/v1`` segment
/// some gateways include in their advertised base URL.
fn spawn_mock(
    status: u16,
    reason: &str,
    body: String,
    versioned: bool,
) -> (String, mpsc::Receiver<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (sender, receiver) = mpsc::channel();
    let reason = reason.to_string();
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
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let mut stream = reader.into_inner();
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    });
    let suffix = if versioned { "/v1" } else { "" };
    (
        format!("http://127.0.0.1:{port}/apps/anthropic{suffix}"),
        receiver,
    )
}

fn qwen_call(base_url: &str) -> ChatCompletionsCall {
    ChatCompletionsCall {
        base_url: base_url.into(),
        key: "test-key".into(),
        model: "qwen3.8-max".into(),
        prompt: "test prompt".into(),
        images: Vec::new(),
        proxy: ChatProxy::Direct,
        timeout_seconds: 30,
    }
}

fn message_envelope(content: &str) -> String {
    json!({
        "id": "msg-test",
        "type": "message",
        "role": "assistant",
        "model": "qwen3.8-max",
        "content": [{"type": "text", "text": content}],
        "stop_reason": "end_turn",
    })
    .to_string()
}

#[test]
fn anthropic_face_posts_x_api_key_and_parses_lane_output() {
    let lane = common::valid_lane_output();
    let (base_url, requests) = spawn_mock(200, "OK", message_envelope(&lane.to_string()), false);
    let outcome = execute_chat_completions(&qwen_call(&base_url), 1_048_576);

    assert_eq!(
        outcome.exit_code,
        Some(0),
        "stderr: {:?}",
        String::from_utf8_lossy(&outcome.stderr)
    );
    assert!(outcome.stderr.is_empty());
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
        request.request_line.starts_with("POST /apps/anthropic/v1/messages "),
        "unexpected request line: {}",
        request.request_line
    );
    assert_eq!(request.header("x-api-key"), Some("test-key"));
    assert!(request.header("authorization").is_none());
    assert!(request.header("anthropic-version").is_some());
    assert_eq!(request.body["model"], "qwen3.8-max");
    assert_eq!(request.body["temperature"], 0.0);
    assert!(request.body["max_tokens"].as_u64().unwrap_or(0) > 0);
    assert_eq!(request.body["messages"][0]["role"], "user");
    assert_eq!(request.body["messages"][0]["content"], "test prompt");
}

#[test]
fn versioned_base_url_keeps_a_single_v1_segment() {
    let lane = common::valid_lane_output();
    let (base_url, requests) = spawn_mock(200, "OK", message_envelope(&lane.to_string()), true);
    let outcome = execute_chat_completions(&qwen_call(&base_url), 1_048_576);

    assert_eq!(outcome.exit_code, Some(0));
    let request = requests.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        request
            .request_line
            .starts_with("POST /apps/anthropic/v1/messages "),
        "unexpected request line: {}",
        request.request_line
    );
}

#[test]
fn anthropic_error_envelope_fails_closed() {
    let body = json!({
        "type": "error",
        "error": {"type": "overloaded_error", "message": "provider overloaded"},
    })
    .to_string();
    let (base_url, _requests) = spawn_mock(200, "OK", body, false);
    let outcome = execute_chat_completions(&qwen_call(&base_url), 1_048_576);

    assert_eq!(outcome.exit_code, Some(0));
    let error = parse_typed_output_with_limit(
        OutputKind::ChatCompletions,
        OutputContract::Lane,
        &outcome.stdout,
        1_048_576,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("provider overloaded"),
        "error: {error}"
    );
}

#[test]
fn anthropic_429_keeps_the_typed_rate_limit_signal() {
    let body = json!({
        "type": "error",
        "error": {"type": "rate_limit_error", "message": "quota exhausted"},
    })
    .to_string();
    let (base_url, _requests) = spawn_mock(429, "Too Many Requests", body, false);
    let outcome = execute_chat_completions(&qwen_call(&base_url), 1_048_576);

    assert_eq!(outcome.exit_code, Some(1));
    let stderr = String::from_utf8(outcome.stderr).unwrap();
    assert!(stderr.contains("HTTP 429"), "stderr: {stderr}");
    let structured: Value = serde_json::from_slice(&outcome.stdout).unwrap();
    assert_eq!(structured["error"]["type"], "rate_limit_error");
}
