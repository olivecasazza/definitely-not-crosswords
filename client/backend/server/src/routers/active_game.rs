//! `active_game` router — port of server/trpc/router/activeGame.ts (HTTP only).
//! Subscriptions (onAddActions / onGameCompleted) are skipped — WebSocket phase.
use crate::ctx::Ctx;
use serde_json::{json, Value};
use sqlx::Row;
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
        _ => None,
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
    .map_err(|e| e.to_string())?;

    let ag_row = match ag_row {
        None => return Ok(Value::Null),
        Some(r) => r,
    };

    let ag_id: String = ag_row.get("ag_id");
    let game_id: String = ag_row.get("gameId");

    // Questions for this game.
    // direction::text produces "ACROSS"/"DOWN" matching Direction's #[serde(rename_all = "UPPERCASE")].
    let q_rows = sqlx::query(
        r#"
        SELECT id, type, number, answer, "questionText", "rootX", "rootY",
               direction::text AS direction, "gameId"
        FROM "Question"
        WHERE "gameId" = $1
        ORDER BY number ASC
        "#,
    )
    .bind(&game_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let questions: Vec<Value> = q_rows
        .iter()
        .map(|r| {
            json!({
                "id":           r.get::<String, _>("id"),
                "type":         r.get::<String, _>("type"),
                "number":       r.get::<i32, _>("number"),
                "answer":       r.get::<String, _>("answer"),
                "questionText": r.get::<String, _>("questionText"),
                "rootX":        r.get::<i32, _>("rootX"),
                "rootY":        r.get::<i32, _>("rootY"),
                "direction":    r.get::<String, _>("direction"),
                "gameId":       r.get::<String, _>("gameId"),
            })
        })
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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

    let completed_game_id: Option<String> = completed_row.map(|r| r.get("id"));

    Ok(json!({
        "id":              game_row.get::<String, _>("id"),
        "title":           game_row.get::<String, _>("title"),
        "source":          game_row.get::<String, _>("source"),
        "questionCount":   question_count,
        "gridSize":        grid_size,
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
    .map_err(|e| e.to_string())?;

    if let Some(row) = existing {
        let id: String = row.get("id");
        return Ok(json!({ "id": id }));
    }

    // Verify the game exists and is published.
    let game_row = sqlx::query(r#"SELECT id, published FROM "Game" WHERE id = $1"#)
        .bind(game_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?;

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
    .map_err(|e| e.to_string())?
    .is_some();
    if !is_member {
        return Err("FORBIDDEN".to_string());
    }

    // A clear is `{ number: null }`; a focus carries number + direction.
    let number = input.get("number").and_then(|v| v.as_i64()).map(|n| n as i32);
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
    let name_row = sqlx::query(r#"SELECT COALESCE(name, 'Anonymous Player') AS name FROM "User" WHERE id = $1"#)
        .bind(&user.id)
        .fetch_one(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
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

/// activeGame.addActions — protected.
/// Persists one or more GameActions; returns the created records.
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
    .map_err(|e| e.to_string())?
    .is_some();
    if !is_member {
        return Err("FORBIDDEN".to_string());
    }

    let mut created: Vec<Value> = Vec::with_capacity(actions_arr.len());

    for action in actions_arr {
        let action_id = Uuid::new_v4().to_string();
        let cord_x = action.get("cordX").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let cord_y = action.get("cordY").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let action_type = action
            .get("actionType")
            .and_then(|v| v.as_str())
            .unwrap_or("placeholder");
        let previous_state = action
            .get("previousState")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let state = action.get("state").and_then(|v| v.as_str()).unwrap_or("");

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
        .bind(cord_x)
        .bind(cord_y)
        .bind(action_type)
        .bind(previous_state)
        .bind(state)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

        created.push(json!({
            "id":            action_id,
            "type":          "GameActions",
            "activeGameId":  id,
            "userId":        user.id,
            "cordX":         cord_x,
            "cordY":         cord_y,
            "actionType":    action_type,
            "previousState": previous_state,
            "state":         state,
            "submittedAt":   chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        }));
    }

    // Broadcast for activeGame.onAddActions (live multiplayer).
    ctx.events
        .publish(crossword_db::AppEvent::GameActionsAdded {
            active_game_id: id.to_string(),
            actions: created.clone(),
        });
    Ok(json!(created))
}

/// activeGame.complete — protected; caller must be a member of the active game.
///
/// Scoring: +10 per correctGuess action, -2 per incorrectGuess action, floor 0.
/// Returns `{ id }` of the created CompletedGame; the frontend navigates there.
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

    // Membership check: only a member of the active game may complete it
    // (this is destructive — it deletes the ActiveGame and cascades GameActions).
    let is_member = sqlx::query(
        r#"SELECT 1 FROM "GameMember" WHERE "activeGameId" = $1 AND "userId" = $2 LIMIT 1"#,
    )
    .bind(active_game_id)
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?
    .is_some();
    if !is_member {
        return Err("FORBIDDEN".to_string());
    }

    // Load the active game.
    let ag = sqlx::query(r#"SELECT id, "gameId" FROM "ActiveGame" WHERE id = $1"#)
        .bind(active_game_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

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
            .map_err(|e| e.to_string())?;

    // Load actions for scoring.
    let action_rows = sqlx::query(
        r#"SELECT "userId", "actionType"::text AS "actionType" FROM "GameAction" WHERE "activeGameId" = $1"#,
    )
    .bind(active_game_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    // Compute per-member stats.
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

            let correct = action_rows
                .iter()
                .filter(|a| {
                    let uid: String = a.get("userId");
                    let at: String = a.get("actionType");
                    uid == user_id && at == "correctGuess"
                })
                .count() as i32;

            let incorrect = action_rows
                .iter()
                .filter(|a| {
                    let uid: String = a.get("userId");
                    let at: String = a.get("actionType");
                    uid == user_id && at == "incorrectGuess"
                })
                .count() as i32;

            let score = (correct * 10 - incorrect * 2).max(0);

            MemberStat {
                member_id,
                score,
                correct,
                incorrect,
            }
        })
        .collect();

    // Run the mutation sequence inside a transaction.
    let mut tx = ctx.pool.begin().await.map_err(|e| e.to_string())?;

    let stats_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO "GameStats" (id, "createdAt", "updatedAt") VALUES ($1, now(), now())"#,
    )
    .bind(&stats_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    let completed_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO "CompletedGame" (id, "gameId", "gameStatsId", "createdAt", "updatedAt")
        VALUES ($1, $2, $3, now(), now())
        "#,
    )
    .bind(&completed_id)
    .bind(&game_id)
    .bind(&stats_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

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
        .map_err(|e| e.to_string())?;

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
        .map_err(|e| e.to_string())?;
    }

    // Deleting ActiveGame cascades to GameActions (onDelete: Cascade in schema).
    sqlx::query(r#"DELETE FROM "ActiveGame" WHERE id = $1"#)
        .bind(active_game_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    // Broadcast for activeGame.onGameCompleted (navigate players to results).
    ctx.events.publish(crossword_db::AppEvent::GameCompleted {
        active_game_id: active_game_id.to_string(),
        completed_game_id: completed_id.clone(),
    });
    Ok(json!({ "id": completed_id }))
}
