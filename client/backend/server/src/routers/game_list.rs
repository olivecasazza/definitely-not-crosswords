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
async fn get(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let email = input["email"]
        .as_str()
        .ok_or_else(|| "missing email".to_string())?;

    // Active games the user is a member of, joined with their parent Game's title.
    // DISTINCT guards against multiple GameMember rows per (user, game).
    let active_rows = sqlx::query(
        r#"
        SELECT DISTINCT ag.id, ag."gameId" AS game_id, g.title AS game_title
        FROM "ActiveGame" ag
        JOIN "Game" g ON g.id = ag."gameId"
        JOIN "GameMember" gm ON gm."activeGameId" = ag.id
        JOIN "User" u ON u.id = gm."userId"
        WHERE u.email = $1
        "#,
    )
    .bind(email)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    // Completed games the user is a member of.
    let completed_rows = sqlx::query(
        r#"
        SELECT DISTINCT cg.id, cg."gameId" AS game_id, g.title AS game_title
        FROM "CompletedGame" cg
        JOIN "Game" g ON g.id = cg."gameId"
        JOIN "GameMember" gm ON gm."completedGameId" = cg.id
        JOIN "User" u ON u.id = gm."userId"
        WHERE u.email = $1
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
        SELECT id, title
        FROM "Game"
        WHERE published = true
          AND id != ALL($1::text[])
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
        }));
    }
    for r in &completed_rows {
        result.push(json!({
            "type": "CompletedGame",
            "id": r.get::<String, _>("id"),
            "game": { "title": r.get::<String, _>("game_title") },
        }));
    }
    for r in &active_rows {
        result.push(json!({
            "type": "ActiveGame",
            "id": r.get::<String, _>("id"),
            "game": { "title": r.get::<String, _>("game_title") },
        }));
    }

    Ok(json!(result))
}
