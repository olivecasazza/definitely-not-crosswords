//! Home: signed-out keeps the marketing Welcome + Get Started panels; signed-in
//! is a "what do I play right now" dashboard — Play Now (resume + fresh
//! puzzles), Pulse (career stat tiles), and a small demoted Brand panel.

use crossword_core::fmt::{plural, rel_time};
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::components::brand::BrandLogo;
use crate::components::game_list::{
    error_status, game_list, status, GameRow, GameStatus, GAME_LIST_CSS,
};
use crate::components::identicon::Identicon;
use crate::components::ui::StatTile;
use crate::net;
use crate::store::use_app_state;
use crate::Route;

#[component]
pub fn Home() -> Element {
    let state = use_app_state();
    // Session tri-state: `None` = still loading, `Some(None)` = signed out,
    // `Some(Some(_))` = signed in. Pick the surface only once the session
    // resolves so the wrong workspace never flashes (each variant is its own
    // component with its own hooks + workspace key).
    let session = state.session.read().clone();
    match session {
        None => status("muted", "Loading…", true),
        Some(None) => rsx! {
            GuestHome {}
        },
        Some(Some(_)) => rsx! {
            Dashboard {}
        },
    }
}

// ---------------------------------------------------------------------------
// Signed-out: Welcome + Get Started (unchanged)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum GuestPanel {
    Welcome,
    Start,
}

impl PanelKind for GuestPanel {
    fn title(self) -> &'static str {
        match self {
            GuestPanel::Welcome => "Welcome",
            GuestPanel::Start => "Get Started",
        }
    }
}

/// First-mount geometry, computed as viewport proportions (a saved layout
/// always wins; panel-kit re-scales floating panels on window resize).
fn guest_layout() -> Vec<PanelWin<GuestPanel>> {
    let (vw, vh) = viewport();
    const GAP: f64 = 20.0;
    let welcome_w = (vw * 0.30).clamp(380.0, 620.0);
    let start_w = (vw * 0.20).clamp(280.0, 420.0);
    let h = (vh * 0.55).clamp(420.0, 620.0);
    let x0 = ((vw - welcome_w - GAP - start_w) / 2.0).max(16.0);
    let y0 = ((vh - h) / 2.0).max(16.0);
    let mut b = LayoutBuilder::new();
    vec![
        b.at(GuestPanel::Welcome, x0, y0, welcome_w, h),
        b.at(GuestPanel::Start, x0 + welcome_w + GAP, y0, start_w, h),
    ]
}

#[component]
fn GuestHome() -> Element {
    let ws = use_workspace("home_layout", guest_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: GuestPanel, _max: bool| -> Element {
        match kind {
            GuestPanel::Welcome => rsx! {
                div { class: "home-welcome",
                    BrandLogo { size: 64 }
                    h1 { class: "home-title", "definitely-not-crosswords" }
                    p { class: "home-tagline",
                        "Cooperative, real-time crosswords. Race the grid, fill the clues, and climb the leaderboard with friends."
                    }
                    div { class: "home-features",
                        div { class: "home-feature",
                            span { class: "home-feature-icon", "◀▶" }
                            div { class: "home-feature-body",
                                p { class: "home-feature-title", "Real-time co-op" }
                                p { class: "home-feature-desc", "Solve together. See every move as it happens." }
                            }
                        }
                        div { class: "home-feature",
                            span { class: "home-feature-icon", "●" }
                            div { class: "home-feature-body",
                                p { class: "home-feature-title", "Live presence" }
                                p { class: "home-feature-desc", "Know who's on which clue, right now." }
                            }
                        }
                        div { class: "home-feature",
                            span { class: "home-feature-icon", "▲" }
                            div { class: "home-feature-body",
                                p { class: "home-feature-title", "Stats & rankings" }
                                p { class: "home-feature-desc", "Track solve times. Compare head-to-head. Climb." }
                            }
                            // Pricing on the front door, visible without signing in.
                            // Same literals as components/brand.rs — see README "Plans & pricing".
                            div { class: "home-feature",
                                span { class: "home-feature-icon", "★" }
                                div { class: "home-feature-body",
                                    p { class: "home-feature-title", "Free, or Pro for $10/year" }
                                    p { class: "home-feature-desc", "Solving is always free; generate 5 puzzles a month and build teams of 4. Pro adds unlimited generation and teams of 10. Cancel anytime." }
                                }
                            }
                        }
                    }
                }
            },
            GuestPanel::Start => rsx! {
                div { class: "home-start",
                    p { class: "home-start-eyebrow", "Ready when you are" }
                    Link { to: Route::Login {}, class: "app-btn app-btn-active home-cta", "Sign in" }
                    Link { to: Route::Signup {}, class: "app-btn home-cta", "Create account" }
                    p { class: "home-start-foot", "Free to play. No card required." }
                }
            },
        }
    };

    rsx! {
        style { {HOME_CSS} }
        div {
            class: ws.root_class(),
            tabindex: "0",
            onmousemove: move |e| ws.handle_mouse_move(&e),
            onmouseup: move |_| ws.handle_mouse_up(),
            {ws.render(body)}
            {ws.dock()}
        }
    }
}

// ---------------------------------------------------------------------------
// Signed-in: Play Now / Pulse / Brand dashboard
// ---------------------------------------------------------------------------

/// Minimal projection of `gameList.get` items — only what Home renders. Same
/// per-item tolerant parse as the games library: rows that fail to deserialize
/// are dropped, not fatal.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
enum HomeItem {
    Game(FreshGame),
    ActiveGame(ResumeGame),
    /// On the wire but not rendered here — parsed so it doesn't get dropped
    /// as "unknown" and skew anyone counting items later.
    CompletedGame(IgnoredGame),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct GridSize {
    w: i64,
    h: i64,
}

/// Unstarted puzzle — feeds the shared [`GameRow`] renderer. Optional fields
/// stay optional so an older/leaner server response still deserializes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct FreshGame {
    id: String,
    title: String,
    #[serde(default)]
    clues: i64,
    #[serde(default)]
    at: Option<String>,
    #[serde(default, rename = "gridSize")]
    grid_size: Option<GridSize>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct NestedGame {
    title: String,
}

/// In-progress game — feeds the Resume cards.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct ResumeGame {
    id: String,
    game: NestedGame,
    #[serde(default)]
    players: i64,
    #[serde(default)]
    at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct IgnoredGame {}

/// Career aggregates from `stats.getUserStats` — the four Pulse tiles. The
/// server also returns profile / totals / recentGames; serde ignores the rest.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PulseStats {
    games_played: i64,
    total_score: i64,
    accuracy: i64,
    global_rank: i64,
    total_players: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    PlayNow,
    Pulse,
    Brand,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::PlayNow => "Play Now",
            Panel::Pulse => "Pulse",
            Panel::Brand => "Welcome",
        }
    }
}

fn viewport() -> (f64, f64) {
    web_sys::window()
        .and_then(|w| {
            Some((
                w.inner_width().ok()?.as_f64()?,
                w.inner_height().ok()?.as_f64()?,
            ))
        })
        .unwrap_or((1440.0, 900.0))
}

/// First-mount geometry, viewport-proportional like the games library (a saved
/// layout always wins). Vec order is also the mobile stacking order:
/// Play Now → Pulse → Brand.
fn dash_layout() -> Vec<PanelWin<Panel>> {
    let (vw, vh) = viewport();
    const GAP: f64 = 16.0;
    let play_w = (vw * 0.52).clamp(460.0, 960.0);
    let side_w = (vw * 0.30).clamp(320.0, 520.0);
    let h = (vh * 0.82).clamp(480.0, 1000.0);
    let brand_h = (h * 0.26).clamp(170.0, 240.0);
    let pulse_h = (h - brand_h - GAP).max(280.0);
    let x0 = ((vw - play_w - GAP - side_w) / 2.0).max(16.0);
    let y0 = ((vh - h) / 2.0).max(16.0);
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::PlayNow, x0, y0, play_w, h),
        b.at(Panel::Pulse, x0 + play_w + GAP, y0, side_w, pulse_h),
        b.at(
            Panel::Brand,
            x0 + play_w + GAP,
            y0 + pulse_h + GAP,
            side_w,
            brand_h,
        ),
    ]
}

/// `at` is an ISO-8601 stamp from the server; parse via the JS Date so we don't
/// pull chrono into the wasm bundle. Returns None on an absent/unparsable stamp.
fn age(at: &Option<String>) -> Option<String> {
    let ms = js_sys::Date::parse(at.as_deref()?);
    if ms.is_nan() {
        return None;
    }
    Some(rel_time(js_sys::Date::now(), ms))
}

/// Epoch ms for sorting; absent/unparsable stamps sort oldest.
fn ts(at: &Option<String>) -> f64 {
    at.as_deref()
        .map(js_sys::Date::parse)
        .filter(|ms| !ms.is_nan())
        .unwrap_or(0.0)
}

#[component]
fn Dashboard() -> Element {
    let state = use_app_state();

    // ONE gameList.get fetch feeds both Play Now sections (resume + fresh).
    let mut games_res = use_resource(move || async move {
        let email = state.user().and_then(|u| u.email)?;
        let result = net::query_as::<Vec<serde_json::Value>>(
            "gameList.get",
            Some(json!({ "email": email })),
        )
        .await;
        Some(result)
    });

    // Parse once per fetch, not once per panel per render. `None` means "not
    // ready" (loading or error) — the panel body reads the resource for the
    // exact status. Keeping this here also keeps all hooks out of the `body`
    // closure, which panel-kit calls once per panel.
    let items = use_memo(move || match &*games_res.read() {
        Some(Some(Ok(raw))) => Some(
            raw.iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect::<Vec<HomeItem>>(),
        ),
        _ => None,
    });

    // Career tiles for the Pulse panel (non-critical: errors hide the panel
    // body instead of blocking the dashboard).
    let stats_res = use_resource(move || async move {
        let email = state.user().and_then(|u| u.email)?;
        Some(
            net::query_as::<PulseStats>("stats.getUserStats", Some(json!({ "email": email })))
                .await,
        )
    });

    let nav = use_navigator();

    // "_v2": the panel set changed shape — don't let a persisted Welcome/Start
    // layout fight the new defaults.
    let ws = use_workspace("home_layout_v2", dash_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::PlayNow => {
                let items_v = items();
                let Some(items) = &items_v else {
                    return match &*games_res.read_unchecked() {
                        Some(Some(Err(e))) => error_status(e, move |_| games_res.restart()),
                        _ => status("muted", "Loading…", true),
                    };
                };

                // Resume: newest active games first, up to 3.
                let mut actives: Vec<&ResumeGame> = items
                    .iter()
                    .filter_map(|i| match i {
                        HomeItem::ActiveGame(g) => Some(g),
                        _ => None,
                    })
                    .collect();
                actives.sort_by(|a, b| {
                    ts(&b.at)
                        .partial_cmp(&ts(&a.at))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                actives.truncate(3);

                // Fresh: newest unstarted puzzles, up to 3, through the shared
                // GameRow renderer (same projection the games library uses).
                let mut fresh: Vec<&FreshGame> = items
                    .iter()
                    .filter_map(|i| match i {
                        HomeItem::Game(g) => Some(g),
                        _ => None,
                    })
                    .collect();
                fresh.sort_by(|a, b| {
                    ts(&b.at)
                        .partial_cmp(&ts(&a.at))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let rows: Vec<GameRow> = fresh
                    .into_iter()
                    .take(3)
                    .map(|g| GameRow {
                        id: g.id.clone(),
                        title: g.title.clone(),
                        status: GameStatus::New,
                        clues: g.clues,
                        size: g.grid_size.as_ref().map(|s| (s.w, s.h)),
                        when: age(&g.at),
                        score: None,
                        progress: None,
                    })
                    .collect();

                rsx! {
                    div { class: "home-play",
                        if !actives.is_empty() {
                            div { class: "home-head",
                                span { class: "home-eyebrow", "JUMP BACK IN" }
                            }
                            div { class: "home-resume-cards",
                                for g in actives {
                                    {
                                        let meta = [
                                            (g.players > 0).then(|| plural(g.players, "player")),
                                            age(&g.at),
                                        ]
                                        .into_iter()
                                        .flatten()
                                        .collect::<Vec<_>>()
                                        .join(" · ");
                                        rsx! {
                                            Link {
                                                key: "{g.id}",
                                                to: Route::GamePlay { id: g.id.clone() },
                                                class: "home-resume-card",
                                                span { class: "home-resume-title", "{g.game.title}" }
                                                if !meta.is_empty() {
                                                    span { class: "home-resume-meta", "{meta}" }
                                                }
                                                span { class: "home-resume-cta", "Resume →" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "home-head",
                            span { class: "home-eyebrow", "START SOMETHING FRESH" }
                        }
                        if rows.is_empty() {
                            {status("muted", "No new puzzles right now.", false)}
                        } else {
                            {game_list(rows, move |_st, id| {
                                nav.push(Route::GameNew { id });
                            })}
                        }
                        Link { to: Route::Games {}, class: "home-link home-play-foot", "All games →" }
                    }
                }
            }

            Panel::Pulse => {
                let user = state.user();
                let seed = user.as_ref().map(|u| u.id.clone()).unwrap_or_default();
                let head = rsx! {
                    div { class: "home-head",
                        span { class: "home-eyebrow", "YOUR PULSE" }
                        if !seed.is_empty() {
                            span { class: "home-pulse-ic", Identicon { seed, size: 20 } }
                        }
                    }
                };
                match &*stats_res.read() {
                    // Loading skeleton: real labels, muted em-dash values.
                    None => rsx! {
                        div { class: "home-pulse",
                            {head}
                            div { class: "home-pulse-grid", aria_busy: "true",
                                StatTile { label: "Global Rank".to_string(), value: "—".to_string() }
                                StatTile { label: "Career Score".to_string(), value: "—".to_string() }
                                StatTile { label: "Accuracy".to_string(), value: "—".to_string() }
                                StatTile { label: "Games Played".to_string(), value: "—".to_string() }
                            }
                        }
                    },
                    Some(Some(Ok(s))) if s.games_played == 0 => rsx! {
                        div { class: "home-pulse",
                            {head}
                            div { class: "home-pulse-empty",
                                p { "No stats yet — your pulse starts with your first solve." }
                                Link { to: Route::Games {}, class: "app-btn app-btn-active", "Play your first game" }
                            }
                        }
                    },
                    Some(Some(Ok(s))) => rsx! {
                        div { class: "home-pulse",
                            {head}
                            div { class: "home-pulse-grid",
                                StatTile {
                                    label: "Global Rank".to_string(),
                                    value: format!("#{}", s.global_rank),
                                    sub: format!("of {}", plural(s.total_players, "player")),
                                }
                                StatTile { label: "Career Score".to_string(), value: s.total_score.to_string() }
                                StatTile { label: "Accuracy".to_string(), value: format!("{}%", s.accuracy) }
                                StatTile { label: "Games Played".to_string(), value: s.games_played.to_string() }
                            }
                            Link { to: Route::Stats {}, class: "home-link home-pulse-foot", "Full stats →" }
                        }
                    },
                    // Error (or no email): non-critical panel — hide content.
                    _ => rsx! {},
                }
            }

            Panel::Brand => {
                let name = user_name(&state);
                rsx! {
                    div { class: "home-brand",
                        BrandLogo { size: 44 }
                        p { class: "home-brand-greet", "Welcome back, {name}" }
                        p { class: "home-brand-tag",
                            "Cooperative, real-time crosswords — race the grid with friends."
                        }
                    }
                }
            }
        }
    };

    rsx! {
        style { {GAME_LIST_CSS} }
        style { {HOME_DASH_CSS} }
        div {
            class: ws.root_class(),
            tabindex: "0",
            onmousemove: move |e| ws.handle_mouse_move(&e),
            onmouseup: move |_| ws.handle_mouse_up(),
            {ws.render(body)}
            {ws.dock()}
        }
    }
}

fn user_name(state: &crate::store::AppState) -> String {
    state
        .user()
        .and_then(|u| u.name)
        .unwrap_or_else(|| "friend".to_string())
}

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

/// Signed-out panels. Square corners throughout (house style).
const HOME_CSS: &str = "
.home-welcome { height: 100%; display: flex; flex-direction: column; align-items: center;
  justify-content: center; text-align: center; gap: 1rem; padding: 1.75rem 1.5rem; }
.home-title { font-family: var(--mono, monospace); font-size: 1.4rem; font-weight: 800; margin: 0;
  letter-spacing: -.01em; color: var(--text-primary, var(--fg)); }
.home-tagline { color: var(--text-secondary, var(--dim)); max-width: 28rem; margin: 0;
  line-height: 1.6; font-size: .88rem; }
.home-features { display: flex; flex-direction: column; gap: .65rem; margin-top: .75rem;
  width: 100%; max-width: 26rem; }
.home-feature { display: flex; align-items: flex-start; gap: .65rem; text-align: left;
  padding: .55rem .7rem; border: 1px solid var(--border-app, var(--line2));
  background: var(--bg-card, transparent); }
.home-feature-icon { font-size: .8rem; color: var(--pastel-yellow); line-height: 1.4;
  flex-shrink: 0; min-width: 1.2rem; text-align: center; }
.home-feature-body { flex: 1; min-width: 0; }
.home-feature-title { margin: 0; font-size: .82rem; font-weight: 700; color: var(--text-primary, var(--fg)); }
.home-feature-desc { margin: .15rem 0 0; font-size: .72rem; color: var(--text-secondary, var(--dim));
  line-height: 1.45; }

.home-start { height: 100%; display: flex; flex-direction: column; justify-content: center;
  gap: .6rem; padding: 1.75rem 1.5rem; }
.home-start-eyebrow { margin: 0 0 .35rem; font-size: .68rem; font-weight: 700; text-transform: uppercase;
  letter-spacing: .08em; color: var(--text-secondary, var(--dim)); }
.home-cta { padding: .65rem 1.1rem; font-weight: 600; text-align: center; font-size: .88rem; }
.home-start-foot { margin: .5rem 0 0; font-size: .72rem; color: var(--text-secondary, var(--dim));
  text-align: center; }

@media (max-width: 760px) {
  .home-welcome { padding: 1.25rem 1rem; gap: .75rem; }
  .home-title { font-size: 1.2rem; }
  .home-tagline { font-size: .82rem; }
  .home-features { gap: .5rem; margin-top: .5rem; }
  .home-feature { padding: .45rem .55rem; }
  .home-start { padding: 1.25rem 1rem; gap: .55rem; }
  .home-cta { padding: .75rem 1rem; font-size: .9rem; }
}
";

/// Signed-in dashboard panels. Square corners throughout (house style).
const HOME_DASH_CSS: &str = "
.home-head { display: flex; align-items: center; gap: .5rem; padding: .75rem 1rem .5rem; }
.home-eyebrow { font-family: var(--mono, monospace); font-size: var(--fs-2xs); font-weight: 700;
  letter-spacing: .08em; text-transform: uppercase; color: var(--text-secondary); }
.home-link { font-family: var(--mono, monospace); font-size: var(--fs-2xs); font-weight: 700;
  color: var(--text-secondary); }
.home-link:hover { color: var(--text-primary); }

.home-play { height: 100%; display: flex; flex-direction: column; overflow-y: auto; }
.home-resume-cards { display: flex; flex-direction: column; }
.home-resume-card { display: flex; flex-direction: column; gap: .4rem; padding: .875rem 1rem;
  text-decoration: none; border-bottom: 1px solid var(--border-app);
  border-left: 2px solid var(--pastel-green); transition: background-color .12s ease; }
.home-resume-card:hover, .home-resume-card:focus-visible { outline: none;
  background: var(--bg-cell-letter); }
.home-resume-title { font-weight: 700; font-size: var(--fs-md); color: var(--text-primary);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.home-resume-meta { font-family: var(--mono, monospace); font-size: var(--fs-2xs);
  color: var(--text-secondary); }
.home-resume-cta { align-self: flex-start; margin-top: .25rem; padding: .5rem 1rem;
  border: 1px solid var(--pastel-green); color: var(--pastel-green);
  font-family: var(--mono, monospace); font-size: var(--fs-xs); font-weight: 700;
  text-transform: uppercase; }
.home-resume-card:hover .home-resume-cta, .home-resume-card:focus-visible .home-resume-cta {
  background: var(--pastel-green); color: var(--contrast-ink); }
.home-play-foot { display: inline-block; margin: auto 1rem 1rem; padding-top: .75rem;
  align-self: flex-start; }

.home-pulse { height: 100%; display: flex; flex-direction: column; }
.home-pulse-ic { margin-left: auto; display: inline-flex; }
.home-pulse-grid { display: grid; grid-template-columns: 1fr 1fr; gap: .75rem;
  padding: .25rem 1rem .75rem; }
.home-pulse-foot { margin: 0 1rem 1rem; align-self: flex-start; }
.home-pulse-empty { flex: 1 1 auto; display: flex; flex-direction: column; align-items: center;
  justify-content: center; gap: .75rem; padding: 1rem; text-align: center; }
.home-pulse-empty p { margin: 0; font-size: var(--fs-xs); color: var(--text-secondary);
  line-height: 1.5; }

.home-brand { height: 100%; display: flex; flex-direction: column; align-items: center;
  justify-content: center; text-align: center; gap: .6rem; padding: 1.25rem 1rem; }
.home-brand-greet { margin: 0; font-family: var(--mono, monospace); font-size: var(--fs-md);
  font-weight: 800; color: var(--text-primary); }
.home-brand-tag { margin: 0; font-size: var(--fs-2xs); color: var(--text-secondary);
  line-height: 1.5; }

@media (max-width: 760px) {
  /* Comfortable tap targets; no hover-stick on touch. */
  .home-resume-card { padding: 1rem; }
  .home-resume-card:hover { background: transparent; }
  .home-resume-cta { padding: .625rem 1rem; }
  .home-pulse-grid { gap: .5rem; }
}
";

#[cfg(test)]
mod tests {
    use super::*;

    /// All three wire variants deserialize; Home splits actives from fresh and
    /// ignores completed rows. (`at` stays absent so no JS Date is touched.)
    #[test]
    fn wire_items_deserialize_and_split() {
        let raw = json!([
            { "type": "Game", "id": "g1", "title": "Monday Mini", "clues": 8,
              "gridSize": { "w": 5, "h": 5 } },
            { "type": "Game", "id": "g2", "title": "Tuesday Twister" },
            { "type": "ActiveGame", "id": "a1", "game": { "title": "Co-op Cryptic" },
              "clues": 12, "players": 2 },
            { "type": "CompletedGame", "id": "c1", "game": { "title": "Sunday Giant" },
              "clues": 40, "players": 1, "score": 340 },
        ]);
        let items: Vec<HomeItem> = raw
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        assert_eq!(items.len(), 4);
        let fresh: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                HomeItem::Game(g) => Some(g),
                _ => None,
            })
            .collect();
        assert_eq!(fresh.len(), 2);
        assert_eq!(fresh[0].grid_size, Some(GridSize { w: 5, h: 5 }));
        assert_eq!(fresh[1].clues, 0); // defaulted
        let actives: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                HomeItem::ActiveGame(g) => Some(g),
                _ => None,
            })
            .collect();
        assert_eq!(actives.len(), 1);
        assert_eq!(actives[0].game.title, "Co-op Cryptic");
        assert_eq!(actives[0].players, 2);
    }

    /// getUserStats response shape (verified against routers/stats.rs).
    #[test]
    fn pulse_stats_deserialize() {
        let s: PulseStats = serde_json::from_value(json!({
            "profile": { "id": "u1", "name": "Olive", "email": null },
            "gamesPlayed": 12,
            "totalScore": 3400,
            "totalCorrect": 300,
            "totalIncorrect": 50,
            "accuracy": 86,
            "globalRank": 3,
            "totalPlayers": 128,
            "recentGames": [],
        }))
        .unwrap();
        assert_eq!(s.games_played, 12);
        assert_eq!(s.total_score, 3400);
        assert_eq!(s.accuracy, 86);
        assert_eq!(s.global_rank, 3);
        assert_eq!(s.total_players, 128);
    }
}
