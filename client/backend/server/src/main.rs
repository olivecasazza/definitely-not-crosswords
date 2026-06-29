//! crossword-server — the Rust backend replacing the Nuxt server.
//!
//! Speaks the EXACT tRPC HTTP-batch wire format the Dioxus client uses:
//!   query    GET  /api/trpc/<proc>?batch=1&input={"0":<input>}
//!   mutation POST /api/trpc/<proc>?batch=1   body {"0":<input>}
//!   response [{"result":{"data":<D>}}]  (or [{"error":{...}}])
//! Auth: the next-auth session cookie is decrypted by `crossword-auth` into an
//! `AuthContext` per request and handed to handlers as `Ctx`. Procedures live in
//! `routers/<name>.rs`, one module per tRPC router. WS subscriptions land with
//! the events crate (Phase C).

mod auth_routes;
mod ctx;
mod routers;

use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use crossword_auth::{AuthContext, AuthService, RequestAuth};
use crossword_db::AppEvent;
use crossword_events::EventBus;
use ctx::Ctx;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    auth: AuthService,
    events: EventBus,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()))
        .init();

    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .connect(&db_url)
        .await?;
    // allow_dev_admin=true enables the local-dev/native bearer paths in dev.
    let auth = AuthService::new(true);
    let events = EventBus::default();

    let app = Router::new()
        .route("/api/healthz", get(|| async { "ok" }))
        .route("/api/auth/session", get(session))
        .route("/api/auth/csrf", get(auth_routes::csrf))
        .route(
            "/api/auth/callback/credentials",
            post(auth_routes::credentials),
        )
        .route("/api/auth/callback/local-dev", post(auth_routes::local_dev))
        .route(
            "/api/auth/signout",
            get(auth_routes::signout).post(auth_routes::signout),
        )
        .route("/api/trpc-ws", get(trpc_ws))
        .route("/api/trpc/:proc", get(trpc_get).post(trpc_post))
        .with_state(AppState { pool, auth, events });

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("crossword-server listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Build the auth request from headers (next-auth cookie + optional bearer).
fn req_auth(headers: &HeaderMap) -> RequestAuth {
    RequestAuth {
        cookie_header: headers
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        bearer_token: headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string()),
    }
}

/// next-auth-compatible `GET /api/auth/session`: `{}` signed out, `{user}` in.
async fn session(State(st): State<AppState>, headers: HeaderMap) -> Json<Value> {
    let auth = st.auth.authenticate(&req_auth(&headers));
    let Some(u) = auth.user else {
        return Json(json!({}));
    };
    // Enrich with the display name from the DB (the cookie only carries id/email/role).
    let name: Option<String> = sqlx::query(r#"SELECT name FROM "User" WHERE id = $1"#)
        .bind(&u.id)
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.get::<Option<String>, _>("name"));
    Json(json!({
        "user": { "id": u.id, "email": u.email, "name": name, "image": Value::Null, "role": u.role }
    }))
}

async fn trpc_get(
    Path(proc): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    State(st): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let input = q
        .get("input")
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.get("0").cloned())
        .unwrap_or(Value::Null);
    let ctx = Ctx {
        pool: st.pool.clone(),
        auth: st.auth.authenticate(&req_auth(&headers)),
        events: st.events.clone(),
    };
    envelope(routers::dispatch(&proc, &input, &ctx).await)
}

async fn trpc_post(
    Path(proc): Path<String>,
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let input = body.get("0").cloned().unwrap_or(Value::Null);
    let ctx = Ctx {
        pool: st.pool.clone(),
        auth: st.auth.authenticate(&req_auth(&headers)),
        events: st.events.clone(),
    };
    envelope(routers::dispatch(&proc, &input, &ctx).await)
}

fn envelope(res: Result<Value, String>) -> Json<Value> {
    match res {
        Ok(data) => Json(json!([{ "result": { "data": data } }])),
        Err(e) => Json(json!([{
            "error": { "message": e, "code": -32600,
                       "data": { "code": "BAD_REQUEST", "httpStatus": 400 } }
        }])),
    }
}

// ── tRPC WebSocket subscriptions ─────────────────────────────────────────────
// The client opens /api/trpc-ws and sends
//   {id, method:"subscription", params:{path, input}}
// we reply {id, result:{type:"started"}} then {id, result:{type:"data", data}}
// per matching AppEvent, and {id, result:{type:"stopped"}} on stop.

async fn trpc_ws(
    ws: WebSocketUpgrade,
    State(st): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // The next-auth cookie rides the upgrade request; authenticate once here so
    // admin-only subscriptions (generator.runGeneration) can be gated.
    let auth = st.auth.authenticate(&req_auth(&headers));
    ws.on_upgrade(move |socket| handle_ws(socket, st, auth))
}

async fn handle_ws(socket: WebSocket, st: AppState, auth: AuthContext) {
    let (mut sender, mut receiver) = socket.split();
    // Single writer task drains an mpsc so multiple subscription forwarders can
    // share the one socket.
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(WsMessage::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let mut subs: HashMap<i64, tokio::task::JoinHandle<()>> = HashMap::new();
    while let Some(Ok(msg)) = receiver.next().await {
        let WsMessage::Text(t) = msg else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&t) else {
            continue;
        };
        let id = v["id"].as_i64().unwrap_or(1);
        match v["method"].as_str() {
            Some("subscription") => {
                let path = v["params"]["path"].as_str().unwrap_or("").to_string();
                let _ = tx.send(json!({ "id": id, "result": { "type": "started" } }).to_string());

                if path == "generator.runGeneration" {
                    // Per-client streaming job (not an EventBus broadcast): authorize,
                    // then run the generation pushing data frames to this socket.
                    match routers::generator::authorize(&auth) {
                        Ok(user) => {
                            let input = v["params"]["input"].clone();
                            let txc = tx.clone();
                            let emit_ws: Arc<dyn Fn(Value) + Send + Sync> =
                                Arc::new(move |ev: Value| {
                                    let _ = txc.send(
                                        json!({ "id": id, "result": { "type": "data", "data": ev } })
                                            .to_string(),
                                    );
                                });
                            let pool = st.pool.clone();
                            let handle = tokio::spawn(async move {
                                routers::generator::run_generation(pool, user, input, emit_ws).await;
                            });
                            subs.insert(id, handle);
                        }
                        Err(e) => {
                            let _ = tx.send(
                                json!({ "id": id, "result": { "type": "data",
                                    "data": { "type": "failed", "jobId": null, "error": e } } })
                                .to_string(),
                            );
                        }
                    }
                    continue;
                }

                let mut bus_rx = st.events.subscribe();
                let txc = tx.clone();
                let handle = tokio::spawn(async move {
                    while let Ok(ev) = bus_rx.recv().await {
                        if let Some(data) = event_data_for(&path, &ev) {
                            let _ = txc.send(
                                json!({ "id": id, "result": { "type": "data", "data": data } })
                                    .to_string(),
                            );
                        }
                    }
                });
                subs.insert(id, handle);
            }
            Some("subscription.stop") => {
                if let Some(h) = subs.remove(&id) {
                    h.abort();
                    let _ =
                        tx.send(json!({ "id": id, "result": { "type": "stopped" } }).to_string());
                }
            }
            _ => {}
        }
    }
    for (_, h) in subs {
        h.abort();
    }
    writer.abort();
}

/// Map a subscription path + event to the `data` payload the client expects.
/// These emitters are global (the client filters by activeGameId itself).
fn event_data_for(path: &str, ev: &AppEvent) -> Option<Value> {
    match (path, ev) {
        ("activeGame.onAddActions", AppEvent::GameActionsAdded { actions, .. }) => {
            Some(json!(actions))
        }
        (
            "activeGame.onGameCompleted",
            AppEvent::GameCompleted {
                active_game_id,
                completed_game_id,
            },
        ) => Some(json!({ "activeGameId": active_game_id, "completedGameId": completed_game_id })),
        ("message.onMessage", AppEvent::Message { text }) => Some(json!({ "text": text })),
        _ => None,
    }
}
