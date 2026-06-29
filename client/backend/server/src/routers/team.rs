//! `team` router — port of server/trpc/router/team.ts
use crate::ctx::Ctx;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

const FREE_MAX_SIZE: i32 = 4;
const PRO_MAX_SIZE: i32 = 10;

pub async fn try_handle(proc: &str, input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "team.create" => Some(create(input, ctx).await),
        "team.setVisibility" => Some(set_visibility(input, ctx).await),
        "team.list" => Some(list(ctx).await),
        "team.myTeams" => Some(my_teams(ctx).await),
        "team.join" => Some(join(input, ctx).await),
        "team.leave" => Some(leave(input, ctx).await),
        "team.invite" => Some(invite(input, ctx).await),
        "team.myInvites" => Some(my_invites(ctx).await),
        "team.respondToInvite" => Some(respond_to_invite(input, ctx).await),
        "team.getTeamLeaderboard" => Some(get_team_leaderboard(ctx).await),
        _ => None,
    }
}

/// Pro = active/cancelled subscription OR vipPass (same rule as subscription.ts/generator.ts).
async fn user_is_pro(user_id: &str, pool: &sqlx::PgPool) -> Result<bool, String> {
    let vip: Option<bool> = sqlx::query_scalar(r#"SELECT "vipPass" FROM "User" WHERE id = $1"#)
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?;

    let sub_status: Option<String> =
        sqlx::query_scalar(r#"SELECT status::text FROM "Subscription" WHERE "userId" = $1"#)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

    Ok(matches!(sub_status.as_deref(), Some("ACTIVE") | Some("CANCELLED")) || vip.unwrap_or(false))
}

/// Check whether `user_id` is a member of `team_id`.
async fn is_member(team_id: &str, user_id: &str, pool: &sqlx::PgPool) -> Result<bool, String> {
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "TeamMember" WHERE "teamId" = $1 AND "userId" = $2"#,
    )
    .bind(team_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

/// `team.create` — protectedProcedure, input {name, visibility}
async fn create(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let name = input["name"]
        .as_str()
        .ok_or("missing name")?
        .trim()
        .to_string();
    let visibility = input["visibility"].as_str().unwrap_or("PUBLIC");

    if name.len() < 2 || name.len() > 40 {
        return Err("Team name must be 2–40 characters.".into());
    }
    if visibility != "PUBLIC" && visibility != "PRIVATE" {
        return Err("visibility must be PUBLIC or PRIVATE.".into());
    }

    // Uniqueness check
    let existing: Option<String> = sqlx::query_scalar(r#"SELECT id FROM "Team" WHERE name = $1"#)
        .bind(&name)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
    if existing.is_some() {
        return Err("A team with that name already exists.".into());
    }

    let max_size = if user_is_pro(&user.id, &ctx.pool).await? {
        PRO_MAX_SIZE
    } else {
        FREE_MAX_SIZE
    };

    let team_id = Uuid::new_v4().to_string();
    let member_id = Uuid::new_v4().to_string();

    let mut tx = ctx.pool.begin().await.map_err(|e| e.to_string())?;

    sqlx::query(
        r#"INSERT INTO "Team" (id, name, "ownerId", visibility, "maxSize", "createdAt")
           VALUES ($1, $2, $3, $4::"TeamVisibility", $5, now())"#,
    )
    .bind(&team_id)
    .bind(&name)
    .bind(&user.id)
    .bind(visibility)
    .bind(max_size)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        r#"INSERT INTO "TeamMember" (id, "teamId", "userId", "joinedAt")
           VALUES ($1, $2, $3, now())"#,
    )
    .bind(&member_id)
    .bind(&team_id)
    .bind(&user.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(json!({
        "id": team_id,
        "name": name,
        "ownerId": user.id,
        "visibility": visibility,
        "maxSize": max_size,
        "_count": { "members": 1 },
    }))
}

/// `team.setVisibility` — protectedProcedure, input {teamId, visibility}
async fn set_visibility(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let team_id = input["teamId"].as_str().ok_or("missing teamId")?;
    let visibility = input["visibility"].as_str().ok_or("missing visibility")?;

    if visibility != "PUBLIC" && visibility != "PRIVATE" {
        return Err("visibility must be PUBLIC or PRIVATE.".into());
    }

    let row = sqlx::query(r#"SELECT id, "ownerId" FROM "Team" WHERE id = $1"#)
        .bind(team_id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Team not found.".to_string())?;

    let owner_id: String = row.get("ownerId");
    if owner_id != user.id {
        return Err("Only the team owner can change visibility.".into());
    }

    sqlx::query(r#"UPDATE "Team" SET visibility = $1::"TeamVisibility" WHERE id = $2"#)
        .bind(visibility)
        .bind(team_id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "ok": true }))
}

/// `team.list` — publicProcedure
async fn list(ctx: &Ctx) -> Result<Value, String> {
    let rows = sqlx::query(
        r#"
        SELECT t.id,
               t.name,
               t."maxSize",
               t.visibility::text        AS visibility,
               COALESCE(u.name, u.email, 'Unknown') AS owner_display,
               COUNT(tm.id)              AS member_count
        FROM "Team" t
        JOIN  "User"       u  ON u.id       = t."ownerId"
        LEFT JOIN "TeamMember" tm ON tm."teamId" = t.id
        GROUP BY t.id, t.name, t."maxSize", t.visibility, u.name, u.email
        ORDER BY t."createdAt" DESC
        "#,
    )
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let teams: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":          r.get::<String, _>("id"),
                "name":        r.get::<String, _>("name"),
                "owner":       r.get::<String, _>("owner_display"),
                "memberCount": r.get::<i64, _>("member_count"),
                "maxSize":     r.get::<i32, _>("maxSize"),
                "visibility":  r.get::<String, _>("visibility"),
            })
        })
        .collect();

    Ok(json!(teams))
}

/// `team.myTeams` — protectedProcedure
async fn my_teams(ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let rows = sqlx::query(
        r#"
        SELECT t.id,
               t.name,
               t."maxSize",
               t.visibility::text        AS visibility,
               t."ownerId",
               COUNT(tm2.id)             AS member_count
        FROM "TeamMember" tm
        JOIN  "Team"       t   ON t.id        = tm."teamId"
        LEFT JOIN "TeamMember" tm2 ON tm2."teamId" = t.id
        WHERE tm."userId" = $1
        GROUP BY t.id, t.name, t."maxSize", t.visibility, t."ownerId"
        "#,
    )
    .bind(&user.id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let teams: Vec<Value> = rows
        .iter()
        .map(|r| {
            let owner_id: String = r.get("ownerId");
            json!({
                "id":          r.get::<String, _>("id"),
                "name":        r.get::<String, _>("name"),
                "memberCount": r.get::<i64, _>("member_count"),
                "maxSize":     r.get::<i32, _>("maxSize"),
                "visibility":  r.get::<String, _>("visibility"),
                "isOwner":     owner_id == user.id,
            })
        })
        .collect();

    Ok(json!(teams))
}

/// `team.join` — protectedProcedure, input {teamId}. Public teams only.
async fn join(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let team_id = input["teamId"].as_str().ok_or("missing teamId")?;

    let row = sqlx::query(
        r#"
        SELECT t.id,
               t.visibility::text AS visibility,
               t."maxSize",
               COUNT(tm.id)       AS member_count
        FROM "Team" t
        LEFT JOIN "TeamMember" tm ON tm."teamId" = t.id
        WHERE t.id = $1
        GROUP BY t.id, t.visibility, t."maxSize"
        "#,
    )
    .bind(team_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Team not found.".to_string())?;

    let visibility: String = row.get("visibility");
    let max_size: i32 = row.get("maxSize");
    let member_count: i64 = row.get("member_count");

    if visibility == "PRIVATE" {
        return Err("This team is invite-only.".into());
    }

    if is_member(team_id, &user.id, &ctx.pool).await? {
        return Ok(json!({ "joined": true }));
    }

    if member_count >= max_size as i64 {
        return Err("This team is full.".into());
    }

    let member_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"INSERT INTO "TeamMember" (id, "teamId", "userId", "joinedAt") VALUES ($1, $2, $3, now())"#,
    )
    .bind(&member_id)
    .bind(team_id)
    .bind(&user.id)
    .execute(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({ "joined": true }))
}

/// `team.leave` — protectedProcedure, input {teamId}
async fn leave(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let team_id = input["teamId"].as_str().ok_or("missing teamId")?;

    sqlx::query(r#"DELETE FROM "TeamMember" WHERE "teamId" = $1 AND "userId" = $2"#)
        .bind(team_id)
        .bind(&user.id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "left": true }))
}

/// `team.invite` — protectedProcedure, input {teamId, identifier}.
/// Caller must be a member; invite by username OR email.
async fn invite(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let team_id = input["teamId"].as_str().ok_or("missing teamId")?;
    let identifier = input["identifier"]
        .as_str()
        .ok_or("missing identifier")?
        .trim()
        .to_string();

    if !is_member(team_id, &user.id, &ctx.pool).await? {
        return Err("Only team members can invite.".into());
    }

    let row = sqlx::query(
        r#"
        SELECT t.id, t."maxSize", COUNT(tm.id) AS member_count
        FROM "Team" t
        LEFT JOIN "TeamMember" tm ON tm."teamId" = t.id
        WHERE t.id = $1
        GROUP BY t.id, t."maxSize"
        "#,
    )
    .bind(team_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Team not found.".to_string())?;

    let max_size: i32 = row.get("maxSize");
    let member_count: i64 = row.get("member_count");

    if member_count >= max_size as i64 {
        return Err("This team is full.".into());
    }

    let id_lower = identifier.to_lowercase();
    let invitee_row =
        sqlx::query(r#"SELECT id FROM "User" WHERE username = $1 OR email = $2 LIMIT 1"#)
            .bind(&identifier)
            .bind(&id_lower)
            .fetch_optional(&ctx.pool)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "No user found with that username or email.".to_string())?;

    let invitee_id: String = invitee_row.get("id");

    if is_member(team_id, &invitee_id, &ctx.pool).await? {
        return Err("That user is already on the team.".into());
    }

    let invite_id = Uuid::new_v4().to_string();

    // Upsert: reset to PENDING on conflict (same as TS's upsert { update: { status, invitedById } })
    sqlx::query(
        r#"
        INSERT INTO "TeamInvite" (id, "teamId", "inviteeId", "invitedById", status, "createdAt")
        VALUES ($1, $2, $3, $4, 'PENDING'::"TeamInviteStatus", now())
        ON CONFLICT ("teamId", "inviteeId") DO UPDATE
            SET status       = 'PENDING'::"TeamInviteStatus",
                "invitedById" = EXCLUDED."invitedById"
        "#,
    )
    .bind(&invite_id)
    .bind(team_id)
    .bind(&invitee_id)
    .bind(&user.id)
    .execute(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({ "invited": true }))
}

/// `team.myInvites` — protectedProcedure. PENDING invites for the current user.
async fn my_invites(ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let rows = sqlx::query(
        r#"
        SELECT ti.id,
               ti."teamId",
               t.name                                       AS team_name,
               COALESCE(u.name, u.email, 'Someone')        AS invited_by_display
        FROM "TeamInvite" ti
        JOIN  "Team" t ON t.id   = ti."teamId"
        JOIN  "User" u ON u.id   = ti."invitedById"
        WHERE ti."inviteeId" = $1
          AND ti.status = 'PENDING'::"TeamInviteStatus"
        ORDER BY ti."createdAt" DESC
        "#,
    )
    .bind(&user.id)
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let invites: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id":        r.get::<String, _>("id"),
                "teamId":    r.get::<String, _>("teamId"),
                "teamName":  r.get::<String, _>("team_name"),
                "invitedBy": r.get::<String, _>("invited_by_display"),
            })
        })
        .collect();

    Ok(json!(invites))
}

/// `team.respondToInvite` — protectedProcedure, input {inviteId, accept}
async fn respond_to_invite(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let invite_id = input["inviteId"].as_str().ok_or("missing inviteId")?;
    let accept = input["accept"].as_bool().ok_or("missing accept")?;

    let row = sqlx::query(
        r#"
        SELECT ti.id,
               ti."teamId",
               ti."inviteeId",
               t."maxSize",
               COUNT(tm.id) AS member_count
        FROM "TeamInvite" ti
        JOIN  "Team"       t  ON t.id       = ti."teamId"
        LEFT JOIN "TeamMember" tm ON tm."teamId" = t.id
        WHERE ti.id = $1
        GROUP BY ti.id, ti."teamId", ti."inviteeId", t."maxSize"
        "#,
    )
    .bind(invite_id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let row = match row {
        Some(r) => r,
        None => return Err("Invite not found.".into()),
    };

    let invitee_id: String = row.get("inviteeId");
    if invitee_id != user.id {
        return Err("Invite not found.".into());
    }

    if !accept {
        sqlx::query(
            r#"UPDATE "TeamInvite" SET status = 'DECLINED'::"TeamInviteStatus" WHERE id = $1"#,
        )
        .bind(invite_id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
        return Ok(json!({ "accepted": false }));
    }

    let max_size: i32 = row.get("maxSize");
    let member_count: i64 = row.get("member_count");
    let team_id: String = row.get("teamId");

    if member_count >= max_size as i64 {
        return Err("This team is now full.".into());
    }

    let mut tx = ctx.pool.begin().await.map_err(|e| e.to_string())?;

    // Upsert member (update:{} in TS = DO NOTHING if already a member)
    let member_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO "TeamMember" (id, "teamId", "userId", "joinedAt")
        VALUES ($1, $2, $3, now())
        ON CONFLICT ("teamId", "userId") DO NOTHING
        "#,
    )
    .bind(&member_id)
    .bind(&team_id)
    .bind(&user.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(r#"UPDATE "TeamInvite" SET status = 'ACCEPTED'::"TeamInviteStatus" WHERE id = $1"#)
        .bind(invite_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    Ok(json!({ "accepted": true }))
}

/// `team.getTeamLeaderboard` — publicProcedure.
/// Aggregates members' completed-game scores per team, sorted by total score desc.
async fn get_team_leaderboard(ctx: &Ctx) -> Result<Value, String> {
    let rows = sqlx::query(
        r#"
        SELECT t.id,
               t.name,
               t."maxSize",
               t.visibility::text                           AS visibility,
               COUNT(DISTINCT tm.id)                        AS member_count,
               COUNT(DISTINCT gm.id)                        AS games_played,
               COALESCE(SUM(ms.score), 0)                   AS total_score,
               COALESCE(SUM(ms."correctGuesses"), 0)        AS total_correct,
               COALESCE(SUM(ms."incorrectGuesses"), 0)      AS total_incorrect
        FROM "Team" t
        LEFT JOIN "TeamMember" tm ON tm."teamId"  = t.id
        LEFT JOIN "GameMember" gm ON gm."userId"  = tm."userId"
                                 AND gm."completedGameId" IS NOT NULL
        LEFT JOIN "MemberScore" ms ON ms."memberId" = gm.id
        GROUP BY t.id, t.name, t."maxSize", t.visibility
        ORDER BY total_score DESC, games_played DESC, t.name ASC
        "#,
    )
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let board: Vec<Value> = rows
        .iter()
        .map(|r| {
            let total_correct: i64 = r.get("total_correct");
            let total_incorrect: i64 = r.get("total_incorrect");
            let total_guesses = total_correct + total_incorrect;
            let accuracy = if total_guesses > 0 {
                (total_correct as f64 / total_guesses as f64 * 100.0).round() as i64
            } else {
                0
            };
            json!({
                "id":          r.get::<String, _>("id"),
                "name":        r.get::<String, _>("name"),
                "memberCount": r.get::<i64, _>("member_count"),
                "maxSize":     r.get::<i32, _>("maxSize"),
                "visibility":  r.get::<String, _>("visibility"),
                "totalScore":  r.get::<i64, _>("total_score"),
                "gamesPlayed": r.get::<i64, _>("games_played"),
                "accuracy":    accuracy,
            })
        })
        .collect();

    Ok(json!(board))
}
