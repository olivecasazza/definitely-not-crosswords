//! crossword-server — the Rust backend replacing the Nuxt server.
//!
//! De-risk slice: proves the architecture end-to-end —
//! Dioxus client → tRPC-compatible Axum HTTP → sqlx → Postgres.
//! The client (`crossword-core::rpc`) speaks tRPC HTTP-batch:
//!   query    GET  /api/trpc/<proc>?batch=1&input={"0":<input>}
//!   mutation POST /api/trpc/<proc>?batch=1   body {"0":<input>}
//!   response [{"result":{"data":<D>}}]  (or [{"error":{...}}])
//! One real procedure is implemented (stats.getGlobalLeaderboard); the rest
//! land in Phase C. WS subscriptions (/api/trpc-ws) come with the events crate.

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::collections::HashMap;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
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

    let app = Router::new()
        .route("/api/healthz", get(|| async { "ok" }))
        .route("/api/trpc/:proc", get(trpc_get).post(trpc_post))
        .with_state(AppState { pool });

    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("crossword-server listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// tRPC query: input lives in `?input={"0":<input>}`.
async fn trpc_get(
    Path(proc): Path<String>,
    Query(q): Query<HashMap<String, String>>,
    State(st): State<AppState>,
) -> impl IntoResponse {
    let input = q
        .get("input")
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .and_then(|v| v.get("0").cloned())
        .unwrap_or(Value::Null);
    envelope(dispatch(&proc, input, &st).await)
}

/// tRPC mutation: input is the POST body `{"0":<input>}`.
async fn trpc_post(
    Path(proc): Path<String>,
    State(st): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    let input = body.get("0").cloned().unwrap_or(Value::Null);
    envelope(dispatch(&proc, input, &st).await)
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

async fn dispatch(proc: &str, _input: Value, st: &AppState) -> Result<Value, String> {
    match proc {
        "stats.getGlobalLeaderboard" => stats_global_leaderboard(st).await,
        other => Err(format!(
            "procedure not implemented in Rust backend yet: {other}"
        )),
    }
}

/// Port of `stats.getGlobalLeaderboard` — aggregate each user's completed-game
/// scores. (TS did this with nested JS loops; SQL does it directly.)
async fn stats_global_leaderboard(st: &AppState) -> Result<Value, String> {
    let rows = sqlx::query(
        r#"
        SELECT u.id,
               COALESCE(u.name, u.email, 'Anonymous Player') AS name,
               u.email,
               COUNT(DISTINCT gm.id) AS games_played,
               COALESCE(SUM(ms.score), 0)              AS total_score,
               COALESCE(SUM(ms."correctGuesses"), 0)   AS total_correct,
               COALESCE(SUM(ms."incorrectGuesses"), 0) AS total_incorrect
        FROM "User" u
        LEFT JOIN "GameMember" gm
               ON gm."userId" = u.id AND gm."completedGameId" IS NOT NULL
        LEFT JOIN "MemberScore" ms ON ms."memberId" = gm.id
        GROUP BY u.id, u.name, u.email
        ORDER BY total_score DESC, games_played DESC, name ASC
        "#,
    )
    .fetch_all(&st.pool)
    .await
    .map_err(|e| e.to_string())?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            let total_correct: i64 = r.get("total_correct");
            let total_incorrect: i64 = r.get("total_incorrect");
            let total_guesses = total_correct + total_incorrect;
            let accuracy = if total_guesses > 0 {
                (total_correct as f64 / total_guesses as f64 * 100.0).round() as i64
            } else {
                0
            };
            json!({
                "id": r.get::<String, _>("id"),
                "name": r.get::<String, _>("name"),
                "email": r.get::<Option<String>, _>("email"),
                "gamesPlayed": r.get::<i64, _>("games_played"),
                "totalScore": r.get::<i64, _>("total_score"),
                "totalCorrect": total_correct,
                "totalIncorrect": total_incorrect,
                "accuracy": accuracy,
            })
        })
        .collect();
    Ok(json!(entries))
}
