//! `subscription` router — port of server/trpc/router/subscription.ts
use crate::ctx::Ctx;
use serde_json::{json, Value};
use sqlx::Row;

const FREE_LIMIT: i64 = 5;

/// ISO 8601 format for `to_char(col, ...)` — matches JS `Date.toISOString()`.
/// Prisma stores DateTime as TIMESTAMP(3) (UTC-naive, 3 fractional digits).
const TS_FMT: &str = r#"YYYY-MM-DD"T"HH24:MI:SS.MS"Z""#;

pub async fn try_handle(proc: &str, _input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "subscription.getStatus" => Some(get_status(ctx).await),
        "subscription.stop" => Some(stop(ctx).await),
        _ => None,
    }
}

/// subscription.getStatus — protectedProcedure.
/// Returns { isPro, quotaUsed, quotaLimit, currentPeriodEnd } matching
/// client/web/src/store.rs SubStatus.
/// isPro = subscription status ACTIVE or CANCELLED, OR User.vipPass is true.
/// quotaLimit is null (unlimited) for Pro users, FREE_LIMIT for free users.
/// currentPeriodEnd is an ISO 8601 string, or null when there is no
/// subscription row / no period end recorded.
async fn get_status(ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let row = sqlx::query(&format!(
        r#"
        SELECT
            u."vipPass",
            -- Cast to text: Prisma generates a native PG enum for SubscriptionStatus;
            -- reading a native enum OID into String without ::text panics at runtime.
            s.status::text AS subscription_status,
            -- currentPeriodEnd is stored UTC-naive, so to_char + the literal Z
            -- suffix yields a correct ISO-8601 UTC instant.
            to_char(s."currentPeriodEnd", '{TS_FMT}') AS current_period_end,
            -- Whether the paid period is still active. Computed in SQL to avoid needing
            -- the sqlx chrono feature. currentPeriodEnd is stored UTC-naive by the webhook,
            -- so compare against NOW() AT TIME ZONE 'UTC' to keep both sides in UTC.
            (s."currentPeriodEnd" IS NOT NULL
                AND s."currentPeriodEnd" > (NOW() AT TIME ZONE 'UTC')) AS period_active,
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
        "#
    ))
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "user not found".to_string())?;

    let vip_pass: bool = row.get("vipPass");
    let sub_status: Option<String> = row.get("subscription_status");
    // NULL when there is no subscription row (LEFT JOIN); treat as not-active.
    let period_active: bool = row.try_get("period_active").unwrap_or(false);

    // CANCELLED preserves Pro only until the already-paid period ends. Without the
    // period_active gate a CANCELLED subscription would grant Pro indefinitely,
    // relying on a later subscription_expired webhook that may be dropped or delayed.
    let is_pro = sub_status
        .as_deref()
        .map(|s| s == "ACTIVE" || (s == "CANCELLED" && period_active))
        .unwrap_or(false)
        || vip_pass;

    // quota_used is always non-null (CASE ELSE 0), but use try_get to be safe.
    let quota_used: i64 = row.try_get::<i32, _>("quota_used").unwrap_or(0) as i64;
    let quota_limit: Option<i64> = if is_pro { None } else { Some(FREE_LIMIT) };

    Ok(json!({
        "isPro": is_pro,
        "quotaUsed": quota_used,
        "quotaLimit": quota_limit,
        "currentPeriodEnd": row.get::<Option<String>, _>("current_period_end"),
    }))
}

/// subscription.stop — protectedProcedure.
/// Cancels the caller's subscription at Lemon Squeezy
/// (DELETE /v1/subscriptions/{id} — same API key + JSON:API idiom as
/// checkout.rs) and marks the local row CANCELLED so the UI flips without
/// waiting for the subscription_cancelled webhook. Pro access persists until
/// the paid period ends (getStatus treats CANCELLED + live period as Pro).
async fn stop(ctx: &Ctx) -> Result<Value, String> {
    let user = match ctx.require_user() {
        Ok(u) => u,
        Err(e) => return Err(e),
    };

    let row = sqlx::query(
        r#"SELECT "lemonSqueezyId", status::text AS status
           FROM "Subscription" WHERE "userId" = $1"#,
    )
    .bind(&user.id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let Some(row) = row else {
        return Err("No active subscription to cancel.".to_string());
    };
    let status: String = row.get("status");
    // Idempotent: stopping an already-cancelled subscription is a no-op success.
    if status == "CANCELLED" {
        return Ok(json!({ "stopped": true }));
    }
    if status == "EXPIRED" {
        return Err("No active subscription to cancel.".to_string());
    }
    let ls_id: String = row.get("lemonSqueezyId");

    let api_key = std::env::var("LEMONSQUEEZY_API_KEY")
        .map_err(|_| "LEMONSQUEEZY_API_KEY is not set".to_string())?;

    // DELETE marks the subscription cancelled at Lemon Squeezy; billing stops
    // at period end (it is not an immediate hard delete).
    let resp = reqwest::Client::new()
        .delete(format!(
            "https://api.lemonsqueezy.com/v1/subscriptions/{ls_id}"
        ))
        .bearer_auth(&api_key)
        .header("Accept", "application/vnd.api+json")
        .send()
        .await
        .map_err(|e| format!("Lemon Squeezy request failed: {e}"))?;

    let http_status = resp.status();
    let ls: Value = resp.json().await.unwrap_or(Value::Null);
    if !http_status.is_success() {
        let msg = ls["errors"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["detail"].as_str())
            .unwrap_or("failed to cancel subscription");
        return Err(msg.to_string());
    }

    // Reflect the cancellation locally right away. ends_at comes back on the
    // cancel response; keep the stored period end if LS omits it. lastEventAt
    // is left untouched so the follow-up subscription_cancelled webhook (which
    // carries the authoritative timestamps) still applies over this row.
    let ends_at = ls["data"]["attributes"]["ends_at"].as_str();
    sqlx::query(
        r#"UPDATE "Subscription" SET
             status = 'CANCELLED'::"SubscriptionStatus",
             "currentPeriodEnd" = COALESCE(($1::timestamptz AT TIME ZONE 'UTC'), "currentPeriodEnd"),
             "updatedAt" = NOW()
           WHERE "userId" = $2"#,
    )
    .bind(ends_at)
    .bind(&user.id)
    .execute(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!({ "stopped": true }))
}
