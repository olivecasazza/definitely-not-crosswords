//! `stats` router — port of server/trpc/router/stats.ts
use crate::ctx::Ctx;
use serde_json::{json, Value};
use sqlx::Row;

pub async fn try_handle(proc: &str, _input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "stats.getGlobalLeaderboard" => Some(global_leaderboard(ctx).await),
        // TODO (Phase C): getUserStats, getAllPlayers, getHeadToHead, getCompletedGame
        _ => None,
    }
}

/// Aggregate each user's completed-game scores. (TS did nested JS loops.)
async fn global_leaderboard(ctx: &Ctx) -> Result<Value, String> {
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
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|r| {
            let total_correct: i64 = r.get("total_correct");
            let total_incorrect: i64 = r.get("total_incorrect");
            let tg = total_correct + total_incorrect;
            let accuracy = if tg > 0 {
                (total_correct as f64 / tg as f64 * 100.0).round() as i64
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
