//! POST /api/webhooks/lemonsqueezy — port of server/api/webhooks/lemonsqueezy.post.ts.
//!
//! Lemon Squeezy calls this on subscription lifecycle events. We verify the
//! HMAC-SHA256 signature (X-Signature header, keyed by LEMONSQUEEZY_WEBHOOK_SECRET)
//! over the RAW body, then upsert the user's Subscription row so `isPro` flips
//! (subscription.getStatus treats ACTIVE/CANCELLED as Pro). `custom_data.user_id`
//! is the id we attached at checkout.

use crate::AppState;
use axum::{body::Bytes, extract::State, http::HeaderMap, response::IntoResponse};
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;

type Response = axum::response::Response;

pub async fn lemonsqueezy(State(st): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let Ok(secret) = std::env::var("LEMONSQUEEZY_WEBHOOK_SECRET").map(|s| s.trim().to_string()) else {
        // Not configured — refuse rather than accept unverified events.
        return err(500, "LEMONSQUEEZY_WEBHOOK_SECRET is not set");
    };
    if secret.is_empty() {
        return err(500, "LEMONSQUEEZY_WEBHOOK_SECRET is empty");
    }

    // Verify the signature over the raw body (constant-time).
    let Some(sig_hex) = headers.get("x-signature").and_then(|v| v.to_str().ok()) else {
        return err(401, "Missing signature");
    };
    let Ok(sig) = hex::decode(sig_hex) else {
        return err(401, "Malformed signature");
    };
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key of any size");
    mac.update(&body);
    if mac.verify_slice(&sig).is_err() {
        return err(401, "Invalid signature");
    }

    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return err(400, "Invalid JSON body");
    };

    let event_name = payload["meta"]["event_name"].as_str().unwrap_or("");
    let user_id = payload["meta"]["custom_data"]["user_id"].as_str();
    let ls_id = payload["data"]["id"].as_str();
    let attrs = &payload["data"]["attributes"];

    // custom_data.user_id + the LS subscription id are required to attribute the sub.
    let (Some(user_id), Some(ls_id)) = (user_id, ls_id) else {
        // 200 so LS doesn't retry a payload we can't act on (e.g. non-subscription events).
        return ok();
    };

    // Unrecognized event / status → leave the subscription untouched (never grant
    // Pro by default). Ack with 200 so LS doesn't retry an event we intentionally skip.
    let Some(status) = resolve_status(event_name, attrs) else {
        tracing::info!("lemonsqueezy webhook: ignoring unmapped event {event_name} for user {user_id}");
        return ok();
    };
    // customer_id arrives as a JSON number; store as text (matches Prisma schema).
    let customer_id = attrs["customer_id"]
        .as_i64()
        .map(|n| n.to_string())
        .or_else(|| attrs["customer_id"].as_str().map(str::to_string));
    // Prefer ends_at (set on cancel), else renews_at; both ISO-8601 or null.
    let period_end = attrs["ends_at"]
        .as_str()
        .or_else(|| attrs["renews_at"].as_str())
        .map(str::to_string);
    // Event timestamp used as a monotonic ordering key: a replayed or out-of-order
    // event whose updated_at is older than what we last applied is dropped below.
    let event_ts = attrs["updated_at"].as_str().map(str::to_string);

    // Upsert by userId (schema: one Subscription per user; userId is UNIQUE).
    // The ON CONFLICT ... WHERE guard makes stale events a no-op: an update only
    // lands when this event is newer-or-equal than the last one we recorded (or
    // when either side lacks a timestamp, in which case we fall back to applying).
    let res = sqlx::query(
        r#"
        INSERT INTO "Subscription"
            (id, "userId", "lemonSqueezyId", "lemonSqueezyCustomerId",
             status, "currentPeriodEnd", "lastEventAt", "updatedAt")
        VALUES
            ($1, $2, $3, $4, $5::"SubscriptionStatus",
             ($6::timestamptz AT TIME ZONE 'UTC'),
             ($7::timestamptz AT TIME ZONE 'UTC'), NOW())
        ON CONFLICT ("userId") DO UPDATE SET
            "lemonSqueezyId"         = EXCLUDED."lemonSqueezyId",
            "lemonSqueezyCustomerId" = EXCLUDED."lemonSqueezyCustomerId",
            status                   = EXCLUDED.status,
            "currentPeriodEnd"       = EXCLUDED."currentPeriodEnd",
            "lastEventAt"            = EXCLUDED."lastEventAt",
            "updatedAt"              = NOW()
        WHERE "Subscription"."lastEventAt" IS NULL
           OR EXCLUDED."lastEventAt" IS NULL
           OR EXCLUDED."lastEventAt" >= "Subscription"."lastEventAt"
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(ls_id)
    .bind(customer_id)
    .bind(status)
    .bind(period_end)
    .bind(event_ts)
    .execute(&st.pool)
    .await;

    match res {
        Ok(_) => {
            tracing::info!("lemonsqueezy webhook: {event_name} -> {status} for user {user_id}");
            ok()
        }
        Err(e) => err(500, &format!("failed to upsert subscription: {e}")),
    }
}

/// Map a webhook event (+ the attributes' own `status` for updates) to our enum.
///
/// Returns `None` for any event/status we don't explicitly recognize so the caller
/// leaves the row untouched — we must never grant Pro (`ACTIVE`) by default. Pause
/// and refund events are handled explicitly and *revoke* Pro rather than retaining it.
fn resolve_status(event_name: &str, attrs: &Value) -> Option<&'static str> {
    match event_name {
        "subscription_created"
        | "subscription_payment_success"
        | "subscription_payment_recovered" => Some("ACTIVE"),
        "subscription_payment_failed" => Some("PAST_DUE"),
        "subscription_expired" => Some("EXPIRED"),
        "subscription_cancelled" => Some("CANCELLED"),
        // Pause and refund must remove Pro access, not keep it.
        "subscription_paused" | "subscription_payment_refunded" => Some("EXPIRED"),
        "subscription_updated" => match attrs["status"].as_str() {
            Some("active") | Some("on_trial") => Some("ACTIVE"),
            Some("past_due") => Some("PAST_DUE"),
            Some("cancelled") => Some("CANCELLED"),
            Some("expired") => Some("EXPIRED"),
            // paused / unpaid and any unknown status are non-Pro; drop, don't grant.
            Some("paused") | Some("unpaid") => Some("EXPIRED"),
            _ => None,
        },
        // Unknown top-level event: leave the subscription untouched.
        _ => None,
    }
}

fn ok() -> Response {
    axum::Json(json!({ "ok": true })).into_response()
}

fn err(code: u16, msg: &str) -> Response {
    (
        axum::http::StatusCode::from_u16(code).unwrap(),
        axum::Json(json!({ "error": msg })),
    )
        .into_response()
}
