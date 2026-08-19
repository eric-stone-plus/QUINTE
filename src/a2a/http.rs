use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, bail};

use crate::store::Store;

use super::card::agent_card;
use super::handle_rpc;
use super::wire::{A2A_VERSION_HEADER, A2aError, ERR_VERSION, check_a2a_version, rpc_error};

const MAX_BODY: usize = 16 * 1024 * 1024;

pub struct A2aServer {
    pub endpoint: String,
    pub card_url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

pub struct ServeOptions {
    pub bind: String,
    pub token: Option<String>,
}

struct Shared {
    store: Arc<Store>,
    token: Option<String>,
    interface_url: String,
}

impl A2aServer {
    pub fn start(store: Store, options: ServeOptions) -> anyhow::Result<Self> {
        validate_bind(&options.bind, options.token.is_some())?;
        let listener = TcpListener::bind(&options.bind)
            .with_context(|| format!("cannot bind A2A listener on {}", options.bind))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let endpoint = format!("http://{addr}/");
        let card_url = format!("http://{addr}/.well-known/agent-card.json");
        let shared = Arc::new(Mutex::new(Shared {
            store: Arc::new(store),
            token: options.token,
            interface_url: endpoint.clone(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::spawn(move || serve(listener, shared, thread_stop));
        Ok(Self {
            endpoint,
            card_url,
            stop,
            thread: Some(thread),
        })
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        self.stop.clone()
    }

    pub fn join(mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for A2aServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn validate_bind(bind: &str, token_configured: bool) -> anyhow::Result<()> {
    let host = bind.rsplit_once(':').map(|(h, _)| h).unwrap_or(bind);
    let loopback = host == "127.0.0.1" || host == "localhost" || host == "::1" || host == "[::1]";
    if !loopback && !token_configured {
        bail!("non-loopback A2A bind requires --token-env (HOST.md §7)");
    }
    Ok(())
}

fn serve(listener: TcpListener, shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let shared = shared.clone();
                thread::spawn(move || {
                    let _ = handle_connection(stream, &shared);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => break,
        }
    }
}

fn handle_connection(stream: TcpStream, shared: &Arc<Mutex<Shared>>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    let mut content_length = 0usize;
    let mut a2a_version: Option<String> = None;
    let mut authorization: Option<String> = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let trimmed = header.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            } else if name.eq_ignore_ascii_case(A2A_VERSION_HEADER) {
                a2a_version = Some(value.to_string());
            } else if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.to_string());
            }
        }
    }
    if content_length > MAX_BODY {
        return write_response(stream, 413, r#"{"error":"payload too large"}"#);
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    let body_text = String::from_utf8_lossy(&body).into_owned();

    let (status, response) = route(
        &method,
        &path,
        &body_text,
        a2a_version.as_deref(),
        authorization.as_deref(),
        shared,
    );
    write_response(stream, status, &response)
}

fn write_response(mut stream: TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\nA2A-Version: 1.0\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn route(
    method: &str,
    path: &str,
    body: &str,
    a2a_version: Option<&str>,
    authorization: Option<&str>,
    shared: &Arc<Mutex<Shared>>,
) -> (u16, String) {
    let guard = match shared.lock() {
        Ok(g) => g,
        Err(_) => return (500, r#"{"error":"lock poisoned"}"#.to_string()),
    };
    if let Some(expected) = &guard.token {
        let ok = authorization
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|t| t == expected);
        if !ok {
            return (401, r#"{"error":"unauthorized"}"#.to_string());
        }
    }
    if let Err(err) = check_a2a_version(a2a_version) {
        return version_error(err, body);
    }

    let path_only = path.split('?').next().unwrap_or(path);
    if method == "GET" && path_only == "/.well-known/agent-card.json" {
        let card = agent_card(&guard.interface_url, guard.token.is_some());
        return (200, card.to_string());
    }
    if method != "POST" {
        return (404, r#"{"error":"not found"}"#.to_string());
    }
    if path_only != "/" && path_only != "/rpc" && !path_only.is_empty() {
        return (404, r#"{"error":"not found"}"#.to_string());
    }
    // Clone the store handle and drop the guard before dispatching: a
    // blocking SendMessage (`returnImmediately=false`) polls its run for up
    // to the 3600s ceiling, and holding the only server lock through that
    // poll would freeze GetTask/ListTasks/CancelTask and card discovery for
    // every other connection. The one-active-run rule is still enforced —
    // by the run lock inside `host start`, mapped to -32010.
    let store = Arc::clone(&guard.store);
    drop(guard);
    let (status, value) = handle_rpc(&store, body);
    (
        status,
        serde_json::to_string(&value).unwrap_or_else(|_| r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"encode failed"}}"#.to_string()),
    )
}

fn version_error(err: A2aError, body: &str) -> (u16, String) {
    let id = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .unwrap_or(serde_json::Value::Null);
    let envelope = if err.code == ERR_VERSION {
        rpc_error(id, &err)
    } else {
        rpc_error(id, &err)
    };
    (
        200,
        serde_json::to_string(&envelope).unwrap_or_else(|_| err.message),
    )
}
