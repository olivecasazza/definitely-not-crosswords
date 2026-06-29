//! `stats` router — port of server/trpc/router/stats.ts
use crate::ctx::Ctx;
use serde_json::{json, Value};
use sqlx::Row;

pub async fn try_handle(proc: &str, input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "stats.getGlobalLeaderboard" => Some(global_leaderboard(ctx).await),
        "stats.getUserStats" => Some(user_stats(input, ctx).await),
        "stats.getAllPlayers" => Some(all_players(input, ctx).await),
        "stats.getHeadToHead" => Some(head_to_head(input, ctx).await),
        "stats.getCompletedGame" => Some(completed_game(input, ctx).await),
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

/// Deep stats for a single user by email. Ports `getUserStats`.
async fn user_stats(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let email = input["email"].as_str().ok_or("missing email")?;

    // Career aggregates — also validates that the user exists.
    // Career sums go through memberId (gm.memberScore[]), not gameStatsId.
    let career_row = sqlx::query(
        r#"
        SELECT u.id,
               u.name,
               u.email,
               COUNT(DISTINCT gm.id)::bigint                AS games_played,
               COALESCE(SUM(ms.score), 0)::bigint           AS total_score,
               COALESCE(SUM(ms."correctGuesses"), 0)::bigint  AS total_correct,
               COALESCE(SUM(ms."incorrectGuesses"), 0)::bigint AS total_incorrect
        FROM "User" u
        LEFT JOIN "GameMember" gm
               ON gm."userId" = u.id AND gm."completedGameId" IS NOT NULL
        LEFT JOIN "MemberScore" ms ON ms."memberId" = gm.id
        WHERE u.email = $1
        GROUP BY u.id, u.name, u.email
        "#,
    )
    .bind(email)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "User not found".to_string())?;

    let user_id: String = career_row.get("id");
    let user_name: Option<String> = career_row.get("name");
    let user_email: Option<String> = career_row.get("email");
    let games_played: i64 = career_row.get("games_played");
    let total_score: i64 = career_row.get("total_score");
    let total_correct: i64 = career_row.get("total_correct");
    let total_incorrect: i64 = career_row.get("total_incorrect");

    let total_guesses = total_correct + total_incorrect;
    let accuracy = if total_guesses > 0 {
        (total_correct as f64 / total_guesses as f64 * 100.0).round() as i64
    } else {
        0
    };

    // Recent games with per-game rank (via gameStatsId, matching TS gameStats.memberScores).
    // ROW_NUMBER matches JS findIndex+1 behaviour (no tie-collapsing).
    let recent_rows = sqlx::query(
        r#"
        WITH ranked_ms AS (
            SELECT ms."memberId",
                   ms."gameStatsId",
                   ms.score::bigint                    AS score,
                   ms."correctGuesses"::bigint         AS correct_guesses,
                   ms."incorrectGuesses"::bigint       AS incorrect_guesses,
                   ROW_NUMBER() OVER (
                       PARTITION BY ms."gameStatsId"
                       ORDER BY ms.score DESC
                   )::bigint                           AS rank,
                   COUNT(*) OVER (
                       PARTITION BY ms."gameStatsId"
                   )::bigint                           AS total_participants
            FROM "MemberScore" ms
        )
        SELECT cg.id,
               g.title,
               to_char(cg."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
               COALESCE(r.score, 0)              AS score,
               COALESCE(r.correct_guesses, 0)   AS correct_guesses,
               COALESCE(r.incorrect_guesses, 0) AS incorrect_guesses,
               COALESCE(r.rank, 1)              AS rank,
               COALESCE(r.total_participants, 0) AS total_participants
        FROM "GameMember" gm
        JOIN "CompletedGame" cg ON cg.id = gm."completedGameId"
        JOIN "Game" g           ON g.id  = cg."gameId"
        JOIN "GameStats" gs     ON gs.id = cg."gameStatsId"
        LEFT JOIN ranked_ms r   ON r."memberId" = gm.id
                               AND r."gameStatsId" = gs.id
        WHERE gm."userId" = $1
          AND gm."completedGameId" IS NOT NULL
        ORDER BY cg."createdAt" DESC
        "#,
    )
    .bind(&user_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let recent_games: Vec<Value> = recent_rows
        .iter()
        .map(|r| {
            json!({
                "id":                r.get::<String, _>("id"),
                "title":             r.get::<String, _>("title"),
                "createdAt":         r.get::<String, _>("created_at"),
                "score":             r.get::<i64, _>("score"),
                "correctGuesses":    r.get::<i64, _>("correct_guesses"),
                "incorrectGuesses":  r.get::<i64, _>("incorrect_guesses"),
                "rank":              r.get::<i64, _>("rank"),
                "totalParticipants": r.get::<i64, _>("total_participants"),
            })
        })
        .collect();

    // Global rank among all users by career total score.
    let rank_row = sqlx::query(
        r#"
        WITH scores AS (
            SELECT u.id AS user_id,
                   COALESCE(SUM(ms.score), 0)::bigint AS total_score
            FROM "User" u
            LEFT JOIN "GameMember" gm
                   ON gm."userId" = u.id AND gm."completedGameId" IS NOT NULL
            LEFT JOIN "MemberScore" ms ON ms."memberId" = gm.id
            GROUP BY u.id
        ),
        ranked AS (
            SELECT user_id,
                   ROW_NUMBER() OVER (ORDER BY total_score DESC)::bigint AS global_rank,
                   COUNT(*) OVER ()::bigint                              AS total_players
            FROM scores
        )
        SELECT global_rank, total_players FROM ranked WHERE user_id = $1
        "#,
    )
    .bind(&user_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let (global_rank, total_players) = match rank_row {
        Some(rr) => (
            rr.get::<i64, _>("global_rank"),
            rr.get::<i64, _>("total_players"),
        ),
        None => (1i64, 1i64),
    };

    Ok(json!({
        "profile": { "id": user_id, "name": user_name, "email": user_email },
        "gamesPlayed":    games_played,
        "totalScore":     total_score,
        "totalCorrect":   total_correct,
        "totalIncorrect": total_incorrect,
        "accuracy":       accuracy,
        "globalRank":     global_rank,
        "totalPlayers":   total_players,
        "recentGames":    recent_games,
    }))
}

/// All users for the H2H opponent dropdown. Ports `getAllPlayers`.
async fn all_players(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let exclude_email = input["excludeEmail"].as_str();

    let rows = if let Some(excl) = exclude_email {
        sqlx::query(
            r#"SELECT id, name, email FROM "User"
               WHERE (email IS NULL OR email != $1)
               ORDER BY name ASC NULLS LAST"#,
        )
        .bind(excl)
        .fetch_all(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query(r#"SELECT id, name, email FROM "User" ORDER BY name ASC NULLS LAST"#)
            .fetch_all(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?
    };

    let players: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":    r.get::<String, _>("id"),
                "name":  r.get::<Option<String>, _>("name"),
                "email": r.get::<Option<String>, _>("email"),
            })
        })
        .collect();

    Ok(json!(players))
}

/// Head-to-head comparison between the logged-in user and an opponent.
/// Ports `getHeadToHead` (protectedProcedure).
async fn head_to_head(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };
    let opponent_id = input["opponentId"].as_str().ok_or("missing opponentId")?;

    // Fetch opponent display name.
    let opp_row = sqlx::query(r#"SELECT id, name, email FROM "User" WHERE id = $1"#)
        .bind(opponent_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Opponent not found".to_string())?;

    let opp_name: Option<String> = opp_row.get("name");
    let opp_email: Option<String> = opp_row.get("email");
    let opponent_display = opp_name
        .as_deref()
        .or(opp_email.as_deref())
        .unwrap_or("Opponent")
        .to_string();

    // Common completed games — use EXISTS to match TS Prisma `some` semantics,
    // avoiding row multiplication if a user has >1 GameMember entry per game.
    // Per-game scores are fetched via correlated subqueries (gameStatsId path)
    // matching TS `gameStats.memberScores.find(...)` exactly.
    let rows = sqlx::query(
        r#"
        SELECT
            cg.id AS game_id,
            g.title,
            to_char(cg."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
            COALESCE((
                SELECT ms.score::bigint
                FROM "GameMember" gm2
                JOIN "MemberScore" ms
                  ON ms."memberId" = gm2.id AND ms."gameStatsId" = gs.id
                WHERE gm2."completedGameId" = cg.id AND gm2."userId" = $1
                LIMIT 1
            ), 0) AS user_score,
            COALESCE((
                SELECT ms."correctGuesses"::bigint
                FROM "GameMember" gm2
                JOIN "MemberScore" ms
                  ON ms."memberId" = gm2.id AND ms."gameStatsId" = gs.id
                WHERE gm2."completedGameId" = cg.id AND gm2."userId" = $1
                LIMIT 1
            ), 0) AS user_correct,
            COALESCE((
                SELECT ms."incorrectGuesses"::bigint
                FROM "GameMember" gm2
                JOIN "MemberScore" ms
                  ON ms."memberId" = gm2.id AND ms."gameStatsId" = gs.id
                WHERE gm2."completedGameId" = cg.id AND gm2."userId" = $1
                LIMIT 1
            ), 0) AS user_incorrect,
            COALESCE((
                SELECT ms.score::bigint
                FROM "GameMember" gm2
                JOIN "MemberScore" ms
                  ON ms."memberId" = gm2.id AND ms."gameStatsId" = gs.id
                WHERE gm2."completedGameId" = cg.id AND gm2."userId" = $2
                LIMIT 1
            ), 0) AS opponent_score,
            COALESCE((
                SELECT ms."correctGuesses"::bigint
                FROM "GameMember" gm2
                JOIN "MemberScore" ms
                  ON ms."memberId" = gm2.id AND ms."gameStatsId" = gs.id
                WHERE gm2."completedGameId" = cg.id AND gm2."userId" = $2
                LIMIT 1
            ), 0) AS opponent_correct,
            COALESCE((
                SELECT ms."incorrectGuesses"::bigint
                FROM "GameMember" gm2
                JOIN "MemberScore" ms
                  ON ms."memberId" = gm2.id AND ms."gameStatsId" = gs.id
                WHERE gm2."completedGameId" = cg.id AND gm2."userId" = $2
                LIMIT 1
            ), 0) AS opponent_incorrect
        FROM "CompletedGame" cg
        JOIN "Game"      g  ON g.id  = cg."gameId"
        JOIN "GameStats" gs ON gs.id = cg."gameStatsId"
        WHERE EXISTS (
            SELECT 1 FROM "GameMember" gm
            WHERE gm."completedGameId" = cg.id AND gm."userId" = $1
        )
        AND EXISTS (
            SELECT 1 FROM "GameMember" gm
            WHERE gm."completedGameId" = cg.id AND gm."userId" = $2
        )
        ORDER BY cg."createdAt" DESC
        "#,
    )
    .bind(&user.id)
    .bind(opponent_id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let games_played = rows.len() as i64;
    let mut user_wins = 0i64;
    let mut opponent_wins = 0i64;
    let mut ties = 0i64;
    let mut user_total_score = 0i64;
    let mut opponent_total_score = 0i64;
    let mut user_total_correct = 0i64;
    let mut opponent_total_correct = 0i64;
    let mut user_total_incorrect = 0i64;
    let mut opponent_total_incorrect = 0i64;

    let mut matches: Vec<Value> = Vec::with_capacity(rows.len());
    for r in &rows {
        let user_score: i64 = r.get("user_score");
        let opponent_score: i64 = r.get("opponent_score");
        let user_correct: i64 = r.get("user_correct");
        let opponent_correct: i64 = r.get("opponent_correct");
        let user_incorrect: i64 = r.get("user_incorrect");
        let opponent_incorrect: i64 = r.get("opponent_incorrect");

        user_total_score += user_score;
        opponent_total_score += opponent_score;
        user_total_correct += user_correct;
        opponent_total_correct += opponent_correct;
        user_total_incorrect += user_incorrect;
        opponent_total_incorrect += opponent_incorrect;

        let result = if user_score > opponent_score {
            user_wins += 1;
            "WIN"
        } else if opponent_score > user_score {
            opponent_wins += 1;
            "LOSS"
        } else {
            ties += 1;
            "TIE"
        };

        matches.push(json!({
            "gameId":        r.get::<String, _>("game_id"),
            "title":         r.get::<String, _>("title"),
            "createdAt":     r.get::<String, _>("created_at"),
            "userScore":     user_score,
            "opponentScore": opponent_score,
            "result":        result,
        }));
    }

    let user_guesses = user_total_correct + user_total_incorrect;
    let user_accuracy = if user_guesses > 0 {
        (user_total_correct as f64 / user_guesses as f64 * 100.0).round() as i64
    } else {
        0
    };
    let opp_guesses = opponent_total_correct + opponent_total_incorrect;
    let opp_accuracy = if opp_guesses > 0 {
        (opponent_total_correct as f64 / opp_guesses as f64 * 100.0).round() as i64
    } else {
        0
    };
    // Use f64 division to avoid integer truncation (matches JS Math.round).
    let user_avg = if games_played > 0 {
        (user_total_score as f64 / games_played as f64).round() as i64
    } else {
        0
    };
    let opp_avg = if games_played > 0 {
        (opponent_total_score as f64 / games_played as f64).round() as i64
    } else {
        0
    };

    Ok(json!({
        "opponentName": opponent_display,
        "gamesPlayed":  games_played,
        "record": {
            "wins":   user_wins,
            "losses": opponent_wins,
            "ties":   ties,
        },
        "scores": {
            "userTotal":     user_total_score,
            "userAvg":       user_avg,
            "opponentTotal": opponent_total_score,
            "opponentAvg":   opp_avg,
        },
        "accuracy": {
            "user":            user_accuracy,
            "opponent":        opp_accuracy,
            "userCorrect":     user_total_correct,
            "opponentCorrect": opponent_total_correct,
        },
        "matches": matches,
    }))
}

/// Full completed-game detail with ranked member scores.
/// Ports `getCompletedGame` (public). Returns JSON null if not found.
async fn completed_game(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let id = input["id"].as_str().ok_or("missing id")?;

    // Check existence + grab game metadata first.
    let cg_opt = sqlx::query(
        r#"
        SELECT cg.id,
               to_char(cg."createdAt", 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at,
               g.title,
               g.source::text AS source
        FROM "CompletedGame" cg
        JOIN "Game" g ON g.id = cg."gameId"
        WHERE cg.id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let cg_row = match cg_opt {
        None => return Ok(json!(null)),
        Some(r) => r,
    };

    let cg_id: String = cg_row.get("id");
    let created_at: String = cg_row.get("created_at");
    let game_title: String = cg_row.get("title");
    let game_source: String = cg_row.get("source");

    // Per-game scores via gameStatsId path (matches TS gameStats.memberScores include).
    let score_rows = sqlx::query(
        r#"
        SELECT ms.id,
               ms.score::bigint                    AS score,
               ms."correctGuesses"::bigint         AS correct_guesses,
               ms."incorrectGuesses"::bigint       AS incorrect_guesses,
               gm."isOwner"                        AS is_owner,
               u.name                              AS user_name,
               u.email                             AS user_email
        FROM "CompletedGame" cg
        JOIN "GameStats"   gs ON gs.id = cg."gameStatsId"
        JOIN "MemberScore" ms ON ms."gameStatsId" = gs.id
        JOIN "GameMember"  gm ON gm.id = ms."memberId"
        JOIN "User"        u  ON u.id  = gm."userId"
        WHERE cg.id = $1
        ORDER BY ms.score DESC
        "#,
    )
    .bind(id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let member_scores: Vec<Value> = score_rows
        .iter()
        .map(|r| {
            json!({
                "id":              r.get::<String, _>("id"),
                "score":           r.get::<i64, _>("score"),
                "correctGuesses":  r.get::<i64, _>("correct_guesses"),
                "incorrectGuesses":r.get::<i64, _>("incorrect_guesses"),
                "member": {
                    "isOwner": r.get::<bool, _>("is_owner"),
                    "user": {
                        "name":  r.get::<Option<String>, _>("user_name"),
                        "email": r.get::<Option<String>, _>("user_email"),
                    }
                }
            })
        })
        .collect();

    Ok(json!({
        "id":        cg_id,
        "createdAt": created_at,
        "game": {
            "title":     game_title,
            "source":    game_source,
            "questions": null,
        },
        "gameStats": {
            "memberScores": member_scores,
        },
    }))
}
