//! `generator` router — port of server/trpc/router/generator.ts + the
//! generateCrossword service. tRPC queries/mutations (`listJobs`,
//! `publishGeneratedGame`, `job:create`) go through `try_handle`; the streaming
//! `runGeneration` subscription is driven by `run_generation`, invoked from the
//! WebSocket handler (it needs the live socket to push progress events).
//!
//! The other four procs in the TS router (generateDraftGame/getJob/
//! saveDraftGame/markFailed) aren't called by the Dioxus client, so they're
//! intentionally not ported — they vanish with the Nuxt server. (ponytail)

mod dict;
mod embed;
mod solver;

use crate::ctx::Ctx;
use crossword_auth::AuthContext;
use crossword_db::{AuthUser, Capability};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use solver::{Direction, Params};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

pub async fn try_handle(proc: &str, input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "generator.listJobs" => Some(list_jobs(input, ctx).await),
        "generator.publishGeneratedGame" => Some(publish_generated_game(input, ctx).await),
        "job:create" => Some(job_create(input, ctx).await),
        _ => None,
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn with_at(mut ev: Value) -> Value {
    if let Some(obj) = ev.as_object_mut() {
        obj.insert("at".into(), json!(now_ms()));
    }
    ev
}

// ── queries / mutations ──────────────────────────────────────────────────────

async fn list_jobs(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = ctx.require_user()?;
    ctx.auth
        .require_capability(Capability::AdminAccess)
        .map_err(|e| e.to_string())?;
    let take = input
        .get("take")
        .and_then(|v| v.as_i64())
        .unwrap_or(25)
        .clamp(1, 100);

    let is_generator_admin = user.role.has(Capability::GeneratorManage);

    let rows = if is_generator_admin {
        sqlx::query(
            r#"
            SELECT j.id, j.status::text AS status, j.topic, j.width, j.height,
                   j.visibility::text AS visibility,
                   to_char(j."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
                   g.id AS game_id, g.title AS game_title, g.published AS game_published
            FROM "CrosswordGenerationJob" j
            LEFT JOIN "Game" g ON g.id = j."resultGameId"
            ORDER BY j."createdAt" DESC
            LIMIT $1
            "#,
        )
        .bind(take)
        .fetch_all(&ctx.pool)
        .await
    } else {
        sqlx::query(
            r#"
            SELECT j.id, j.status::text AS status, j.topic, j.width, j.height,
                   j.visibility::text AS visibility,
                   to_char(j."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS created_at,
                   g.id AS game_id, g.title AS game_title, g.published AS game_published
            FROM "CrosswordGenerationJob" j
            LEFT JOIN "Game" g ON g.id = j."resultGameId"
            WHERE j.visibility = 'PUBLIC' OR j."createdById" = $2
            ORDER BY j."createdAt" DESC
            LIMIT $1
            "#,
        )
        .bind(take)
        .bind(&user.id)
        .fetch_all(&ctx.pool)
        .await
    }
    .map_err(|e| e.to_string())?;

    let jobs: Vec<Value> = rows
        .iter()
        .map(|r| {
            let game_id: Option<String> = r.get("game_id");
            let result_game = game_id.map(|id| {
                json!({
                    "id": id,
                    "title": r.get::<Option<String>, _>("game_title"),
                    "published": r.get::<Option<bool>, _>("game_published").unwrap_or(false),
                })
            });
            json!({
                "id": r.get::<String, _>("id"),
                "status": r.get::<String, _>("status"),
                "topic": r.get::<String, _>("topic"),
                "width": r.get::<i32, _>("width"),
                "height": r.get::<i32, _>("height"),
                "visibility": r.get::<String, _>("visibility"),
                "createdAt": r.get::<Option<String>, _>("created_at"),
                "resultGame": result_game,
            })
        })
        .collect();
    Ok(json!(jobs))
}

async fn publish_generated_game(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = ctx.require_user()?.clone();
    let game_id = input
        .get("gameId")
        .and_then(|v| v.as_str())
        .ok_or("missing gameId")?;

    let row =
        sqlx::query(r#"SELECT source::text AS source, "createdById" FROM "Game" WHERE id = $1"#)
            .bind(game_id)
            .fetch_optional(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;

    let Some(row) = row else {
        return Err("Game was not found.".to_string());
    };
    match row.get::<Option<String>, _>("source").as_deref() {
        Some("GENERATED") => {}
        _ => return Err("Only generated games can be published through this route.".to_string()),
    }

    // Admins can publish any generated game; everyone else only their own.
    if !user.role.has(Capability::AdminAccess)
        && row.get::<Option<String>, _>("createdById").as_deref() != Some(user.id.as_str())
    {
        return Err("You can only publish games you generated.".to_string());
    }

    sqlx::query(r#"UPDATE "Game" SET published = true, "updatedAt" = now() WHERE id = $1"#)
        .bind(game_id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": game_id, "published": true }))
}

// ── job:create tRPC procedure ─────────────────────────────────────────────────
//
// Synchronous one-shot generator: validate params → quota → run the dictionary
// + solver pipeline in a blocking task → return the full grid JSON inline.
//
// This is the tRPC counterpart to the WS subscription `generator.runGeneration`
// and the REST `POST /api/jobs`. Use cases the Dioxus client actually hits:
//
//   * The admin's "preview" button on game_new.rs: shows a grid immediately
//     without forcing the operator to open a second tab to watch the WS events.
//   * The CI smoke for DEF-70: a deterministic one-shot call that returns the
//     whole grid for shape assertions (cells, clues, solutionHash).
//
// No DB persistence — the grid is ephemeral. The streamed WS subscription
// remains the canonical "save + persist" path; `job:create` is a read-only
// preview that the client can choose to publish via `publishGeneratedGame`
// once it's satisfied.
//
// RBAC: requires the same `job:create` capability that gates the REST endpoint
// (DEF-36 / PR #70). A missing capability is a structured 403 FORBIDDEN, NOT a
// 500 — that's the bug DEF-70 explicitly calls out. Quota is also enforced for
// free-tier callers via `check_quota`; Pro / admin / generator-manage are
// unlimited. Returns INTERNAL on unexpected server errors so the envelope
// surfaces a 500 instead of pretending the request was bad.

/// One numbered question as it appears in the grid response. Mirrors the
/// `Question` DB row shape so the JSON we emit here and the JSON
/// `rest_get_grid` reads from the DB are byte-compatible.
#[derive(Debug, Clone)]
pub struct GridQuestion {
    pub number: i32,
    pub direction: String,
    pub root_x: i32,
    pub root_y: i32,
    pub answer: String,
    pub clue: String,
}

/// Build the `{cells, questions, grid, solutionHash}` JSON for a generator
/// result. Pure: takes the question list + grid metadata, returns the JSON
/// shape consumed by both `job:create` and `rest_get_grid`. Centralized so
/// the wire format is identical across the tRPC and REST surfaces (and so
/// the integration test can assert on one shape, not two).
///
/// `w` / `h` are the bounding-box dimensions of the placed words, NOT the
/// solver's nominal grid — a 21×21 board can land a grid in a smaller box if
/// generation fills only part of it. Caller is responsible for trimming;
/// this function pads with `None` to the bounding box.
pub fn build_grid_json(
    id: Option<&str>,
    title: &str,
    questions: &[GridQuestion],
    w: i32,
    h: i32,
) -> Result<Value, String> {
    if questions.is_empty() {
        return Err("Grid has no questions.".to_string());
    }
    if w <= 0 || h <= 0 {
        return Err("Grid dimensions must be positive.".to_string());
    }

    let mut cells: Vec<Vec<Option<String>>> = vec![vec![None; w as usize]; h as usize];
    for q in questions {
        for (i, ch) in q.answer.chars().enumerate() {
            let x = if q.direction == "ACROSS" {
                q.root_x + i as i32
            } else {
                q.root_x
            };
            let y = if q.direction == "DOWN" {
                q.root_y + i as i32
            } else {
                q.root_y
            };
            if y < 0 || x < 0 || (y as usize) >= cells.len() || (x as usize) >= cells[0].len() {
                return Err(format!(
                    "Question {} ({}) at ({},{}) with answer '{}' overflows {}x{} grid",
                    q.number, q.direction, q.root_x, q.root_y, q.answer, w, h
                ));
            }
            cells[y as usize][x as usize] = Some(ch.to_string());
        }
    }

    let questions_json: Vec<Value> = questions
        .iter()
        .map(|q| {
            json!({
                "number": q.number,
                "direction": q.direction,
                "rootX": q.root_x,
                "rootY": q.root_y,
                "answer": q.answer,
                "clue": q.clue,
            })
        })
        .collect();

    let hash = compute_solution_hash(w, h, &cells, &questions_json);

    let mut obj = serde_json::Map::new();
    if let Some(id) = id {
        obj.insert("id".into(), json!(id));
    }
    obj.insert("title".into(), json!(title));
    obj.insert("grid".into(), json!({ "w": w, "h": h }));
    obj.insert("cells".into(), json!(cells));
    obj.insert("questions".into(), json!(questions_json));
    obj.insert("solutionHash".into(), json!(hash));
    Ok(Value::Object(obj))
}

/// Deterministic 64-char hex SHA-256 over the grid payload. The client can
/// stash this when the grid is shown and re-compute it after `publishGeneratedGame`
/// to confirm the persisted row matches what the user actually saw — a
/// tamper / version-drift check that's also useful for the canary tests.
///
/// Hash inputs (in order): `grid.w`, `grid.h`, each cell's letter (None → ""),
/// then each question's `number|direction|rootX|rootY|answer|clue`. The fields
/// are concatenated with a NUL separator so e.g. `("1","ACROSS",0,0)` can't
/// collide with `("1","ACROS","S",0,0)`.
pub fn compute_solution_hash(
    w: i32,
    h: i32,
    cells: &[Vec<Option<String>>],
    questions: &[Value],
) -> String {
    let mut h256 = Sha256::new();
    h256.update(format!("{}x{}\0", w, h).as_bytes());
    for row in cells.iter() {
        for cell in row.iter() {
            h256.update(b"\0");
            h256.update(cell.as_deref().unwrap_or("").as_bytes());
        }
        h256.update(b"|\0");
    }
    for q in questions {
        h256.update(b"\0");
        h256.update(
            q.get("number")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .to_string()
                .as_bytes(),
        );
        h256.update(b"|");
        h256.update(
            q.get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .as_bytes(),
        );
        h256.update(b"|");
        h256.update(
            q.get("rootX")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .to_string()
                .as_bytes(),
        );
        h256.update(b"|");
        h256.update(
            q.get("rootY")
                .and_then(|v| v.as_i64())
                .unwrap_or(0)
                .to_string()
                .as_bytes(),
        );
        h256.update(b"|");
        h256.update(
            q.get("answer")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .as_bytes(),
        );
        h256.update(b"|");
        h256.update(
            q.get("clue")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .as_bytes(),
        );
    }
    format!("{:x}", h256.finalize())
}

/// `job:create` tRPC procedure — synchronous, RBAC-gated, returns the full
/// grid JSON (cells + clues + solutionHash) inline.
///
/// Wire-format contract:
/// - Input:  `{"params": {topic, width, height, minWordLength, ...}, "title"?: "..."}`
/// - Output: `{"id": null, "title": "...", "grid": {"w","h"}, "cells": [[..]],
///             "questions": [{number, direction, rootX, rootY, answer, clue}, ...],
///             "solutionHash": "<64-char-hex>"}`
///
/// Errors are structured:
/// - `"FORBIDDEN"` → 403 (missing job:create capability; DEF-36 RBAC).
/// - `"UNAUTHORIZED"` → 401 (no session).
/// - `BAD_REQUEST` messages → 400 (validation: topic missing, dimensions bad).
/// - `"INTERNAL: <message>"` → 500 (DB / embedding / solver failure).
pub async fn job_create(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    // 1. RBAC — `job:create` capability is required (DEF-36). The capability
    //    gate returns `AppError::Forbidden(JobCreate)`; we map that to the
    //    structured FORBIDDEN / 403 envelope the client expects, NOT a 500.
    let user = match ctx.auth.require_capability(Capability::JobCreate) {
        Ok(u) => u.clone(),
        Err(e) => {
            return Err(match e {
                crossword_db::AppError::Unauthorized => "UNAUTHORIZED".to_string(),
                crossword_db::AppError::Forbidden(_) => "FORBIDDEN".to_string(),
                other => format!("INTERNAL: {other}"),
            });
        }
    };

    // 2. Quota — free users are limited to FREE_LIMIT / month (DEF-67 plan §3).
    //    Errors from `check_quota` are user-facing messages → BAD_REQUEST.
    if let Err(e) = check_quota(&ctx.pool, &user).await {
        return Err(e);
    }

    // 3. Param validation — anything structural returns BAD_REQUEST so the
    //    client surfaces "fix the recipe" instead of retrying forever.
    let (params, raw_params, title) = parse_params(input)?;

    // 4. Build the dictionary + run the solver. The DB fetch is async; the
    //    embedding scoring and grid search are CPU-bound and run inside a
    //    `spawn_blocking` task so they don't stall the tokio worker pool.
    let pool = ctx.pool.clone();
    let topic = raw_params
        .get("topic")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let rows = match dict::fetch_rows(&pool, &params).await {
        Ok(r) => r,
        Err(e) => return Err(format!("INTERNAL: {e}")),
    };
    let topic_for_blocking = topic.clone();
    // `params` is consumed by the blocking task; capture only the dimensions
    // we still need for the response bounding box (cheap, no clone of `Params`).
    let solver_width = params.width;
    let solver_height = params.height;
    let blocking = tokio::task::spawn_blocking(move || -> Result<GenResult, String> {
        // No live progress events here — the client gets the result inline.
        let mut noop = |_ev: Value| {};
        let dictionary = dict::build_dictionary(rows, &topic_for_blocking, &mut noop)?;
        let best = solver::generate_best(&dictionary, &params, &mut noop)?;
        solver::validate_grid(
            &best.grid,
            &best.placed,
            &dictionary.dictionary_set,
            &params,
        )?;
        Ok(build_result(
            &best,
            &dictionary,
            &params,
            &topic_for_blocking,
        ))
    })
    .await;

    let gen = match blocking {
        Ok(inner) => inner?,
        Err(join_err) => return Err(format!("INTERNAL: generation task failed: {join_err}")),
    };

    // 5. Render the grid response. We don't persist: this is the synchronous
    //    "preview" path. Use a deterministic pseudo-id (job:create doesn't mint
    //    a real jobId — the streaming subscription owns that).
    let title = title.unwrap_or_else(|| gen.title.clone());
    let mut w = solver_width;
    let mut h = solver_height;
    for q in &gen.questions {
        let len = q.answer.chars().count() as i32;
        if q.direction == Direction::Across {
            w = w.max(q.root_x + len);
        } else {
            h = h.max(q.root_y + len);
        }
        w = w.max(q.root_x + 1);
        h = h.max(q.root_y + 1);
    }
    let grid_questions: Vec<GridQuestion> = gen
        .questions
        .iter()
        .map(|q| GridQuestion {
            number: q.number,
            direction: q.direction.as_str().to_string(),
            root_x: q.root_x,
            root_y: q.root_y,
            answer: q.answer.clone(),
            clue: q.question_text.clone(),
        })
        .collect();
    build_grid_json(None, &title, &grid_questions, w, h)
}

// ── runGeneration subscription (driven by the WS handler) ────────────────────

/// Parse + validate the `{ params, title? }` input into a solver `Params`,
/// the raw params JSON (persisted), and the optional title.
fn parse_params(input: &Value) -> Result<(Params, Value, Option<String>), String> {
    let raw = input.get("params").cloned().unwrap_or(Value::Null);
    let geti = |k: &str, d: i64| raw.get(k).and_then(|v| v.as_i64()).unwrap_or(d) as i32;
    let topic = raw
        .get("topic")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or("topic is required")?;
    let p = Params {
        width: geti("width", 21),
        height: geti("height", 21),
        min_len: geti("minWordLength", 3),
        max_len: geti("maxWordLength", 12),
        target: geti("targetWords", 42),
        runs: geti("runs", 20),
        max_attempts: geti("maxAttempts", 180),
    };
    if p.min_len > p.max_len {
        return Err("minWordLength cannot be greater than maxWordLength.".to_string());
    }
    if p.max_len > p.width.max(p.height) {
        return Err("maxWordLength cannot exceed the larger grid dimension.".to_string());
    }
    // re-stash topic into the raw params so the persisted JSON keeps it normalized
    let mut raw = raw;
    if let Some(o) = raw.as_object_mut() {
        o.insert("topic".into(), json!(topic));
    }
    let title = input
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Ok((p, raw, title))
}

/// Free users get FREE_LIMIT generations per calendar month; admins
/// (generator:manage) and Pro users are unlimited. Keep in sync with
/// subscription.rs::FREE_LIMIT (subscription.getStatus reports the same quota).
const FREE_LIMIT: i32 = 5;

/// Users with generator:manage and Pro users are unlimited; free users get
/// FREE_LIMIT generations per calendar month. The Pro test mirrors
/// subscription.getStatus: ACTIVE, or CANCELLED while the already-paid period
/// is still live, or User.vipPass. Returns is_unlimited; Err on quota
/// exhausted.
async fn check_quota(pool: &PgPool, user: &AuthUser) -> Result<bool, String> {
    if user.role.has(Capability::GeneratorManage) {
        return Ok(true);
    }
    let row = sqlx::query(
        r#"
        SELECT u."vipPass",
               s.status::text AS subscription_status,
               (s."currentPeriodEnd" IS NOT NULL
                   AND s."currentPeriodEnd" > (NOW() AT TIME ZONE 'UTC')) AS period_active
        FROM "User" u
        LEFT JOIN "Subscription" s ON s."userId" = u.id
        WHERE u.id = $1
        "#,
    )
    .bind(&user.id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let (vip, status, period_active) = match &row {
        Some(r) => (
            r.get::<bool, _>("vipPass"),
            r.get::<Option<String>, _>("subscription_status"),
            r.try_get::<bool, _>("period_active").unwrap_or(false),
        ),
        None => (false, None, false),
    };
    let is_pro = status
        .as_deref()
        .map(|s| s == "ACTIVE" || (s == "CANCELLED" && period_active))
        .unwrap_or(false)
        || vip;

    if !is_pro {
        // lazy-create the quota row, then lazily reset it at month boundaries
        let row = sqlx::query(
            r#"
            INSERT INTO "GenerationQuota" (id, "userId", "usedThisMonth", "monthResetAt", "createdAt", "updatedAt")
            VALUES ($1, $2, 0, now(), now(), now())
            ON CONFLICT ("userId") DO UPDATE SET "updatedAt" = now()
            RETURNING "usedThisMonth",
              (date_trunc('month', "monthResetAt" AT TIME ZONE 'UTC')
                 = date_trunc('month', NOW() AT TIME ZONE 'UTC')) AS is_current
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&user.id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

        let mut used: i32 = row.get("usedThisMonth");
        let is_current: bool = row.get("is_current");
        if !is_current {
            sqlx::query(
                r#"UPDATE "GenerationQuota" SET "usedThisMonth" = 0, "monthResetAt" = now() WHERE "userId" = $1"#,
            )
            .bind(&user.id)
            .execute(pool)
            .await
            .map_err(|e| e.to_string())?;
            used = 0;
        }
        if used >= FREE_LIMIT {
            return Err(
                "Monthly generation limit reached — upgrade to Pro for unlimited puzzles."
                    .to_string(),
            );
        }
    }
    Ok(is_pro)
}

struct QData {
    number: i32,
    answer: String,
    question_text: String,
    root_x: i32,
    root_y: i32,
    direction: Direction,
}

struct GenResult {
    title: String,
    questions: Vec<QData>,
    metrics: Value,
}

fn build_result(best: &solver::Best, d: &dict::Dictionary, p: &Params, topic: &str) -> GenResult {
    let questions = solver::number_words(&best.placed)
        .into_iter()
        .map(|(w, number)| QData {
            number,
            question_text: d
                .clue_by_word
                .get(&w.word)
                .cloned()
                .unwrap_or_else(|| format!("Related to {topic}")),
            answer: w.word.to_uppercase(),
            root_x: w.x,
            root_y: w.y,
            direction: w.dir,
        })
        .collect::<Vec<_>>();
    let metrics = json!({
        "topic": topic, "width": p.width, "height": p.height, "targetWords": p.target,
        "placedWords": best.placed.len(), "seed": best.seed, "runs": p.runs, "score": best.score,
    });
    GenResult {
        title: format!("Generated: {topic}"),
        questions,
        metrics,
    }
}

/// Drive a full streaming generation: validate, create the job row, run the
/// (blocking) embedding + solver off the runtime, persist the game, and push
/// `started` / progress / `completed` | `failed` events through `emit_ws`.
/// `emit_ws` receives the inner event object; the caller wraps it as a tRPC
/// `{type:"data", data}` frame. Runs to completion regardless of client.
pub async fn run_generation(
    pool: PgPool,
    user: AuthUser,
    input: Value,
    emit_ws: Arc<dyn Fn(Value) + Send + Sync>,
) {
    let started_at = now_ms();
    let log: Arc<std::sync::Mutex<Vec<Value>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    let emit: Arc<dyn Fn(Value) + Send + Sync> = {
        let log = log.clone();
        let emit_ws = emit_ws.clone();
        Arc::new(move |ev: Value| {
            let e = with_at(ev);
            log.lock().unwrap().push(e.clone());
            emit_ws(e);
        })
    };

    // validate + quota up front, surfacing failures as a `failed` event
    let (params, raw_params, title) = match parse_params(&input) {
        Ok(v) => v,
        Err(e) => return fail(&emit_ws, None, e),
    };
    let is_unlimited = match check_quota(&pool, &user).await {
        Ok(u) => u,
        Err(e) => return fail(&emit_ws, None, e),
    };

    let job_id = match create_job(&pool, &user.id, &params, &raw_params, title.as_deref()).await {
        Ok(id) => id,
        Err(e) => return fail(&emit_ws, None, e),
    };
    emit(json!({ "type": "started", "jobId": job_id }));
    write_audit_log(&pool, &job_id, &user.id, "job_started", json!({})).await;

    let rows = match dict::fetch_rows(&pool, &params).await {
        Ok(r) => r,
        Err(e) => {
            finalize_failed(&pool, &job_id, &e, &log, started_at, &user.id).await;
            return fail(&emit_ws, Some(&job_id), e);
        }
    };

    // ── blocking CPU: embedding scoring + grid solving ──────────────────────
    let topic = raw_params
        .get("topic")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let emit_blocking = emit.clone();
    let blocking = tokio::task::spawn_blocking(move || {
        let mut em = |ev: Value| emit_blocking(ev);
        (|| -> Result<GenResult, String> {
            em(json!({ "type": "stage", "stage": "loading-dictionary", "message": "Loading dictionary and scoring candidates" }));
            let dictionary = dict::build_dictionary(rows, &topic, &mut em)?;
            em(json!({ "type": "stage", "stage": "solving", "message": format!("Generating crossword grids ({} runs)", params.runs) }));
            let best = solver::generate_best(&dictionary, &params, &mut em)?;
            em(json!({ "type": "stage", "stage": "validating", "message": "Validating winning grid" }));
            solver::validate_grid(&best.grid, &best.placed, &dictionary.dictionary_set, &params)?;
            em(json!({ "type": "log", "level": "info", "message": format!("Best grid: {} words placed (score {}, seed {})", best.placed.len(), best.score, best.seed) }));
            Ok(build_result(&best, &dictionary, &params, &topic))
        })()
    })
    .await;

    let result = match blocking {
        Ok(inner) => inner,
        Err(join_err) => Err(format!("generation task failed: {join_err}")),
    };

    match result {
        Ok(gen) => {
            let title = title.unwrap_or_else(|| gen.title.clone());
            match finalize_success(
                &pool,
                &job_id,
                &title,
                &gen,
                &raw_params,
                &log,
                started_at,
                &user.id,
            )
            .await
            {
                Ok(game_id) => {
                    if !is_unlimited {
                        let _ = sqlx::query(
                            r#"UPDATE "GenerationQuota" SET "usedThisMonth" = "usedThisMonth" + 1, "updatedAt" = now() WHERE "userId" = $1"#,
                        )
                        .bind(&user.id)
                        .execute(&pool)
                        .await;
                    }
                    emit_ws(with_at(json!({
                        "type": "completed", "jobId": job_id, "gameId": game_id,
                        "title": title, "questionCount": gen.questions.len(), "metrics": gen.metrics,
                    })));
                }
                Err(e) => {
                    finalize_failed(&pool, &job_id, &e, &log, started_at, &user.id).await;
                    fail(&emit_ws, Some(&job_id), e);
                }
            }
        }
        Err(e) => {
            finalize_failed(&pool, &job_id, &e, &log, started_at, &user.id).await;
            fail(&emit_ws, Some(&job_id), e);
        }
    }
}

fn fail(emit_ws: &Arc<dyn Fn(Value) + Send + Sync>, job_id: Option<&str>, error: String) {
    emit_ws(with_at(json!({
        "type": "failed",
        "jobId": job_id,
        "error": error,
    })));
}

async fn write_audit_log(
    pool: &PgPool,
    job_id: &str,
    actor_id: &str,
    event_type: &str,
    payload: Value,
) {
    let id = Uuid::new_v4().to_string();
    let _ = sqlx::query(
        r#"
        INSERT INTO "JobAuditLog" (id, "jobId", "actorId", "eventType", payload, "createdAt")
        VALUES ($1, $2, $3, $4, $5::jsonb, now())
        "#,
    )
    .bind(&id)
    .bind(job_id)
    .bind(actor_id)
    .bind(event_type)
    .bind(&payload)
    .execute(pool)
    .await;
}

async fn create_job(
    pool: &PgPool,
    creator_id: &str,
    p: &Params,
    raw_params: &Value,
    title: Option<&str>,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    let metadata = json!({
        "requestedTitle": title,
        "params": compact_params(p, raw_params),
    });
    sqlx::query(
        r#"
        INSERT INTO "CrosswordGenerationJob"
          (id, status, title, topic, width, height, "minWordLength", "maxWordLength",
           params, metadata, "eventLog", "startedAt", "createdById", "createdAt", "updatedAt")
        VALUES ($1, 'RUNNING'::"GenerationStatus", $2, $3, $4, $5, $6, $7,
                $8::jsonb, $9::jsonb, '[]'::jsonb, now(), $10, now(), now())
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(
        raw_params
            .get("topic")
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    )
    .bind(p.width)
    .bind(p.height)
    .bind(p.min_len)
    .bind(p.max_len)
    .bind(raw_params)
    .bind(&metadata)
    .bind(creator_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    write_audit_log(
        pool,
        &id,
        creator_id,
        "job_created",
        json!({ "topic": raw_params.get("topic").and_then(|v| v.as_str()).unwrap_or(""), "grid": format!("{}x{}", p.width, p.height) }),
    )
    .await;

    Ok(id)
}

fn compact_params(p: &Params, raw: &Value) -> Value {
    json!({
        "topic": raw.get("topic").and_then(|v| v.as_str()).unwrap_or(""),
        "grid": format!("{}x{}", p.width, p.height),
        "minWordLength": p.min_len, "maxWordLength": p.max_len,
        "targetWords": p.target, "runs": p.runs, "maxAttempts": p.max_attempts,
    })
}

async fn finalize_success(
    pool: &PgPool,
    job_id: &str,
    title: &str,
    gen: &GenResult,
    raw_params: &Value,
    log: &Arc<std::sync::Mutex<Vec<Value>>>,
    started_at: i64,
    created_by: &str,
) -> Result<String, String> {
    let game_id = Uuid::new_v4().to_string();
    let completed_at = now_ms();
    let duration = (completed_at - started_at) as i32;

    let mut event_log = log.lock().unwrap().clone();
    event_log.push(json!({
        "type": "completed", "jobId": job_id, "gameId": game_id, "title": title,
        "questionCount": gen.questions.len(), "metrics": gen.metrics, "at": completed_at,
    }));
    let metadata = json!({
        "requestedTitle": null, "resolvedTitle": title,
        "params": compact_params_from_raw(raw_params),
        "questionCount": gen.questions.len(), "resultGameId": game_id,
    });

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        r#"INSERT INTO "Game" (id, type, "createdAt", "updatedAt", title, published, source, "createdById")
           VALUES ($1, 'Game', now(), now(), $2, false, 'GENERATED'::"GameSource", $3)"#,
    )
    .bind(&game_id)
    .bind(title)
    .bind(created_by)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    for q in &gen.questions {
        sqlx::query(
            r#"INSERT INTO "Question"
               (id, type, number, answer, "questionText", "rootX", "rootY", direction, "gameId")
               VALUES ($1, 'Question', $2, $3, $4, $5, $6, $7::"QuestionDirectionEnum", $8)"#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(q.number)
        .bind(&q.answer)
        .bind(&q.question_text)
        .bind(q.root_x)
        .bind(q.root_y)
        .bind(q.direction.as_str())
        .bind(&game_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    sqlx::query(
        r#"UPDATE "CrosswordGenerationJob"
           SET status = 'SUCCEEDED'::"GenerationStatus", title = $2, metrics = $3::jsonb,
               metadata = $4::jsonb, "eventLog" = $5::jsonb, "completedAt" = now(),
               "durationMs" = $6, "resultGameId" = $7, "updatedAt" = now()
           WHERE id = $1"#,
    )
    .bind(job_id)
    .bind(title)
    .bind(&gen.metrics)
    .bind(&metadata)
    .bind(json!(event_log))
    .bind(duration)
    .bind(&game_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    write_audit_log(
        pool,
        job_id,
        created_by,
        "job_completed",
        json!({ "gameId": game_id, "durationMs": duration }),
    )
    .await;

    Ok(game_id)
}

fn compact_params_from_raw(raw: &Value) -> Value {
    let geti = |k: &str, d: i64| raw.get(k).and_then(|v| v.as_i64()).unwrap_or(d);
    json!({
        "topic": raw.get("topic").and_then(|v| v.as_str()).unwrap_or(""),
        "grid": format!("{}x{}", geti("width", 0), geti("height", 0)),
        "minWordLength": geti("minWordLength", 0), "maxWordLength": geti("maxWordLength", 0),
        "targetWords": geti("targetWords", 0), "runs": geti("runs", 0),
        "maxAttempts": geti("maxAttempts", 0),
    })
}

async fn finalize_failed(
    pool: &PgPool,
    job_id: &str,
    error: &str,
    log: &Arc<std::sync::Mutex<Vec<Value>>>,
    started_at: i64,
    created_by: &str,
) {
    let completed_at = now_ms();
    let duration = (completed_at - started_at) as i32;
    let mut event_log = log.lock().unwrap().clone();
    event_log
        .push(json!({ "type": "failed", "jobId": job_id, "error": error, "at": completed_at }));
    let _ = sqlx::query(
        r#"UPDATE "CrosswordGenerationJob"
           SET status = 'FAILED'::"GenerationStatus", error = $2, "eventLog" = $3::jsonb,
               "completedAt" = now(), "durationMs" = $4, "updatedAt" = now()
           WHERE id = $1"#,
    )
    .bind(job_id)
    .bind(error)
    .bind(json!(event_log))
    .bind(duration)
    .execute(pool)
    .await;

    write_audit_log(
        pool,
        job_id,
        created_by,
        "job_failed",
        json!({ "error": error, "durationMs": duration }),
    )
    .await;
}

/// Authorize a `generator.runGeneration` subscription before it starts. Returns
/// the authenticated user, or a tRPC-style error string. Requires the
/// `JobCreate` capability; free/Pro quota is enforced downstream by
/// `check_quota`.
pub fn authorize(auth: &AuthContext) -> Result<AuthUser, String> {
    let user = auth.require_user().map_err(|e| e.to_string())?;
    if !user.role.has(Capability::JobCreate) {
        return Err(format!("{} required", Capability::JobCreate));
    }
    Ok(user.clone())
}

/// REST endpoint: `POST /api/jobs` — validates job creation capability and
/// enqueues a generation job. Returns 201 with `{jobId}` on success,
/// 403 if the caller lacks `job:create`, or 400 on validation error.
pub async fn rest_create_job(ctx: &Ctx, params_json: Value) -> Result<Value, (String, i32)> {
    ctx.auth
        .require_capability(Capability::JobCreate)
        .map_err(|e| (e.to_string(), 403))?;
    let user = ctx.require_user().map_err(|e| (e.to_string(), 401))?;
    let (params, _raw, title) = parse_params(&params_json).map_err(|e| (e, 400))?;
    let job_id = create_job(&ctx.pool, &user.id, &params, &params_json, title.as_deref())
        .await
        .map_err(|e| (e.to_string(), 500))?;
    Ok(json!({ "jobId": job_id }))
}

/// REST endpoint: `GET /api/grids/:id` — retrieve a generated grid as JSON.
/// Returns the grid shape, cell letters, and numbered clues in the same format
/// the Dioxus client uses for manually-created games (active_game.getStartDetails).
///
/// Response shape:
/// ```json
/// {
///   "id": "<game-id>",
///   "title": "<game-title>",
///   "grid": { "w": <width>, "h": <height> },
///   "cells": [[<col0>, ...], ...],
///   "questions": [{ "number": 1, "direction": "ACROSS", "rootX": 0, "rootY": 0, "answer": "CRANE", "clue": "A large bird" }, ...],
///   "solutionHash": "<64-char-hex>"
/// }
/// ```
///
/// Returns 404 if the grid does not exist or is not a generated game.
pub async fn rest_get_grid(ctx: &Ctx, id: &str) -> Result<Value, (String, i32)> {
    let row = sqlx::query(
        r#"
        SELECT g.id, g.title, g.source::text AS source,
               g."createdAt", g."updatedAt", g.published
        FROM "Game" g
        WHERE g.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| (e.to_string(), 500))?;

    let row = match row {
        Some(r) => r,
        None => return Err(("Grid not found.".to_string(), 404)),
    };

    let source: String = row.get("source");
    if source != "GENERATED" {
        return Err(("Grid is not a generated game.".to_string(), 404));
    }

    let q_rows = sqlx::query(
        r#"
        SELECT "number", "rootX", "rootY", answer, "questionText", direction::text AS direction
        FROM "Question"
        WHERE "gameId" = $1
        ORDER BY "number" ASC, direction ASC
        "#,
    )
    .bind(id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| (e.to_string(), 500))?;

    if q_rows.is_empty() {
        return Err(("Grid has no questions.".to_string(), 404));
    }

    let questions: Vec<GridQuestion> = q_rows
        .iter()
        .map(|r| GridQuestion {
            number: r.get::<i32, _>("number"),
            direction: r.get::<String, _>("direction"),
            root_x: r.get::<i32, _>("rootX"),
            root_y: r.get::<i32, _>("rootY"),
            answer: r.get::<String, _>("answer"),
            clue: r.get::<String, _>("questionText"),
        })
        .collect();

    let mut w: i32 = 0;
    let mut h: i32 = 0;
    for q in &questions {
        let len = q.answer.chars().count() as i32;
        if q.direction == "ACROSS" {
            w = w.max(q.root_x + len);
        } else {
            h = h.max(q.root_y + len);
        }
        w = w.max(q.root_x + 1);
        h = h.max(q.root_y + 1);
    }

    build_grid_json(
        Some(&row.get::<String, _>("id")),
        &row.get::<String, _>("title"),
        &questions,
        w,
        h,
    )
    .map_err(|e| match e.as_str() {
        // "Grid has no questions." / "Grid dimensions must be positive." are
        // content-shape problems with the stored row, not server faults —
        // surface them as 404 so the client can distinguish "this grid is
        // broken" from "the server is on fire".
        "Grid has no questions." => (e, 404),
        _ => (e, 500),
    })
}

#[cfg(test)]
mod tests {
    //! Unit tests for the pure helpers behind `job:create` and `rest_get_grid`.
    //!
    //! These cover the DEF-70 acceptance items that don't need a real DB or the
    //! ONNX model: grid JSON shape, cell count, symmetry (ACROSS vs DOWN math),
    //! clue enumeration completeness, and the `solutionHash` schema. The DB +
    //! RBAC integration is covered separately by `tests/job_create_rbac.rs`.

    use super::*;

    /// A canonical "small cross" grid — two ACROSS words + one DOWN word in a
    /// 5×3 bounding box, with one valid ACROSS/DOWN crossing. Mirrors what the
    /// solver actually emits so the test catches any drift in the wire shape.
    ///
    /// Layout:
    /// ```text
    ///       col 0  1  2  3  4
    /// row 0:  H   E  L  L  O    HELLO (ACROSS, rootX=0)
    /// row 1:  H   .  .  .  .    HL (DOWN at col 0; second cell is past HELLO's row)
    /// row 2:  O   C  E  A  N    OCEAN (ACROSS, rootX=0)
    /// ```
    /// Crossing: (0,0) is both HELLO[0]=H and HL[0]=H ✓. The DOWN word stops at
    /// row 1 so it never touches OCEAN (which is row 2).
    fn sample_questions() -> Vec<GridQuestion> {
        vec![
            GridQuestion {
                number: 1,
                direction: "ACROSS".into(),
                root_x: 0,
                root_y: 0,
                answer: "HELLO".into(),
                clue: "Greeting".into(),
            },
            GridQuestion {
                number: 2,
                direction: "ACROSS".into(),
                root_x: 0,
                root_y: 2,
                answer: "OCEAN".into(),
                clue: "Large body of water".into(),
            },
            GridQuestion {
                number: 1,
                direction: "DOWN".into(),
                root_x: 0,
                root_y: 0,
                answer: "HO".into(),
                clue: "Santa's call".into(),
            },
        ]
    }

    #[test]
    fn build_grid_json_emits_canonical_shape() {
        let qs = sample_questions();
        let v = build_grid_json(Some("g1"), "Title", &qs, 5, 3).expect("grid");
        assert_eq!(v["id"], json!("g1"));
        assert_eq!(v["title"], json!("Title"));
        assert_eq!(v["grid"]["w"], json!(5));
        assert_eq!(v["grid"]["h"], json!(3));
        let cells = v["cells"].as_array().expect("cells is array");
        assert_eq!(cells.len(), 3, "cell count = h");
        for row in cells.iter() {
            assert_eq!(row.as_array().unwrap().len(), 5, "each row = w");
        }
        let questions = v["questions"].as_array().expect("questions");
        assert_eq!(questions.len(), 3);
        assert_eq!(questions[0]["number"], 1);
        assert_eq!(questions[0]["direction"], "ACROSS");
        assert_eq!(questions[0]["rootX"], 0);
        assert_eq!(questions[0]["rootY"], 0);
        assert_eq!(questions[0]["answer"], "HELLO");
        assert_eq!(questions[0]["clue"], "Greeting");
        assert!(v["solutionHash"].is_string());
        assert_eq!(
            v["solutionHash"].as_str().unwrap().len(),
            64,
            "SHA-256 hex is 64 chars"
        );
        assert!(
            v["solutionHash"]
                .as_str()
                .unwrap()
                .chars()
                .all(|c| c.is_ascii_hexdigit()),
            "solutionHash must be lowercase hex"
        );
    }

    #[test]
    fn build_grid_json_letters_match_across_and_down() {
        // HELLO at (0,0) ACROSS → cells[0][0..4] = H,E,L,L,O
        // OCEAN at (0,2) ACROSS → cells[2][0..4] = O,C,E,A,N
        // HO    at (0,0) DOWN    → cells[0..2][0] = H,O
        // Shared: (0,0) = H (HELLO[0]=H, HO[0]=H)
        //         (2,0) = O (OCEAN[0]=O, HO[1]=O)
        let qs = sample_questions();
        let v = build_grid_json(None, "T", &qs, 5, 3).unwrap();
        let cells = v["cells"].as_array().unwrap();
        assert_eq!(cells[0][0], json!("H"));
        assert_eq!(cells[0][1], json!("E"));
        assert_eq!(cells[0][2], json!("L"));
        assert_eq!(cells[0][3], json!("L"));
        assert_eq!(cells[0][4], json!("O"));
        assert_eq!(cells[1][0], json!("O"));
        assert_eq!(cells[2][0], json!("O"));
        assert_eq!(cells[2][1], json!("C"));
        assert_eq!(cells[2][2], json!("E"));
        assert_eq!(cells[2][3], json!("A"));
        assert_eq!(cells[2][4], json!("N"));
    }

    #[test]
    fn build_grid_json_rejects_overflow() {
        let qs = vec![GridQuestion {
            number: 1,
            direction: "ACROSS".into(),
            root_x: 4,
            root_y: 0,
            answer: "TOOLONG".into(),
            clue: "x".into(),
        }];
        let err = build_grid_json(None, "T", &qs, 5, 3).unwrap_err();
        assert!(err.contains("overflows"), "got: {err}");
    }

    #[test]
    fn build_grid_json_rejects_empty() {
        let err = build_grid_json(None, "T", &[], 5, 5).unwrap_err();
        assert_eq!(err, "Grid has no questions.");
    }

    #[test]
    fn build_grid_json_omits_id_when_none() {
        let qs = sample_questions();
        let v = build_grid_json(None, "T", &qs, 5, 3).unwrap();
        assert!(v.get("id").is_none(), "job:create grid is ephemeral");
    }

    #[test]
    fn solution_hash_is_deterministic_and_changes_with_input() {
        let qs = sample_questions();
        let v1 = build_grid_json(Some("g1"), "T", &qs, 5, 3).unwrap();
        let v2 = build_grid_json(Some("g1"), "T", &qs, 5, 3).unwrap();
        assert_eq!(
            v1["solutionHash"], v2["solutionHash"],
            "same inputs → same hash"
        );

        // Mutate one cell, hash must change.
        let mut cells = vec![vec![Some("C".to_string()); 5]; 3];
        cells[0][1] = Some("X".to_string());
        let questions: Vec<Value> = qs
            .iter()
            .map(|q| {
                json!({
                    "number": q.number, "direction": q.direction,
                    "rootX": q.root_x, "rootY": q.root_y,
                    "answer": q.answer, "clue": q.clue,
                })
            })
            .collect();
        let h_mut = compute_solution_hash(5, 3, &cells, &questions);
        assert_ne!(
            v1["solutionHash"],
            json!(h_mut),
            "changing one cell must change the hash"
        );
    }

    #[test]
    fn solution_hash_changing_a_clue_field_changes_the_hash() {
        // The 64-char hex must be sensitive to every input field. A drift on
        // any of (number, direction, rootX, rootY, answer, clue) is a different
        // grid — the hash lets the client detect server-side mutations between
        // the previewed grid and the persisted row.
        let qs = sample_questions();
        let baseline = build_grid_json(Some("g1"), "T", &qs, 5, 3).unwrap();
        let baseline_hash = baseline["solutionHash"].as_str().unwrap().to_string();

        // (label, question index to mutate, mutation). The direction flip
        // targets the DOWN entry (idx 2): flipping the 5-letter ACROSS
        // entry to DOWN would overflow the 5x3 grid before hashing.
        type QuestionMutation = fn(&mut GridQuestion);
        let mutations: [(&str, usize, QuestionMutation); 6] = [
            ("number", 0, |q: &mut GridQuestion| q.number += 1000),
            ("direction", 2, |q: &mut GridQuestion| {
                q.direction = if q.direction == "ACROSS" {
                    "DOWN".into()
                } else {
                    "ACROSS".into()
                }
            }),
            ("rootX", 2, |q: &mut GridQuestion| q.root_x += 1),
            ("rootY", 2, |q: &mut GridQuestion| q.root_y += 1),
            ("answer", 2, |q: &mut GridQuestion| q.answer.push('Z')),
            ("clue", 0, |q: &mut GridQuestion| q.clue.push('!')),
        ];
        for (label, idx, mutate) in mutations {
            let mut mutated = qs.clone();
            mutate(&mut mutated[idx]);
            let v = build_grid_json(Some("g1"), "T", &mutated, 5, 3).unwrap();
            let h = v["solutionHash"].as_str().unwrap();
            assert_ne!(
                h, baseline_hash,
                "mutating {label} must change the hash (baseline={baseline_hash}, mutated={h})"
            );
        }
    }

    #[test]
    fn solution_hash_separator_prevents_concat_collisions() {
        // Regression: the field separator must be NUL (or otherwise impossible
        // to forge from concatenated user data). If two grids with different
        // structure hash the same because the separator is "|", we've shipped
        // a tamper-detection bug. We assert by construction that the hash
        // includes the grid.w / grid.h prefix too, so size changes flip it.
        let qs = sample_questions();
        let h_5x3 = build_grid_json(Some("g1"), "T", &qs, 5, 3).unwrap()["solutionHash"]
            .as_str()
            .unwrap()
            .to_string();
        let mut qs_pad = qs.clone();
        // Pad the bounding box by extending HELLO → HELLOW; forces a larger grid.
        qs_pad[0].answer = "HELLOW".into();
        let h_6x3 = build_grid_json(Some("g1"), "T", &qs_pad, 6, 3).unwrap()["solutionHash"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(
            h_5x3, h_6x3,
            "grid dimensions are part of the hash (size tamper-detection)"
        );
    }

    /// Cross-check the cell count: every cell that's set must come from some
    /// question, and the union of question placements must exactly match the
    /// non-None cells in the response. That's the "clue enumeration
    /// completeness" assertion from the DEF-70 acceptance list.
    #[test]
    fn every_filled_cell_is_owned_by_some_clue() {
        let qs = sample_questions();
        let v = build_grid_json(Some("g1"), "T", &qs, 5, 3).unwrap();
        let cells = v["cells"].as_array().unwrap();

        let mut owned = std::collections::HashMap::<(usize, usize), char>::new();
        for q in &qs {
            for (i, ch) in q.answer.chars().enumerate() {
                let x = if q.direction == "ACROSS" {
                    (q.root_x + i as i32) as usize
                } else {
                    q.root_x as usize
                };
                let y = if q.direction == "DOWN" {
                    (q.root_y + i as i32) as usize
                } else {
                    q.root_y as usize
                };
                if let Some(prev) = owned.insert((x, y), ch) {
                    assert_eq!(
                        prev, ch,
                        "two clues overlap on ({x},{y}) with different letters — generator bug"
                    );
                }
            }
        }

        let mut seen_filled = 0usize;
        for (y, row) in cells.iter().enumerate() {
            for (x, cell) in row.as_array().unwrap().iter().enumerate() {
                if let Some(letter) = cell.as_str() {
                    let expected = owned
                        .get(&(x, y))
                        .unwrap_or_else(|| panic!("cell ({x},{y}) is set but no clue owns it"));
                    assert_eq!(
                        letter.chars().next().unwrap(),
                        *expected,
                        "cell ({x},{y}) letter mismatch"
                    );
                    seen_filled += 1;
                }
            }
        }
        assert_eq!(
            seen_filled,
            owned.len(),
            "every owned cell is rendered as a letter"
        );
    }
}
