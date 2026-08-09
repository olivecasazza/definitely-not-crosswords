use std::collections::HashSet;

use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crossword_core::fmt::plural;

use crate::components::game_list::status;
use crate::components::ui::StatTile;
use crate::net;
use crate::store::{use_app_state, Severity};
use crate::Route;

/// One question of the pre-start grid silhouette: coordinates and answer
/// *length* only — the server deliberately never sends answer/question text
/// before the game starts. `direction` is the wire format "ACROSS"/"DOWN".
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SilhouetteQ {
    root_x: i32,
    root_y: i32,
    direction: String,
    len: i32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartDetails {
    id: String,
    title: String,
    source: String,
    question_count: i64,
    grid_size: i64,
    /// `default` so older servers that don't send the silhouette still parse.
    #[serde(default)]
    questions: Vec<SilhouetteQ>,
    active_game_id: Option<String>,
    completed_game_id: Option<String>,
}

/// Union of covered cells plus derived grid extent (max occupied coordinate
/// + 1 per axis).
fn silhouette_cells(qs: &[SilhouetteQ]) -> (i32, i32, Vec<(i32, i32)>) {
    let mut cells: HashSet<(i32, i32)> = HashSet::new();
    for q in qs {
        for i in 0..q.len.max(0) {
            let (x, y) = if q.direction == "ACROSS" {
                (q.root_x + i, q.root_y)
            } else {
                (q.root_x, q.root_y + i)
            };
            if x >= 0 && y >= 0 {
                cells.insert((x, y));
            }
        }
    }
    let w = cells.iter().map(|c| c.0).max().map_or(0, |m| m + 1);
    let h = cells.iter().map(|c| c.1).max().map_or(0, |m| m + 1);
    let mut cells: Vec<_> = cells.into_iter().collect();
    cells.sort_unstable(); // HashSet order is random; keep renders stable
    (w, h, cells)
}

/// Blank grid shape as a dot-matrix of squares. Pure presentational.
#[component]
fn Silhouette(questions: Vec<SilhouetteQ>) -> Element {
    let (w, h, cells) = silhouette_cells(&questions);
    if w == 0 || h == 0 {
        return rsx! {};
    }
    // 10 viewBox units per cell, 9-unit squares → a 1-unit gap, centred.
    let view_box = format!("0 0 {} {}", w * 10, h * 10);
    // Natural size scales with the grid but never exceeds the container.
    let natural_px = w * 22;
    let rects: Vec<(String, String)> = cells
        .into_iter()
        .map(|(x, y)| (format!("{}.5", x * 10), format!("{}.5", y * 10)))
        .collect();
    rsx! {
        svg {
            view_box: "{view_box}",
            role: "img",
            "aria-label": "Puzzle grid silhouette",
            style: "display: block; width: {natural_px}px; max-width: 100%; height: auto;",
            for (x, y) in rects {
                rect {
                    x: "{x}",
                    y: "{y}",
                    width: "9",
                    height: "9",
                    fill: "var(--bg-cell-letter)",
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Puzzle,
    Start,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Puzzle => "Puzzle",
            Panel::Start => "Start",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Puzzle, 16.0, 16.0, 1240.0, 948.0),
        b.at(Panel::Start, 1272.0, 16.0, 632.0, 948.0),
    ]
}

#[component]
pub fn GameNew(id: String) -> Element {
    let details_res = {
        let id = id.clone();
        use_resource(move || {
            let id = id.clone();
            async move {
                net::query_as::<StartDetails>(
                    "activeGame.getStartDetails",
                    Some(json!({ "gameId": id })),
                )
                .await
            }
        })
    };

    let is_starting = use_signal(|| false);
    let start_error = use_signal(|| String::new());
    // Co-op invite flow: started-but-not-entered game id (None = row hidden).
    let coop_starting = use_signal(|| false);
    let coop_game_id = use_signal(|| Option::<String>::None);
    let nav = use_navigator();
    let state = use_app_state();

    let ws = use_workspace("game_new_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let mut details_res = details_res;
    let body = move |kind: Panel, _max: bool| -> Element {
        let details_snapshot = details_res.read_unchecked();
        match (&*details_snapshot, kind) {
            (None, _) => status("muted", "Loading…", true),
            (Some(Err(e)), Panel::Puzzle) => rsx! {
                div { class: "app-card", style: "padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem;",
                    h1 {
                        style: "font-size: 1.125rem; font-weight: 700; font-family: monospace; text-transform: uppercase; color: var(--pastel-red); margin: 0;",
                        "Game Unavailable"
                    }
                    p { class: "muted", style: "font-size: .875rem;", "This puzzle could not be found or is not available to start." }
                    p { class: "error", style: "font-size: .75rem; font-family: monospace;", "{e}" }
                    div { class: "row",
                        button { class: "app-btn", onclick: move |_| details_res.restart(), "Retry" }
                        Link { to: Route::Games {}, class: "app-btn", "Back to Games" }
                    }
                }
            },
            (Some(Err(_)), Panel::Start) => status("muted", "Unavailable", false),
            (Some(Ok(details)), Panel::Puzzle) => {
                let (grid_w, grid_h, _) = silhouette_cells(&details.questions);
                let clues = details.question_count;
                // 90s per clue, rounded up to whole minutes — hence the tilde.
                let est_min = (clues * 90 + 59) / 60;
                rsx! {
                    div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
                        // Header
                        div { style: "display: flex; flex-direction: row; align-items: flex-start; justify-content: space-between; gap: 1rem; flex-wrap: wrap;",
                            div {
                                p {
                                    class: "muted",
                                    style: "font-size: var(--fs-2xs); font-family: var(--mono, monospace); text-transform: uppercase; letter-spacing: .1em; margin: 0 0 .5rem 0;",
                                    "New Game"
                                }
                                h1 {
                                    style: "font-size: 1.75rem; font-weight: 700; color: var(--text-primary); margin: 0;",
                                    "{details.title}"
                                }
                            }
                            span { class: "gn-chip", "{details.source.to_lowercase()}" }
                        }
                        // Stat tiles
                        div { class: "gn-stats",
                            StatTile { label: "Clues".to_string(), value: clues.to_string() }
                            if grid_w > 0 && grid_h > 0 {
                                StatTile { label: "Grid".to_string(), value: format!("{grid_w}×{grid_h}") }
                            }
                            if clues > 0 {
                                StatTile {
                                    label: "Est. solve".to_string(),
                                    value: format!("~{est_min} min"),
                                    sub: format!("{} × 90s", plural(clues, "clue")),
                                }
                            }
                        }
                        // Grid silhouette (only when the server sent coordinates)
                        if !details.questions.is_empty() {
                            Silhouette { questions: details.questions.clone() }
                        }
                        // Play-state status line
                        if details.active_game_id.is_some() {
                            p { class: "gn-status", "You have this in progress" }
                        } else if details.completed_game_id.is_some() {
                            p { class: "gn-status", "You solved this puzzle" }
                        }
                    }
                }
            }
            (Some(Ok(details)), Panel::Start) => {
                let action_label = if details.active_game_id.is_some() {
                    "Continue Game"
                } else if details.completed_game_id.is_some() {
                    "Review Completed Game"
                } else {
                    "Start Game"
                };
                let d = details.clone();
                let id_for_start = id.clone();
                let handle_start = move || {
                    // Continue → navigate to active game
                    if let Some(active_id) = d.active_game_id.clone() {
                        nav.push(Route::GamePlay { id: active_id });
                        return;
                    }
                    // Review → navigate to completed
                    if let Some(completed_id) = d.completed_game_id.clone() {
                        nav.push(Route::GameCompleted { id: completed_id });
                        return;
                    }
                    // Fresh start
                    let id = id_for_start.clone();
                    let mut is_starting = is_starting;
                    let mut start_error = start_error;
                    let nav = nav;
                    spawn_local(async move {
                        is_starting.set(true);
                        start_error.set(String::new());
                        // POST, not GET: starting a game is a write. A GET can be
                        // cached / prefetched / retried by the browser or an edge
                        // proxy, which can make "Start" appear to hang even though
                        // the server responds fast.
                        match net::mutation_as::<serde_json::Value>(
                            "activeGame.start",
                            Some(json!({ "gameId": id })),
                        )
                        .await
                        {
                            Ok(res) => {
                                if let Some(new_id) = res.get("id").and_then(|v| v.as_str()) {
                                    nav.push(Route::GamePlay {
                                        id: new_id.to_string(),
                                    });
                                } else {
                                    start_error.set("Unexpected response from server.".into());
                                }
                            }
                            Err(e) => {
                                start_error.set(e);
                            }
                        }
                        is_starting.set(false);
                    });
                };

                // Invite: an already-active game is shareable without re-starting;
                // otherwise "Start co-op" starts it but stays here to show the link.
                let invite_id: Option<String> = coop_game_id
                    .read()
                    .clone()
                    .or_else(|| details.active_game_id.clone());
                let show_coop_button = invite_id.is_none() && details.completed_game_id.is_none();

                let id_for_coop = id.clone();
                let handle_coop = move || {
                    let id = id_for_coop.clone();
                    let mut coop_starting = coop_starting;
                    let mut coop_game_id = coop_game_id;
                    let mut start_error = start_error;
                    spawn_local(async move {
                        coop_starting.set(true);
                        start_error.set(String::new());
                        match net::mutation_as::<serde_json::Value>(
                            "activeGame.start",
                            Some(json!({ "gameId": id })),
                        )
                        .await
                        {
                            Ok(res) => {
                                if let Some(new_id) = res.get("id").and_then(|v| v.as_str()) {
                                    coop_game_id.set(Some(new_id.to_string()));
                                } else {
                                    start_error.set("Unexpected response from server.".into());
                                }
                            }
                            Err(e) => {
                                start_error.set(net::trpc_err_msg(e));
                            }
                        }
                        coop_starting.set(false);
                    });
                };

                rsx! {
                    div { style: "display: flex; flex-direction: column; gap: 1rem;",
                        if !start_error.read().is_empty() {
                            p { class: "error", style: "font-size: .875rem;", "{start_error}" }
                        }
                        button {
                            class: "app-btn app-btn-active",
                            style: "justify-content: center;",
                            disabled: *is_starting.read(),
                            onclick: move |_| handle_start(),
                            if *is_starting.read() { "Starting..." } else { "{action_label}" }
                        }
                        // Invite row: co-op start, then share the join URL.
                        if show_coop_button {
                            button {
                                class: "app-btn",
                                style: "justify-content: center;",
                                disabled: *coop_starting.read(),
                                onclick: move |_| handle_coop(),
                                if *coop_starting.read() { "Starting..." } else { "Start co-op" }
                            }
                        }
                        if let Some(gid) = invite_id {
                            {
                                let origin = web_sys::window()
                                    .and_then(|w| w.location().origin().ok())
                                    .unwrap_or_default();
                                let url = format!("{origin}/game/{gid}");
                                let url_for_copy = url.clone();
                                let state = state;
                                // Clipboard via the JS API (same idiom as game_play's
                                // invite button — no extra Rust deps).
                                let copy_link = move |_| {
                                    let script = format!(
                                        "navigator.clipboard && navigator.clipboard.writeText({})",
                                        serde_json::to_string(&url_for_copy).unwrap_or_default()
                                    );
                                    dioxus::document::eval(&script);
                                    state.toast(Severity::Success, "Link copied");
                                };
                                let gid_for_enter = gid.clone();
                                let enter_game = move |_| {
                                    nav.push(Route::GamePlay {
                                        id: gid_for_enter.clone(),
                                    });
                                };
                                rsx! {
                                    div { class: "gn-invite",
                                        p { class: "gn-invite-label muted", "Share this link to play co-op" }
                                        code { class: "gn-invite-url", "{url}" }
                                        div { style: "display: flex; flex-direction: row; gap: .5rem; flex-wrap: wrap;",
                                            button { class: "app-btn", onclick: copy_link, "Copy link" }
                                            button { class: "app-btn app-btn-active", onclick: enter_game, "Enter game →" }
                                        }
                                    }
                                }
                            }
                        }
                        // Panel footer
                        div { style: "margin-top: auto; padding-top: 1rem;",
                            Link { to: Route::Games {}, class: "app-btn", style: "justify-content: center;", "← Back to Games" }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        style { {NEW_CSS} }
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

const NEW_CSS: &str = "
.gn-chip { padding: .375rem .75rem; border: 1px solid var(--border-app); background: var(--bg-cell-empty);
  font-size: var(--fs-xs); font-family: var(--mono, monospace); text-transform: uppercase;
  letter-spacing: .05em; color: var(--text-secondary); white-space: nowrap; }
.gn-stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: .75rem; }
.gn-status { font-size: var(--fs-sm); font-family: var(--mono, monospace); text-transform: uppercase;
  letter-spacing: .05em; color: var(--text-secondary); margin: 0; }
.gn-invite { border: 1px solid var(--border-app); background: var(--bg-cell-empty); padding: .75rem;
  display: flex; flex-direction: column; gap: .5rem; }
.gn-invite-label { font-size: var(--fs-2xs); font-family: var(--mono, monospace); text-transform: uppercase;
  letter-spacing: .05em; margin: 0; }
.gn-invite-url { font-family: var(--mono, monospace); font-size: var(--fs-xs); color: var(--text-secondary);
  word-break: break-all; }
";
