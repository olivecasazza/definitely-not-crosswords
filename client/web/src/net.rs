//! Async tRPC client over the wire format defined in `crossword_core::rpc`.
//!
//! Queries/mutations go over HTTP (`gloo-net`, same-origin so the next-auth
//! cookie rides along). Subscriptions speak tRPC's WebSocket JSON-RPC protocol.
//! The base path is relative (`/api/trpc`), so in dev `dx serve` proxies it to
//! Nuxt (see `Dioxus.toml`) and everything stays same-origin.

use crossword_core::rpc;
use futures::{FutureExt, SinkExt, StreamExt};
use gloo_net::http::Request;
use gloo_net::websocket::{futures::WebSocket, Message};
use serde::de::DeserializeOwned;
use serde_json::Value;
use wasm_bindgen_futures::spawn_local;

const HTTP_BASE: &str = "/api/trpc";

/// A tRPC query. `input` is the raw procedure input (None for no-arg procs).
pub async fn query(proc: &str, input: Option<Value>) -> Result<Value, String> {
    let url = rpc::query_url(HTTP_BASE, proc, input.as_ref());
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    rpc::parse_batch_single(&text)
}

/// A tRPC mutation (POST).
pub async fn mutation(proc: &str, input: Option<Value>) -> Result<Value, String> {
    let (url, body) = rpc::mutation_request(HTTP_BASE, proc, input.as_ref());
    let resp = Request::post(&url)
        .header("content-type", "application/json")
        .body(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    rpc::parse_batch_single(&text)
}

/// Typed query: deserialize the unwrapped `result.data` into `T`.
pub async fn query_as<T: DeserializeOwned>(proc: &str, input: Option<Value>) -> Result<T, String> {
    let data = query(proc, input).await?;
    serde_json::from_value(data).map_err(|e| e.to_string())
}

/// Typed mutation.
pub async fn mutation_as<T: DeserializeOwned>(
    proc: &str,
    input: Option<Value>,
) -> Result<T, String> {
    let data = mutation(proc, input).await?;
    serde_json::from_value(data).map_err(|e| e.to_string())
}

/// WebSocket origin for subscriptions, derived from the page origin.
fn ws_url() -> String {
    let loc = web_sys::window().unwrap().location();
    let proto = if loc.protocol().unwrap_or_default() == "https:" {
        "wss"
    } else {
        "ws"
    };
    let host = loc.host().unwrap_or_default();
    format!("{proto}://{host}/api/trpc-ws")
}

/// Open a tRPC subscription. `on_data` fires for every `data` frame with the
/// raw payload; the returned [`Subscription`] cancels the stream on drop.
///
/// tRPC WS JSON-RPC: client sends
/// `{id, method:"subscription", params:{path, input}}`; server replies with
/// `{id, result:{type:"started"|"data"|"stopped", data?}}`.
pub fn subscribe(
    proc: &str,
    input: Option<Value>,
    mut on_data: impl FnMut(Value) + 'static,
) -> Subscription {
    let proc = proc.to_string();
    let (cancel_tx, mut cancel_rx) = futures::channel::oneshot::channel::<()>();

    spawn_local(async move {
        let ws = match WebSocket::open(&ws_url()) {
            Ok(ws) => ws,
            Err(e) => {
                web_sys::console::error_1(&format!("ws open failed: {e}").into());
                return;
            }
        };
        let (mut write, mut read) = ws.split();

        let start = serde_json::json!({
            "id": 1,
            "method": "subscription",
            "params": { "path": proc, "input": input.unwrap_or(Value::Null) },
        });
        if write.send(Message::Text(start.to_string())).await.is_err() {
            return;
        }

        loop {
            futures::select! {
                _ = cancel_rx => {
                    // best-effort stop frame, then drop the socket
                    let stop = serde_json::json!({ "id": 1, "method": "subscription.stop" });
                    let _ = write.send(Message::Text(stop.to_string())).await;
                    break;
                }
                msg = read.next().fuse() => {
                    match msg {
                        Some(Ok(Message::Text(t))) => {
                            if let Ok(v) = serde_json::from_str::<Value>(&t) {
                                if v["result"]["type"] == "data" {
                                    on_data(v["result"]["data"].clone());
                                }
                            }
                        }
                        Some(Ok(Message::Bytes(_))) => {}
                        _ => break, // closed or error
                    }
                }
            }
        }
    });

    Subscription {
        _cancel: Some(cancel_tx),
    }
}

/// Handle to a live subscription; dropping it cancels the stream.
pub struct Subscription {
    _cancel: Option<futures::channel::oneshot::Sender<()>>,
}
