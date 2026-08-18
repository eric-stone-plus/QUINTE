//! Minimal HTTP face: one thread per connection, std only. Two routes:
//! the agent card (GET) and the JSON-RPC endpoint (POST). Nothing else.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use crate::prompt::Seat;
use crate::provider::Provider;
use crate::task::TaskStore;

const CARD_PATH: &str = "/.well-known/agent.json";

pub fn handle(
    mut stream: TcpStream,
    store: &Arc<Mutex<TaskStore>>,
    seat: &Seat,
    provider: &Provider,
) -> Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(60)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    // Drain headers (content-length needed for POST bodies).
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    match (method.as_str(), path.as_str()) {
        ("GET", CARD_PATH) => {
            let body = serde_json::to_vec_pretty(&crate::card::agent_card(seat))?;
            respond(&mut stream, "200 OK", "application/json", &body)?;
        }
        ("POST", "/") => {
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body)?;
            let reply = crate::rpc::dispatch(
                std::str::from_utf8(&body).context("RPC body is not UTF-8")?,
                store,
                seat,
                provider,
            );
            respond(&mut stream, "200 OK", "application/json", reply.as_bytes())?;
        }
        _ => {
            let body = b"{\"error\":{\"code\":-32601,\"message\":\"method not found\"}}";
            respond(&mut stream, "404 Not Found", "application/json", body)?;
        }
    }
    Ok(())
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()?;
    Ok(())
}
