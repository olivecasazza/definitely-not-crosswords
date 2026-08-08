//! App-wide shared state, provided once at the root via Dioxus context.
//!
//! - `session`: the logged-in user, fetched from next-auth's `/api/auth/session`
//!   (the wasm app is same-origin with Nuxt, so the JWT cookie is sent).
//! - `sub`: subscription/quota status from the REST `/api/subscription` route.

use crossword_core::auth::Role;
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use panel_kit::Mode;
use serde::Deserialize;
use wasm_bindgen_futures::spawn_local;

/// Share the floating⇄tiling mode across every view. Each route is its own
/// panel-kit workspace (independent mode), so without this, navigating between
/// views would flip the layout mode back to whatever that view last persisted.
/// Call once per page, right after `use_workspace`, passing `ws.mode`.
pub fn sync_panel_mode(mut mode: Signal<Mode>) {
    // On mount, adopt the shared mode (overriding this workspace's own).
    // Default is Tiling; only an explicit prior "floating" choice opts out.
    use_hook(move || {
        let m = match LocalStorage::get::<String>("panel_mode").as_deref() {
            Ok("floating") => Mode::Floating,
            _ => Mode::Tiling,
        };
        mode.set(m);
    });
    // Persist any change to the shared key so other views pick it up.
    use_effect(move || {
        let s = match *mode.read() {
            Mode::Tiling => "tiling",
            Mode::Floating => "floating",
        };
        let _ = LocalStorage::set("panel_mode", s);
    });
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct User {
    #[serde(default)]
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub image: Option<String>,
    #[serde(default)]
    pub role: Role,
    /// Real email-verification status from the session endpoint.
    #[serde(default, rename = "emailVerified")]
    pub email_verified: bool,
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

/// Runtime config from `GET /api/config`. The wasm bundle is shared across
/// environments, so feature-gating is driven by the server's `APP_ENV` at
/// runtime, not build-time constants.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
pub struct AppConfig {
    /// "local" | "staging" | "production".
    #[serde(default)]
    pub environment: String,
    #[serde(default)]
    pub features: Features,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    /// Show the "Developer Admin Bypass" login button (local only).
    #[serde(default)]
    pub dev_login_bypass: bool,
    /// Show the staging beta banner (staging only).
    #[serde(default)]
    pub staging_banner: bool,
}

/// Severity of a [`Toast`]; picks the accent border colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Success,
    Warning,
    Error,
}

impl Severity {
    pub fn class(self) -> &'static str {
        match self {
            Severity::Success => "toast-success",
            Severity::Warning => "toast-warning",
            Severity::Error => "toast-error",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub severity: Severity,
    pub msg: String,
}

#[derive(Clone, Copy)]
pub struct AppState {
    /// `None` = still loading; `Some(None)` = signed out; `Some(Some(u))` = user.
    pub session: Signal<Option<Option<User>>>,
    pub sub: Signal<Option<SubStatus>>,
    /// `None` until `/api/config` resolves; features default to off meanwhile.
    pub config: Signal<Option<AppConfig>>,
    /// Live toasts, rendered by `components::ui::ToastHost` in the shell.
    pub toasts: Signal<Vec<Toast>>,
}

impl AppState {
    pub fn user(&self) -> Option<User> {
        self.session.read().clone().flatten()
    }
    pub fn is_admin(&self) -> bool {
        self.user().map(|u| u.role == Role::Admin).unwrap_or(false)
    }
    /// Read a feature flag; false while config is still loading (safe default).
    pub fn feature(&self, pick: impl Fn(&Features) -> bool) -> bool {
        self.config
            .read()
            .as_ref()
            .map(|c| pick(&c.features))
            .unwrap_or(false)
    }
    /// True while the initial session fetch is still in flight (distinct from
    /// signed-out). Used by route guards to avoid bouncing a real user mid-load.
    pub fn is_loading(&self) -> bool {
        self.session.read().is_none()
    }
    /// Queue a toast; auto-dismisses after 5s (click dismisses sooner).
    pub fn toast(&self, severity: Severity, msg: impl Into<String>) {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut toasts = self.toasts;
        toasts.write().push(Toast {
            id,
            severity,
            msg: msg.into(),
        });
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(5_000).await;
            toasts.write().retain(|t| t.id != id);
        });
    }
}

/// Provide [`AppState`] and kick off the session fetch. Call once in the root.
pub fn provide_app_state() -> AppState {
    let state = use_context_provider(|| AppState {
        session: Signal::new(None),
        sub: Signal::new(None),
        config: Signal::new(None),
        toasts: Signal::new(Vec::new()),
    });
    use_hook(|| {
        let mut session = state.session;
        let mut sub = state.sub;
        let mut config = state.config;
        spawn_local(async move {
            // Config drives feature flags (banner, dev bypass) — fetch it first.
            match fetch_config().await {
                Ok(c) => config.set(Some(c)),
                Err(_) => {
                    config.set(Some(AppConfig::default()));
                    state.toast(
                        Severity::Warning,
                        "Couldn't load app settings — some features may be unavailable.",
                    );
                }
            }
            let user = match fetch_session().await {
                Ok(user) => user,
                Err(_) => {
                    state.toast(
                        Severity::Error,
                        "Couldn't check your sign-in status. Refresh to retry.",
                    );
                    None
                }
            };
            let signed_in = user.is_some();
            session.set(Some(user));
            // Only hit the authed /api/subscription endpoint when signed in —
            // otherwise it 401s and clutters the console.
            if signed_in {
                match fetch_sub().await {
                    Ok(s) => sub.set(Some(s)),
                    Err(_) => {
                        state.toast(Severity::Warning, "Couldn't load your subscription status.")
                    }
                }
            }
        });
    });
    state
}

pub fn use_app_state() -> AppState {
    use_context::<AppState>()
}

async fn fetch_session() -> Result<Option<User>, String> {
    let resp = gloo_net::http::Request::get("/api/auth/session")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let parsed: SessionResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(parsed.user)
}

async fn fetch_sub() -> Result<SubStatus, String> {
    let resp = gloo_net::http::Request::get("/api/subscription")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}

async fn fetch_config() -> Result<AppConfig, String> {
    let resp = gloo_net::http::Request::get("/api/config")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    resp.json().await.map_err(|e| e.to_string())
}
