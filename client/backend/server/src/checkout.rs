//! POST /api/checkout — create a Lemon Squeezy checkout for Pro. Port of the
//! old Nuxt server/api/checkout.post.ts, plus discount support: the checkout
//! applies (in priority order) a user-entered discount code, else the
//! environment's `PRO_CHECKOUT_DISCOUNT_CODE` — that env is set on staging to a
//! ~90%-off code so Pro is $1 there.
//!
//! Needs LEMONSQUEEZY_API_KEY, LEMONSQUEEZY_STORE_ID, LEMONSQUEEZY_VARIANT_ID.

use crate::AppState;
use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};
use serde_json::{json, Value};

pub async fn checkout(
    State(st): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<Value>>,
) -> Response {
    // Authenticate via the next-auth cookie.
    let auth = st.auth.authenticate(&crate::req_auth(&headers));
    let Some(user) = auth.user else {
        return err(401, "Unauthorized");
    };

    // Discount: user-entered code wins, else the env default (staging).
    let user_code = body
        .as_ref()
        .and_then(|b| b.get("discountCode"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty());
    let discount_code = user_code.or_else(|| {
        std::env::var("PRO_CHECKOUT_DISCOUNT_CODE")
            .ok()
            .filter(|s| !s.is_empty())
    });

    let (api_key, store_id, variant_id) = match ls_config() {
        Ok(c) => c,
        Err(e) => return err(500, &e),
    };

    // The email comes from the DB (the cookie carries id/email/role).
    let mut checkout_data = json!({
        "email": user.email,
        "custom": { "user_id": user.id },
    });
    if let Some(code) = &discount_code {
        checkout_data["discount_code"] = json!(code);
    }

    let payload = json!({
        "data": {
            "type": "checkouts",
            "attributes": { "checkout_data": checkout_data },
            "relationships": {
                "store":   { "data": { "type": "stores",   "id": store_id } },
                "variant": { "data": { "type": "variants", "id": variant_id } }
            }
        }
    });

    let resp = match reqwest::Client::new()
        .post("https://api.lemonsqueezy.com/v1/checkouts")
        .bearer_auth(&api_key)
        .header("Content-Type", "application/vnd.api+json")
        .header("Accept", "application/vnd.api+json")
        .json(&payload)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return err(502, &format!("Lemon Squeezy request failed: {e}")),
    };

    let status = resp.status();
    let ls: Value = resp.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = ls["errors"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|e| e["detail"].as_str())
            .unwrap_or("failed to create checkout");
        return err(500, msg);
    }

    match ls["data"]["attributes"]["url"].as_str() {
        Some(url) => Json(json!({ "checkoutUrl": url })).into_response(),
        None => err(500, "Lemon Squeezy response missing checkout url"),
    }
}

type Response = axum::response::Response;

fn ls_config() -> Result<(String, String, String), String> {
    let get = |k: &str| std::env::var(k).map_err(|_| format!("{k} is not set"));
    Ok((
        get("LEMONSQUEEZY_API_KEY")?,
        get("LEMONSQUEEZY_STORE_ID")?,
        get("LEMONSQUEEZY_VARIANT_ID")?,
    ))
}

fn err(code: u16, msg: &str) -> Response {
    (
        axum::http::StatusCode::from_u16(code).unwrap(),
        Json(json!({ "error": msg })),
    )
        .into_response()
}
