//! Shared application state for the Axum router, split out of `main.rs` so
//! the library target (and integration tests) can reuse it.

use crossword_auth::AuthService;
use crossword_events::EventBus;
use sqlx::PgPool;

use crate::mailer::Mailer;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthService,
    pub events: EventBus,
    pub mailer: Mailer,
    /// Deploy environment: "local" | "staging" | "production" (from APP_ENV).
    /// The wasm bundle is shared across envs, so the frontend learns the env at
    /// runtime from `/api/config` rather than a build-time constant.
    pub env: String,
}

/// Build the auth request from headers (next-auth cookie + optional bearer).
pub fn req_auth(headers: &axum::http::HeaderMap) -> crossword_auth::RequestAuth {
    crossword_auth::RequestAuth {
        cookie_header: headers
            .get("cookie")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
        bearer_token: headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string()),
    }
}
