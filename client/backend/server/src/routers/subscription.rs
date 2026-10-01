//! `subscription` router — port of server/trpc/router/subscription.ts
use crate::ctx::{sanitised_db_error, Ctx};
use serde_json::{json, Value};
use sqlx::Row;

const FREE_LIMIT: i64 = 5;

/// ISO 8601 format for `to_char(col, ...)` — matches JS `Date.toISOString()`.
/// Prisma stores DateTime as TIMESTAMP(3) (UTC-naive, 3 fractional digits).
const TS_FMT: &str = r#"YYYY-MM-DD"T"HH24:MI:SS.MS"Z""#;

/// Columns to SELECT to feed [`is_pro`], for a query that has `u` aliased to
/// "User" and `s` to "Subscription". Both use the `u`/`s` aliases because
/// `s."currentPeriodEnd"` and the `u`/`s` join are the only subscription inputs
/// the rule has; `period_active` is computed in SQL so neither caller needs the
/// sqlx chrono feature to compare a timestamp.
pub const PRO_PREDICATE_COLUMNS: &str = r#"
        -- Cast to text: Prisma generates a native PG enum for SubscriptionStatus;
        -- reading a native enum OID into String without ::text panics at runtime.
        s.status::text AS subscription_status,
        -- Whether the paid period is still active. currentPeriodEnd is stored
        -- UTC-naive, so compare against NOW() AT TIME ZONE 'UTC' to keep both
        -- sides in UTC.
        (s."currentPeriodEnd" IS NOT NULL
            AND s."currentPeriodEnd" > (NOW() AT TIME ZONE 'UTC')) AS period_active
"#;

/// The one definition of "this account is Pro". Callers LEFT JOIN "Subscription"
/// as `s` and select [`PRO_PREDICATE_COLUMNS`], then pass the three values here
/// rather than restating the rule — the rule has three inputs and one gate, and
/// restating it is how the cancelled-but-expired case silently starts counting
/// as active Pro.
///
/// `subscription_status` is the `Subscription.status` as text, or None when the
/// user has no subscription row. `period_active` is whether
/// `currentPeriodEnd` is still in the future.
///
/// CANCELLED preserves Pro only until the already-paid period ends. Without the
/// period_active gate a CANCELLED subscription would grant Pro indefinitely,
/// relying on a later subscription_expired webhook that may be dropped or
/// delayed. `vipPass` is a manual admin override, not a purchase: it grants Pro
/// here, but a caller reporting on *purchases* must report it separately.
pub fn is_pro(subscription_status: Option<&str>, period_active: bool, vip_pass: bool) -> bool {
    subscription_status
        .map(|s| s == "ACTIVE" || (s == "CANCELLED" && period_active))
        .unwrap_or(false)
        || vip_pass
}

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
/// isPro comes from [`is_pro`], the one definition of the rule, which
/// `user.listForAdmin` also calls (DEF-154).
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
            -- currentPeriodEnd is stored UTC-naive, so to_char + the literal Z
            -- suffix yields a correct ISO-8601 UTC instant.
            to_char(s."currentPeriodEnd", '{TS_FMT}') AS current_period_end,
            {PRO_PREDICATE_COLUMNS}
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
    .map_err(|e| sanitised_db_error("load the account", &e))?
    .ok_or_else(|| "user not found".to_string())?;

    let vip_pass: bool = row.get("vipPass");
    // NULL when there is no subscription row (LEFT JOIN); treat as not-active.
    let sub_status: Option<String> = row.get("subscription_status");
    let period_active: bool = row.try_get("period_active").unwrap_or(false);

    let is_pro = is_pro(sub_status.as_deref(), period_active, vip_pass);

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
    .map_err(|e| sanitised_db_error("load the subscription", &e))?;

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
    .map_err(|e| sanitised_db_error("update the subscription", &e))?;

    Ok(json!({ "stopped": true }))
}

#[cfg(test)]
mod tests {
    use super::is_pro;

    /// The full truth table for the Pro rule, so the gate that `user.listForAdmin`
    /// now depends on is pinned by something other than a prose comment.
    ///
    /// The two rows DEF-154 calls out are `cancelled_period_expired_is_not_pro`
    /// and `cancelled_period_live_is_pro`: a bare `status == 'CANCELLED'` check
    /// reads as equivalent to the real rule and is not, because it grants Pro
    /// forever to someone who churned. Over-reporting churned accounts as active
    /// Pro is the credibility failure the readout exists to avoid, and nothing
    /// downstream of this function would notice.
    #[test]
    fn pro_predicate_truth_table() {
        // (status, period_active, vip_pass, expected, why)
        let cases: &[(Option<&str>, bool, bool, bool, &str)] = &[
            (None, false, false, false, "no subscription row, no vipPass"),
            (
                None,
                false,
                true,
                true,
                "vipPass alone is a manual override",
            ),
            (Some("ACTIVE"), true, false, true, "active subscription"),
            (
                Some("ACTIVE"),
                false,
                false,
                true,
                "ACTIVE ignores the period gate",
            ),
            (
                Some("CANCELLED"),
                true,
                false,
                true,
                "cancelled but the paid period has not ended yet",
            ),
            (
                Some("CANCELLED"),
                false,
                false,
                false,
                "cancelled and the period ended: not Pro, this is the drift case",
            ),
            (
                Some("CANCELLED"),
                true,
                true,
                true,
                "cancelled, period live, and vipPass",
            ),
            (
                Some("CANCELLED"),
                false,
                true,
                true,
                "vipPass overrides an expired cancelled period",
            ),
            (Some("PAST_DUE"), true, false, false, "unpaid is not Pro"),
            (Some("EXPIRED"), true, false, false, "expired is not Pro"),
            (
                Some("INCOMPLETE"),
                true,
                false,
                false,
                "never finished checkout, so nothing to count",
            ),
        ];

        for &(status, period_active, vip_pass, expected, why) in cases {
            assert_eq!(
                is_pro(status, period_active, vip_pass),
                expected,
                "status={status:?} period_active={period_active} vip_pass={vip_pass} \
                 should be Pro={expected} ({why})"
            );
        }
    }

    /// The regression DEF-154's acceptance names explicitly, stated as its own
    /// test so the intent survives the table above being refactored away.
    #[test]
    fn cancelled_period_expired_is_not_pro() {
        assert!(!is_pro(Some("CANCELLED"), false, false));
    }

    #[test]
    fn cancelled_period_live_is_pro() {
        assert!(is_pro(Some("CANCELLED"), true, false));
    }
}
