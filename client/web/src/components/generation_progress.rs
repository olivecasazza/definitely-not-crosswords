use dioxus::prelude::*;
use serde_json::Value;

/// Matches GenerationProgressEvent / GenerationEvent from the backend.
/// Only extracted from a `type=="progress"` event — rest handled as raw Value.
#[derive(Clone, PartialEq)]
pub struct Progress {
    pub stage: String,
    pub current: i64,
    pub total: i64,
    pub message: Option<String>,
}

const STAGE_LABELS: &[(&str, &str)] = &[
    ("loading-dictionary", "Loading dictionary"),
    ("embedding-model", "Loading embedding model"),
    ("embedding", "Embedding candidates"),
    ("solving", "Solving grids"),
    ("solving-attempts", "Placing words"),
    ("validating", "Validating grid"),
];

fn stage_label(stage: &str) -> &str {
    STAGE_LABELS
        .iter()
        .find(|(k, _)| *k == stage)
        .map(|(_, v)| *v)
        .unwrap_or(stage)
}

#[derive(Clone)]
struct LogLine {
    time: String,
    text: String,
    class: &'static str,
}

fn format_time_delta(at: i64, first_at: i64) -> String {
    // ponytail: no js-sys/Date; use integer delta from first event in seconds
    let delta = ((at - first_at).max(0) / 1000) as u64;
    if delta < 60 {
        format!("+{delta}s")
    } else {
        format!("+{}m{:02}s", delta / 60, delta % 60)
    }
}

fn event_to_line(event: &Value, first_at: i64) -> LogLine {
    let at = event["at"].as_i64().unwrap_or(first_at);
    let time = format_time_delta(at, first_at);
    let etype = event["type"].as_str().unwrap_or("");
    let (text, class) = match etype {
        "started" => ("▶ generation started".to_string(), "muted"),
        "stage" => {
            let stage = event["stage"].as_str().unwrap_or("");
            let msg = event["message"].as_str().unwrap_or("");
            let label = stage_label(stage);
            let text = if msg.is_empty() {
                format!("■ {label}")
            } else {
                format!("■ {label}: {msg}")
            };
            (text, "text-[var(--text-primary)]")
        }
        "log" => {
            let msg = event["message"].as_str().unwrap_or("").to_string();
            let level = event["level"].as_str().unwrap_or("info");
            let class = match level {
                "error" => "error",
                "warn" => "muted",
                _ => "muted",
            };
            (msg, class)
        }
        "completed" => {
            let count = event["questionCount"].as_i64().unwrap_or(0);
            let title = event["title"].as_str().unwrap_or("");
            let text = if title.is_empty() {
                format!("✓ completed — {count} answers")
            } else {
                format!("✓ completed — {count} answers (\"{title}\")")
            };
            (text, "success")
        }
        "failed" => {
            let err = event["error"].as_str().unwrap_or("unknown error");
            (format!("✗ failed — {err}"), "error")
        }
        _ => {
            let msg = event["message"].as_str().unwrap_or(etype).to_string();
            (msg, "muted")
        }
    };
    LogLine { time, text, class }
}

#[component]
pub fn GenerationProgress(
    log: Vec<Value>,
    progress: Option<Progress>,
    running: bool,
    status: String,
    /// Elapsed seconds counter — owner ticks this via gloo_timers
    elapsed_secs: u64,
) -> Element {
    let first_at = log.first().and_then(|e| e["at"].as_i64()).unwrap_or(0);

    let stage_label_text = {
        if status == "succeeded" {
            "Done".to_string()
        } else if status == "failed" {
            "Failed".to_string()
        } else if let Some(p) = &progress {
            stage_label(&p.stage).to_string()
        } else {
            // fall back to most recent stage event
            log.iter()
                .rev()
                .find(|e| e["type"] == "stage")
                .and_then(|e| e["stage"].as_str())
                .map(|s| stage_label(s).to_string())
                .unwrap_or_else(|| {
                    if running {
                        "Starting…".to_string()
                    } else {
                        "Idle".to_string()
                    }
                })
        }
    };

    let pct: u64 = progress
        .as_ref()
        .filter(|p| p.total > 0)
        .map(|p| ((p.current as f64 / p.total as f64) * 100.0).min(100.0) as u64)
        .unwrap_or(0);

    let indeterminate = running && progress.is_none();
    let bar_width = if status == "succeeded" {
        "100%".to_string()
    } else if indeterminate {
        "100%".to_string()
    } else {
        format!("{pct}%")
    };
    let bar_color = if status == "failed" {
        "background:var(--color-error)"
    } else if status == "succeeded" {
        "background:var(--color-success)"
    } else {
        "background:var(--color-primary,var(--pastel-yellow))"
    };
    let bar_anim = if indeterminate {
        "animation:pulse 1s infinite"
    } else {
        ""
    };

    let elapsed_label = {
        let s = elapsed_secs;
        let m = s / 60;
        let rem = s % 60;
        if m > 0 {
            format!("{m}m {rem:02}s")
        } else {
            format!("{s}s")
        }
    };

    let header_text = if status == "running" {
        "GENERATING…"
    } else if status == "succeeded" {
        "GENERATION COMPLETE"
    } else if status == "failed" {
        "GENERATION FAILED"
    } else {
        "GENERATION LOG"
    };

    let lines: Vec<LogLine> = log.iter().map(|e| event_to_line(e, first_at)).collect();
    let progress_msg = progress.as_ref().and_then(|p| p.message.clone());

    rsx! {
        div { class: "app-card col", style: "padding:1rem;gap:0.75rem",
            // header row
            div { class: "row", style: "justify-content:space-between;border-bottom:1px solid var(--border-app);padding-bottom:0.5rem",
                h2 {
                    style: "font-family:monospace;font-size:0.875rem;font-weight:bold;letter-spacing:0.05em",
                    if status == "succeeded" {
                        span { class: "success", {header_text} }
                    } else if status == "failed" {
                        span { class: "error", {header_text} }
                    } else {
                        span { {header_text} }
                    }
                }
                if (running || status == "succeeded" || status == "failed") && elapsed_secs > 0 {
                    span { class: "muted", style: "font-family:monospace;font-size:0.75rem",
                        {elapsed_label}
                    }
                }
            }

            // progress bar
            div { class: "col", style: "gap:0.375rem",
                div { class: "row", style: "justify-content:space-between;font-family:monospace;font-size:0.75rem",
                    span { class: "muted", style: "text-transform:uppercase;letter-spacing:0.05em",
                        {stage_label_text}
                    }
                    if let Some(p) = &progress {
                        span { class: "muted",
                            {format!("{} / {} ({pct}%)", p.current, p.total)}
                        }
                    }
                }
                div { style: "height:0.5rem;width:100%;overflow:hidden;border-radius:9999px;background:var(--bg-cell-empty)",
                    div {
                        style: "height:100%;border-radius:9999px;transition:width 0.2s ease-out;{bar_color};{bar_anim};width:{bar_width}",
                    }
                }
                if let Some(msg) = progress_msg {
                    p { class: "muted", style: "font-size:0.75rem", {msg} }
                }
            }

            // event feed
            div {
                style: "display:flex;flex-direction:column;gap:0.125rem;max-height:14rem;overflow-y:auto;border-radius:0.25rem;background:var(--bg-cell-empty);padding:0.5rem;font-family:monospace;font-size:0.6875rem;line-height:1.6",
                if lines.is_empty() {
                    span { class: "muted", "Waiting for events…" }
                } else {
                    for line in lines {
                        div { class: "row", style: "gap:0.5rem",
                            span { class: "muted", style: "flex-shrink:0", {line.time} }
                            span { class: "{line.class}", style: "word-break:break-all", {line.text} }
                        }
                    }
                }
            }
        }
    }
}
