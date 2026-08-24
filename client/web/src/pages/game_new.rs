use std::collections::HashSet;

use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crossword_core::fmt::plural;

use crate::components::game_list::status;
use crate::components::generation_progress::{GenerationProgress, Progress};
use crate::components::ui::{SectionTabs, StatTile};
use crate::net;
use crate::store::{use_app_state, AppState, Severity};
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

// ── Create-your-own: generation plumbing ─────────────────────────────────────

/// S / M / L grid sizes for the segmented size control.
const GEN_SIZES: [(i64, i64); 3] = [(15, 15), (21, 21), (25, 25)];

/// Fixed solver knobs — same defaults the admin generator uses.
const GEN_RUNS: i64 = 20;
const GEN_MAX_ATTEMPTS: i64 = 180;

/// Substring of the server's quota-exhaustion error:
/// "Monthly generation limit reached — upgrade to Pro for unlimited puzzles."
const QUOTA_ERR_NEEDLE: &str = "Monthly generation limit reached";

/// The user's generation recipe. Width/height come from the S/M/L control;
/// the rest from the Advanced disclosure.
#[derive(Clone)]
struct Recipe {
    topic: String,
    width: i64,
    height: i64,
    min_word_length: i64,
    max_word_length: i64,
    target_words: i64,
}

/// Exact `generator.runGeneration` input — mirrors admin_generator.rs.
fn recipe_input(r: &Recipe) -> Value {
    json!({
        "params": {
            "topic": r.topic,
            "width": r.width,
            "height": r.height,
            "minWordLength": r.min_word_length,
            "maxWordLength": r.max_word_length,
            "targetWords": r.target_words,
            "runs": GEN_RUNS,
            "maxAttempts": GEN_MAX_ATTEMPTS,
        }
    })
}

/// All generation-run signals bundled so helpers stay call-site friendly.
/// `Signal<T>` is Copy, so this is too.
#[derive(Clone, Copy)]
struct GenSignals {
    log: Signal<Vec<Value>>,
    progress: Signal<Option<Progress>>,
    /// "idle" | "running" | "succeeded" | "failed"
    status: Signal<String>,
    error: Signal<String>,
    elapsed: Signal<u64>,
    /// Live subscription handle kept alive in a signal; drop = unsubscribe.
    handle: Signal<Option<net::Subscription>>,
    game_id: Signal<Option<String>>,
    game_title: Signal<Option<String>>,
    publishing: Signal<bool>,
    publish_error: Signal<String>,
    /// Set when the server reports quota exhaustion mid-flight.
    quota_blocked: Signal<bool>,
    /// Last submitted recipe, for "Try again".
    last_recipe: Signal<Option<Recipe>>,
}

/// Kick off a generation run: reset state, start the 1s elapsed ticker, and
/// subscribe to the event stream (same idioms as admin_generator.rs).
fn run_generation(recipe: Recipe, mut gs: GenSignals, state: AppState) {
    // Drop any previous subscription, reset run state.
    gs.handle.set(None);
    gs.log.set(vec![]);
    gs.progress.set(None);
    gs.status.set("running".to_string());
    gs.error.set(String::new());
    gs.game_id.set(None);
    gs.game_title.set(None);
    gs.publishing.set(false);
    gs.publish_error.set(String::new());
    gs.elapsed.set(0);
    gs.last_recipe.set(Some(recipe.clone()));

    // Tick elapsed every second until the run leaves "running". Runtime-aware
    // `spawn` (never `wasm_bindgen_futures::spawn_local`): the task is owned by
    // this scope, so it is cancelled on unmount instead of ticking dropped
    // signals forever.
    spawn(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(1_000).await;
            if gs.status.read().as_str() != "running" {
                break;
            }
            let cur = *gs.elapsed.read();
            gs.elapsed.set(cur + 1);
        }
    });

    let input = recipe_input(&recipe);
    let handle = net::subscribe(
        "generator.runGeneration",
        Some(input),
        move |data: Value| {
            let etype = data["type"].as_str().unwrap_or("").to_string();
            if etype == "progress" {
                let stage = data["stage"].as_str().unwrap_or("").to_string();
                let current = data["current"].as_i64().unwrap_or(0);
                let total = data["total"].as_i64().unwrap_or(0);
                let message = data["message"].as_str().map(|s| s.to_string());
                gs.progress.set(Some(Progress {
                    stage,
                    current,
                    total,
                    message,
                }));
                return;
            }

            gs.log.write().push(data.clone());

            match etype.as_str() {
                "completed" => {
                    gs.status.set("succeeded".to_string());
                    gs.progress.set(None);
                    if let Some(gid) = data["gameId"].as_str() {
                        gs.game_id.set(Some(gid.to_string()));
                    }
                    if let Some(t) = data["title"].as_str() {
                        gs.game_title.set(Some(t.to_string()));
                    }
                    state.toast(Severity::Success, "Your puzzle is ready");
                }
                "failed" => {
                    gs.status.set("failed".to_string());
                    let err = data["error"]
                        .as_str()
                        .unwrap_or("unknown error")
                        .to_string();
                    if err.contains(QUOTA_ERR_NEEDLE) {
                        gs.quota_blocked.set(true);
                    }
                    gs.error.set(err);
                }
                _ => {}
            }
        },
    );
    gs.handle.set(Some(handle));
}

/// Success CTA: publish the generated game (creator-gated server-side), start
/// it, and enter play. Errors (e.g. permission) surface inline via
/// `publish_error`.
fn publish_and_play(game_id: String, mut gs: GenSignals, nav: Navigator) {
    // Dioxus `spawn`, NOT `wasm_bindgen_futures::spawn_local`: `nav.push` needs
    // the Dioxus runtime scope to resolve the router's history context. A raw
    // wasm task has no scope, so push panics (RuntimeError) and poisons the
    // router's inner lock.
    spawn(async move {
        gs.publishing.set(true);
        gs.publish_error.set(String::new());
        if let Err(e) = net::mutation(
            "generator.publishGeneratedGame",
            Some(json!({ "gameId": game_id })),
        )
        .await
        {
            gs.publish_error.set(net::trpc_err_msg(e));
            gs.publishing.set(false);
            return;
        }
        match net::mutation_as::<Value>("activeGame.start", Some(json!({ "gameId": game_id })))
            .await
        {
            Ok(res) => {
                if let Some(id) = res.get("id").and_then(|v| v.as_str()) {
                    nav.push(Route::GamePlay { id: id.to_string() });
                } else {
                    gs.publish_error
                        .set("Unexpected response from server.".into());
                }
            }
            Err(e) => gs.publish_error.set(net::trpc_err_msg(e)),
        }
        gs.publishing.set(false);
    });
}

/// Inline (never modal) upgrade pitch shown when the monthly quota is spent.
fn upgrade_card() -> Element {
    rsx! {
        div { class: "app-card", style: "padding: 1rem; display: flex; flex-direction: column; gap: .625rem; align-items: flex-start; flex-shrink: 0;",
            p { style: "font-size: .875rem; font-weight: 600; margin: 0; color: var(--text-primary);",
                "You've used all your puzzle generations this month."
            }
            p { class: "muted", style: "font-size: .75rem; margin: 0;",
                "Pro members generate unlimited puzzles."
            }
            Link { to: Route::Profile {}, class: "app-btn app-btn-active", "Upgrade to Pro" }
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

    // ── Create-your-own tab state ─────────────────────────────────────────
    // 0 = Play this puzzle (default), 1 = Create your own.
    let mut create_tab = use_signal(|| 0usize);
    let mut topic = use_signal(String::new);
    // Index into GEN_SIZES; default M · 21×21.
    let mut size_idx = use_signal(|| 1usize);
    let mut advanced_open = use_signal(|| false);
    let mut min_len = use_signal(|| 3i64);
    let mut max_len = use_signal(|| 12i64);
    let mut answers = use_signal(|| 42i64);
    let gen = GenSignals {
        log: use_signal(Vec::new),
        progress: use_signal(|| None),
        status: use_signal(|| "idle".to_string()),
        error: use_signal(String::new),
        elapsed: use_signal(|| 0u64),
        handle: use_signal(|| None),
        game_id: use_signal(|| None),
        game_title: use_signal(|| None),
        publishing: use_signal(|| false),
        publish_error: use_signal(String::new),
        quota_blocked: use_signal(|| false),
        last_recipe: use_signal(|| None),
    };

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
                let tab = *create_tab.read();
                let (grid_w, grid_h, _) = silhouette_cells(&details.questions);
                let clues = details.question_count;
                // 90s per clue, rounded up to whole minutes — hence the tilde.
                let est_min = (clues * 90 + 59) / 60;

                // ── Create-your-own pane ──────────────────────────────────
                // `None` = session still loading, `Some(None)` = signed out.
                let session = state.session.read().clone();
                let sub_now = state.sub.read().clone();
                // Omit the quota line entirely while sub status is unknown.
                let quota_line = sub_now.as_ref().map(|s| {
                    if s.is_pro || s.quota_limit.is_none() {
                        format!("{} / ∞ this month", s.quota_used)
                    } else {
                        format!(
                            "{} / {} this month",
                            s.quota_used,
                            s.quota_limit.unwrap_or(0)
                        )
                    }
                });
                let client_exhausted = sub_now.as_ref().map_or(false, |s| {
                    !s.is_pro && s.quota_limit.map_or(false, |l| s.quota_used >= l)
                });
                let exhausted = client_exhausted || *gen.quota_blocked.read();
                let gen_running = gen.status.read().as_str() == "running";
                let topic_empty = topic.read().trim().is_empty();
                let advanced = *advanced_open.read();

                let create_pane = match session {
                    None => status("muted", "Loading…", true),
                    Some(None) => rsx! {
                        div { class: "app-card", style: "padding: 1.5rem; display: flex; flex-direction: column; gap: .75rem; align-items: flex-start;",
                            p { style: "font-size: .875rem; font-weight: 600; margin: 0; color: var(--text-primary);",
                                "Sign in to create puzzles"
                            }
                            p { class: "muted", style: "font-size: .75rem; margin: 0;",
                                "Generate a custom crossword from any topic you like."
                            }
                            Link { to: Route::Login {}, class: "app-btn app-btn-active", "Sign in" }
                        }
                    },
                    Some(Some(_)) => rsx! {
                        div { style: "display: flex; flex-direction: column; gap: 1.25rem;",
                            // Eyebrow + quota line (top-right)
                            div { style: "display: flex; align-items: baseline; justify-content: space-between; gap: 1rem; flex-wrap: wrap;",
                                p {
                                    class: "muted",
                                    style: "font-size: var(--fs-2xs); font-family: var(--mono, monospace); text-transform: uppercase; letter-spacing: .1em; margin: 0;",
                                    "Create your own"
                                }
                                if let Some(q) = quota_line {
                                    span {
                                        class: "muted",
                                        style: "font-size: var(--fs-2xs); font-family: var(--mono, monospace); text-transform: uppercase; letter-spacing: .05em;",
                                        "{q}"
                                    }
                                }
                            }
                            // Topic
                            div { style: "display: flex; flex-direction: column; gap: .375rem;",
                                label {
                                    r#for: "gn-topic",
                                    class: "muted",
                                    style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em;",
                                    "Topic"
                                }
                                input {
                                    id: "gn-topic",
                                    class: "app-input",
                                    style: "padding: .75rem 1rem; font-size: 1.125rem; width: 100%;",
                                    r#type: "text",
                                    placeholder: "e.g. deep sea creatures",
                                    value: "{topic}",
                                    oninput: move |e| topic.set(e.value()),
                                }
                            }
                            // Size
                            div { style: "display: flex; flex-direction: column; gap: .375rem;",
                                span {
                                    class: "muted",
                                    style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em;",
                                    "Size"
                                }
                                SectionTabs {
                                    tabs: vec!["S · 15×15".to_string(), "M · 21×21".to_string(), "L · 25×25".to_string()],
                                    active: *size_idx.read(),
                                    on_select: move |i| size_idx.set(i),
                                }
                            }
                            // Advanced disclosure
                            button {
                                class: "app-btn",
                                style: "align-self: flex-start;",
                                onclick: move |_| {
                                    let cur = *advanced_open.peek();
                                    advanced_open.set(!cur);
                                },
                                if advanced { "Advanced ▴" } else { "Advanced ▸" }
                            }
                            if advanced {
                                div { style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); gap: .75rem;",
                                    for (lbl, value, min_v, max_v, idx) in [
                                        ("Min word len", *min_len.read(), 2i64, 50i64, 0usize),
                                        ("Max word len", *max_len.read(), 2, 50, 1),
                                        ("Answers", *answers.read(), 1, 250, 2),
                                    ] {
                                        label { class: "muted", style: "display: flex; flex-direction: column; gap: .25rem; font-size: .75rem; text-transform: uppercase; letter-spacing: .05em;",
                                            {lbl}
                                            input {
                                                class: "app-input",
                                                style: "padding: .375rem .5rem; font-size: .875rem;",
                                                r#type: "number",
                                                min: "{min_v}",
                                                max: "{max_v}",
                                                value: "{value}",
                                                oninput: move |e| {
                                                    if let Ok(n) = e.value().parse::<i64>() {
                                                        match idx {
                                                            0 => min_len.set(n),
                                                            1 => max_len.set(n),
                                                            _ => answers.set(n),
                                                        }
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                            // CTA — swapped for the upgrade card when the quota is spent
                            if exhausted {
                                {upgrade_card()}
                            } else {
                                button {
                                    class: "app-btn app-btn-active",
                                    style: "justify-content: center; font-weight: 700;",
                                    disabled: gen_running || topic_empty,
                                    onclick: move |_| {
                                        let t = topic.read().trim().to_string();
                                        if t.is_empty() {
                                            return;
                                        }
                                        let (w, h) = GEN_SIZES[(*size_idx.read()).min(GEN_SIZES.len() - 1)];
                                        let recipe = Recipe {
                                            topic: t,
                                            width: w,
                                            height: h,
                                            min_word_length: *min_len.read(),
                                            max_word_length: *max_len.read(),
                                            target_words: *answers.read(),
                                        };
                                        run_generation(recipe, gen, state);
                                    },
                                    if gen_running { "Generating…" } else { "Generate puzzle" }
                                }
                            }
                        }
                    },
                };

                rsx! {
                    div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
                        SectionTabs {
                            tabs: vec!["Play this puzzle".to_string(), "Create your own".to_string()],
                            active: tab,
                            on_select: move |i| create_tab.set(i),
                        }
                        if tab == 0 {
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
                        } else {
                            {create_pane}
                        }
                    }
                }
            }
            // Create-your-own: the Start panel becomes the generation stage.
            (Some(Ok(_)), Panel::Start) if *create_tab.read() == 1 => {
                let status_now = gen.status.read().clone();
                if status_now == "idle" {
                    rsx! {
                        div { class: "muted", style: "font-size: .875rem; text-align: center; padding: 2rem 0;",
                            "Your puzzle will build here."
                        }
                    }
                } else {
                    let is_running = status_now == "running";
                    let failed = status_now == "failed";
                    let quota_blocked = *gen.quota_blocked.read();
                    // Title reveal only once the completed event delivered both.
                    let game = if status_now == "succeeded" {
                        let gid = gen.game_id.read().clone();
                        let gtitle = gen.game_title.read().clone();
                        gid.zip(gtitle)
                    } else {
                        None
                    };
                    rsx! {
                        div { style: "display: flex; flex-direction: column; gap: 1rem; height: 100%;",
                            // Log/progress: the shared GenerationProgress component,
                            // wrapped so the feed takes the remaining panel height.
                            div { style: "flex: 1 1 auto; min-height: 0;",
                                GenerationProgress {
                                    log: gen.log.read().clone(),
                                    progress: gen.progress.read().clone(),
                                    running: is_running,
                                    status: status_now.clone(),
                                    elapsed_secs: *gen.elapsed.read(),
                                }
                            }
                            if failed {
                                if quota_blocked {
                                    // Server-side quota exhaustion — same inline
                                    // upgrade card as the recipe CTA, never a modal.
                                    {upgrade_card()}
                                } else {
                                    div { class: "app-card", style: "padding: 1rem; display: flex; flex-direction: column; gap: .75rem; border-color: var(--color-error); flex-shrink: 0;",
                                        p { class: "error", style: "font-size: .875rem; margin: 0;", "{gen.error}" }
                                        button {
                                            class: "app-btn",
                                            style: "justify-content: center;",
                                            onclick: move |_| {
                                                // Clone out first: run_generation writes
                                                // last_recipe, so no read guard may be
                                                // held across the call.
                                                let recipe = gen.last_recipe.read().clone();
                                                if let Some(r) = recipe {
                                                    run_generation(r, gen, state);
                                                }
                                            },
                                            "Try again"
                                        }
                                    }
                                }
                            }
                            if let Some((gid, gtitle)) = game {
                                div { class: "app-card", style: "padding: 1rem; display: flex; flex-direction: column; gap: .75rem; border-color: var(--color-success); flex-shrink: 0;",
                                    p {
                                        class: "muted",
                                        style: "font-size: var(--fs-2xs); font-family: var(--mono, monospace); text-transform: uppercase; letter-spacing: .1em; margin: 0;",
                                        "Your puzzle is ready"
                                    }
                                    p { style: "font-size: 1.125rem; font-weight: 700; margin: 0; color: var(--text-primary);",
                                        "{gtitle}"
                                    }
                                    if !gen.publish_error.read().is_empty() {
                                        p { class: "error", style: "font-size: .75rem; margin: 0;", "{gen.publish_error}" }
                                    }
                                    button {
                                        class: "app-btn app-btn-active",
                                        style: "justify-content: center; font-weight: 700;",
                                        disabled: *gen.publishing.read(),
                                        onclick: move |_| publish_and_play(gid.clone(), gen, nav),
                                        if *gen.publishing.read() { "Starting…" } else { "Play it now" }
                                    }
                                }
                            }
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
                    // Dioxus `spawn` (runtime-aware), NOT spawn_local: the
                    // `nav.push` below must run inside the runtime scope or the
                    // router can't find its history context and panics.
                    spawn(async move {
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
                    spawn(async move {
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
