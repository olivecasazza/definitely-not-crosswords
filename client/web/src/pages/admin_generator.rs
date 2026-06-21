use crate::components::admin_nav::AdminNav;
use crate::components::generation_progress::{GenerationProgress, Progress};
use crate::net::{mutation, query, subscribe, Subscription};
use crate::Route;
use dioxus::prelude::*;
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

fn format_datetime(s: &str) -> String {
    if s.len() >= 19 {
        s[..19].replace('T', " ")
    } else {
        s.to_string()
    }
}

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
) {
    jobs_loading.set(true);
    jobs_error.set(String::new());
    spawn_local(async move {
        match query("generator.listJobs", Some(json!({"take": 25}))).await {
            Ok(v) => {
                let parsed: Vec<JobRow> = serde_json::from_value(v).unwrap_or_default();
                jobs.set(parsed);
            }
            Err(e) => jobs_error.set(e),
        }
        jobs_loading.set(false);
    });
}

// ── component ─────────────────────────────────────────────────────────────────

#[component]
pub fn AdminGenerator() -> Element {
    let nav = use_navigator();

    let mut form = use_signal(GenForm::default);
    let mut jobs = use_signal(Vec::<JobRow>::new);
    let mut jobs_loading = use_signal(|| false);
    let mut jobs_error = use_signal(String::new);

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

    use_effect(move || {
        do_load_jobs(jobs, jobs_loading, jobs_error);
    });

    // ── render ────────────────────────────────────────────────────────────────

    let status = gen_status.read().clone();
    let is_running = status == "running";

    rsx! {
        div { class: "container",
            div { class: "col", style: "gap:1.5rem",

                // ── header card ───────────────────────────────────────────────
                div { class: "app-card col", style: "padding:1.5rem;gap:1.5rem",
                    AdminNav {}
                    div { style: "border-bottom:1px solid var(--border-app);padding-bottom:1rem",
                        h1 { style: "font-size:1.125rem;font-weight:bold;letter-spacing:0.05em",
                            "CROSSWORD GENERATOR"
                        }
                    }

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
                                                do_load_jobs(jobs, jobs_loading, jobs_error);
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

                    // numeric params grid
                    div { style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(90px,1fr));gap:0.75rem",
                        for (label, value, min_val, max_val, setter) in [
                            ("Width", form.read().width, 3i64, 50i64, 0usize),
                            ("Height", form.read().height, 3, 50, 1),
                            ("Min Len", form.read().min_word_length, 2, 50, 2),
                            ("Max Len", form.read().max_word_length, 2, 50, 3),
                            ("Answers", form.read().target_words, 1, 250, 4),
                            ("Runs", form.read().runs, 1, 100, 5),
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
                                                _ => {}
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    }
                }

                // ── live progress ─────────────────────────────────────────────
                if status != "idle" {
                    GenerationProgress {
                        log: gen_log.read().clone(),
                        progress: gen_progress.read().clone(),
                        running: is_running,
                        status: status.clone(),
                        elapsed_secs: *elapsed_secs.read(),
                    }
                }

                // ── gen error ─────────────────────────────────────────────────
                if !gen_error.read().is_empty() {
                    div { class: "app-card error", style: "padding:1rem;font-size:0.875rem",
                        {gen_error.read().clone()}
                    }
                }

                // ── completed game CTA ────────────────────────────────────────
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
                                if !*gen_game_published.read() {
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
                                                            do_load_jobs(jobs, jobs_loading, jobs_error);
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
                                    onclick: move |_| { nav.push(Route::Games {}); },
                                    "View Games"
                                }
                            }
                        }
                    }
                }

                // ── jobs table ────────────────────────────────────────────────
                div { class: "app-card", style: "overflow:hidden",
                    div { class: "row", style: "padding:1rem;border-bottom:1px solid var(--border-app);justify-content:space-between;align-items:center",
                        h2 { style: "font-size:0.875rem;font-weight:bold;font-family:monospace;letter-spacing:0.05em",
                            "GENERATION JOBS"
                        }
                        button {
                            class: "app-btn",
                            style: "font-size:0.75rem;font-family:monospace;text-transform:uppercase",
                            disabled: *jobs_loading.read(),
                            onclick: move |_| do_load_jobs(jobs, jobs_loading, jobs_error),
                            if *jobs_loading.read() { "Refreshing" } else { "Refresh" }
                        }
                    }

                    if !jobs_error.read().is_empty() {
                        div { class: "error", style: "padding:0.75rem 1rem;font-size:0.875rem;border-bottom:1px solid var(--border-app)",
                            {jobs_error.read().clone()}
                        }
                    }

                    div { style: "overflow-x:auto",
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
                                for job in jobs.read().iter() {
                                    tr { style: "border-bottom:1px solid var(--border-app);font-family:monospace;font-size:0.75rem",
                                        td { style: "padding:0.75rem 1rem",
                                            {
                                                let bg = match job.status.as_str() {
                                                    "SUCCEEDED" => "background:var(--color-success);color:#0f172a",
                                                    "FAILED" => "background:var(--color-error);color:#0f172a",
                                                    _ => "background:var(--pastel-yellow,#fde68a);color:#0f172a",
                                                };
                                                rsx! {
                                                    span {
                                                        style: "padding:0.125rem 0.5rem;border-radius:0.25rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;{bg}",
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
                                } else if jobs.read().is_empty() {
                                    tr {
                                        td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "5",
                                            "No generation jobs found."
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
