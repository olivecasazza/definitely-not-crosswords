//! App-wide shared state, provided once at the root via Dioxus context.
//!
//! - `session`: the logged-in user, fetched from next-auth's `/api/auth/session`
//!   (the wasm app is same-origin with Nuxt, so the JWT cookie is sent).
//! - `sub`: subscription/quota status from the REST `/api/subscription` route.

use crossword_core::auth::Role;
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, SessionStorage, Storage};
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

/// The signed-in user's most recent in-progress game, cached once at store
/// level from `gameList.get`. Powers the header ▶ Resume button (and later the
/// Home/Games resume surfaces) without per-page refetching.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveGameSummary {
    pub id: String,
    pub title: String,
    pub at: Option<String>,
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
    /// Most recent in-progress game; `None` while loading / signed out / none.
    pub active_game: Signal<Option<ActiveGameSummary>>,
    /// Whether the staging strip was dismissed (persisted; header shows the
    /// BETA chip instead once true).
    pub banner_dismissed: Signal<bool>,
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
        active_game: Signal::new(None),
        banner_dismissed: Signal::new(
            LocalStorage::get::<bool>("staging_dismissed").unwrap_or(false),
        ),
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
                let mut active_game = state.active_game;
                active_game.set(fetch_active_game().await);
            }
        });
    });
    state
}

/// Latest ActiveGame row from `gameList.get` (best-effort; None on any error —
/// the Resume affordances just don't render).
async fn fetch_active_game() -> Option<ActiveGameSummary> {
    let v = crate::net::query("gameList.get", None).await.ok()?;
    let mut best: Option<(f64, ActiveGameSummary)> = None;
    for item in v.as_array()? {
        if item["type"] != "ActiveGame" {
            continue;
        }
        let Some(id) = item["id"].as_str() else {
            continue;
        };
        let at = item["at"].as_str().map(|s| s.to_string());
        let ms = at
            .as_deref()
            .map(js_sys::Date::parse)
            .filter(|m| !m.is_nan())
            .unwrap_or(0.0);
        let summary = ActiveGameSummary {
            id: id.to_string(),
            title: item["game"]["title"]
                .as_str()
                .unwrap_or("Untitled")
                .to_string(),
            at,
        };
        if best.as_ref().map(|(b, _)| ms > *b).unwrap_or(true) {
            best = Some((ms, summary));
        }
    }
    best.map(|(_, s)| s)
}

pub fn use_app_state() -> AppState {
    use_context::<AppState>()
}

/// Re-fetch session + subscription after an auth change (e.g. login) so the
/// header and route guards update without a full page reload. Falls back to a
/// hard reload if the session endpoint can't be reached — stale "signed out"
/// chrome after a successful login is worse than a refresh.
pub async fn refresh_session(state: AppState) {
    let mut session = state.session;
    let mut sub = state.sub;
    match fetch_session().await {
        Ok(user) => {
            let signed_in = user.is_some();
            session.set(Some(user));
            if signed_in {
                if let Ok(s) = fetch_sub().await {
                    sub.set(Some(s));
                }
                let mut active_game = state.active_game;
                active_game.set(fetch_active_game().await);
            }
        }
        Err(_) => {
            if let Some(w) = web_sys::window() {
                let _ = w.location().reload();
            }
        }
    }
}

fn current_path() -> Option<String> {
    let loc = web_sys::window()?.location();
    let mut p = loc.pathname().ok()?;
    if let Ok(q) = loc.search() {
        p.push_str(&q);
    }
    Some(p)
}

/// Pop the stashed post-login destination, if any. Parses through the route
/// table; the NotFound catch-all means any stashed string yields a route.
pub fn take_return_to() -> Option<crate::Route> {
    use std::str::FromStr;
    let dest = SessionStorage::get::<String>("return_to").ok()?;
    SessionStorage::delete("return_to");
    crate::Route::from_str(&dest).ok()
}

/// Route guard (auth'd pages): `None` means render the page. `Some(el)` is
/// what to render instead — a quiet "verifying" placeholder while the session
/// loads, or nothing while bouncing an unauthorized visitor to Login (with the
/// intended path stashed in sessionStorage["return_to"] for the post-login
/// redirect). Call after the page's other hooks so the hook order is stable.
pub fn use_auth_guard(required: Role) -> Option<Element> {
    let state = use_app_state();
    let nav = use_navigator();
    if state.is_loading() {
        return Some(rsx! {
            div { class: "game-status muted", aria_busy: "true", "Verifying access…" }
        });
    }
    let authorized = match required {
        Role::Admin => state.is_admin(),
        Role::User => state.user().is_some(),
    };
    if !authorized {
        if let Some(p) = current_path() {
            let _ = SessionStorage::set("return_to", p);
        }
        nav.push(crate::Route::Login {});
        return Some(rsx! {});
    }
    None
}

/// Guest-only pages (login/signup): bounce visitors who are already signed in
/// to their stashed destination, or Home. Verify-email and reset-password are
/// NOT guest-only — a signed-in user legitimately lands there from email links.
pub fn use_guest_only() {
    let state = use_app_state();
    let nav = use_navigator();
    if !state.is_loading() && state.user().is_some() {
        let dest = take_return_to().unwrap_or(crate::Route::Home {});
        nav.push(dest);
    }
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
