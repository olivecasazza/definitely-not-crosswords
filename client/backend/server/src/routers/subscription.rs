//! `subscription` router — port of server/trpc/router/subscription.ts
use crate::ctx::Ctx;
use serde_json::{json, Value};
use sqlx::Row;

const FREE_LIMIT: i64 = 5;

pub async fn try_handle(proc: &str, _input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "subscription.getStatus" => Some(get_status(ctx).await),
        _ => None,
    }
}

/// subscription.getStatus — protectedProcedure.
/// Returns { isPro, quotaUsed, quotaLimit } matching client/web/src/store.rs SubStatus.
/// isPro = subscription status ACTIVE or CANCELLED, OR User.vipPass is true.
/// quotaLimit is null (unlimited) for Pro users, FREE_LIMIT for free users.
async fn get_status(ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let row = sqlx::query(
        r#"
        SELECT
            u."vipPass",
            -- Cast to text: Prisma generates a native PG enum for SubscriptionStatus;
            -- reading a native enum OID into String without ::text panics at runtime.
            s.status::text AS subscription_status,
            -- Month comparison done in SQL to avoid needing the sqlx chrono feature.
            -- Mirrors TS: resetDate.getUTCFullYear/Month === now.getUTCFullYear/Month
            CASE
                WHEN gq."monthResetAt" IS NOT NULL
                     AND date_trunc('month', gq."monthResetAt" AT TIME ZONE 'UTC')
                         = date_trunc('month', NOW() AT TIME ZONE 'UTC')
                THEN gq."usedThisMonth"
                ELSE 0
            END AS quota_used
        FROM "User" u
        LEFT JOIN "Subscription" s ON s."userId" = u.id
        LEFT JOIN "GenerationQuota" gq ON gq."userId" = u.id
        WHERE u.id = $1
        "#,
    )
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "user not found".to_string())?;

    let vip_pass: bool = row.get("vipPass");
    let sub_status: Option<String> = row.get("subscription_status");

    let is_pro = sub_status
        .as_deref()
        .map(|s| s == "ACTIVE" || s == "CANCELLED")
        .unwrap_or(false)
        || vip_pass;

    // quota_used is always non-null (CASE ELSE 0), but use try_get to be safe.
    let quota_used: i64 = row.try_get::<i32, _>("quota_used").unwrap_or(0) as i64;
    let quota_limit: Option<i64> = if is_pro { None } else { Some(FREE_LIMIT) };

    Ok(json!({
        "isPro": is_pro,
        "quotaUsed": quota_used,
        "quotaLimit": quota_limit,
    }))
}
