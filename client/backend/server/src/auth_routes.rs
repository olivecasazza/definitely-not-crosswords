//! next-auth-compatible login endpoints — issue/clear the JWE session cookie so
//! the app can authenticate entirely against the Rust backend.
//! Ports: /api/auth/csrf, /api/auth/callback/{credentials,local-dev}, /signout.

use crate::AppState;
use axum::{
    extract::{Form, State},
    http::header::SET_COOKIE,
    response::{IntoResponse, Redirect, Response},
    Json,
};
use crossword_auth::encode_session_token;
use rand::Rng;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;

const COOKIE: &str = "next-auth.session-token";

fn session_cookie(token: &str) -> String {
    // dev is http, so no `Secure`. 30-day expiry like next-auth's default.
    format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000")
}
fn clear_cookie() -> String {
    format!("{COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

fn error_response() -> Response {
    // The client checks whether the returned `url` contains `error=`.
    Json(json!({ "url": "/api/auth/error?error=CredentialsSignin" })).into_response()
}

fn issue(
    st: &AppState,
    id: &str,
    email: &str,
    role: &str,
    name: Option<&str>,
    callback: &str,
) -> Response {
    let claims = json!({ "name": name, "email": email, "sub": id, "id": id, "role": role });
    match encode_session_token(&claims, &st.auth.nextauth_secret) {
        Ok(token) => (
            [(SET_COOKIE, session_cookie(&token))],
            Json(json!({ "url": callback })),
        )
            .into_response(),
        Err(_) => error_response(),
    }
}

/// Verify a `scrypt:<saltHex>:<hashHex>` hash (matches lib/auth/password.ts).
fn verify_password(plain: &str, stored: Option<&str>) -> bool {
    let Some(s) = stored else { return false };
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 || parts[0] != "scrypt" {
        return false;
    }
    let (Ok(salt), Ok(expected)) = (hex::decode(parts[1]), hex::decode(parts[2])) else {
        return false;
    };
    let params = match scrypt::Params::new(14, 8, 1, expected.len()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut out = vec![0u8; expected.len()];
    if scrypt::scrypt(plain.as_bytes(), &salt, &params, &mut out).is_err() {
        return false;
    }
    out == expected
}

async fn lookup(
    st: &AppState,
    email: &str,
) -> Option<(String, String, Option<String>, Option<String>)> {
    let row = sqlx::query(
        r#"SELECT id, role::text AS role, password, name FROM "User" WHERE email = $1"#,
    )
    .bind(email)
    .fetch_optional(&st.pool)
    .await
    .ok()
    .flatten()?;
    Some((
        row.get("id"),
        row.get("role"),
        row.get("password"),
        row.get("name"),
    ))
}

/// GET /api/auth/csrf — issue a csrf token (not strictly enforced in dev).
pub async fn csrf() -> impl IntoResponse {
    let mut b = [0u8; 32];
    rand::thread_rng().fill(&mut b);
    let token = hex::encode(b);
    (
        [(
            SET_COOKIE,
            format!("next-auth.csrf-token={token}; Path=/; HttpOnly; SameSite=Lax"),
        )],
        Json(json!({ "csrfToken": token })),
    )
}

/// POST /api/auth/callback/credentials — email + password.
pub async fn credentials(
    State(st): State<AppState>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let email = form
        .get("email")
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();
    let password = form.get("password").cloned().unwrap_or_default();
    let callback = form
        .get("callbackUrl")
        .cloned()
        .unwrap_or_else(|| "/".into());

    let Some((id, role, stored, name)) = lookup(&st, &email).await else {
        return error_response();
    };
    if !verify_password(&password, stored.as_deref()) {
        return error_response();
    }
    issue(&st, &id, &email, &role, name.as_deref(), &callback)
}

/// POST /api/auth/callback/local-dev — email-only admin login (dev/E2E).
pub async fn local_dev(
    State(st): State<AppState>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let email = form
        .get("email")
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("LOCAL_ADMIN_EMAIL")
                .ok()
                .map(|s| s.to_lowercase())
        })
        .unwrap_or_default();
    let callback = form
        .get("callbackUrl")
        .cloned()
        .unwrap_or_else(|| "/".into());

    let Some((id, role, _pw, name)) = lookup(&st, &email).await else {
        return error_response();
    };
    issue(&st, &id, &email, &role, name.as_deref(), &callback)
}

/// GET /api/auth/signout — clear the session cookie and return home.
pub async fn signout() -> Response {
    ([(SET_COOKIE, clear_cookie())], Redirect::to("/")).into_response()
}
