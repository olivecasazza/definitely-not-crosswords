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
mod checkout;
mod ctx;
mod routers;
mod webhook;

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
    /// Deploy environment: "local" | "staging" | "production" (from APP_ENV).
    /// The wasm bundle is shared across envs, so the frontend learns the env at
    /// runtime from `/api/config` rather than a build-time constant.
    env: String,
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
    // "local" | "staging" | "production". Default production: the safe (most
    // locked-down) choice if the var is ever missing.
    let env = std::env::var("APP_ENV").unwrap_or_else(|_| "production".into());
    let is_local = env == "local";
    tracing::info!("APP_ENV={env} (dev admin bypass: {is_local})");

    // Batch-seed mode: when SEED_GAMES_COUNT is set (>0), the binary generates
    // that many published platform games under the Platform system user, then
    // exits instead of serving HTTP. Used by the weekly seeding CronJob.
    if let Ok(cs) = std::env::var("SEED_GAMES_COUNT") {
        let count: usize = cs.trim().parse().unwrap_or(0);
        if count > 0 {
            seed_games(pool.clone(), count).await?;
            return Ok(());
        }
    }

    // The dev-admin bypass (local-dev credential provider + "dev-admin" bearer) is
    // enabled ONLY in local. Outside local it's off AND the route is unregistered,
    // so it can't mint an admin session on staging/prod.
    let auth = AuthService::new(is_local);

    // Fail closed: refuse to serve PRODUCTION with an empty or the known-weak
    // session-signing secret, since it makes admin session cookies forgeable
    // offline. Staging/local tolerate the default to keep setup frictionless;
    // secret rotation itself is an operator task.
    if env == "production"
        && (auth.nextauth_secret.is_empty() || auth.nextauth_secret == "supersecretsecret")
    {
        anyhow::bail!(
            "refusing to start: NEXTAUTH_SECRET is empty or the known-weak default \
             \"supersecretsecret\"; set a strong NEXTAUTH_SECRET"
        );
    }

    let events = EventBus::default();

    let mut app = Router::new()
        .route("/api/healthz", get(|| async { "ok" }))
        .route("/api/config", get(config))
        .route("/api/auth/session", get(session))
        .route("/api/auth/csrf", get(auth_routes::csrf))
        .route(
            "/api/auth/callback/credentials",
            post(auth_routes::credentials),
        )
        .route(
            "/api/auth/signout",
            get(auth_routes::signout).post(auth_routes::signout),
        )
        .route("/api/checkout", post(checkout::checkout))
        .route("/api/webhooks/lemonsqueezy", post(webhook::lemonsqueezy))
        .route("/api/trpc-ws", get(trpc_ws))
        .route("/api/trpc/:proc", get(trpc_get).post(trpc_post));
    // The `local-dev` callback issues a valid session for any account with NO
    // password check — a dev/E2E convenience. Only mount it in local so it can
    // never be reached on staging/prod.
    if is_local {
        app = app.route("/api/auth/callback/local-dev", post(auth_routes::local_dev));
    }
    let app = app.with_state(AppState {
        pool,
        auth,
        events,
        env,
    });

    // Optionally serve the built wasm frontend on the same origin, so the
    // relative `/api` paths + page-derived WS origin "just work" with no proxy.
    // SPA fallback: unknown paths return index.html for client-side routing.
    let app = match std::env::var("WEB_DIST") {
        Ok(dir) if !dir.is_empty() => {
            let index = format!("{dir}/index.html");
            let serve = tower_http::services::ServeDir::new(&dir)
                .fallback(tower_http::services::ServeFile::new(index));
            tracing::info!("serving frontend bundle from {dir}");
            app.fallback_service(serve)
        }
        _ => app,
    };

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("crossword-server listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Batch-generate `count` published platform games under the Platform system
/// user, cycling through the configured topics. Best-effort: a failing
/// generation is logged and skipped, never aborting the whole run. Called from
/// `main()` when `SEED_GAMES_COUNT` is set; the process exits afterwards.
async fn seed_games(pool: PgPool, count: usize) -> anyhow::Result<()> {
    // Ensure the Platform system user exists (id is a text PK). It must be role
    // ADMIN so the generator's quota check is bypassed, and must exist before any
    // generation so the Game.createdById FK is satisfiable.
    sqlx::query(
        r#"INSERT INTO "User" (id, email, name, username, "emailVerified", role, "vipPass")
           VALUES ('platform-system', 'platform@crosswords.system', 'Platform', 'platform',
                   now(), 'ADMIN'::"UserRole", true)
           ON CONFLICT (id) DO NOTHING"#,
    )
    .execute(&pool)
    .await?;

    let platform_user = crossword_db::AuthUser {
        id: "platform-system".to_string(),
        email: "platform@crosswords.system".to_string(),
        role: crossword_db::Role::Admin,
    };

    let topics: Vec<String> = match std::env::var("SEED_GAMES_TOPICS") {
        Ok(s) if !s.trim().is_empty() => s
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect(),
        _ => [
            "animals",
            "science",
            "music",
            "history",
            "food",
            "sports",
            "geography",
            "movies",
            "space",
            "nature",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    };
    if topics.is_empty() {
        anyhow::bail!("no seed topics available");
    }

    tracing::info!(
        "seeding {count} platform games across {} topics",
        topics.len()
    );

    // No live client; generation progress events are discarded.
    let noop_emit: Arc<dyn Fn(Value) + Send + Sync> = Arc::new(|_| {});

    for i in 0..count {
        let topic = &topics[i % topics.len()];
        tracing::info!("seed {}/{count}: generating topic '{topic}'", i + 1);
        let input = json!({ "params": { "topic": topic } });
        // run_generation returns () and handles its own errors internally as
        // `failed` events (swallowed by noop_emit), so seeding is best-effort:
        // a topic that yields no grid simply produces no Game row.
        routers::generator::run_generation(
            pool.clone(),
            platform_user.clone(),
            input,
            noop_emit.clone(),
        )
        .await;
    }

    // Publish exactly the platform's freshly-generated (still unpublished) games.
    // rows_affected is the end-to-end signal that seeding actually produced games.
    let published = sqlx::query(
        r#"UPDATE "Game" SET published = true, "updatedAt" = now()
           WHERE "createdById" = 'platform-system' AND published = false"#,
    )
    .execute(&pool)
    .await?
    .rows_affected();
    tracing::info!("seeding complete: published {published} platform games");
    Ok(())
}

/// Build the auth request from headers (next-auth cookie + optional bearer).
pub(crate) fn req_auth(headers: &HeaderMap) -> RequestAuth {
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

/// GET /api/config — runtime config for the shared wasm bundle: the deploy
/// environment + derived feature flags. The frontend fetches this at startup
/// because one bundle serves all environments (build-time constants can't tell
/// staging from prod).
async fn config(State(st): State<AppState>) -> Json<Value> {
    let env = st.env.as_str();
    Json(json!({
        "environment": env,
        "features": {
            // The dev-admin login button only works where the backend registers
            // the local-dev route — local docker-compose.
            "devLoginBypass": env == "local",
            // Beta banner (+ $1 Pro messaging) on staging only.
            "stagingBanner": env == "staging",
        }
    }))
}

/// next-auth-compatible `GET /api/auth/session`: `{}` signed out, `{user}` in.
async fn session(State(st): State<AppState>, headers: HeaderMap) -> Json<Value> {
    let auth = st.auth.authenticate(&req_auth(&headers));
    let Some(u) = auth.user else {
        return Json(json!({}));
    };
    // Enrich with the display name + real verification status from the DB (the
    // cookie only carries id/email/role).
    let row = sqlx::query(
        r#"SELECT name, ("emailVerified" IS NOT NULL) AS email_verified
           FROM "User" WHERE id = $1"#,
    )
    .bind(&u.id)
    .fetch_optional(&st.pool)
    .await
    .ok()
    .flatten();
    let name: Option<String> = row.as_ref().and_then(|r| r.get::<Option<String>, _>("name"));
    let email_verified: bool = row
        .as_ref()
        .map(|r| r.get::<bool, _>("email_verified"))
        .unwrap_or(false);
    Json(json!({
        "user": {
            "id": u.id, "email": u.email, "name": name, "image": Value::Null,
            "role": u.role, "emailVerified": email_verified
        }
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
                            let emit_ws: Arc<dyn Fn(Value) + Send + Sync> = Arc::new(
                                move |ev: Value| {
                                    let _ = txc.send(
                                        json!({ "id": id, "result": { "type": "data", "data": ev } })
                                            .to_string(),
                                    );
                                },
                            );
                            let pool = st.pool.clone();
                            let handle = tokio::spawn(async move {
                                routers::generator::run_generation(pool, user, input, emit_ws)
                                    .await;
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
