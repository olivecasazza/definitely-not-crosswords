//! `game_list` router — port of server/trpc/router/gameList.ts, plus the
//! daily-puzzle proc `game.getDaily` (B14).
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
        "game.getDaily" => Some(get_daily(ctx).await),
        _ => None,
    }
}

/// game.getDaily — today's featured puzzle (B14). Public procedure: works
/// without a session, but when one exists the response also carries the
/// caller's state on that game.
///
/// Pick rule (documented here, implemented in the candidate query below):
/// the day's game is the LEAST-recently-picked published Game — games that
/// have never been a DailyPick come first (NULLS FIRST on their last picked
/// date), then games whose most recent pick is oldest; ties break toward the
/// newest `createdAt`. The winning pick is persisted lazily on the first
/// request of the UTC day with `INSERT ... ON CONFLICT DO NOTHING` + re-select,
/// so concurrent first requests all converge on the same row.
///
/// Response: `{ gameId, title, clues, alreadyCompleted, activeGameId }` —
/// `alreadyCompleted` / `activeGameId` are only meaningful with a session
/// (false / null otherwise).
async fn get_daily(ctx: &Ctx) -> Result<Value, String> {
    // The UTC calendar day, computed by Postgres so app hosts' clocks/zones
    // can't disagree. YYYY-MM-DD text sorts lexicographically == chronologically.
    let today: String =
        sqlx::query_scalar(r#"SELECT to_char(now() AT TIME ZONE 'utc', 'YYYY-MM-DD')"#)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;

    // Today's pick, with title + clue count in one row.
    let select_pick = r#"
        SELECT dp."gameId" AS game_id, g.title,
               (SELECT COUNT(*) FROM "Question" q WHERE q."gameId" = g.id) AS clues
        FROM "DailyPick" dp
        JOIN "Game" g ON g.id = dp."gameId"
        WHERE dp."date" = $1
        "#;

    let mut row = sqlx::query(select_pick)
        .bind(&today)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    if row.is_none() {
        // First request of the day: choose deterministically (pick rule above).
        let candidate: Option<String> = sqlx::query_scalar(
            r#"
            SELECT g.id
            FROM "Game" g
            LEFT JOIN (
                SELECT "gameId", MAX("date") AS last_picked
                FROM "DailyPick"
                GROUP BY "gameId"
            ) dp ON dp."gameId" = g.id
            WHERE g.published = true
            ORDER BY dp.last_picked ASC NULLS FIRST, g."createdAt" DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

        let Some(candidate) = candidate else {
            return Err("no published games available for the daily puzzle".to_string());
        };

        // Racing first-requests: whoever inserts first wins, everyone re-selects
        // the same persisted row.
        sqlx::query(r#"INSERT INTO "DailyPick" ("date", "gameId") VALUES ($1, $2) ON CONFLICT ("date") DO NOTHING"#)
            .bind(&today)
            .bind(&candidate)
            .execute(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;

        row = sqlx::query(select_pick)
            .bind(&today)
            .fetch_optional(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;
    }

    let row = row.ok_or_else(|| "daily pick vanished after insert".to_string())?;
    let game_id: String = row.get("game_id");
    let title: String = row.get("title");
    let clues: i64 = row.get("clues");

    // Per-user state — only when a session exists (public proc otherwise).
    let (already_completed, active_game_id) = match ctx.auth.user.as_ref() {
        Some(user) => {
            let completed: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM "CompletedGame" cg
                    JOIN "GameMember" gm ON gm."completedGameId" = cg.id
                    WHERE cg."gameId" = $1 AND gm."userId" = $2
                )
                "#,
            )
            .bind(&game_id)
            .bind(&user.id)
            .fetch_one(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;

            // Most recently played active session on this game, if any.
            let active: Option<String> = sqlx::query_scalar(
                r#"
                SELECT ag.id FROM "ActiveGame" ag
                JOIN "GameMember" gm ON gm."activeGameId" = ag.id
                WHERE ag."gameId" = $1 AND gm."userId" = $2
                ORDER BY ag."updatedAt" DESC
                LIMIT 1
                "#,
            )
            .bind(&game_id)
            .bind(&user.id)
            .fetch_optional(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?;

            (completed, active)
        }
        None => (false, None),
    };

    Ok(json!({
        "gameId": game_id,
        "title": title,
        "clues": clues,
        "alreadyCompleted": already_completed,
        "activeGameId": active_game_id,
    }))
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
