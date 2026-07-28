//! `game_list` router — port of server/trpc/router/gameList.ts
use crate::ctx::Ctx;
use serde_json::{json, Value};
use sqlx::Row;

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
/// count, and a timestamp (created / last-played / completed). All of it is
/// aggregated inside the three existing queries — no per-row follow-up query.
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

    // Combine in TS order: [...games, ...completedGames, ...activeGames]
    let mut result: Vec<Value> =
        Vec::with_capacity(game_rows.len() + completed_rows.len() + active_rows.len());

    for r in &game_rows {
        result.push(json!({
            "type": "Game",
            "id": r.get::<String, _>("id"),
            "title": r.get::<String, _>("title"),
            "source": r.get::<String, _>("source"),
            "at": r.get::<Option<String>, _>("at"),
            "clues": r.get::<i64, _>("clues"),
        }));
    }
    for r in &completed_rows {
        result.push(json!({
            "type": "CompletedGame",
            "id": r.get::<String, _>("id"),
            "game": { "title": r.get::<String, _>("game_title") },
            "at": r.get::<Option<String>, _>("at"),
            "clues": r.get::<i64, _>("clues"),
            "players": r.get::<i64, _>("players"),
            "score": r.get::<Option<i32>, _>("score"),
        }));
    }
    for r in &active_rows {
        result.push(json!({
            "type": "ActiveGame",
            "id": r.get::<String, _>("id"),
            "game": { "title": r.get::<String, _>("game_title") },
            "at": r.get::<Option<String>, _>("at"),
            "clues": r.get::<i64, _>("clues"),
            "players": r.get::<i64, _>("players"),
        }));
    }

    Ok(json!(result))
}
