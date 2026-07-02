//! `generator` router — port of server/trpc/router/generator.ts + the
//! generateCrossword service. tRPC queries/mutations (`listJobs`,
//! `publishGeneratedGame`) go through `try_handle`; the streaming
//! `runGeneration` subscription is driven by `run_generation`, invoked from the
//! WebSocket handler (it needs the live socket to push progress events).
//!
//! The other five procs in the TS router (generateDraftGame/createJob/getJob/
//! saveDraftGame/markFailed) aren't called by the Dioxus client, so they're
//! intentionally not ported — they vanish with the Nuxt server. (ponytail)

mod dict;
mod embed;
mod solver;

use crate::ctx::Ctx;
use crossword_auth::AuthContext;
use crossword_db::{AuthUser, Capability};
use serde_json::{json, Value};
use solver::{Direction, Params};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

pub async fn try_handle(proc: &str, input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "generator.listJobs" => Some(list_jobs(input, ctx).await),
        "generator.publishGeneratedGame" => Some(publish_generated_game(input, ctx).await),
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
    ctx.auth
        .require_capability(Capability::AdminAccess)
        .map_err(|e| e.to_string())?;
    let take = input
        .get("take")
        .and_then(|v| v.as_i64())
        .unwrap_or(25)
        .clamp(1, 100);

    let rows = sqlx::query(
        r#"
        SELECT j.id, j.status::text AS status, j.topic, j.width, j.height,
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
                "createdAt": r.get::<Option<String>, _>("created_at"),
                "resultGame": result_game,
            })
        })
        .collect();
    Ok(json!(jobs))
}

async fn publish_generated_game(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    ctx.auth
        .require_capability(Capability::AdminAccess)
        .map_err(|e| e.to_string())?;
    let game_id = input
        .get("gameId")
        .and_then(|v| v.as_str())
        .ok_or("missing gameId")?;

    let source: Option<String> =
        sqlx::query(r#"SELECT source::text AS source FROM "Game" WHERE id = $1"#)
            .bind(game_id)
            .fetch_optional(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| r.get("source"));

    match source.as_deref() {
        None => return Err("Game was not found.".to_string()),
        Some("GENERATED") => {}
        Some(_) => {
            return Err("Only generated games can be published through this route.".to_string())
        }
    }

    sqlx::query(r#"UPDATE "Game" SET published = true, "updatedAt" = now() WHERE id = $1"#)
        .bind(game_id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({ "id": game_id, "published": true }))
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

/// Users with generator:manage and Pro users are unlimited; free users get 5
/// generations per calendar month. Returns is_unlimited; Err on quota exhausted.
async fn check_quota(pool: &PgPool, user: &AuthUser) -> Result<bool, String> {
    if user.role.has(Capability::GeneratorManage) {
        return Ok(true);
    }
    let vip: bool = sqlx::query(r#"SELECT "vipPass" FROM "User" WHERE id = $1"#)
        .bind(&user.id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .map(|r| r.get::<bool, _>("vipPass"))
        .unwrap_or(false);
    let status: Option<String> =
        sqlx::query(r#"SELECT status::text AS status FROM "Subscription" WHERE "userId" = $1"#)
            .bind(&user.id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
            .map(|r| r.get("status"));
    let is_pro = matches!(status.as_deref(), Some("ACTIVE") | Some("CANCELLED")) || vip;

    if !is_pro {
        // lazy-create the quota row, then lazily reset it at month boundaries
        let row = sqlx::query(
            r#"
            INSERT INTO "GenerationQuota" (id, "userId", "usedThisMonth", "monthResetAt", "createdAt", "updatedAt")
            VALUES ($1, $2, 0, now(), now(), now())
            ON CONFLICT ("userId") DO UPDATE SET "updatedAt" = now()
            RETURNING "usedThisMonth",
              (date_trunc('month', "monthResetAt") = date_trunc('month', now())) AS is_current
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
        if used >= 5 {
            return Err(
                "Monthly generation limit reached. Upgrade to Pro for unlimited generations."
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

    let rows = match dict::fetch_rows(&pool, &params).await {
        Ok(r) => r,
        Err(e) => {
            finalize_failed(&pool, &job_id, &e, &log, started_at).await;
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
                &pool, &job_id, &title, &gen, &raw_params, &log, started_at, &user.id,
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
                    finalize_failed(&pool, &job_id, &e, &log, started_at).await;
                    fail(&emit_ws, Some(&job_id), e);
                }
            }
        }
        Err(e) => {
            finalize_failed(&pool, &job_id, &e, &log, started_at).await;
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

async fn create_job(
    pool: &PgPool,
    admin_id: &str,
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
    .bind(admin_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
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
}

/// Authorize a `generator.runGeneration` subscription before it starts. Returns
/// the authenticated user, or a tRPC-style error string. The free/Pro/admin
/// distinction is enforced downstream by `check_quota`, so any signed-in user
/// may reach generation here.
pub fn authorize(auth: &AuthContext) -> Result<AuthUser, String> {
    let user = auth.require_user().map_err(|e| e.to_string())?;
    Ok(user.clone())
}
