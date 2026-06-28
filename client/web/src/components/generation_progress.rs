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
        style { {PROGRESS_CSS} }
        div { class: "gp-root",
            // status + elapsed row
            div { class: "gp-header",
                if status == "succeeded" {
                    span { class: "gp-title success", {header_text} }
                } else if status == "failed" {
                    span { class: "gp-title error", {header_text} }
                } else {
                    span { class: "gp-title", {header_text} }
                }
                if (running || status == "succeeded" || status == "failed") && elapsed_secs > 0 {
                    span { class: "gp-elapsed muted", {elapsed_label} }
                }
            }

            // progress bar + stage label
            div { class: "gp-bar-section",
                div { class: "gp-bar-meta",
                    span { class: "gp-stage muted", {stage_label_text} }
                    if let Some(p) = &progress {
                        span { class: "gp-counts muted",
                            {format!("{}/{} ({pct}%)", p.current, p.total)}
                        }
                    }
                }
                div { class: "gp-bar-track",
                    div {
                        class: "gp-bar-fill",
                        style: "{bar_color};{bar_anim};width:{bar_width}",
                    }
                }
                if let Some(msg) = progress_msg {
                    p { class: "gp-prog-msg muted", {msg} }
                }
            }

            // event feed — fills remaining panel height
            div { class: "gp-feed",
                if lines.is_empty() {
                    span { class: "muted", "Waiting for events…" }
                } else {
                    for line in lines {
                        div { class: "gp-line",
                            span { class: "gp-ts muted", {line.time} }
                            span { class: "{line.class} gp-msg", {line.text} }
                        }
                    }
                }
            }
        }
    }
}

const PROGRESS_CSS: &str = r#"
.gp-root {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    height: 100%;
    padding: 0.5rem 0.625rem;
    box-sizing: border-box;
    font-family: monospace;
}
.gp-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    border-bottom: 1px solid var(--border-app);
    padding-bottom: 0.375rem;
    flex-shrink: 0;
}
.gp-title {
    font-size: 0.6875rem;
    font-weight: 700;
    letter-spacing: 0.07em;
    text-transform: uppercase;
}
.gp-elapsed {
    font-size: 0.6875rem;
}
.gp-bar-section {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex-shrink: 0;
}
.gp-bar-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 0.625rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}
.gp-stage {}
.gp-counts {}
.gp-bar-track {
    height: 3px;
    width: 100%;
    overflow: hidden;
    background: var(--bg-cell-empty);
}
.gp-bar-fill {
    height: 100%;
    transition: width 0.2s ease-out;
}
.gp-prog-msg {
    font-size: 0.625rem;
    margin: 0;
}
.gp-feed {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.0625rem;
    background: var(--bg-cell-empty);
    padding: 0.375rem 0.5rem;
    font-size: 0.625rem;
    line-height: 1.65;
}
.gp-line {
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
}
.gp-ts {
    flex-shrink: 0;
    opacity: 0.6;
}
.gp-msg {
    word-break: break-all;
}
"#;
