//! App-wide shared state, provided once at the root via Dioxus context.
//!
//! - `session`: the logged-in user, fetched from next-auth's `/api/auth/session`
//!   (the wasm app is same-origin with Nuxt, so the JWT cookie is sent).
//! - `sub`: subscription/quota status from the REST `/api/subscription` route.

use crossword_core::auth::Role;
use dioxus::prelude::*;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct User {
    #[serde(default)]
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub image: Option<String>,
    #[serde(default)]
    pub role: Role,
}

/// Shape of `GET /api/auth/session`: `{}` when signed out, `{user, expires}`
/// when signed in.
#[derive(Debug, Clone, Deserialize)]
struct SessionResponse {
    user: Option<User>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubStatus {
    pub is_pro: bool,
    #[serde(default)]
    pub quota_used: i64,
    /// null = unlimited
    pub quota_limit: Option<i64>,
}

#[derive(Clone, Copy)]
pub struct AppState {
    /// `None` = still loading; `Some(None)` = signed out; `Some(Some(u))` = user.
    pub session: Signal<Option<Option<User>>>,
    pub sub: Signal<Option<SubStatus>>,
}

impl AppState {
    pub fn user(&self) -> Option<User> {
        self.session.read().clone().flatten()
    }
    pub fn is_admin(&self) -> bool {
        self.user().map(|u| u.role == Role::Admin).unwrap_or(false)
    }
}

/// Provide [`AppState`] and kick off the session fetch. Call once in the root.
pub fn provide_app_state() -> AppState {
    let state = use_context_provider(|| AppState {
        session: Signal::new(None),
        sub: Signal::new(None),
    });
    use_hook(|| {
        let mut session = state.session;
        let mut sub = state.sub;
        spawn_local(async move {
            session.set(Some(fetch_session().await));
            if let Some(s) = fetch_sub().await {
                sub.set(Some(s));
            }
        });
    });
    state
}

pub fn use_app_state() -> AppState {
    use_context::<AppState>()
}

async fn fetch_session() -> Option<User> {
    let resp = gloo_net::http::Request::get("/api/auth/session")
        .send()
        .await
        .ok()?;
    let parsed: SessionResponse = resp.json().await.ok()?;
    parsed.user
}

async fn fetch_sub() -> Option<SubStatus> {
    let resp = gloo_net::http::Request::get("/api/subscription")
        .send()
        .await
        .ok()?;
    resp.json().await.ok()
}
