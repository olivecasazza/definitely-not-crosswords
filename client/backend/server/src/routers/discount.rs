//! `discount` router — port of server/trpc/router/discount.ts
use crate::ctx::Ctx;
use crossword_db::Capability;
use serde_json::{json, Value};
use sqlx::Row;

pub async fn try_handle(proc: &str, input: &Value, ctx: &Ctx) -> Option<Result<Value, String>> {
    match proc {
        "discount.listForAdmin" => Some(list_for_admin(ctx).await),
        "discount.create" => Some(create(input, ctx).await),
        "discount.setActive" => Some(set_active(input, ctx).await),
        "discount.remove" => Some(remove(input, ctx).await),
        "discount.validate" => Some(validate(input, ctx).await),
        _ => None,
    }
}

fn require_admin(ctx: &Ctx) -> Result<(), String> {
    ctx.auth
        .require_capability(Capability::AdminAccess)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

fn require_user_auth(ctx: &Ctx) -> Result<(), String> {
    ctx.auth
        .require_user()
        .map(|_| ())
        .map_err(|e| format!("{e:?}"))
}

/// Lemon Squeezy env: API key and store id. Returns Err if either is absent.
fn ls_creds() -> Result<(String, String), String> {
    let api_key = std::env::var("LEMONSQUEEZY_API_KEY")
        .map_err(|_| "LEMONSQUEEZY_API_KEY env var is not set".to_string())?;
    let store_id = std::env::var("LEMONSQUEEZY_STORE_ID")
        .map_err(|_| "LEMONSQUEEZY_STORE_ID env var is not set".to_string())?;
    Ok((api_key, store_id))
}

/// Discount row → JSON object, given columns already fetched as text casts.
/// Columns expected: id, code, name, lemonSqueezyId, amountType (text),
/// amount (i32), duration (text), maxRedemptions (Option<i32>),
/// timesRedeemed (i32), expiresAt (Option<String>), isActive (bool), testMode (bool).
fn row_to_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id":              r.get::<String, _>("id"),
        "code":            r.get::<String, _>("code"),
        "name":            r.get::<String, _>("name"),
        "lemonSqueezyId":  r.get::<Option<String>, _>("lemonSqueezyId"),
        "amountType":      r.get::<String, _>("amountType"),
        "amount":          r.get::<i32, _>("amount"),
        "duration":        r.get::<String, _>("duration"),
        "maxRedemptions":  r.get::<Option<i32>, _>("maxRedemptions"),
        "timesRedeemed":   r.get::<i32, _>("timesRedeemed"),
        "expiresAt":       r.get::<Option<String>, _>("expiresAt"),
        "isActive":        r.get::<bool, _>("isActive"),
        "testMode":        r.get::<bool, _>("testMode"),
    })
}

/// Common SELECT projection with enum→text casts and ISO timestamp formatting.
const DISCOUNT_COLS: &str = r#"
    id, code, name, "lemonSqueezyId",
    "amountType"::text AS "amountType",
    amount,
    duration::text AS duration,
    "maxRedemptions", "timesRedeemed",
    to_char("expiresAt", 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS "expiresAt",
    "isActive", "testMode"
"#;

async fn list_for_admin(ctx: &Ctx) -> Result<Value, String> {
    require_admin(ctx)?;

    let rows = sqlx::query(&format!(
        r#"SELECT {DISCOUNT_COLS} FROM "Discount" ORDER BY "createdAt" DESC"#
    ))
    .fetch_all(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(json!(rows.iter().map(row_to_json).collect::<Vec<_>>()))
}

async fn create(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    require_admin(ctx)?;

    // ── Extract input ──────────────────────────────────────────────────────
    let code = input["code"].as_str().unwrap_or("").trim().to_uppercase();
    let name = input["name"].as_str().unwrap_or("").trim().to_string();
    let amount_type = input["amountType"].as_str().unwrap_or("").to_string();
    let amount = input["amount"].as_i64().unwrap_or(0);
    let duration = input["duration"].as_str().unwrap_or("ONCE").to_string();
    let duration_in_months: Option<i64> = input["durationInMonths"].as_i64();
    let max_redemptions: Option<i64> = input["maxRedemptions"].as_i64();
    let expires_at: Option<&str> = input["expiresAt"].as_str();
    let test_mode = input["testMode"].as_bool().unwrap_or(false);

    // ── Validate ───────────────────────────────────────────────────────────
    let code_ok = code.len() >= 3
        && code.len() <= 256
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if !code_ok {
        return Err("Code must be 3-256 uppercase letters and numbers only.".to_string());
    }
    if name.len() < 2 || name.len() > 120 {
        return Err("Name must be between 2 and 120 characters.".to_string());
    }
    if !matches!(amount_type.as_str(), "PERCENT" | "FIXED") {
        return Err("Invalid amountType.".to_string());
    }
    if amount <= 0 {
        return Err("Amount must be a positive integer.".to_string());
    }
    // `amount` is persisted as an i32 column (`.bind(amount as i32)` below) while
    // the full i64 is sent to Lemon Squeezy — reject out-of-range values so the DB
    // cannot silently wrap (and diverge from what LS was told).
    if amount > i32::MAX as i64 {
        return Err("Amount is too large.".to_string());
    }
    if amount_type == "PERCENT" && amount > 100 {
        return Err("Percentage discount must be between 1 and 100.".to_string());
    }
    if !matches!(duration.as_str(), "ONCE" | "FOREVER" | "REPEATING") {
        return Err("Invalid duration.".to_string());
    }
    // Same i32-column guard for `maxRedemptions`.
    if let Some(mr) = max_redemptions {
        if mr < 1 || mr > i32::MAX as i64 {
            return Err("maxRedemptions must be between 1 and 2147483647.".to_string());
        }
    }

    // ── Duplicate code check ───────────────────────────────────────────────
    let existing = sqlx::query(r#"SELECT id FROM "Discount" WHERE code = $1"#)
        .bind(&code)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;
    if existing.is_some() {
        return Err("A discount with this code already exists.".to_string());
    }

    // ── Lemon Squeezy create ───────────────────────────────────────────────
    let (api_key, store_id) = ls_creds()?;

    // Map our DB enums to Lemon Squeezy's lowercase literals.
    let ls_amount_type = if amount_type == "PERCENT" {
        "percent"
    } else {
        "fixed"
    };
    let ls_duration = match duration.as_str() {
        "FOREVER" => "forever",
        "REPEATING" => "repeating",
        _ => "once",
    };

    let mut attributes = json!({
        "name": name,
        "code": code,
        "amount": amount,
        "amount_type": ls_amount_type,
        "duration": ls_duration,
        "test_mode": test_mode,
    });

    if duration == "REPEATING" {
        if let Some(dim) = duration_in_months {
            attributes["duration_in_months"] = json!(dim);
        }
    }
    if let Some(mr) = max_redemptions {
        attributes["is_limited_redemptions"] = json!(true);
        attributes["max_redemptions"] = json!(mr);
    }
    if let Some(ea) = expires_at {
        attributes["expires_at"] = json!(ea);
    }

    let payload = json!({
        "data": {
            "type": "discounts",
            "attributes": attributes,
            "relationships": {
                "store": {
                    "data": { "type": "stores", "id": store_id }
                }
            }
        }
    });

    let ls_resp = reqwest::Client::new()
        .post("https://api.lemonsqueezy.com/v1/discounts")
        .bearer_auth(&api_key)
        .header("Content-Type", "application/vnd.api+json")
        .header("Accept", "application/vnd.api+json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Lemon Squeezy request failed: {e}"))?;

    let ls_status = ls_resp.status();
    let ls_body: Value = ls_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Lemon Squeezy response: {e}"))?;

    if !ls_status.is_success() {
        let msg = ls_body["errors"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|e| e["detail"].as_str())
            .unwrap_or("unknown error");
        return Err(format!("Lemon Squeezy rejected the discount: {msg}"));
    }

    let ls_id = ls_body["data"]["id"]
        .as_str()
        .ok_or_else(|| "Lemon Squeezy returned no discount id".to_string())?
        .to_string();

    // ── Persist to DB ──────────────────────────────────────────────────────
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO "Discount"
            (id, code, name, "lemonSqueezyId",
             "amountType", amount, duration,
             "maxRedemptions", "expiresAt", "testMode",
             "createdAt", "updatedAt")
        VALUES
            ($1, $2, $3, $4,
             $5::"DiscountAmountType", $6, $7::"DiscountDuration",
             $8, ($9::text::timestamptz) AT TIME ZONE 'UTC', $10,
             now(), now())
        "#,
    )
    .bind(&id)
    .bind(&code)
    .bind(&name)
    .bind(&ls_id)
    .bind(amount_type.as_str())
    .bind(amount as i32)
    .bind(duration.as_str())
    .bind(max_redemptions.map(|n| n as i32))
    .bind(expires_at)
    .bind(test_mode)
    .execute(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let discount = json!({
        "id": id,
        "code": code,
        "name": name,
        "lemonSqueezyId": ls_id,
        "amountType": amount_type,
        "amount": amount,
        "duration": duration,
        "maxRedemptions": max_redemptions,
        "timesRedeemed": 0,
        "expiresAt": expires_at,
        "isActive": true,
        "testMode": test_mode,
    });

    Ok(json!({ "success": true, "discount": discount }))
}

async fn set_active(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    require_admin(ctx)?;

    let id = input["id"].as_str().unwrap_or("").trim().to_string();
    if id.is_empty() {
        return Err("id is required".to_string());
    }
    let is_active = match input["isActive"].as_bool() {
        Some(v) => v,
        None => return Err("isActive must be a boolean".to_string()),
    };

    let row = sqlx::query(&format!(
        r#"UPDATE "Discount" SET "isActive" = $1, "updatedAt" = now()
           WHERE id = $2
           RETURNING {DISCOUNT_COLS}"#
    ))
    .bind(is_active)
    .bind(&id)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Discount not found.".to_string())?;

    Ok(json!({ "success": true, "discount": row_to_json(&row) }))
}

async fn remove(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    require_admin(ctx)?;

    let id = input["id"].as_str().unwrap_or("").trim().to_string();
    if id.is_empty() {
        return Err("id is required".to_string());
    }

    // Look up discount (need lemonSqueezyId before deleting).
    let row = sqlx::query(r#"SELECT id, "lemonSqueezyId" FROM "Discount" WHERE id = $1"#)
        .bind(&id)
        .fetch_optional(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Discount not found.".to_string())?;

    let ls_id: Option<String> = row.get("lemonSqueezyId");

    // Delete from Lemon Squeezy if it was registered there.
    if let Some(ls_id) = ls_id {
        let (api_key, _) = ls_creds()?;

        let resp = reqwest::Client::new()
            .delete(format!("https://api.lemonsqueezy.com/v1/discounts/{ls_id}"))
            .bearer_auth(&api_key)
            .header("Accept", "application/vnd.api+json")
            .send()
            .await
            .map_err(|e| format!("Failed to delete discount in Lemon Squeezy: {e}"))?;

        let status = resp.status();
        // 404 from LS (already gone) is fine; surface other failures.
        if !status.is_success() && status.as_u16() != 404 {
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            let msg = body["errors"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|e| e["detail"].as_str())
                .unwrap_or("unknown error");
            return Err(format!("Failed to delete discount in Lemon Squeezy: {msg}"));
        }
    }

    sqlx::query(r#"DELETE FROM "Discount" WHERE id = $1"#)
        .bind(&id)
        .execute(&ctx.pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(json!({ "success": true }))
}

async fn validate(input: &Value, ctx: &Ctx) -> Result<Value, String> {
    // protectedProcedure — any authenticated user.
    require_user_auth(ctx)?;

    let code = input["code"].as_str().unwrap_or("").trim().to_uppercase();

    if code.is_empty() {
        return Err("code is required".to_string());
    }

    // expiresAt is TIMESTAMP(3) (no tz); compare against UTC wall time.
    let row = sqlx::query(
        r#"
        SELECT code, name,
               "amountType"::text AS "amountType",
               amount,
               duration::text AS duration,
               "maxRedemptions", "timesRedeemed", "isActive",
               ("expiresAt" IS NOT NULL
                AND "expiresAt" < (now() AT TIME ZONE 'UTC')) AS "isExpired"
        FROM "Discount"
        WHERE code = $1
        "#,
    )
    .bind(&code)
    .fetch_optional(&ctx.pool)
    .await
    .map_err(|e| e.to_string())?;

    let Some(row) = row else {
        return Ok(json!({ "valid": false, "reason": "This code is not valid." }));
    };

    if !row.get::<bool, _>("isActive") {
        return Ok(json!({ "valid": false, "reason": "This code is not valid." }));
    }
    if row.get::<bool, _>("isExpired") {
        return Ok(json!({ "valid": false, "reason": "This code has expired." }));
    }

    let max_red: Option<i32> = row.get("maxRedemptions");
    let redeemed: i32 = row.get("timesRedeemed");
    if let Some(max) = max_red {
        if redeemed >= max {
            return Ok(json!({
                "valid": false,
                "reason": "This code has reached its redemption limit."
            }));
        }
    }

    Ok(json!({
        "valid": true,
        "discount": {
            "code":       row.get::<String, _>("code"),
            "name":       row.get::<String, _>("name"),
            "amountType": row.get::<String, _>("amountType"),
            "amount":     row.get::<i32, _>("amount"),
            "duration":   row.get::<String, _>("duration"),
        }
    }))
}
