//! `active_game` router — port of server/trpc/router/activeGame.ts (HTTP only).
//! Subscriptions (onAddActions / onGameCompleted) are skipped — WebSocket phase.
use crate::ctx::{sanitised_db_error, Ctx};
use serde_json::{json, Value};
use sqlx::{Executor, Postgres, Row};
use uuid::Uuid;

pub async fn try_handle(proc: &str, input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "activeGame.get" => Some(get(input, ctx).await),
        "activeGame.getStartDetails" => Some(get_start_details(input, ctx).await),
        "activeGame.start" => Some(start(input, ctx).await),
        "activeGame.join" => Some(join(input, ctx).await),
        "activeGame.addActions" => Some(add_actions(input, ctx).await),
        "activeGame.publishPresence" => Some(publish_presence(input, ctx).await),
        "activeGame.complete" => Some(complete(input, ctx).await),
        "activeGame.abandon" => Some(abandon(input, ctx).await),
        _ => None,
    }
}

/// One `Question` row as it goes on the wire, with the answer text removed.
///
/// Split out of `get` so the AC-4 property is testable without a database: the
/// fields a client needs to lay out the grid (number, clue text, root, direction,
/// and `len` — how many cells the clue covers) are projected, and the letters
/// are not. `len` is exactly what `getStartDetails` already returned, so this
/// matches the precedent rather than inventing a new shape.
pub fn question_view(q: &ClueView) -> Value {
    json!({
        "id":           q.id,
        "type":         q.kind,
        "number":       q.number,
        "len":          q.len,
        "questionText": q.question_text,
        "rootX":        q.root_x,
        "rootY":        q.root_y,
        "direction":    q.direction,
        "gameId":       q.game_id,
    })
}

/// A `Question` row reduced to the fields that go on the wire. A struct rather
/// than a `PgRow` so [`question_view`] is testable without a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClueView {
    pub id: String,
    pub kind: String,
    pub number: i32,
    pub len: i32,
    pub question_text: String,
    pub root_x: i32,
    pub root_y: i32,
    pub direction: String,
    pub game_id: String,
}

impl ClueView {
    fn from_row(r: &sqlx::postgres::PgRow) -> ClueView {
        ClueView {
            id: r.get("id"),
            kind: r.get("type"),
            number: r.get("number"),
            len: r.get("len"),
            question_text: r.get("questionText"),
            root_x: r.get("rootX"),
            root_y: r.get("rootY"),
            direction: r.get("direction"),
            game_id: r.get("gameId"),
        }
    }
}

/// activeGame.get — public.
/// Returns `{ id, gameId, game: { id, title, source, questions: [...] }, actions: [...], gameMembers: [...] }`
/// or JSON null when the active game does not exist.
/// The Dioxus frontend reads `data.game.questions`, `data.actions`, `data.gameMembers`.
async fn get(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let id = input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing id".to_string())?;

    let ag_row = sqlx::query(
        r#"
        SELECT ag.id AS ag_id, ag."gameId",
               g.id AS g_id, g.title, g.source::text AS source,
               g.published,
               to_char(ag."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS ag_created_at,
               to_char(ag."updatedAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS ag_updated_at
        FROM "ActiveGame" ag
        JOIN "Game" g ON g.id = ag."gameId"
        WHERE ag.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the active game", &e))?;

    let ag_row = match ag_row {
        None => return Ok(Value::Null),
        Some(r) => r,
    };

    let ag_id: String = ag_row.get("ag_id");
    let game_id: String = ag_row.get("gameId");

    // Questions for this game.
    // direction::text produces "ACROSS"/"DOWN" matching Direction's #[serde(rename_all = "UPPERCASE")].
    //
    // The answer key is NOT selected. DEF-243: the server scores the grid
    // authoritatively (`scoring`), so shipping the letters would make that
    // scoring advisory — a client could read the key and post every cell right
    // in one call. The grid silhouette (root + direction + length) is all the
    // board needs to lay itself out, exactly as `getStartDetails` already
    // returns it at :271-282.
    let q_rows = sqlx::query(
        r#"
        SELECT id, type, number, length(answer) AS len, "questionText", "rootX", "rootY",
               direction::text AS direction, "gameId"
        FROM "Question"
        WHERE "gameId" = $1
        ORDER BY number ASC
        "#,
    )
    .bind(&game_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the questions", &e))?;

    let questions: Vec<Value> = q_rows
        .iter()
        .map(|r| question_view(&ClueView::from_row(r)))
        .collect();

    // Actions for this active game.
    // actionType::text gives "correctGuess"/"incorrectGuess"/"placeholder" matching
    // ActionType's #[serde(rename_all = "camelCase")].
    // to_char avoids the sqlx chrono feature (not in Cargo.toml).
    let action_rows = sqlx::query(
        r#"
        SELECT id, type, "activeGameId", "userId",
               "actionType"::text    AS "actionType",
               "cordX", "cordY", "previousState", state,
               to_char("submittedAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS "submittedAt"
        FROM "GameAction"
        WHERE "activeGameId" = $1
        ORDER BY "submittedAt" ASC
        "#,
    )
    .bind(&ag_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the action log", &e))?;

    let actions: Vec<Value> = action_rows
        .iter()
        .map(|r| {
            json!({
                "id":            r.get::<String, _>("id"),
                "type":          r.get::<String, _>("type"),
                "activeGameId":  r.get::<String, _>("activeGameId"),
                "userId":        r.get::<String, _>("userId"),
                "actionType":    r.get::<String, _>("actionType"),
                "cordX":         r.get::<i32, _>("cordX"),
                "cordY":         r.get::<i32, _>("cordY"),
                "previousState": r.get::<String, _>("previousState"),
                "state":         r.get::<String, _>("state"),
                "submittedAt":   r.get::<String, _>("submittedAt"),
            })
        })
        .collect();

    // Game members for this active game, with display names for the players
    // strip. Names are already public via the leaderboard; emails stay hidden.
    let member_rows = sqlx::query(
        r#"
        SELECT gm.id, gm.type, gm."userId", gm."isOwner", gm."activeGameId", gm."completedGameId",
               to_char(gm."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS "createdAt",
               to_char(gm."updatedAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS "updatedAt",
               COALESCE(u.name, 'Anonymous Player') AS user_name
        FROM "GameMember" gm
        JOIN "User" u ON u.id = gm."userId"
        WHERE gm."activeGameId" = $1
        ORDER BY gm."createdAt" ASC
        "#,
    )
    .bind(&ag_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the members", &e))?;

    let game_members: Vec<Value> = member_rows
        .iter()
        .map(|r| {
            json!({
                "id":              r.get::<String, _>("id"),
                "type":            r.get::<String, _>("type"),
                "userId":          r.get::<String, _>("userId"),
                "userName":        r.get::<String, _>("user_name"),
                "isOwner":         r.get::<bool, _>("isOwner"),
                "activeGameId":    r.get::<Option<String>, _>("activeGameId"),
                "completedGameId": r.get::<Option<String>, _>("completedGameId"),
                "createdAt":       r.get::<String, _>("createdAt"),
                "updatedAt":       r.get::<String, _>("updatedAt"),
            })
        })
        .collect();

    Ok(json!({
        "id":          ag_id,
        "gameId":      game_id,
        "createdAt":   ag_row.get::<String, _>("ag_created_at"),
        "updatedAt":   ag_row.get::<String, _>("ag_updated_at"),
        "game": {
            "id":        ag_row.get::<String, _>("g_id"),
            "title":     ag_row.get::<String, _>("title"),
            "source":    ag_row.get::<String, _>("source"),
            "published": ag_row.get::<bool, _>("published"),
            "questions": questions,
        },
        "actions":     actions,
        "gameMembers": game_members,
    }))
}

/// activeGame.getStartDetails — protected.
/// Returns metadata + current play-state (active / completed game ids).
async fn get_start_details(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let game_id = input
        .get("gameId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing gameId".to_string())?;

    let game_row = sqlx::query(
        r#"SELECT id, title, source::text AS source, published FROM "Game" WHERE id = $1"#,
    )
    .bind(game_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the game", &e))?;

    let game_row = match game_row {
        None => return Err("Game not found".to_string()),
        Some(r) => r,
    };

    if !game_row.get::<bool, _>("published") {
        return Err("Game not found".to_string());
    }

    let q_rows = sqlx::query(
        r#"SELECT "rootX", "rootY", answer, direction::text AS direction FROM "Question" WHERE "gameId" = $1"#,
    )
    .bind(game_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the questions", &e))?;

    let question_count = q_rows.len() as i64;

    // gridSize = max(rootX + len) for ACROSS, max(rootY + len) for DOWN.
    // Answers are ASCII, so .len() == char count.
    let grid_size: i32 = q_rows.iter().fold(0i32, |acc, r| {
        let root_x: i32 = r.get("rootX");
        let root_y: i32 = r.get("rootY");
        let answer: String = r.get("answer");
        let direction: String = r.get("direction");
        let extent = if direction == "ACROSS" {
            root_x + answer.len() as i32
        } else {
            root_y + answer.len() as i32
        };
        acc.max(extent)
    });

    // Existing active game for this user on this game.
    let active_row = sqlx::query(
        r#"
        SELECT ag.id
        FROM "ActiveGame" ag
        JOIN "GameMember" gm ON gm."activeGameId" = ag.id
        WHERE ag."gameId" = $1 AND gm."userId" = $2
        LIMIT 1
        "#,
    )
    .bind(game_id)
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the active game", &e))?;

    let active_game_id: Option<String> = active_row.map(|r| r.get("id"));

    // Existing completed game for this user on this game.
    let completed_row = sqlx::query(
        r#"
        SELECT cg.id
        FROM "CompletedGame" cg
        JOIN "GameMember" gm ON gm."completedGameId" = cg.id
        WHERE cg."gameId" = $1 AND gm."userId" = $2
        LIMIT 1
        "#,
    )
    .bind(game_id)
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the completed game", &e))?;

    let completed_game_id: Option<String> = completed_row.map(|r| r.get("id"));

    // Grid silhouette for the pre-start brief: coordinates + answer *length*
    // only. Never the answer text or question text — the game hasn't started.
    // Answers are ASCII, so .len() == char count (same as gridSize above).
    let questions: Vec<Value> = q_rows
        .iter()
        .map(|r| {
            let answer: String = r.get("answer");
            json!({
                "rootX":     r.get::<i32, _>("rootX"),
                "rootY":     r.get::<i32, _>("rootY"),
                "direction": r.get::<String, _>("direction"),
                "len":       answer.len() as i32,
            })
        })
        .collect();

    Ok(json!({
        "id":              game_row.get::<String, _>("id"),
        "title":           game_row.get::<String, _>("title"),
        "source":          game_row.get::<String, _>("source"),
        "questionCount":   question_count,
        "gridSize":        grid_size,
        "questions":       questions,
        "activeGameId":    active_game_id,
        "completedGameId": completed_game_id,
    }))
}

/// activeGame.start — protected.
/// Returns `{ id }` of either an existing or newly-created ActiveGame.
async fn start(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let game_id = input
        .get("gameId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing gameId".to_string())?;

    // Return existing active game if the user already has one for this game.
    let existing = sqlx::query(
        r#"
        SELECT ag.id
        FROM "ActiveGame" ag
        JOIN "GameMember" gm ON gm."activeGameId" = ag.id
        WHERE ag."gameId" = $1 AND gm."userId" = $2
        LIMIT 1
        "#,
    )
    .bind(game_id)
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the active game", &e))?;

    if let Some(row) = existing {
        let id: String = row.get("id");
        return Ok(json!({ "id": id }));
    }

    // Verify the game exists and is published.
    let game_row = sqlx::query(r#"SELECT id, published FROM "Game" WHERE id = $1"#)
        .bind(game_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| sanitised_db_error("load the game", &e))?;

    let game_row = match game_row {
        None => return Err("Game not found".to_string()),
        Some(r) => r,
    };
    if !game_row.get::<bool, _>("published") {
        return Err("Game not found".to_string());
    }

    let ag_id = Uuid::new_v4().to_string();
    let member_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"INSERT INTO "ActiveGame" (id, "gameId", "createdAt", "updatedAt") VALUES ($1, $2, now(), now())"#,
    )
    .bind(&ag_id)
    .bind(game_id)
    .execute(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("start the active game", &e))?;

    sqlx::query(
        r#"
        INSERT INTO "GameMember" (id, "userId", "isOwner", "activeGameId", "createdAt", "updatedAt")
        VALUES ($1, $2, true, $3, now(), now())
        "#,
    )
    .bind(&member_id)
    .bind(&user.id)
    .bind(&ag_id)
    .execute(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("start the active game", &e))?;

    Ok(json!({ "id": ag_id }))
}

/// activeGame.join — protected.
/// Adds the caller as a (non-owner) GameMember of an existing ActiveGame.
/// This is the co-op entry point: the owner shares `/game/<activeGameId>` and
/// friends join through it. Idempotent — re-joining returns the same id.
async fn join(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let id = input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing id".to_string())?;

    // The active game must exist and its parent Game must still be published.
    let ag = sqlx::query(
        r#"
        SELECT ag.id
        FROM "ActiveGame" ag
        JOIN "Game" g ON g.id = ag."gameId"
        WHERE ag.id = $1 AND g.published = true
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the active game", &e))?;

    if ag.is_none() {
        return Err("Active game not found".to_string());
    }

    // Already a member (owner or prior join) — treat as success.
    let existing = sqlx::query(
        r#"SELECT 1 FROM "GameMember" WHERE "activeGameId" = $1 AND "userId" = $2 LIMIT 1"#,
    )
    .bind(id)
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("check membership", &e))?;

    if existing.is_some() {
        return Ok(json!({ "id": id, "joined": true }));
    }

    let member_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO "GameMember" (id, "userId", "isOwner", "activeGameId", "createdAt", "updatedAt")
        VALUES ($1, $2, false, $3, now(), now())
        "#,
    )
    .bind(&member_id)
    .bind(&user.id)
    .bind(id)
    .execute(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("join the active game", &e))?;

    Ok(json!({ "id": id, "joined": true }))
}

/// activeGame.publishPresence — protected; caller must be a member.
/// Broadcasts the caller's currently-selected clue (or a clear) to the other
/// members via `activeGame.onPresence`. Ephemeral: nothing is persisted.
async fn publish_presence(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let id = input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing id".to_string())?;

    let is_member = sqlx::query(
        r#"SELECT 1 FROM "GameMember" WHERE "activeGameId" = $1 AND "userId" = $2 LIMIT 1"#,
    )
    .bind(id)
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("check membership", &e))?
    .is_some();
    if !is_member {
        return Err("FORBIDDEN".to_string());
    }

    // A clear is `{ number: null }`; a focus carries number + direction.
    let number = input
        .get("number")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32);
    let direction = input
        .get("direction")
        .and_then(|v| v.as_str())
        .filter(|d| *d == "ACROSS" || *d == "DOWN")
        .map(|d| d.to_string());
    // Direction without a number (or vice versa) is malformed — treat as clear.
    let (number, direction) = match (number, direction) {
        (Some(n), Some(d)) => (Some(n), Some(d)),
        _ => (None, None),
    };

    // Display name for the players strip; same fallback as the leaderboard.
    let name_row = sqlx::query(
        r#"SELECT COALESCE(name, 'Anonymous Player') AS name FROM "User" WHERE id = $1"#,
    )
    .bind(&user.id)
    .fetch_one(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the user", &e))?;
    let name: String = name_row.get("name");

    ctx.events.publish(crossword_db::AppEvent::GamePresence {
        active_game_id: id.to_string(),
        user_id: user.id.clone(),
        name,
        number,
        direction,
    });
    Ok(json!({ "ok": true }))
}

/// What is currently standing in one cell: the server's verdict on the letter,
/// and the letter itself (needed for `previousState`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct CellState {
    verdict: crate::scoring::Verdict,
    letter: String,
}

type CellStates = std::collections::HashMap<(i32, i32), CellState>;

/// The verdicts alone, which is what [`crate::scoring::completion`] counts.
fn latest_verdicts(
    states: &CellStates,
) -> std::collections::HashMap<(i32, i32), crate::scoring::Verdict> {
    states.iter().map(|(c, s)| (*c, s.verdict)).collect()
}

/// Load the answer key for the active game, so every submitted letter can be
/// classified against what actually belongs in that cell.
async fn load_answer_key(
    active_game_id: &str,
    ctx: &Ctx,
) -> Result<crate::scoring::AnswerKey, String> {
    let rows = sqlx::query(
        r#"
        SELECT "rootX", "rootY", answer, direction::text AS direction
        FROM "Question"
        WHERE "gameId" = (SELECT "gameId" FROM "ActiveGame" WHERE id = $1)
        ORDER BY number ASC
        "#,
    )
    .bind(active_game_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the answer key", &e))?;

    let clues: Vec<crate::scoring::Clue> = rows
        .iter()
        .map(|r| crate::scoring::Clue {
            root_x: r.get("rootX"),
            root_y: r.get("rootY"),
            direction: r.get("direction"),
            answer: r.get("answer"),
        })
        .collect();
    Ok(crate::scoring::answer_key(&clues))
}

/// The latest letter standing in each cell of the active game, across all
/// members. One query, `DISTINCT ON` per cell — the same "newest wins" rule the
/// leaderboard read path in `game_list.rs` uses, so the two cannot disagree.
async fn load_cell_states<'e, E>(active_game_id: &str, executor: E) -> Result<CellStates, String>
where
    E: Executor<'e, Database = Postgres>,
{
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT ON ("cordX", "cordY")
               "cordX", "cordY", "actionType"::text AS action_type, state
        FROM "GameAction"
        WHERE "activeGameId" = $1
        ORDER BY "cordX", "cordY", "submittedAt" DESC, id DESC
        "#,
    )
    .bind(active_game_id)
    .fetch_all(executor)
    .await
    .map_err(|e| sanitised_db_error("load the current grid", &e))?;

    let mut states = CellStates::new();
    for r in &rows {
        let letter: String = r.get::<Option<String>, _>("state").unwrap_or_default();
        // An erase is stored as a placeholder with an empty letter and reads
        // back as "nothing standing here" either way.
        let verdict = if letter.is_empty() {
            crate::scoring::Verdict::Placeholder
        } else {
            crate::scoring::Verdict::from_action_type(&r.get::<String, _>("action_type"))
                .unwrap_or(crate::scoring::Verdict::Placeholder)
        };
        states.insert(
            (r.get::<i32, _>("cordX"), r.get::<i32, _>("cordY")),
            CellState { verdict, letter },
        );
    }
    Ok(states)
}

/// The advisory-lock key for one active game. Single source of truth: the two
/// DEF-28x races were each fixed by hand-rolling this string, and a third
/// mutation that forgot it would reintroduce the same class of bug silently.
fn game_write_key(active_game_id: &str) -> String {
    format!("activeGame:write:{active_game_id}")
}

/// Begin a transaction and take the lock that serialises EVERY mutation of one
/// active game. Every handler that writes must go through this.
///
/// Cells live in the append-only `GameAction` log, so there is no per-cell row
/// for `SELECT ... FOR UPDATE` to attach to — an advisory lock keyed on the
/// game is the only honest serialisation point. One key, so a game can never be
/// locked in two orders and deadlock; a hash collision between two games only
/// over-serialises them, it cannot corrupt either.
///
/// This exists because DEF-281 (`add_actions`) and DEF-282 (`complete`) were two
/// separate races with the same shape: read state outside a transaction, then
/// write. Fixing each one in place left the pattern to be remembered, and
/// `join` — which also writes — had no lock at all. One guard removes the
/// remembering.
///
/// The lock is transaction-scoped, so it is released by `commit` or by the
/// rollback that happens when `tx` is dropped. Publish `GameActionsAdded` /
/// `GameCompleted` only AFTER the commit, never inside it.
async fn begin_game_write<'c>(
    ctx: &'c Ctx,
    active_game_id: &str,
) -> Result<sqlx::Transaction<'c, Postgres>, String> {
    let mut tx = ctx
        .pool
        .begin()
        .await
        .map_err(|e| sanitised_db_error("begin the transaction", &e))?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(game_write_key(active_game_id))
        .execute(&mut *tx)
        .await
        .map_err(|e| sanitised_db_error("serialise writes to this game", &e))?;
    Ok(tx)
}

/// activeGame.addActions — protected.
///
/// Persists one or more GameActions; returns the created records plus the
/// grid's authoritative completion state.
///
/// DEF-243: the client sends letters, not verdicts. `actionType` is derived
/// here by comparing each submitted letter against the stored answer key, and
/// `previousState` is read back from what the cell actually held. A
/// client-declared `actionType` is ignored, and a malformed action is refused
/// by name rather than defaulted to a placeholder at cord (0,0).
async fn add_actions(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let id = input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing id".to_string())?;

    let actions_arr = input
        .get("actions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing actions".to_string())?;

    // Membership check: the caller must belong to this active game before
    // injecting actions into it (prevents IDOR into games they aren't part of).
    let is_member = sqlx::query(
        r#"SELECT 1 FROM "GameMember" WHERE "activeGameId" = $1 AND "userId" = $2 LIMIT 1"#,
    )
    .bind(id)
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("check membership", &e))?
    .is_some();
    if !is_member {
        return Err("FORBIDDEN".to_string());
    }

    let key = load_answer_key(id, ctx).await?;

    // Validate the whole batch before writing anything: a batch is one player
    // edit, so a single bad element must not leave a half-applied action log
    // behind. The first refusal names the offending element.
    let mut submissions = Vec::with_capacity(actions_arr.len());
    for (index, action) in actions_arr.iter().enumerate() {
        submissions.push(crate::scoring::validate_submission(action, index, &key)?);
    }

    // DEF-281. Serialise every write to a game, and take the cell snapshot
    // INSIDE that critical section.
    //
    // This used to read `before` from the pool, then INSERT on the pool, with
    // nothing in between. Two players submitting the SAME cell — routine in a
    // crossword, where an ACROSS and a DOWN clue cross — both snapshotted
    // before either wrote, so both logged the same `previousState` and the
    // loser's record was wrong. Measured at a 0.40 conflict rate under four
    // concurrent writers.
    let mut tx = begin_game_write(ctx, id).await?;

    let before = load_cell_states(id, &mut *tx).await?;

    let mut created: Vec<Value> = Vec::with_capacity(submissions.len());

    for sub in &submissions {
        let action_id = Uuid::new_v4().to_string();
        let action_type = sub.verdict.as_action_type();
        let previous_state = before
            .get(&(sub.x, sub.y))
            .map(|s| s.letter.clone())
            .unwrap_or_default();

        sqlx::query(
            r#"
            INSERT INTO "GameAction"
                (id, "activeGameId", "userId", "cordX", "cordY",
                 "actionType", "previousState", state, "submittedAt")
            VALUES ($1, $2, $3, $4, $5, $6::"GameActionTypeEnum", $7, $8, now())
            "#,
        )
        .bind(&action_id)
        .bind(id)
        .bind(&user.id)
        .bind(sub.x)
        .bind(sub.y)
        .bind(action_type)
        .bind(&previous_state)
        .bind(&sub.state)
        .execute(&mut *tx)
        .await
        .map_err(|e| sanitised_db_error("save the submitted letters", &e))?;

        created.push(json!({
            "id":            action_id,
            "type":          "GameActions",
            "activeGameId":  id,
            "userId":        user.id,
            "cordX":         sub.x,
            "cordY":         sub.y,
            "actionType":    action_type,
            "previousState": previous_state,
            "state":         sub.state,
            "submittedAt":   chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        }));
    }

    // Release the advisory lock and make the rows durable BEFORE announcing
    // them. Publishing first would let a subscriber reload the grid and miss
    // these actions, and a rolled-back batch would have already been fanned
    // out to every connected client as phantom moves.
    tx.commit()
        .await
        .map_err(|e| sanitised_db_error("commit the submitted letters", &e))?;

    // Broadcast for activeGame.onAddActions (live multiplayer).
    ctx.events
        .publish(crossword_db::AppEvent::GameActionsAdded {
            active_game_id: id.to_string(),
            actions: created.clone(),
        });

    // The grid's real state after this batch, so the client can finish a solved
    // board without ever holding the key. Previously it computed "solved" itself
    // from the answer the server had been sending it.
    let after = load_cell_states(id, &ctx.pool).await?;
    let completion = crate::scoring::completion(&key, &latest_verdicts(&after));

    Ok(json!({
        "actions": created,
        "solved":  completion.is_solved(),
        "filled":  completion.filled,
        "correct": completion.correct,
        "total":   completion.total,
    }))
}

/// One stored edit, reduced to what scoring needs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LoggedAction {
    user_id: String,
    x: i32,
    y: i32,
    verdict: crate::scoring::Verdict,
}

/// Every action in the active game, oldest first, so the last verdict seen for
/// a cell is the one that stands.
///
/// The verdicts are read back as stored because `add_actions` is now the only
/// thing that writes them and it derives every one from the answer key
/// (DEF-243). A row this module cannot classify is treated as a placeholder
/// rather than being guessed at.
async fn load_action_log(active_game_id: &str, ctx: &Ctx) -> Result<Vec<LoggedAction>, String> {
    let rows = sqlx::query(
        r#"
        SELECT "userId", "cordX", "cordY", "actionType"::text AS action_type, state
        FROM "GameAction"
        WHERE "activeGameId" = $1
        ORDER BY "submittedAt" ASC, id ASC
        "#,
    )
    .bind(active_game_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| sanitised_db_error("load the action log", &e))?;

    Ok(rows
        .iter()
        .map(|r| {
            let letter: String = r.get::<Option<String>, _>("state").unwrap_or_default();
            let verdict = if letter.is_empty() {
                crate::scoring::Verdict::Placeholder
            } else {
                crate::scoring::Verdict::from_action_type(&r.get::<String, _>("action_type"))
                    .unwrap_or(crate::scoring::Verdict::Placeholder)
            };
            LoggedAction {
                user_id: r.get("userId"),
                x: r.get("cordX"),
                y: r.get("cordY"),
                verdict,
            }
        })
        .collect())
}

/// activeGame.complete — protected; caller must be a member of the active game.
///
/// Scoring: +10 per correct cell, -2 per wrong cell, floor 0 — counted **per
/// cell** over that cell's latest submission, never by counting rows. Returns
/// `{ id }` of the created CompletedGame; the frontend navigates there.
///
/// DEF-243: the grid must actually be solved before a CompletedGame is minted.
/// There was no such check at all, so a caller could complete an empty grid and
/// land a score on the public leaderboard. The refusal names how far along the
/// grid is.
///
/// Transaction order:
///   1. Create GameStats
///   2. Create CompletedGame (references GameStats)
///   3. Create MemberScore per member
///   4. Update GameMembers: completedGameId ← new id, activeGameId ← null
///   5. Delete ActiveGame (cascades to GameActions via onDelete: Cascade)
///
/// Step 4 must precede step 5 to avoid the cascade deleting the GameMembers.
async fn complete(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let active_game_id = input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing id".to_string())?;

    // DEF-282. Take the game's write lock BEFORE the existence checks and
    // hold it through the commit below.
    //
    // These two reads used to run on the pool with nothing between them and
    // the transaction that deletes ActiveGame. Four members reaching the
    // results screen together therefore all passed "am I a member?" and all
    // saw the ActiveGame, then each opened its own transaction and each
    // INSERTed a CompletedGame — measured two distinct completedGameIds from
    // one completion, which is two score records and two GameCompleted
    // broadcasts for a single solved board.
    //
    // Under the lock the losers block, then find the ActiveGame already gone
    // and are refused, so exactly one call can win.
    let mut tx = begin_game_write(ctx, active_game_id).await?;

    // Membership check: only a member of the active game may complete it
    // (this is destructive — it deletes the ActiveGame and cascades GameActions).
    let is_member = sqlx::query(
        r#"SELECT 1 FROM "GameMember" WHERE "activeGameId" = $1 AND "userId" = $2 LIMIT 1"#,
    )
    .bind(active_game_id)
    .bind(&user.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| sanitised_db_error("check membership", &e))?
    .is_some();
    if !is_member {
        return Err("FORBIDDEN".to_string());
    }

    // Load the active game — inside the lock, so a second completer sees the
    // row this one is about to delete rather than a stale copy.
    let ag = sqlx::query(r#"SELECT id, "gameId" FROM "ActiveGame" WHERE id = $1"#)
        .bind(active_game_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| sanitised_db_error("load the active game", &e))?;

    let ag = match ag {
        None => return Err("Active game not found".to_string()),
        Some(r) => r,
    };

    let game_id: String = ag.get("gameId");

    // Load members.
    let member_rows =
        sqlx::query(r#"SELECT id, "userId" FROM "GameMember" WHERE "activeGameId" = $1"#)
            .bind(active_game_id)
            .fetch_all(&ctx.pool)
            .await
            .map_err(|e| sanitised_db_error("load the members", &e))?;

    // The answer key, so the grid's real state can be computed without trusting
    // anything the client said.
    let key = load_answer_key(active_game_id, ctx).await?;

    // Every action, oldest first, so the last verdict seen for a cell is the
    // one that stands — the same "newest wins" rule as the leaderboard read.
    let log = load_action_log(active_game_id, ctx).await?;

    // The grid as it stands across all members. This is the gate: a grid that
    // is not solved cannot be completed, whatever the caller asks for.
    let mut latest: std::collections::HashMap<(i32, i32), crate::scoring::Verdict> =
        std::collections::HashMap::new();
    for a in &log {
        latest.insert((a.x, a.y), a.verdict);
    }
    let completion = crate::scoring::completion(&key, &latest);
    if !completion.is_solved() {
        return Err(completion.unsolved_reason());
    }

    // Per-member stats, each cell counted once.
    struct MemberStat {
        member_id: String,
        score: i32,
        correct: i32,
        incorrect: i32,
    }

    let member_stats: Vec<MemberStat> = member_rows
        .iter()
        .map(|m| {
            let member_id: String = m.get("id");
            let user_id: String = m.get("userId");

            let mine: Vec<(i32, i32, crate::scoring::Verdict)> = log
                .iter()
                .filter(|a| a.user_id == user_id)
                .map(|a| (a.x, a.y, a.verdict))
                .collect();
            let progress = crate::scoring::member_progress(&mine);

            MemberStat {
                member_id,
                score: progress.score(),
                correct: progress.correct,
                incorrect: progress.incorrect,
            }
        })
        .collect();

    // The mutation sequence runs in the transaction opened at the top, which
    // still holds the advisory lock. Opening a second one here would drop the
    // lock's protection exactly where it is needed.

    let stats_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO "GameStats" (id, "createdAt", "updatedAt") VALUES ($1, now(), now())"#,
    )
    .bind(&stats_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| sanitised_db_error("record the completed game", &e))?;

    let completed_id = Uuid::new_v4().to_string();
    // "startedAt" ← the ActiveGame's "createdAt" (solve time = completedAt −
    // startedAt). Copied via subquery: the ActiveGame row still exists here
    // (deleted in step 5), and this avoids the sqlx chrono feature.
    sqlx::query(
        r#"
        INSERT INTO "CompletedGame" (id, "gameId", "gameStatsId", "startedAt", "createdAt", "updatedAt")
        VALUES ($1, $2, $3, (SELECT "createdAt" FROM "ActiveGame" WHERE id = $4), now(), now())
        "#,
    )
    .bind(&completed_id)
    .bind(&game_id)
    .bind(&stats_id)
    .bind(active_game_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| sanitised_db_error("record the completed game", &e))?;

    for stat in &member_stats {
        let score_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO "MemberScore"
                (id, "memberId", "gameStatsId", score, "correctGuesses", "incorrectGuesses",
                 "createdAt", "updatedAt")
            VALUES ($1, $2, $3, $4, $5, $6, now(), now())
            "#,
        )
        .bind(&score_id)
        .bind(&stat.member_id)
        .bind(&stats_id)
        .bind(stat.score)
        .bind(stat.correct)
        .bind(stat.incorrect)
        .execute(&mut *tx)
        .await
        .map_err(|e| sanitised_db_error("record the completed game", &e))?;

        // Repoint member from active → completed, then step 5 can safely delete ActiveGame.
        sqlx::query(
            r#"
            UPDATE "GameMember"
            SET "completedGameId" = $1, "activeGameId" = NULL, "updatedAt" = now()
            WHERE id = $2
            "#,
        )
        .bind(&completed_id)
        .bind(&stat.member_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| sanitised_db_error("record the completed game", &e))?;
    }

    // Deleting ActiveGame cascades to GameActions (onDelete: Cascade in schema).
    sqlx::query(r#"DELETE FROM "ActiveGame" WHERE id = $1"#)
        .bind(active_game_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| sanitised_db_error("record the completed game", &e))?;

    tx.commit()
        .await
        .map_err(|e| sanitised_db_error("record the completed game", &e))?;

    // Broadcast for activeGame.onGameCompleted (navigate players to results).
    ctx.events.publish(crossword_db::AppEvent::GameCompleted {
        active_game_id: active_game_id.to_string(),
        completed_game_id: completed_id.clone(),
    });
    Ok(json!({ "id": completed_id }))
}

/// activeGame.abandon — protected.
///
/// Delete an unfinished active game. The counterpart to `complete`: `complete`
/// records a win, `abandon` discards the attempt, and both remove the
/// ActiveGame (cascading GameActions).
///
/// This exists because without it a started game is immortal. `activeGame.start`
/// is idempotent per (gameId, caller), so a user who starts a game and walks
/// away can never start that game again — the row is the only thing marking
/// it as taken, and only completing it removed it. That is not theoretical:
/// the four-player soak exhausted staging's entire pool of published games
/// because every run started one and none could ever be released.
///
/// Gate: any member, matching `complete`. Both are destructive and both are
/// scoped by membership today; adding an owner-only gate here would make
/// abandon stricter than complete, and a game nobody can abandon is the
/// situation this fixes. Worth revisiting if "any member may destroy a shared
/// game" turns out to be wrong.
///
/// Locked with the same per-game advisory lock, so an abandon cannot interleave
/// with a completion or a batch of cell writes and leave a half-deleted game.
async fn abandon(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let active_game_id = input
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing id".to_string())?;

    let mut tx = begin_game_write(ctx, active_game_id).await?;

    let is_member = sqlx::query(
        r#"SELECT 1 FROM "GameMember" WHERE "activeGameId" = $1 AND "userId" = $2 LIMIT 1"#,
    )
    .bind(active_game_id)
    .bind(&user.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| sanitised_db_error("check membership", &e))?
    .is_some();
    if !is_member {
        return Err("FORBIDDEN".to_string());
    }

    let deleted = sqlx::query(r#"DELETE FROM "ActiveGame" WHERE id = $1"#)
        .bind(active_game_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| sanitised_db_error("abandon the active game", &e))?;

    if deleted.rows_affected() == 0 {
        return Err("Active game not found".to_string());
    }

    // Durable before it is announced, same rule as add_actions: publishing
    // first would let a subscriber read a game that a rollback undid.
    tx.commit()
        .await
        .map_err(|e| sanitised_db_error("abandon the active game", &e))?;

    // Subscribers need to know, or a connected client sits on a board that no
    // longer exists and keeps typing into it.
    ctx.events.publish(crossword_db::AppEvent::GameAbandoned {
        active_game_id: active_game_id.to_string(),
    });
    Ok(json!({ "abandoned": true }))
}
