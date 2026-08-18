//! A2A v1.0 JSON-RPC front door (HOST.md) over the existing 0.2.x CLI host.
//!
//! Entry: `quinte host serve`. Wire parsing lives in [`wire`] so tests can
//! drive the shipped handlers without re-implementing envelopes.

pub mod card;
pub mod host_map;
pub mod http;
pub mod wire;

pub use http::{A2aServer, ServeOptions};

use serde_json::Value;

use crate::store::Store;

use wire::{
    A2aError, ERR_METHOD_NOT_FOUND, ERR_PARSE, parse_rpc, rpc_error, rpc_result,
};

/// Dispatch one JSON-RPC body through the shipped handlers.
/// Returns (HTTP status, JSON-RPC envelope).
pub fn handle_rpc(store: &Store, body: &str) -> (u16, Value) {
    let request = match parse_rpc(body) {
        Ok(r) => r,
        Err(err) => {
            let status = if err.code == ERR_PARSE { 200 } else { 200 };
            return (status, rpc_error(Value::Null, &err));
        }
    };
    let result = match request.method.as_str() {
        "SendMessage" => host_map::send_message(store, &request.params),
        "GetTask" => host_map::get_task(store, &request.params),
        "ListTasks" => host_map::list_tasks(store, &request.params),
        "CancelTask" => host_map::cancel_task(store, &request.params),
        other => Err(A2aError::new(
            ERR_METHOD_NOT_FOUND,
            format!("method not found: {other}"),
        )),
    };
    match result {
        Ok(value) => (200, rpc_result(request.id, value)),
        Err(err) => (200, rpc_error(request.id, &err)),
    }
}
