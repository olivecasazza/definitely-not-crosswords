//! Games library: four panel-kit panels — Continue (resume + paste-to-join),
//! Featured (this week's puzzle), Library (the full filterable list), and
//! Progress (client-side stat tiles). One `gameList.get` fetch feeds all four;
//! everything else (filter / sort / search / stats) is client-side projection.

use crossword_core::fmt::{plural, rel_time};
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

use crate::components::game_list::{
    error_status, game_list, progress_bar, progress_pct, status, GameRow, GameStatus, GAME_LIST_CSS,
};
use crate::components::ui::{SectionTabs, StatTile};
use crate::net;
use crate::store::{use_app_state, Severity};
use crate::Route;

/// Raw items returned by gameList.get — a heterogeneous array discriminated by `type`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
enum GameListItem {
    Game(UnstartedGame),
    ActiveGame(PlayedGame),
    CompletedGame(PlayedGame),
}

/// Grid dimensions, when the server aggregates them onto list rows.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct GridSize {
    w: i64,
    h: i64,
}

/// Lobby metadata the server aggregates alongside each row. All optional so an
/// older/leaner server response still deserializes — `gridSize` lands with a
/// separate backend change.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct UnstartedGame {
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

/// ActiveGame and CompletedGame have the same shape on the wire; `score` is only
/// populated for completed rows. `gameId`, `gridSize`, and the cell counts are
/// backend additions that land separately — every consumer here is Option-gated
/// so the page fully works against a server that doesn't send them.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct PlayedGame {
    id: String,
    game: NestedGame,
    #[serde(default)]
    clues: i64,
    #[serde(default)]
    players: i64,
    #[serde(default)]
    at: Option<String>,
    #[serde(default)]
    score: Option<i32>,
    /// Back-reference to the parent Game (Featured started/solved detection).
    #[serde(default, rename = "gameId")]
    game_id: Option<String>,
    #[serde(default, rename = "gridSize")]
    grid_size: Option<GridSize>,
    /// Fill progress counts; the progress bar renders only when both arrive.
    #[serde(default, rename = "filledCount")]
    filled_count: Option<i64>,
    /// Reserved for a correctness stat once the backend counts land.
    #[allow(dead_code)]
    #[serde(default, rename = "correctCount")]
    correct_count: Option<i64>,
    #[serde(default, rename = "totalCells")]
    total_cells: Option<i64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Continue,
    Featured,
    Library,
    Progress,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Continue => "Continue",
            Panel::Featured => "Featured",
            Panel::Library => "Library",
            Panel::Progress => "Progress",
        }
    }
}

/// First-mount geometry, computed as viewport proportions like `home.rs` (a
/// saved layout always wins; panel-kit re-scales floating panels on resize).
/// Vec order is also the mobile stacking order:
/// Continue → Featured → Library → Progress.
fn default_layout() -> Vec<PanelWin<Panel>> {
    let (vw, vh) = web_sys::window()
        .and_then(|w| {
            Some((
                w.inner_width().ok()?.as_f64()?,
                w.inner_height().ok()?.as_f64()?,
            ))
        })
        .unwrap_or((1440.0, 900.0));
    const M: f64 = 16.0;
    const GAP: f64 = 16.0;
    let side_w = (vw * 0.24).clamp(300.0, 460.0);
    let prog_w = (vw * 0.16).clamp(240.0, 340.0);
    let lib_w = (vw - 2.0 * M - 2.0 * GAP - side_w - prog_w).max(380.0);
    let h = (vh * 0.78).clamp(480.0, 1040.0);
    let cont_h = (h * 0.56).max(280.0);
    let feat_h = (h - cont_h - GAP).max(200.0);
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Continue, M, M, side_w, cont_h),
        b.at(Panel::Featured, M, M + cont_h + GAP, side_w, feat_h),
        b.at(Panel::Library, M + side_w + GAP, M, lib_w, h),
        b.at(
            Panel::Progress,
            M + side_w + 2.0 * GAP + lib_w,
            M,
            prog_w,
            h,
        ),
    ]
}

/// Route a row to the right destination — the only place that mapping lives.
fn route_for(status: GameStatus, id: String) -> Route {
    match status {
        GameStatus::New => Route::GameNew { id },
        GameStatus::Active => Route::GamePlay { id },
        GameStatus::Solved => Route::GameCompleted { id },
    }
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

/// Join the non-empty parts of a meta line with the app's separator.
fn meta_line(parts: [Option<String>; 3]) -> String {
    parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// Library tab filter. `query`/`from_query` are the `?f=` wire values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    New,
    Active,
    Solved,
}

impl Filter {
    const TABS: [Filter; 4] = [Filter::All, Filter::New, Filter::Active, Filter::Solved];

    fn index(self) -> usize {
        Self::TABS.iter().position(|f| *f == self).unwrap_or(0)
    }

    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::New => "New",
            Filter::Active => "In Progress",
            Filter::Solved => "Solved",
        }
    }

    fn query(self) -> Option<&'static str> {
        match self {
            Filter::All => None,
            Filter::New => Some("new"),
            Filter::Active => Some("active"),
            Filter::Solved => Some("solved"),
        }
    }

    fn from_query(s: &str) -> Filter {
        match s {
            "new" => Filter::New,
            "active" => Filter::Active,
            "solved" => Filter::Solved,
            _ => Filter::All,
        }
    }

    fn admits(self, st: GameStatus) -> bool {
        match self {
            Filter::All => true,
            Filter::New => st == GameStatus::New,
            Filter::Active => st == GameStatus::Active,
            Filter::Solved => st == GameStatus::Solved,
        }
    }

    fn empty_msg(self) -> &'static str {
        match self {
            Filter::All => "No puzzles yet. New puzzles drop weekly.",
            Filter::New => "You've started everything. Impressive.",
            Filter::Active => "Nothing in progress. Pick something new.",
            Filter::Solved => "No wins yet — finish a puzzle and it lands here.",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Newest,
    Oldest,
    Players,
    Score,
}

impl Sort {
    fn value(self) -> &'static str {
        match self {
            Sort::Newest => "newest",
            Sort::Oldest => "oldest",
            Sort::Players => "players",
            Sort::Score => "score",
        }
    }

    fn from_value(s: &str) -> Sort {
        match s {
            "oldest" => Sort::Oldest,
            "players" => Sort::Players,
            "score" => Sort::Score,
            _ => Sort::Newest,
        }
    }
}

/// A library row plus the sort keys its display shape doesn't carry.
struct Projected {
    ts: f64,
    players: i64,
    /// Completed score, or -1 so unsolved rows sort below any real score.
    score: i64,
    row: GameRow,
}

/// Project every parsed item into one unified library row. Single `O(n)` pass;
/// the only place wire shape becomes display shape.
fn project(items: &[GameListItem]) -> Vec<Projected> {
    items
        .iter()
        .map(|item| match item {
            GameListItem::Game(g) => Projected {
                ts: ts(&g.at),
                players: 0,
                score: -1,
                row: GameRow {
                    id: g.id.clone(),
                    title: g.title.clone(),
                    status: GameStatus::New,
                    clues: g.clues,
                    size: g.grid_size.as_ref().map(|s| (s.w, s.h)),
                    when: age(&g.at),
                    score: None,
                    progress: None,
                },
            },
            GameListItem::ActiveGame(g) => Projected {
                ts: ts(&g.at),
                players: g.players,
                score: -1,
                row: GameRow {
                    id: g.id.clone(),
                    title: g.game.title.clone(),
                    status: GameStatus::Active,
                    clues: g.clues,
                    size: g.grid_size.as_ref().map(|s| (s.w, s.h)),
                    when: age(&g.at),
                    score: None,
                    progress: g.filled_count.zip(g.total_cells),
                },
            },
            GameListItem::CompletedGame(g) => Projected {
                ts: ts(&g.at),
                players: g.players,
                score: g.score.map(i64::from).unwrap_or(-1),
                row: GameRow {
                    id: g.id.clone(),
                    title: g.game.title.clone(),
                    status: GameStatus::Solved,
                    clues: g.clues,
                    size: g.grid_size.as_ref().map(|s| (s.w, s.h)),
                    when: age(&g.at),
                    score: g.score,
                    progress: None,
                },
            },
        })
        .collect()
}

/// Stable sort (ties keep server order) on the projected sort keys.
fn sort_rows(rows: &mut [Projected], sort: Sort) {
    use std::cmp::Ordering;
    let by_ts_desc =
        |a: &Projected, b: &Projected| b.ts.partial_cmp(&a.ts).unwrap_or(Ordering::Equal);
    rows.sort_by(|a, b| match sort {
        Sort::Newest => by_ts_desc(a, b),
        Sort::Oldest => a.ts.partial_cmp(&b.ts).unwrap_or(Ordering::Equal),
        Sort::Players => b.players.cmp(&a.players).then_with(|| by_ts_desc(a, b)),
        Sort::Score => b.score.cmp(&a.score).then_with(|| by_ts_desc(a, b)),
    });
}

/// Newest unstarted game by `at` (strict `>`, so with absent stamps the first
/// row in server order wins).
fn featured_of(items: &[GameListItem]) -> Option<&UnstartedGame> {
    let mut best: Option<&UnstartedGame> = None;
    for item in items {
        if let GameListItem::Game(g) = item {
            match best {
                Some(b) if ts(&g.at) <= ts(&b.at) => {}
                _ => best = Some(g),
            }
        }
    }
    best
}

/// Active/completed rows back-referencing `game_id` (the not-yet-landed
/// `gameId` field — both stay None against an older server).
fn played_match<'a>(
    items: &'a [GameListItem],
    game_id: &str,
) -> (Option<&'a PlayedGame>, Option<&'a PlayedGame>) {
    let mut active = None;
    let mut completed = None;
    for item in items {
        match item {
            GameListItem::ActiveGame(g) if g.game_id.as_deref() == Some(game_id) => {
                active.get_or_insert(g);
            }
            GameListItem::CompletedGame(g) if g.game_id.as_deref() == Some(game_id) => {
                completed.get_or_insert(g);
            }
            _ => {}
        }
    }
    (active, completed)
}

/// Pull the active-game id out of a pasted `/game/{id}` invite link (absolute
/// URL or relative path). None when the text doesn't contain one.
fn parse_invite(text: &str) -> Option<String> {
    let (_, rest) = text.split_once("/game/")?;
    let id: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

/// `?name=` from the current URL (tiny hand parser; web_sys UrlSearchParams
/// isn't in the enabled feature set — same approach as reset_password).
fn query_param(name: &str) -> Option<String> {
    let qs = web_sys::window()?.location().search().ok()?;
    qs.trim_start_matches('?').split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name && !v.is_empty()).then(|| v.to_string())
    })
}

/// Decode a query value (`+` = space, `%xx` via the JS decoder).
fn decode_query(v: &str) -> String {
    let plus = v.replace('+', " ");
    js_sys::decode_uri_component(&plus)
        .map(String::from)
        .unwrap_or(plus)
}

/// Reflect filter + search into the address bar (`/games?f=active&q=nasa`)
/// without a router navigation. `History` isn't in the web-sys feature set,
/// so reach `history.replaceState` through js_sys reflection.
fn write_query(filter: Filter, q: &str) {
    let mut parts: Vec<String> = Vec::new();
    if let Some(f) = filter.query() {
        parts.push(format!("f={f}"));
    }
    if !q.is_empty() {
        parts.push(format!(
            "q={}",
            String::from(js_sys::encode_uri_component(q))
        ));
    }
    let url = if parts.is_empty() {
        "/games".to_string()
    } else {
        format!("/games?{}", parts.join("&"))
    };
    let Some(win) = web_sys::window() else { return };
    let Ok(history) = js_sys::Reflect::get(&win, &wasm_bindgen::JsValue::from_str("history"))
    else {
        return;
    };
    let Ok(replace) =
        js_sys::Reflect::get(&history, &wasm_bindgen::JsValue::from_str("replaceState"))
    else {
        return;
    };
    let Some(replace) = replace.dyn_ref::<js_sys::Function>() else {
        return;
    };
    let _ = replace.call3(
        &history,
        &wasm_bindgen::JsValue::NULL,
        &wasm_bindgen::JsValue::from_str(""),
        &wasm_bindgen::JsValue::from_str(&url),
    );
}

/// Signed-out body, per panel — Continue/Progress track *your* games, so each
/// gets its own prompt, all with a Login link.
fn signed_out(kind: Panel) -> Element {
    let msg = match kind {
        Panel::Continue => "Sign in to track your games",
        Panel::Featured => "Sign in to play this week's puzzle.",
        Panel::Library => "Sign in to see your games.",
        Panel::Progress => "Sign in to see your progress.",
    };
    rsx! {
        div { class: "game-status muted",
            div { "{msg}" }
            Link { to: Route::Login {}, class: "app-btn", style: "margin-top: .75rem;", "Sign in" }
        }
    }
}

#[component]
pub fn Games() -> Element {
    let state = use_app_state();

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
    // ready yet" (loading or error) — the panel body reads the resource for the
    // exact status. Keeping the projection here also keeps all hooks out of the
    // `body` closure, which panel-kit calls once per panel.
    let items = use_memo(move || match &*games_res.read() {
        Some(Some(Ok(raw))) => Some(
            raw.iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect::<Vec<GameListItem>>(),
        ),
        _ => None,
    });

    // Library filter state, seeded from the URL (`/games?f=active&q=nasa`).
    let mut filter = use_signal(|| Filter::from_query(query_param("f").as_deref().unwrap_or("")));
    let mut search = use_signal(|| {
        query_param("q")
            .map(|v| decode_query(&v))
            .unwrap_or_default()
    });
    let mut sort = use_signal(|| Sort::Newest);

    // Paste-to-join state (Continue panel footer).
    let mut join_text = use_signal(String::new);
    let mut join_err = use_signal(|| Option::<String>::None);
    let mut join_busy = use_signal(|| false);

    let nav = use_navigator();

    // Reflect tab/search into the query string on every change (replaceState —
    // shareable URLs without history spam or router churn).
    use_effect(move || {
        write_query(filter(), search.read().trim());
    });

    // "_v2": the panel set changed shape — don't let a persisted 3-panel
    // layout fight the new defaults.
    let ws = use_workspace("games_layout_v2", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        // Distinguish session-loading from signed-out: while `session` is None the
        // request is still in flight (show Loading), and only `Some(None)` is a
        // genuine signed-out state. Collapsing both to None flashed a wrong
        // "Sign in" message on every load.
        match &*state.session.read() {
            None => return status("muted", "Loading…", true),
            Some(None) => return signed_out(kind),
            Some(Some(_)) => {}
        }

        let items_v = items();
        let fetch_err = match &*games_res.read_unchecked() {
            Some(Some(Err(e))) => Some(e.clone()),
            _ => None,
        };

        match kind {
            Panel::Continue => {
                let join_submit = move |e: Event<FormData>| {
                    e.prevent_default();
                    if join_busy() {
                        return;
                    }
                    let text = join_text.read().trim().to_string();
                    let Some(id) = parse_invite(&text) else {
                        join_err.set(Some(
                            "That doesn't look like an invite link — expected …/game/<id>."
                                .to_string(),
                        ));
                        return;
                    };
                    join_err.set(None);
                    join_busy.set(true);
                    spawn_local(async move {
                        let res =
                            net::mutation("activeGame.join", Some(json!({ "id": id.clone() })))
                                .await;
                        join_busy.set(false);
                        match res {
                            Ok(_) => {
                                nav.push(Route::GamePlay { id });
                            }
                            Err(e) => state.toast(
                                Severity::Error,
                                format!("Couldn't join that game: {}", net::trpc_err_msg(e)),
                            ),
                        }
                    });
                };

                let (count, cards) = if let Some(items) = &items_v {
                    let mut actives: Vec<&PlayedGame> = items
                        .iter()
                        .filter_map(|i| match i {
                            GameListItem::ActiveGame(g) => Some(g),
                            _ => None,
                        })
                        .collect();
                    actives.sort_by(|a, b| {
                        ts(&b.at)
                            .partial_cmp(&ts(&a.at))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    let n = actives.len();
                    let cards = if actives.is_empty() {
                        status("muted", "Nothing in progress — pick one below.", false)
                    } else {
                        rsx! {
                            for g in actives.into_iter().take(3) {
                                {
                                    let id = g.id.clone();
                                    let key_id = id.clone();
                                    let pct = progress_pct(g.filled_count, g.total_cells);
                                    let meta = meta_line([
                                        (g.clues > 0).then(|| plural(g.clues, "clue")),
                                        (g.players > 0).then(|| plural(g.players, "player")),
                                        age(&g.at),
                                    ]);
                                    rsx! {
                                        div {
                                            key: "{g.id}",
                                            class: "games-continue-card",
                                            style: "cursor: pointer;",
                                            role: "button",
                                            tabindex: "0",
                                            aria_label: "Resume {g.game.title}",
                                            onclick: move |_| {
                                                nav.push(Route::GamePlay { id: id.clone() });
                                            },
                                            onkeydown: move |e| {
                                                let k = e.key();
                                                if k == Key::Enter || k == Key::Character(" ".into()) {
                                                    e.prevent_default();
                                                    nav.push(Route::GamePlay { id: key_id.clone() });
                                                }
                                            },
                                            span { class: "game-row-title", "{g.game.title}" }
                                            if !meta.is_empty() {
                                                span { class: "games-card-meta", "{meta}" }
                                            }
                                            if let Some(p) = pct {
                                                {progress_bar(p)}
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    };
                    (Some(n), cards)
                } else if fetch_err.is_some() {
                    (None, status("muted", "Couldn't load your games.", false))
                } else {
                    (None, status("muted", "Loading…", true))
                };

                rsx! {
                    div { class: "games-continue",
                        div { class: "games-head",
                            span { class: "games-eyebrow", "CONTINUE PLAYING" }
                            if let Some(n) = count {
                                span { class: "games-chip", "{n}" }
                            }
                            if count.unwrap_or(0) > 0 {
                                button {
                                    class: "games-link",
                                    onclick: move |_| filter.set(Filter::Active),
                                    "Show all →"
                                }
                            }
                        }
                        div { class: "games-continue-cards", {cards} }
                        form { class: "games-join", onsubmit: join_submit,
                            input {
                                class: "app-input",
                                placeholder: "Paste an invite link…",
                                aria_label: "Paste an invite link",
                                value: join_text.read().clone(),
                                oninput: move |e| join_text.set(e.value()),
                            }
                            button { class: "app-btn", r#type: "submit", disabled: join_busy(), "Join" }
                        }
                        if let Some(err) = join_err() {
                            p { class: "games-join-err", role: "alert", "{err}" }
                        }
                    }
                }
            }

            Panel::Featured => {
                let Some(items) = &items_v else {
                    // Non-critical panel: hide content on fetch error.
                    return if fetch_err.is_some() {
                        rsx! {}
                    } else {
                        status("muted", "Loading…", true)
                    };
                };
                let Some(g) = featured_of(items) else {
                    return status("muted", "You've started everything.", false);
                };
                let (active, completed) = played_match(items, &g.id);
                let (badge, badge_style, cta, dest) = if let Some(a) = active {
                    (
                        Some("IN PROGRESS"),
                        "border-color: var(--pastel-green); color: var(--pastel-green);",
                        "Continue →",
                        Route::GamePlay { id: a.id.clone() },
                    )
                } else if let Some(c) = completed {
                    (
                        Some("SOLVED"),
                        "border-color: var(--pastel-yellow); color: var(--pastel-yellow);",
                        "Review →",
                        Route::GameCompleted { id: c.id.clone() },
                    )
                } else {
                    (None, "", "Play →", Route::GameNew { id: g.id.clone() })
                };
                rsx! {
                    div { class: "games-featured",
                        div { class: "games-head",
                            span { class: "games-eyebrow", "THIS WEEK'S PUZZLE" }
                            if let Some(b) = badge {
                                span { class: "games-chip", style: "{badge_style}", "{b}" }
                            }
                        }
                        h2 { class: "games-featured-title", "{g.title}" }
                        if g.clues > 0 {
                            p { class: "games-featured-meta", {plural(g.clues, "clue")} }
                        }
                        Link { to: dest, class: "app-btn app-btn-active games-featured-cta", "{cta}" }
                    }
                }
            }

            Panel::Library => {
                let Some(items) = &items_v else {
                    let err = fetch_err;
                    return match err {
                        Some(msg) => error_status(&msg, move |_| games_res.restart()),
                        None => status("muted", "Loading…", true),
                    };
                };

                let f = filter();
                let q_raw = search.read().trim().to_string();
                let needle = q_raw.to_lowercase();

                let mut projected = project(items);
                projected.retain(|p| f.admits(p.row.status));
                if !needle.is_empty() {
                    projected.retain(|p| p.row.title.to_lowercase().contains(&needle));
                }
                sort_rows(&mut projected, sort());
                let rows: Vec<GameRow> = projected.into_iter().map(|p| p.row).collect();
                let count = rows.len();

                rsx! {
                    div { class: "games-lib",
                        div { class: "games-toolbar",
                            SectionTabs {
                                tabs: Filter::TABS.iter().map(|t| t.label().to_string()).collect::<Vec<_>>(),
                                active: f.index(),
                                on_select: move |i: usize| filter.set(Filter::TABS[i]),
                            }
                            select {
                                class: "app-input games-sort",
                                aria_label: "Sort games",
                                value: "{sort().value()}",
                                onchange: move |e| sort.set(Sort::from_value(&e.value())),
                                option { value: "newest", "Newest" }
                                option { value: "oldest", "Oldest" }
                                option { value: "players", "Most players" }
                                option { value: "score", "Highest score" }
                            }
                            div { class: "games-search",
                                input {
                                    class: "app-input",
                                    r#type: "search",
                                    placeholder: "Search games…",
                                    aria_label: "Search games",
                                    value: search.read().clone(),
                                    oninput: move |e| search.set(e.value()),
                                }
                                span { class: "games-chip", "{count}" }
                            }
                        }
                        if rows.is_empty() {
                            if !q_raw.is_empty() {
                                div { class: "game-status muted",
                                    div { "No games match \"{q_raw}\"." }
                                    button {
                                        class: "app-btn",
                                        style: "margin-top: .75rem;",
                                        onclick: move |_| search.set(String::new()),
                                        "Clear"
                                    }
                                }
                            } else {
                                {status("muted", f.empty_msg(), false)}
                            }
                        } else {
                            {game_list(rows, move |st, id| { nav.push(route_for(st, id)); })}
                        }
                    }
                }
            }

            Panel::Progress => {
                let Some(items) = &items_v else {
                    // Non-critical panel: hide content on fetch error.
                    return if fetch_err.is_some() {
                        rsx! {}
                    } else {
                        status("muted", "Loading…", true)
                    };
                };
                let mut solved = 0i64;
                let mut inprog = 0i64;
                let mut avail = 0i64;
                for item in items {
                    match item {
                        GameListItem::Game(_) => avail += 1,
                        GameListItem::ActiveGame(_) => inprog += 1,
                        GameListItem::CompletedGame(_) => solved += 1,
                    }
                }
                let total = solved + inprog + avail;
                let pct = if total > 0 { solved * 100 / total } else { 0 };
                rsx! {
                    div { class: "games-progress",
                        StatTile {
                            label: "Solved".to_string(),
                            value: format!("{solved} / {total}"),
                            sub: format!("{pct}% of your library"),
                        }
                        StatTile { label: "In progress".to_string(), value: inprog.to_string() }
                        StatTile { label: "Available".to_string(), value: avail.to_string() }
                        Link { to: Route::Stats {}, class: "games-link games-progress-foot", "Full stats →" }
                    }
                }
            }
        }
    };

    rsx! {
        style { {GAME_LIST_CSS} }
        style { {GAMES_CSS} }
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

/// Page-level classes for the four panels. Square corners throughout.
const GAMES_CSS: &str = "
.games-head { display: flex; align-items: center; gap: .5rem; padding: .75rem 1rem .5rem; }
.games-eyebrow { font-family: var(--mono, monospace); font-size: var(--fs-2xs); font-weight: 700;
  letter-spacing: .08em; text-transform: uppercase; color: var(--text-secondary); }
.games-chip { font-family: var(--mono, monospace); font-size: var(--fs-2xs); font-weight: 700;
  color: var(--text-secondary); border: 1px solid var(--border-app); padding: .125rem .375rem;
  flex-shrink: 0; }
.games-link { margin-left: auto; background: none; border: none; cursor: pointer; padding: 0;
  font-family: var(--mono, monospace); font-size: var(--fs-2xs); font-weight: 700;
  color: var(--text-secondary); }
.games-link:hover { color: var(--text-primary); }

.games-continue { height: 100%; display: flex; flex-direction: column;
  border-left: 2px solid var(--pastel-green); }
.games-continue-cards { flex: 1 1 auto; overflow-y: auto; }
.games-continue-card { display: flex; flex-direction: column; gap: .35rem;
  padding: .875rem 1rem; min-height: 2.75rem; border-bottom: 1px solid var(--border-app);
  border-left: 2px solid transparent;
  transition: background-color .12s ease, border-color .12s ease; }
.games-continue-card:hover, .games-continue-card:focus-visible { outline: none;
  background: var(--bg-cell-letter); border-left-color: var(--pastel-green); }
.games-card-meta { font-family: var(--mono, monospace); font-size: var(--fs-2xs);
  color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.games-join { display: flex; gap: .5rem; padding: .75rem 1rem;
  border-top: 1px solid var(--border-app); }
.games-join .app-input { flex: 1 1 auto; min-width: 0;
  font-family: var(--mono, monospace); font-size: var(--fs-xs); }
.games-join-err { margin: 0; padding: 0 1rem .75rem; font-size: var(--fs-2xs);
  font-family: var(--mono, monospace); color: var(--color-error); }

.games-featured { height: 100%; display: flex; flex-direction: column; gap: .5rem;
  padding-bottom: 1rem; }
.games-featured-title { margin: 0; padding: 0 1rem; font-family: var(--mono, monospace);
  font-size: var(--fs-xl); font-weight: 800; letter-spacing: -.01em;
  color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; }
.games-featured-meta { margin: 0; padding: 0 1rem; font-family: var(--mono, monospace);
  font-size: var(--fs-2xs); color: var(--text-secondary); }
.games-featured-cta { margin: .5rem 1rem 0; padding: .75rem 1.25rem; font-weight: 700;
  text-align: center; }

.games-lib { display: flex; flex-direction: column; height: 100%; }
.games-toolbar { display: flex; flex-wrap: wrap; align-items: center; gap: .5rem;
  padding: .625rem .75rem; border-bottom: 1px solid var(--border-app);
  position: sticky; top: 0; background: var(--bg-card); z-index: 1; }
.games-sort { width: auto; font-family: var(--mono, monospace); font-size: var(--fs-xs);
  padding: .35rem .5rem; }
.games-search { display: flex; align-items: center; gap: .5rem; margin-left: auto;
  flex: 1 1 12rem; max-width: 18rem; min-width: 0; }
.games-search .app-input { flex: 1 1 auto; min-width: 0;
  font-family: var(--mono, monospace); font-size: var(--fs-xs); }

.games-progress { display: flex; flex-direction: column; gap: .75rem; padding: .75rem 1rem; }
.games-progress-foot { margin-left: 0; align-self: flex-start; }

@media (max-width: 760px) {
  /* Tap targets ≥44px, search takes the full row, no hover-stick. */
  .games-search { max-width: none; }
  .games-continue-card { padding: 1rem; min-height: 2.75rem; }
  .games-continue-card:hover { background: transparent; border-left-color: transparent; }
  .games-join .app-input, .games-join .app-btn { min-height: 2.75rem; }
  .games-featured-cta { padding: .875rem 1.25rem; }
}
";

#[cfg(test)]
mod tests {
    use super::*;

    /// Old-server shape: none of the new fields present.
    fn old_fixture() -> Vec<GameListItem> {
        serde_json::from_value(json!([
            { "type": "Game", "id": "g1", "title": "Monday Mini", "clues": 1 },
            { "type": "ActiveGame", "id": "a1", "game": { "title": "Co-op Cryptic" }, "clues": 12, "players": 2 },
            { "type": "CompletedGame", "id": "c1", "game": { "title": "Sunday Giant" }, "clues": 40, "players": 1, "score": 340 },
        ]))
        .unwrap()
    }

    /// New-server shape: gameId / gridSize / fill counts present.
    fn new_fixture() -> Vec<GameListItem> {
        serde_json::from_value(json!([
            { "type": "Game", "id": "g1", "title": "Monday Mini", "clues": 8,
              "gridSize": { "w": 5, "h": 5 } },
            { "type": "Game", "id": "g2", "title": "Tuesday Twister", "clues": 20 },
            { "type": "ActiveGame", "id": "a1", "gameId": "g2",
              "game": { "title": "Tuesday Twister" }, "clues": 20, "players": 2,
              "gridSize": { "w": 15, "h": 15 }, "filledCount": 45, "correctCount": 40,
              "totalCells": 225 },
            { "type": "CompletedGame", "id": "c1", "gameId": "g1",
              "game": { "title": "Monday Mini" }, "clues": 8, "players": 1, "score": 340 },
        ]))
        .unwrap()
    }

    #[test]
    fn old_server_rows_still_deserialize_and_project() {
        let rows = project(&old_fixture());
        assert_eq!(rows.len(), 3);
        for p in &rows {
            assert_eq!(p.row.size, None);
            assert_eq!(p.row.progress, None);
        }
        assert_eq!(rows[0].row.status, GameStatus::New);
        assert_eq!(rows[1].row.status, GameStatus::Active);
        assert_eq!(rows[2].row.status, GameStatus::Solved);
        assert_eq!(rows[2].row.score, Some(340));
    }

    #[test]
    fn new_fields_flow_into_rows_when_present() {
        let rows = project(&new_fixture());
        assert_eq!(rows[0].row.size, Some((5, 5)));
        assert_eq!(rows[1].row.size, None);
        assert_eq!(rows[2].row.progress, Some((45, 225)));
        assert_eq!(rows[3].row.progress, None); // solved rows never carry a bar
    }

    #[test]
    fn filter_admits_by_status_and_query_roundtrips() {
        assert!(Filter::All.admits(GameStatus::Solved));
        assert!(Filter::New.admits(GameStatus::New));
        assert!(!Filter::New.admits(GameStatus::Active));
        assert!(Filter::Active.admits(GameStatus::Active));
        assert!(Filter::Solved.admits(GameStatus::Solved));
        for f in Filter::TABS {
            assert_eq!(Filter::from_query(f.query().unwrap_or("")), f);
        }
    }

    #[test]
    fn sorts_by_players_and_score_with_stable_ties() {
        let mut rows = project(&new_fixture());
        sort_rows(&mut rows, Sort::Players);
        assert_eq!(rows[0].row.id, "a1"); // 2 players first
        sort_rows(&mut rows, Sort::Score);
        assert_eq!(rows[0].row.id, "c1"); // only scored row first
    }

    #[test]
    fn featured_is_first_unstarted_and_matches_played_rows() {
        let items = new_fixture();
        // No timestamps → first Game in server order.
        let g = featured_of(&items).unwrap();
        assert_eq!(g.id, "g1");
        // g1 was completed (c1 back-references it), g2 is active (a1).
        let (active, completed) = played_match(&items, "g1");
        assert!(active.is_none());
        assert_eq!(completed.unwrap().id, "c1");
        let (active, completed) = played_match(&items, "g2");
        assert_eq!(active.unwrap().id, "a1");
        assert!(completed.is_none());
    }

    #[test]
    fn parse_invite_extracts_the_id_or_rejects() {
        assert_eq!(
            parse_invite("https://example.com/game/abc-123?x=1"),
            Some("abc-123".to_string())
        );
        assert_eq!(parse_invite("/game/uuid_9"), Some("uuid_9".to_string()));
        assert_eq!(parse_invite("not a link"), None);
        assert_eq!(parse_invite("/game/"), None);
    }

    #[test]
    fn progress_pct_gates_on_real_counts() {
        assert_eq!(progress_pct(Some(45), Some(225)), Some(20));
        assert_eq!(progress_pct(Some(300), Some(225)), Some(100)); // clamped
        assert_eq!(progress_pct(Some(45), None), None);
        assert_eq!(progress_pct(None, Some(225)), None);
        assert_eq!(progress_pct(Some(1), Some(0)), None);
    }
}
