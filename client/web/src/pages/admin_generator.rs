use crate::components::generation_progress::{GenerationProgress, Progress};
use crate::net::{mutation, query, subscribe, Subscription};
use crate::Route;
use crossword_core::fmt::{format_datetime, rel_time};
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use wasm_bindgen_futures::spawn_local;

// ── form state ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct GenForm {
    topic: String,
    width: i64,
    height: i64,
    min_word_length: i64,
    max_word_length: i64,
    target_words: i64,
    runs: i64,
    max_attempts: i64,
}

impl Default for GenForm {
    fn default() -> Self {
        Self {
            topic: "space exploration and planetary science".to_string(),
            width: 21,
            height: 21,
            min_word_length: 3,
            max_word_length: 12,
            target_words: 42,
            runs: 20,
            max_attempts: 180,
        }
    }
}

// ── job list row ─────────────────────────────────────────────────────────────

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobRow {
    id: String,
    status: String,
    topic: String,
    width: i64,
    height: i64,
    created_at: String,
    result_game: Option<ResultGame>,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultGame {
    id: String,
    title: String,
    published: bool,
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn form_to_json(f: &GenForm) -> Value {
    json!({
        "params": {
            "topic": f.topic,
            "width": f.width,
            "height": f.height,
            "minWordLength": f.min_word_length,
            "maxWordLength": f.max_word_length,
            "targetWords": f.target_words,
            "runs": f.runs,
            "maxAttempts": f.max_attempts,
        }
    })
}

fn do_load_jobs(
    mut jobs: Signal<Vec<JobRow>>,
    mut jobs_loading: Signal<bool>,
    mut jobs_error: Signal<String>,
    take: i64,
) {
    jobs_loading.set(true);
    jobs_error.set(String::new());
    spawn_local(async move {
        match query("generator.listJobs", Some(json!({"take": take}))).await {
            Ok(v) => {
                let parsed: Vec<JobRow> = serde_json::from_value(v).unwrap_or_default();
                jobs.set(parsed);
            }
            Err(e) => jobs_error.set(e),
        }
        jobs_loading.set(false);
    });
}

// ── panels ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Parameters,
    Progress,
    Jobs,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Parameters => "Parameters",
            Panel::Progress => "Run",
            Panel::Jobs => "Jobs",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Parameters, 16.0, 16.0, 640.0, 880.0),
        b.at(Panel::Progress, 672.0, 16.0, 1232.0, 432.0),
        b.at(Panel::Jobs, 672.0, 464.0, 1232.0, 432.0),
    ]
}

// ── component ─────────────────────────────────────────────────────────────────

#[component]
pub fn AdminGenerator() -> Element {
    let nav = use_navigator();
    let state = crate::store::use_app_state();

    let mut form = use_signal(GenForm::default);
    let mut jobs = use_signal(Vec::<JobRow>::new);
    let mut jobs_loading = use_signal(|| false);
    let mut jobs_error = use_signal(String::new);
    let mut jobs_take = use_signal(|| 25i64);
    let mut job_search = use_signal(String::new);
    let mut status_filter = use_signal(String::new);

    let mut gen_log = use_signal(Vec::<Value>::new);
    let mut gen_progress = use_signal(|| None::<Progress>);
    // "idle" | "running" | "succeeded" | "failed"
    let mut gen_status = use_signal(|| "idle".to_string());
    let mut gen_error = use_signal(String::new);

    // ponytail: elapsed clock via gloo_timers counter; no Date/js-sys needed
    let mut elapsed_secs = use_signal(|| 0u64);

    // Subscription handle kept alive in a signal; drop = unsubscribe
    let mut sub_handle = use_signal(|| None::<Subscription>);

    // Generated game from completed event (for publish CTA)
    let mut gen_game_id = use_signal(|| None::<String>);
    let mut gen_game_title = use_signal(|| None::<String>);
    let mut gen_game_published = use_signal(|| false);
    let mut publishing = use_signal(|| false);
    let mut publish_error = use_signal(String::new);

    // ── initial load ──────────────────────────────────────────────────────────

    // Reactive on `jobs_take`: bumping it (Load more) refetches with the new size.
    use_effect(move || {
        do_load_jobs(jobs, jobs_loading, jobs_error, *jobs_take.read());
    });

    // ── workspace ─────────────────────────────────────────────────────────────

    let ws = use_workspace("admin_generator_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);
    if let Some(gate) = crate::store::use_auth_guard(crossword_core::auth::Role::Admin) {
        return gate;
    }

    // Mobile (< 760px, panel-kit's own threshold): the parameters form
    // collapses behind a disclosure and Publish is not mounted.
    let mobile_ro = *ws.is_mobile.read();

    let body = move |kind: Panel, _max: bool| -> Element {
        let status = gen_status.read().clone();
        let is_running = status == "running";
        match kind {
            Panel::Parameters => {
                let params_form = rsx! {
                    // topic + submit row
                    div { class: "row", style: "flex-wrap:wrap;align-items:flex-end;gap:0.75rem",
                        div { class: "col", style: "gap:0.375rem;flex:1;min-width:280px",
                            label {
                                r#for: "topic",
                                class: "muted",
                                style: "font-size:0.75rem;font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                "Topic"
                            }
                            input {
                                id: "topic",
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem;width:100%",
                                r#type: "text",
                                value: "{form.read().topic}",
                                oninput: move |e| form.write().topic = e.value(),
                            }
                        }
                        button {
                            class: "app-btn app-btn-active",
                            style: "height:38px;min-width:120px;font-weight:bold",
                            disabled: is_running,
                            onclick: move |_| {
                                // drop previous subscription
                                sub_handle.set(None);
                                gen_log.set(vec![]);
                                gen_progress.set(None);
                                gen_status.set("running".to_string());
                                gen_error.set(String::new());
                                gen_game_id.set(None);
                                gen_game_title.set(None);
                                gen_game_published.set(false);
                                elapsed_secs.set(0);
                                publish_error.set(String::new());

                                // ponytail: tick elapsed every second until status != running
                                spawn_local(async move {
                                    loop {
                                        gloo_timers::future::TimeoutFuture::new(1_000).await;
                                        if gen_status.read().as_str() != "running" {
                                            break;
                                        }
                                        let cur = *elapsed_secs.read();
                                        elapsed_secs.set(cur + 1);
                                    }
                                });

                                let input = form_to_json(&form.read());
                                let handle = subscribe(
                                    "generator.runGeneration",
                                    Some(input),
                                    move |data: Value| {
                                        let etype = data["type"].as_str().unwrap_or("").to_string();
                                        if etype == "progress" {
                                            let stage = data["stage"].as_str().unwrap_or("").to_string();
                                            let current = data["current"].as_i64().unwrap_or(0);
                                            let total = data["total"].as_i64().unwrap_or(0);
                                            let message = data["message"].as_str().map(|s| s.to_string());
                                            gen_progress.set(Some(Progress { stage, current, total, message }));
                                            return;
                                        }

                                        gen_log.write().push(data.clone());

                                        match etype.as_str() {
                                            "completed" => {
                                                gen_status.set("succeeded".to_string());
                                                gen_progress.set(None);
                                                if let Some(gid) = data["gameId"].as_str() {
                                                    gen_game_id.set(Some(gid.to_string()));
                                                }
                                                if let Some(t) = data["title"].as_str() {
                                                    gen_game_title.set(Some(t.to_string()));
                                                }
                                                do_load_jobs(jobs, jobs_loading, jobs_error, *jobs_take.peek());
                                            }
                                            "failed" => {
                                                gen_status.set("failed".to_string());
                                                let err = data["error"].as_str().unwrap_or("unknown error").to_string();
                                                gen_error.set(err);
                                            }
                                            _ => {}
                                        }
                                    },
                                );
                                sub_handle.set(Some(handle));
                            },
                            if is_running { "Generating…" } else { "Generate" }
                        }
                    }

                    // preset chips — client-side sugar over the grid dimensions
                    div { class: "row", style: "gap:0.5rem;flex-wrap:wrap",
                        for (name, w, h, min_len, max_len) in [
                            ("Mini 5×5", 5i64, 5i64, 3i64, 5i64),
                            ("Daily 15×15", 15, 15, 3, 12),
                            ("Sunday 21×21", 21, 21, 3, 12),
                        ] {
                            button {
                                class: "app-btn",
                                style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                onclick: move |_| {
                                    let mut f = form.write();
                                    f.width = w;
                                    f.height = h;
                                    f.min_word_length = min_len;
                                    f.max_word_length = max_len;
                                },
                                {name}
                            }
                        }
                    }

                    // numeric params grid
                    div { style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(90px,1fr));gap:0.75rem",
                        for (label, value, min_val, max_val, setter) in [
                            ("Width", form.read().width, 3i64, 50i64, 0usize),
                            ("Height", form.read().height, 3, 50, 1),
                            ("Min Len", form.read().min_word_length, 2, 50, 2),
                            ("Max Len", form.read().max_word_length, 2, 50, 3),
                            ("Answers", form.read().target_words, 1, 250, 4),
                            ("Runs", form.read().runs, 1, 100, 5),
                            ("Attempts", form.read().max_attempts, 1, 1000, 6),
                        ] {
                            label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                                {label}
                                input {
                                    class: "app-input",
                                    style: "padding:0.375rem 0.5rem;font-size:0.875rem",
                                    r#type: "number",
                                    min: "{min_val}",
                                    max: "{max_val}",
                                    value: "{value}",
                                    oninput: move |e| {
                                        if let Ok(n) = e.value().parse::<i64>() {
                                            let mut f = form.write();
                                            match setter {
                                                0 => f.width = n,
                                                1 => f.height = n,
                                                2 => f.min_word_length = n,
                                                3 => f.max_word_length = n,
                                                4 => f.target_words = n,
                                                5 => f.runs = n,
                                                6 => f.max_attempts = n,
                                                _ => {}
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    }
                };

                rsx! {
                    div { class: "col", style: "gap:1.5rem;padding:1rem;overflow-y:auto;height:100%",
                        div { style: "border-bottom:1px solid var(--border-app);padding-bottom:1rem",
                            h1 { style: "font-size:1.125rem;font-weight:bold;letter-spacing:0.05em",
                                "CROSSWORD GENERATOR"
                            }
                        }
                        // Mobile: collapse the form behind a disclosure. Gated by
                        // rsx (not CSS) so only one branch mounts the controls.
                        if mobile_ro {
                            details { style: "border:1px solid var(--border-app)",
                                summary { class: "muted", style: "cursor:pointer;padding:0.75rem 1rem;font-size:0.75rem;font-weight:600;font-family:monospace;text-transform:uppercase;letter-spacing:0.05em",
                                    "New generation (desktop recommended)"
                                }
                                div { class: "col", style: "gap:1.5rem;padding:1rem;border-top:1px solid var(--border-app)",
                                    {params_form}
                                }
                            }
                        } else {
                            {params_form}
                        }
                    }
                }
            }

            Panel::Progress => {
                // Idle summary of the most recent job from the already-fetched list.
                // ISO stamp parsed via the JS Date, same idiom as games.rs `age()`.
                let last_run = jobs.read().first().map(|j| {
                    let ms = js_sys::Date::parse(&j.created_at);
                    let when = if ms.is_nan() {
                        format_datetime(&j.created_at)
                    } else {
                        rel_time(js_sys::Date::now(), ms)
                    };
                    format!("Last: {} · \"{}\" · {}", j.status, j.topic, when)
                });
                rsx! {
                    div { class: "col", style: "gap:1rem;padding:1rem;height:100%;overflow-y:auto",
                        // ── live progress ─────────────────────────────────────────
                        if status != "idle" {
                            GenerationProgress {
                                log: gen_log.read().clone(),
                                progress: gen_progress.read().clone(),
                                running: is_running,
                                status: status.clone(),
                                elapsed_secs: *elapsed_secs.read(),
                            }
                        } else if let Some(line) = last_run {
                            div { class: "col muted", style: "gap:0.5rem;text-align:center;padding:2rem 0",
                                div { style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                    {line}
                                }
                                div { style: "font-size:0.875rem", "Generation output will appear here." }
                            }
                        } else {
                            div { class: "muted", style: "font-size:0.875rem;text-align:center;padding:2rem 0",
                                "Generation output will appear here."
                            }
                        }

                        // ── gen error ─────────────────────────────────────────────
                        if !gen_error.read().is_empty() {
                            div { class: "app-card error", style: "padding:1rem;font-size:0.875rem",
                                {gen_error.read().clone()}
                            }
                        }

                        // ── completed game CTA ────────────────────────────────────
                        if let (Some(gid), Some(gtitle)) = (gen_game_id.read().clone(), gen_game_title.read().clone()) {
                            div { class: "app-card", style: "padding:1rem;border-color:var(--color-success)",
                                div { class: "row", style: "justify-content:space-between;align-items:center;gap:0.75rem;flex-wrap:wrap",
                                    div { class: "col", style: "gap:0.25rem",
                                        div { style: "font-size:0.875rem;font-weight:600", {gtitle.clone()} }
                                        if !publish_error.read().is_empty() {
                                            div { class: "error", style: "font-size:0.75rem", {publish_error.read().clone()} }
                                        }
                                    }
                                    div { class: "row", style: "gap:0.5rem",
                                        // Publish is desktop-only; mobile stays read-only.
                                        if !mobile_ro && !*gen_game_published.read() {
                                            button {
                                                class: "app-btn app-btn-active",
                                                style: "font-weight:bold",
                                                disabled: *publishing.read(),
                                                onclick: {
                                                    let gid = gid.clone();
                                                    move |_| {
                                                        let game_id = gid.clone();
                                                        publishing.set(true);
                                                        publish_error.set(String::new());
                                                        spawn_local(async move {
                                                            match mutation("generator.publishGeneratedGame", Some(json!({"gameId": game_id}))).await {
                                                                Ok(_) => {
                                                                    gen_game_published.set(true);
                                                                    do_load_jobs(jobs, jobs_loading, jobs_error, *jobs_take.peek());
                                                                }
                                                                Err(e) => publish_error.set(e),
                                                            }
                                                            publishing.set(false);
                                                        });
                                                    }
                                                },
                                                if *publishing.read() { "Publishing…" } else { "Publish" }
                                            }
                                        }
                                        button {
                                            class: "app-btn",
                                            onclick: {
                                                let gid = gid.clone();
                                                move |_| {
                                                    // Copy the game URL via the JS clipboard API (no extra Rust deps).
                                                    let origin = web_sys::window()
                                                        .and_then(|w| w.location().origin().ok())
                                                        .unwrap_or_default();
                                                    let url = format!("{origin}/game/{gid}/new");
                                                    let script = format!(
                                                        "navigator.clipboard && navigator.clipboard.writeText({})",
                                                        serde_json::to_string(&url).unwrap_or_default()
                                                    );
                                                    dioxus::document::eval(&script);
                                                    state.toast(crate::store::Severity::Success, "Link copied");
                                                }
                                            },
                                            "Copy game link"
                                        }
                                        button {
                                            class: "app-btn",
                                            onclick: move |_| { nav.push(Route::Games {}); },
                                            "View Games"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Panel::Jobs => {
                let search = job_search.read().to_lowercase();
                let filter = status_filter.read().clone();
                let filtered: Vec<JobRow> = jobs
                    .read()
                    .iter()
                    .filter(|j| {
                        (search.is_empty() || j.topic.to_lowercase().contains(&search))
                            && (filter.is_empty() || j.status == filter)
                    })
                    .cloned()
                    .collect();
                let shown = filtered.len();
                let any_loaded = !jobs.read().is_empty();
                // Server returned a full page — there may be more to fetch.
                let can_load_more = jobs.read().len() as i64 >= *jobs_take.read();
                rsx! {
                    div { style: "overflow:hidden;height:100%;display:flex;flex-direction:column",
                        div { class: "row", style: "padding:1rem;border-bottom:1px solid var(--border-app);justify-content:space-between;align-items:center",
                            h2 { style: "font-size:0.875rem;font-weight:bold;font-family:monospace;letter-spacing:0.05em",
                                "GENERATION JOBS"
                            }
                            button {
                                class: "app-btn",
                                style: "font-size:0.75rem;font-family:monospace;text-transform:uppercase",
                                disabled: *jobs_loading.read(),
                                onclick: move |_| do_load_jobs(jobs, jobs_loading, jobs_error, *jobs_take.peek()),
                                if *jobs_loading.read() { "Refreshing" } else { "Refresh" }
                            }
                        }

                        if !jobs_error.read().is_empty() {
                            div { class: "error", style: "padding:0.75rem 1rem;font-size:0.875rem;border-bottom:1px solid var(--border-app)",
                                {jobs_error.read().clone()}
                            }
                        }

                        // search + status filters (client-side, over the fetched page)
                        div { class: "row", style: "padding:0.75rem 1rem;border-bottom:1px solid var(--border-app);gap:0.5rem;align-items:center;flex-wrap:wrap",
                            input {
                                class: "app-input",
                                style: "padding:0.375rem 0.5rem;font-size:0.875rem;flex:1;min-width:160px",
                                r#type: "text",
                                placeholder: "Search topics…",
                                value: "{job_search}",
                                oninput: move |e| job_search.set(e.value()),
                            }
                            span {
                                class: "muted",
                                style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:bold;text-transform:uppercase;letter-spacing:0.05em;padding:0.125rem 0.5rem;border:1px solid var(--border-app);white-space:nowrap",
                                {format!("{shown} shown")}
                            }
                            for (chip_label, chip_val) in [
                                ("All", ""),
                                ("Succeeded", "SUCCEEDED"),
                                ("Failed", "FAILED"),
                                ("Running", "RUNNING"),
                            ] {
                                button {
                                    class: if *status_filter.read() == chip_val { "app-btn app-btn-active" } else { "app-btn" },
                                    style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                    onclick: move |_| status_filter.set(chip_val.to_string()),
                                    {chip_label}
                                }
                            }
                        }

                        div { style: "overflow-x:auto;flex:1",
                            table { style: "width:100%;text-align:left;font-size:0.875rem;border-collapse:collapse",
                                thead {
                                    tr {
                                        style: "font-size:0.75rem;text-transform:uppercase;font-family:monospace",
                                        for col in ["Status", "Topic", "Grid", "Game", "Created"] {
                                            th { class: "muted", style: "padding:0.75rem 1rem;border-bottom:1px solid var(--border-app)", {col} }
                                        }
                                    }
                                }
                                tbody {
                                    for job in filtered.iter() {
                                        tr { style: "border-bottom:1px solid var(--border-app);font-family:monospace;font-size:0.75rem",
                                            td { style: "padding:0.75rem 1rem",
                                                {
                                                    let bg = match job.status.as_str() {
                                                        "SUCCEEDED" => "background:var(--color-success);color:var(--contrast-ink)",
                                                        "FAILED" => "background:var(--color-error);color:var(--contrast-ink)",
                                                        _ => "background:var(--pastel-yellow);color:var(--contrast-ink)",
                                                    };
                                                    rsx! {
                                                        span {
                                                            style: "padding:0.125rem 0.5rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;{bg}",
                                                            {job.status.clone()}
                                                        }
                                                    }
                                                }
                                            }
                                            td { style: "padding:0.75rem 1rem;font-family:sans-serif;font-size:0.875rem;font-weight:500",
                                                {job.topic.clone()}
                                            }
                                            td { class: "muted", style: "padding:0.75rem 1rem",
                                                {format!("{}x{}", job.width, job.height)}
                                            }
                                            td { style: "padding:0.75rem 1rem",
                                                if let Some(rg) = &job.result_game {
                                                    div { class: "col", style: "gap:0.125rem",
                                                        span { style: "font-family:sans-serif;font-size:0.875rem;font-weight:500", {rg.title.clone()} }
                                                        span { class: "muted", style: "font-size:0.625rem;font-weight:bold;text-transform:uppercase",
                                                            if rg.published { "published" } else { "draft" }
                                                        }
                                                    }
                                                } else {
                                                    span { class: "muted", "—" }
                                                }
                                            }
                                            td { class: "muted", style: "padding:0.75rem 1rem",
                                                {format_datetime(&job.created_at)}
                                            }
                                        }
                                    }
                                    if *jobs_loading.read() && jobs.read().is_empty() {
                                        tr {
                                            td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "5",
                                                "Loading generation jobs…"
                                            }
                                        }
                                    } else if filtered.is_empty() {
                                        tr {
                                            td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "5",
                                                if any_loaded { "No jobs match the current filters." } else { "No generation jobs found." }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if can_load_more {
                            div { style: "padding:0.75rem 1rem;border-top:1px solid var(--border-app)",
                                button {
                                    class: "app-btn",
                                    style: "width:100%;font-size:0.75rem;font-family:monospace;text-transform:uppercase",
                                    disabled: *jobs_loading.read(),
                                    onclick: move |_| {
                                        let cur = *jobs_take.peek();
                                        jobs_take.set(cur + 25);
                                    },
                                    if *jobs_loading.read() { "Loading" } else { "Load more" }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        div { class: "col", style: "height:100%",
            if mobile_ro {
                div { class: "muted", style: "font-size:0.75rem;padding:0.5rem 1rem;border-bottom:1px solid var(--border-app)",
                    "Editing requires a desktop viewport."
                }
            }
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
}
