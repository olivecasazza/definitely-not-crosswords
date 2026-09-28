//! Async tRPC client over the wire format defined in `crossword_core::rpc`.
//!
//! Queries/mutations go over HTTP (`gloo-net`, same-origin so the next-auth
//! cookie rides along). Subscriptions speak tRPC's WebSocket JSON-RPC protocol.
//! The base path is relative (`/api/trpc`), so in dev `dx serve` proxies it to
//! Nuxt (see `Dioxus.toml`) and everything stays same-origin.
//!
//! Subscription sockets are supervised. `subscribe` used to hand back a bare
//! cancel handle with three silent exits — a failed open, a failed start frame
//! and a dropped stream all returned without telling anyone — so a dead socket
//! looked exactly like a live one and a co-op board quietly stopped being
//! co-op (see DEF-175). There is now one state machine ([`ConnectionState`])
//! that re-opens with jittered backoff, gives up after [`MAX_RETRY_ATTEMPTS`],
//! short-circuits on `navigator.onLine`, and reports every transition through a
//! single signal that all callers share.

use crossword_core::rpc;
use dioxus::prelude::{Readable, ScopeId, Signal, Writable};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::channel::oneshot;
use futures::{FutureExt, SinkExt, StreamExt};
use gloo_net::http::Request;
use gloo_net::websocket::{futures::WebSocket, Message};
use gloo_timers::future::TimeoutFuture;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

/// Absolute backend origin baked at build time for non-browser (Tauri) bundles,
/// e.g. `CROSSWORD_API_BASE=https://crosswords.example.com`. When unset (the web
/// build) we stay same-origin: a relative HTTP path + page-derived WS origin.
const API_ORIGIN: Option<&str> = option_env!("CROSSWORD_API_BASE");

fn http_base() -> String {
    match API_ORIGIN {
        Some(o) => format!("{}/api/trpc", o.trim_end_matches('/')),
        None => "/api/trpc".to_string(),
    }
}

/// Map a non-2xx HTTP status to a friendly message. tRPC app errors come back as
/// HTTP 200 with an error envelope (handled by `parse_batch_single`); a non-2xx
/// here means a transport/gateway failure (an auth redirect, or a 5xx with an
/// HTML body) that must NOT be shown to the user as a raw status/body.
fn status_error(status: u16) -> Option<String> {
    match status {
        200..=299 => None,
        401 => Some("You need to sign in to do that.".into()),
        403 => Some("You don't have access to that.".into()),
        404 => Some("That wasn't found.".into()),
        429 => Some("Too many requests — give it a moment.".into()),
        500..=599 => Some("The server is having trouble right now. Please try again.".into()),
        s => Some(format!("Request failed ({s}).")),
    }
}

/// Extract a human-readable message from a tRPC error string.
///
/// `parse_batch_single` returns the full error JSON object as a string when the
/// server responds with `[{"error":{...}}]`. Try to pull `error.message`; fall
/// back to the raw string for plain network/parse errors.
pub fn trpc_err_msg(e: String) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(&e) {
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
    }
    e
}

/// A tRPC query. `input` is the raw procedure input (None for no-arg procs).
pub async fn query(proc: &str, input: Option<Value>) -> Result<Value, String> {
    let url = rpc::query_url(&http_base(), proc, input.as_ref());
    let resp = Request::get(&url).send().await.map_err(|e| e.to_string())?;
    if let Some(msg) = status_error(resp.status()) {
        return Err(msg);
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    rpc::parse_batch_single(&text)
}

/// A tRPC mutation (POST).
pub async fn mutation(proc: &str, input: Option<Value>) -> Result<Value, String> {
    let (url, body) = rpc::mutation_request(&http_base(), proc, input.as_ref());
    let resp = Request::post(&url)
        .header("content-type", "application/json")
        .body(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(msg) = status_error(resp.status()) {
        return Err(msg);
    }
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

/// WebSocket URL for subscriptions. For a baked absolute origin (Tauri) derive
/// ws(s) from it; otherwise use the page origin (web, same-origin).
fn ws_url() -> String {
    if let Some(o) = API_ORIGIN {
        let o = o.trim_end_matches('/');
        let ws = o
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        return format!("{ws}/api/trpc-ws");
    }
    let loc = web_sys::window().unwrap().location();
    let proto = if loc.protocol().unwrap_or_default() == "https:" {
        "wss"
    } else {
        "ws"
    };
    let host = loc.host().unwrap_or_default();
    format!("{proto}://{host}/api/trpc-ws")
}

// ---------------------------------------------------------------------------
// Supervised subscriptions
// ---------------------------------------------------------------------------

/// Retry attempts before a socket is given up on and the connection is
/// reported [`ConnectionState::Offline`].
pub const MAX_RETRY_ATTEMPTS: u8 = 5;

/// State of the subscription sockets.
///
/// One state for the whole app rather than one per `subscribe` call. Each
/// subscription owns its own socket, but a network blip takes all of them at
/// once, so per-socket states could only ever disagree with each other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConnectionState {
    /// The very first open is in flight. Resolves to `Live` or `Reconnecting`.
    Connecting,
    /// A socket is open and frames are flowing.
    Live,
    /// The socket is gone and a retry is in flight or scheduled. `attempt` is
    /// 1-based and counts against [`MAX_RETRY_ATTEMPTS`].
    Reconnecting { attempt: u8 },
    /// Retries exhausted, or the browser reported `navigator.onLine === false`.
    /// Terminal until [`Conn::retry_now`] or an `online` event.
    Offline,
}

/// Control message pushed to every live retry loop.
#[derive(Clone, Copy)]
enum Cmd {
    /// "Retry now", or the browser came back online: drop the attempt counter
    /// and start the ladder over immediately.
    Retry,
    /// `navigator.onLine` went false. The browser already knows the network is
    /// gone, so spending the rest of the retry ladder proving it is wasted time.
    Offline,
}

/// Handle to the shared connection state.
///
/// Cheap to clone: one `Signal` handle plus a shared registry of live retry
/// loops. See [`ConnectionState`] for why it is a singleton rather than
/// per-subscription.
#[derive(Clone)]
pub struct Conn {
    state: Signal<ConnectionState>,
    /// Every live retry loop. A loop that has finished drops out of the
    /// registry; [`subscribe`] prunes closed entries, so this stays bounded.
    loops: Rc<RefCell<Vec<UnboundedSender<Cmd>>>>,
}

impl Conn {
    /// The current state, read without subscribing (no render dependency).
    pub fn state(&self) -> ConnectionState {
        *self.state.peek()
    }

    /// The state signal to read inside a component — registers the render
    /// dependency that makes the connection pill follow the socket.
    pub fn signal(&self) -> Signal<ConnectionState> {
        self.state
    }

    /// "Retry now": reset the attempt counter and reconnect immediately,
    /// without waiting out whatever backoff is left.
    pub fn retry_now(&self) {
        self.broadcast(Cmd::Retry);
    }

    fn broadcast(&self, cmd: Cmd) {
        for tx in self.loops.borrow().iter() {
            let _ = tx.unbounded_send(cmd);
        }
    }
}

thread_local! {
    static CONN: RefCell<Option<Conn>> = const { RefCell::new(None) };
}

/// The shared subscription-connection state.
///
/// The first call creates a root-scoped signal, which needs a live Dioxus
/// runtime — so call it from a component (every consumer does) and never from a
/// bare task. After that it is a plain read and is safe from event handlers too.
pub fn connection() -> Conn {
    CONN.with(|slot| {
        if let Some(existing) = slot.borrow().clone() {
            return existing;
        }
        let conn = Conn {
            state: Signal::new_in_scope(ConnectionState::Connecting, ScopeId::ROOT),
            loops: Rc::new(RefCell::new(Vec::new())),
        };
        watch_online(&conn);
        slot.borrow_mut().replace(conn.clone());
        conn
    })
}

/// A window event listener closure. Named so the leaked-static below reads as
/// what it is rather than as a pile of angle brackets.
type Listener = Closure<dyn Fn()>;

/// Bridge `navigator.onLine` into the retry loops.
///
/// `offline` parks every loop at once (from whatever state it is in); `online`
/// unparks them and resets the ladder. The two window listeners are
/// app-lifetime and live on `window`, so the closures are intentionally leaked
/// rather than dropped with whatever task first created them.
fn watch_online(conn: &Conn) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let loops = conn.loops.clone();

    let mut handlers: Vec<Listener> = Vec::new();
    for (event, cmd) in [("offline", Cmd::Offline), ("online", Cmd::Retry)] {
        let registry = loops.clone();
        let handler = Closure::<dyn Fn()>::new(move || {
            for tx in registry.borrow().iter() {
                let _ = tx.unbounded_send(cmd);
            }
        });
        let _ = window.add_event_listener_with_callback(event, handler.as_ref().unchecked_ref());
        handlers.push(handler);
    }
    // Leak the registry: it owns the closures, and the listeners they back are
    // registered on `window`, which outlives every task in this module.
    std::mem::forget(Rc::new(handlers));
}

/// Delay before the retry after `attempt` failed: 1s, 2s, 4s, 8s, 16s, ±20%
/// jitter.
///
/// Jitter is deliberate, not polish. Two players on one network drop together,
/// and un-jittered backoff would have both retry in lockstep, so the reconnection
/// storm is the exact thing the backoff is supposed to spread out.
fn backoff_ms(attempt: u8) -> u32 {
    let secs = 1u32 << attempt.saturating_sub(1).min(4);
    let base = secs * 1_000;
    let jitter = base / 5;
    let spread = (js_sys::Math::random() * (jitter as f64 * 2.0)) as u32;
    (base + spread).saturating_sub(jitter)
}

/// How a backoff wait ended.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Wait {
    /// The full delay elapsed.
    Elapsed,
    /// A manual retry / an `online` event — go now, counter reset.
    Restart,
    /// An `offline` event — the browser knows better than our ladder.
    WentOffline,
    /// The caller was cancelled (unmount).
    Cancelled,
}

/// Wait out the backoff for `attempt`, or act the moment something asks to.
async fn wait_backoff(
    attempt: u8,
    mut cancel: &mut oneshot::Receiver<()>,
    cmds: &mut UnboundedReceiver<Cmd>,
) -> Wait {
    futures::select! {
        _ = cancel => Wait::Cancelled,
        msg = cmds.next() => match msg {
            Some(Cmd::Retry) => Wait::Restart,
            Some(Cmd::Offline) => Wait::WentOffline,
            // Every sender is gone (app teardown): stop retrying.
            None => Wait::Cancelled,
        },
        _ = TimeoutFuture::new(backoff_ms(attempt)).fuse() => Wait::Elapsed,
    }
}

/// Outcome of one connection attempt.
enum Attempt {
    /// The caller dropped the [`Subscription`] (unmount). End — do not retry.
    Cancelled,
    /// The socket was dropped on purpose: a manual retry or an `online` event
    /// asked for a fresh one. Restart the ladder.
    Restart,
    /// The socket is gone — it either never opened or it died mid-stream.
    /// This is the third exit path of the old `subscribe`, and it is no longer
    /// silent.
    Lost,
}

/// Open a socket, send the subscription start frame, and pump frames until the
/// socket dies. `on_live` fires the moment the start frame has landed.
///
/// Ordering is load-bearing (DEF-175 §3): a caller that reconciles after
/// reconnecting must see `Live` only once the re-subscription is actually on the
/// wire, so "live" cannot mean "about to be live".
#[allow(clippy::too_many_arguments)]
async fn attempt_once(
    proc: &str,
    input: Option<&Value>,
    on_data: &mut impl FnMut(Value),
    on_live: &mut impl FnMut(),
    mut cancel: &mut oneshot::Receiver<()>,
    cmds: &mut UnboundedReceiver<Cmd>,
) -> Attempt {
    let ws = match WebSocket::open(&ws_url()) {
        Ok(ws) => ws,
        Err(e) => {
            web_sys::console::error_1(&format!("ws open failed: {e}").into());
            return Attempt::Lost;
        }
    };
    let (mut write, mut read) = ws.split();

    let start = serde_json::json!({
        "id": 1,
        "method": "subscription",
        "params": { "path": proc, "input": input.cloned().unwrap_or(Value::Null) },
    });
    if let Err(e) = write.send(Message::Text(start.to_string())).await {
        web_sys::console::error_1(&format!("ws start frame failed: {e}").into());
        return Attempt::Lost;
    }
    on_live();

    loop {
        futures::select! {
            _ = cancel => {
                // best-effort stop frame, then drop the socket
                let stop = serde_json::json!({ "id": 1, "method": "subscription.stop" });
                let _ = write.send(Message::Text(stop.to_string())).await;
                return Attempt::Cancelled;
            }
            msg = cmds.next() => match msg {
                Some(Cmd::Retry) => return Attempt::Restart,
                // Already parked in `Offline` by the browser listener. Keep
                // streaming so the frames still in flight land, and let the
                // socket die on its own.
                Some(Cmd::Offline) => {}
                None => return Attempt::Cancelled,
            },
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
                    _ => return Attempt::Lost, // closed or error
                }
            }
        }
    }
}

/// Supervise one subscription socket for as long as the caller holds the
/// returned [`Subscription`].
async fn supervise(
    mut conn: Conn,
    proc: String,
    input: Option<Value>,
    mut on_data: impl FnMut(Value) + 'static,
    mut on_state: impl FnMut(ConnectionState) + 'static,
    mut cancel: oneshot::Receiver<()>,
) {
    let (tx, mut cmds) = futures::channel::mpsc::unbounded::<Cmd>();
    // Drop finished loops first so the registry stays bounded across repeated
    // mount/unmount of a screen.
    conn.loops.borrow_mut().retain(|t| !t.is_closed());
    conn.loops.borrow_mut().push(tx);

    // Report a transition, exactly once. Several subscriptions can drop at the
    // same moment; without this the play screen would re-render — and
    // re-reconcile — once per socket.
    fn report(conn: &mut Conn, on_state: &mut impl FnMut(ConnectionState), state: ConnectionState) {
        let changed = {
            let mut cur = conn.state.write();
            if *cur == state {
                false
            } else {
                *cur = state;
                true
            }
        };
        if changed {
            on_state(state);
        }
    }

    let mut attempt: u8 = 0;
    let mut reported: Option<ConnectionState> = None;

    loop {
        if attempt > 0 {
            match wait_backoff(attempt, &mut cancel, &mut cmds).await {
                Wait::Cancelled => return,
                Wait::Restart => {
                    attempt = 0;
                    continue;
                }
                Wait::WentOffline => {
                    report(&mut conn, &mut on_state, ConnectionState::Offline);
                    attempt = MAX_RETRY_ATTEMPTS;
                }
                Wait::Elapsed => {}
            }
        }

        if attempt >= MAX_RETRY_ATTEMPTS {
            // Out of retries. Park: the only ways out are "Retry now", the
            // browser coming back, or the screen going away.
            report(&mut conn, &mut on_state, ConnectionState::Offline);
            futures::select! {
                _ = cancel => return,
                msg = cmds.next() => match msg {
                    Some(Cmd::Retry) => attempt = 0,
                    Some(Cmd::Offline) => {}
                    None => return,
                },
            }
            continue;
        }

        attempt += 1;
        let state = if reported.is_none() {
            ConnectionState::Connecting
        } else {
            ConnectionState::Reconnecting { attempt }
        };
        report(&mut conn, &mut on_state, state);
        reported = Some(state);

        let outcome = {
            let mut on_live = || report(&mut conn, &mut on_state, ConnectionState::Live);
            attempt_once(
                &proc,
                input.as_ref(),
                &mut on_data,
                &mut on_live,
                &mut cancel,
                &mut cmds,
            )
            .await
        };

        match outcome {
            Attempt::Cancelled => return,
            Attempt::Restart => attempt = 0,
            Attempt::Lost => {
                // Say it the moment the socket drops, not after a backoff:
                // the player's own letters stopped landing right then, and a
                // pill that appears a second late is a pill that lies.
                report(
                    &mut conn,
                    &mut on_state,
                    ConnectionState::Reconnecting { attempt },
                );
            }
        }
    }
}

/// A connection-state callback for screens that do not render the state yet.
///
/// The socket is supervised either way, so a dropped stream reconnects instead
/// of going quiet — this only declines to *display* it. See DEF-175.
pub fn ignore_state(_: ConnectionState) {}

/// Open a tRPC subscription. `on_data` fires for every `data` frame with the
/// raw payload; `on_state` fires on every connection transition. The returned
/// [`Subscription`] cancels the stream on drop.
///
/// tRPC WS JSON-RPC: client sends
/// `{id, method:"subscription", params:{path, input}}`; server replies with
/// `{id, result:{type:"started"|"data"|"stopped", data?}}`.
///
/// The socket is supervised: it re-opens with jittered backoff, gives up after
/// [`MAX_RETRY_ATTEMPTS`] attempts, and reports every transition through the
/// shared [`Conn`]. A dropped socket is no longer silent.
pub fn subscribe(
    proc: &str,
    input: Option<Value>,
    on_data: impl FnMut(Value) + 'static,
    on_state: impl FnMut(ConnectionState) + 'static,
) -> Subscription {
    let proc = proc.to_string();
    let (cancel_tx, cancel_rx) = futures::channel::oneshot::channel::<()>();
    let conn = connection();

    spawn_local(supervise(conn, proc, input, on_data, on_state, cancel_rx));

    Subscription {
        _cancel: Some(cancel_tx),
    }
}

/// Handle to a live subscription; dropping it cancels the stream and, with it,
/// the retry loop behind it.
pub struct Subscription {
    _cancel: Option<futures::channel::oneshot::Sender<()>>,
}
