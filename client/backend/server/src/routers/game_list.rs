//! `game_list` router — port of server/trpc/router/gameList.ts
use crate::ctx::Ctx;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

/// Per-game grid metadata derived from Question rows: rendered board bounds
/// plus the number of distinct letter cells (shared crossing cells count once).
struct GridInfo {
    w: i32,
    h: i32,
    total_cells: i64,
}

pub async fn try_handle(proc: &str, input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "gameList.get" => Some(get(input, ctx).await),
        _ => None,
    }
}

/// gameList.get({ email }) — returns published unstarted Games, the caller's
/// ActiveGames, and their CompletedGames, each tagged with a `type` discriminator
/// matching the Prisma model name (Game / ActiveGame / CompletedGame).
///
/// Every row also carries lobby metadata the list UI shows: clue count, player
/// count, a timestamp (created / last-played / completed), and `gridSize`
/// `{ w, h }`. ActiveGame rows add `gameId`, `filledCount`, `correctCount`,
/// and `totalCells`; CompletedGame rows add `gameId`. Everything is batched —
/// five queries total for the whole lobby, never one per row.
async fn get(_input: &Value, ctx: &Ctx) -> Result<Value, String> {
    // Scope to the authenticated caller — ignore any client-supplied email to
    // prevent enumerating another user's game activity (IDOR).
    let email = ctx.require_user()?.email.as_str();

    // Active games the user is a member of, joined with their parent Game's title.
    // DISTINCT guards against multiple GameMember rows per (user, game).
    // `updatedAt` is the last-played stamp; player/clue counts come from
    // correlated aggregates so the row count stays 1-per-game.
    let active_rows = sqlx::query(
        r#"
        SELECT DISTINCT ag.id, ag."gameId" AS game_id, g.title AS game_title,
               to_char(ag."updatedAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS at,
               (SELECT COUNT(*) FROM "Question" q WHERE q."gameId" = g.id) AS clues,
               (SELECT COUNT(*) FROM "GameMember" m WHERE m."activeGameId" = ag.id) AS players
        FROM "ActiveGame" ag
        JOIN "Game" g ON g.id = ag."gameId"
        JOIN "GameMember" gm ON gm."activeGameId" = ag.id
        JOIN "User" u ON u.id = gm."userId"
        WHERE u.email = $1
        ORDER BY at DESC
        "#,
    )
    .bind(email)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    // Completed games the user is a member of, plus the caller's own score.
    let completed_rows = sqlx::query(
        r#"
        SELECT DISTINCT cg.id, cg."gameId" AS game_id, g.title AS game_title,
               to_char(cg."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS at,
               (SELECT COUNT(*) FROM "Question" q WHERE q."gameId" = g.id) AS clues,
               (SELECT COUNT(*) FROM "GameMember" m WHERE m."completedGameId" = cg.id) AS players,
               -- MemberScore has no unique on "memberId", so scope to this
               -- CompletedGame's stats row and LIMIT 1: a bare scalar subquery
               -- would error out ("more than one row") and fail the whole lobby.
               (SELECT ms.score FROM "MemberScore" ms
                 WHERE ms."memberId" = gm.id AND ms."gameStatsId" = cg."gameStatsId"
                 LIMIT 1) AS score
        FROM "CompletedGame" cg
        JOIN "Game" g ON g.id = cg."gameId"
        JOIN "GameMember" gm ON gm."completedGameId" = cg.id
        JOIN "User" u ON u.id = gm."userId"
        WHERE u.email = $1
        ORDER BY at DESC
        "#,
    )
    .bind(email)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    // Game IDs already started or completed — exclude these from the available list.
    // Matches TS: filterIds = [...completedGames.map(c => c.game.id), ...activeGames.map(a => a.game.id)]
    let exclude_ids: Vec<String> = completed_rows
        .iter()
        .map(|r| r.get::<String, _>("game_id"))
        .chain(active_rows.iter().map(|r| r.get::<String, _>("game_id")))
        .collect();

    // Published games not yet touched by this user.
    // $1::text[] — explicit cast so sqlx can resolve the type even when the slice is empty
    // (id != ALL(empty array) is TRUE for every row, which is the correct "exclude nothing" behaviour).
    let game_rows = sqlx::query(
        r#"
        SELECT g.id, g.title, g.source::text AS source,
               to_char(g."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS at,
               (SELECT COUNT(*) FROM "Question" q WHERE q."gameId" = g.id) AS clues
        FROM "Game" g
        WHERE g.published = true
          AND g.id != ALL($1::text[])
        ORDER BY g."createdAt" DESC
        "#,
    )
    .bind(exclude_ids.as_slice())
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    // Grid dimensions for every listed game, in one query. Fetch the raw
    // Question geometry for all relevant gameIds and fold in Rust, mirroring
    // activeGame.getStartDetails / core's board_size exactly: each answer
    // occupies rootX+i (ACROSS) or rootY+i (DOWN); the board is the max
    // occupied coordinate +1 on each axis. Walking the cells also yields
    // totalCells (distinct letter cells — crossings count once), which pure
    // MAX aggregates can't give us.
    let all_game_ids: Vec<String> = {
        let mut seen: HashSet<String> = HashSet::new();
        game_rows
            .iter()
            .map(|r| r.get::<String, _>("id"))
            .chain(completed_rows.iter().map(|r| r.get::<String, _>("game_id")))
            .chain(active_rows.iter().map(|r| r.get::<String, _>("game_id")))
            .filter(|id| seen.insert(id.clone()))
            .collect()
    };

    let question_rows = sqlx::query(
        r#"
        SELECT "gameId" AS game_id, "rootX", "rootY",
               length(answer) AS len, direction::text AS direction
        FROM "Question"
        WHERE "gameId" = ANY($1::text[])
        "#,
    )
    .bind(all_game_ids.as_slice())
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let grid_info: HashMap<String, GridInfo> = {
        // gameId -> set of occupied (x, y) cells.
        let mut cells: HashMap<String, HashSet<(i32, i32)>> = HashMap::new();
        for r in &question_rows {
            let game_id: String = r.get("game_id");
            let root_x: i32 = r.get("rootX");
            let root_y: i32 = r.get("rootY");
            let len: i32 = r.get("len");
            let direction: String = r.get("direction");
            let entry = cells.entry(game_id).or_default();
            for i in 0..len {
                if direction == "ACROSS" {
                    entry.insert((root_x + i, root_y));
                } else {
                    entry.insert((root_x, root_y + i));
                }
            }
        }
        cells
            .into_iter()
            .map(|(game_id, set)| {
                let w = set.iter().map(|c| c.0).max().unwrap_or(-1) + 1;
                let h = set.iter().map(|c| c.1).max().unwrap_or(-1) + 1;
                let total_cells = set.len() as i64;
                (game_id, GridInfo { w, h, total_cells })
            })
            .collect()
    };
    // Games with no questions (shouldn't happen, but don't 500 the lobby).
    let grid_json = |game_id: &str| -> Value {
        match grid_info.get(game_id) {
            Some(g) => json!({ "w": g.w, "h": g.h }),
            None => json!({ "w": 0, "h": 0 }),
        }
    };

    // Fill/correct progress for every listed active game, in one query.
    // Latest action per cell wins (core's current_cell_state: newest
    // submittedAt; id DESC is a deterministic tiebreak). A cell is filled iff
    // that latest state is non-empty (an erase un-fills it), and correct iff
    // it's filled and the latest action is a correctGuess — mirroring
    // core::game::is_cell_correct.
    let active_ids: Vec<String> = active_rows
        .iter()
        .map(|r| r.get::<String, _>("id"))
        .collect();

    let progress_rows = sqlx::query(
        r#"
        SELECT ag_id,
               COUNT(*) FILTER (WHERE state <> '') AS filled,
               COUNT(*) FILTER (WHERE state <> '' AND action_type = 'correctGuess') AS correct
        FROM (
            SELECT DISTINCT ON ("activeGameId", "cordX", "cordY")
                   "activeGameId" AS ag_id,
                   "actionType"::text AS action_type,
                   state
            FROM "GameAction"
            WHERE "activeGameId" = ANY($1::text[])
            ORDER BY "activeGameId", "cordX", "cordY", "submittedAt" DESC, id DESC
        ) latest
        GROUP BY ag_id
        "#,
    )
    .bind(active_ids.as_slice())
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    // activeGameId -> (filledCount, correctCount); games with no actions yet
    // simply have no row here and fall back to 0/0.
    let progress: HashMap<String, (i64, i64)> = progress_rows
        .iter()
        .map(|r| {
            (
                r.get::<String, _>("ag_id"),
                (r.get::<i64, _>("filled"), r.get::<i64, _>("correct")),
            )
        })
        .collect();

    // Combine in TS order: [...games, ...completedGames, ...activeGames]
    let mut result: Vec<Value> =
        Vec::with_capacity(game_rows.len() + completed_rows.len() + active_rows.len());

    for r in &game_rows {
        let id = r.get::<String, _>("id");
        result.push(json!({
            "type": "Game",
            "gridSize": grid_json(&id),
            "id": id,
            "title": r.get::<String, _>("title"),
            "source": r.get::<String, _>("source"),
            "at": r.get::<Option<String>, _>("at"),
            "clues": r.get::<i64, _>("clues"),
        }));
    }
    for r in &completed_rows {
        let game_id = r.get::<String, _>("game_id");
        result.push(json!({
            "type": "CompletedGame",
            "id": r.get::<String, _>("id"),
            "gameId": game_id,
            "gridSize": grid_json(&game_id),
            "game": { "title": r.get::<String, _>("game_title") },
            "at": r.get::<Option<String>, _>("at"),
            "clues": r.get::<i64, _>("clues"),
            "players": r.get::<i64, _>("players"),
            "score": r.get::<Option<i32>, _>("score"),
        }));
    }
    for r in &active_rows {
        let id = r.get::<String, _>("id");
        let game_id = r.get::<String, _>("game_id");
        let (filled, correct) = progress.get(&id).copied().unwrap_or((0, 0));
        let total_cells = grid_info.get(&game_id).map(|g| g.total_cells).unwrap_or(0);
        result.push(json!({
            "type": "ActiveGame",
            "id": id,
            "gameId": game_id,
            "gridSize": grid_json(&game_id),
            "game": { "title": r.get::<String, _>("game_title") },
            "at": r.get::<Option<String>, _>("at"),
            "clues": r.get::<i64, _>("clues"),
            "players": r.get::<i64, _>("players"),
            "filledCount": filled,
            "correctCount": correct,
            "totalCells": total_cells,
        }));
    }

    Ok(json!(result))
}
