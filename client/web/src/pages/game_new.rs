use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crate::net;
use crate::Route;

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartDetails {
    id: String,
    title: String,
    source: String,
    question_count: i64,
    grid_size: i64,
    active_game_id: Option<String>,
    completed_game_id: Option<String>,
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
    let nav = use_navigator();

    let id_for_start = id.clone();
    let handle_start = move |details: StartDetails| {
        // Continue → navigate to active game
        if let Some(active_id) = details.active_game_id.clone() {
            nav.push(Route::GamePlay { id: active_id });
            return;
        }
        // Review → navigate to completed
        if let Some(completed_id) = details.completed_game_id.clone() {
            nav.push(Route::GameCompleted { id: completed_id });
            return;
        }

        // Fresh start
        let id = id_for_start.clone();
        let mut is_starting = is_starting.clone();
        let mut start_error = start_error.clone();
        let nav = nav.clone();

        spawn_local(async move {
            is_starting.set(true);
            start_error.set(String::new());
            match net::query_as::<serde_json::Value>(
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

    rsx! {
        div {
            style: "flex: 1; width: 100%; max-width: 48rem; margin: 0 auto; padding: 2rem 1rem;",

            match &*details_res.read_unchecked() {
                None => rsx! {
                    div { class: "muted", style: "padding: 2rem; font-family: monospace; font-size: .75rem;", "Loading..." }
                },
                Some(Err(e)) => rsx! {
                    div { class: "app-card", style: "padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem;",
                        h1 {
                            style: "font-size: 1.125rem; font-weight: 700; font-family: monospace; text-transform: uppercase; color: var(--pastel-red); margin: 0;",
                            "Game Unavailable"
                        }
                        p { class: "muted", style: "font-size: .875rem;", "This puzzle could not be found or is not available to start." }
                        p { class: "error", style: "font-size: .75rem; font-family: monospace;", "{e}" }
                        Link { to: Route::Games {}, class: "app-btn", style: "width: max-content;", "Back to Games" }
                    }
                },
                Some(Ok(details)) => {
                    let action_label = if details.active_game_id.is_some() {
                        "Continue Game"
                    } else if details.completed_game_id.is_some() {
                        "Review Completed Game"
                    } else {
                        "Start Game"
                    };
                    let status_label = if details.active_game_id.is_some() {
                        "Active"
                    } else if details.completed_game_id.is_some() {
                        "Completed"
                    } else {
                        "Ready"
                    };
                    let d = details.clone();
                    rsx! {
                        div { style: "display: flex; flex-direction: column; gap: 1.5rem;",
                            div { class: "app-card", style: "padding: 1.5rem 1.75rem; display: flex; flex-direction: column; gap: 1.5rem;",
                                // Header
                                div { style: "display: flex; flex-direction: row; align-items: flex-start; justify-content: space-between; gap: 1rem; flex-wrap: wrap;",
                                    div {
                                        p {
                                            class: "muted",
                                            style: "font-size: .75rem; font-family: monospace; text-transform: uppercase; letter-spacing: .1em; margin: 0 0 .5rem 0;",
                                            "New Game"
                                        }
                                        h1 {
                                            style: "font-size: 1.75rem; font-weight: 700; color: var(--text-primary); margin: 0;",
                                            "{details.title}"
                                        }
                                    }
                                    span {
                                        style: "padding: .375rem .75rem; border-radius: .375rem; border: 1px solid var(--border-app); background: var(--bg-cell-empty); font-size: .75rem; font-family: monospace; text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); white-space: nowrap;",
                                        "{details.source.to_lowercase()}"
                                    }
                                }

                                // Stats grid
                                div { style: "display: grid; grid-template-columns: repeat(3, 1fr); gap: .75rem;",
                                    div {
                                        style: "border: 1px solid var(--border-app); background: var(--bg-cell-empty); border-radius: .5rem; padding: 1rem;",
                                        p {
                                            class: "muted",
                                            style: "font-size: .625rem; font-family: monospace; text-transform: uppercase; letter-spacing: .05em; margin: 0 0 .25rem 0;",
                                            "Clues"
                                        }
                                        p { style: "font-size: 1.25rem; font-weight: 700; color: var(--text-primary); margin: 0;", "{details.question_count}" }
                                    }
                                    div {
                                        style: "border: 1px solid var(--border-app); background: var(--bg-cell-empty); border-radius: .5rem; padding: 1rem;",
                                        p {
                                            class: "muted",
                                            style: "font-size: .625rem; font-family: monospace; text-transform: uppercase; letter-spacing: .05em; margin: 0 0 .25rem 0;",
                                            "Grid"
                                        }
                                        p { style: "font-size: 1.25rem; font-weight: 700; color: var(--text-primary); margin: 0;", "{details.grid_size} x {details.grid_size}" }
                                    }
                                    div {
                                        style: "border: 1px solid var(--border-app); background: var(--bg-cell-empty); border-radius: .5rem; padding: 1rem;",
                                        p {
                                            class: "muted",
                                            style: "font-size: .625rem; font-family: monospace; text-transform: uppercase; letter-spacing: .05em; margin: 0 0 .25rem 0;",
                                            "Status"
                                        }
                                        p { style: "font-size: 1.25rem; font-weight: 700; color: var(--text-primary); margin: 0;", "{status_label}" }
                                    }
                                }

                                if !start_error.read().is_empty() {
                                    p { class: "error", style: "font-size: .875rem;", "{start_error}" }
                                }

                                div { style: "display: flex; flex-direction: row; gap: .75rem; flex-wrap: wrap;",
                                    button {
                                        class: "app-btn app-btn-active",
                                        style: "justify-content: center;",
                                        disabled: *is_starting.read(),
                                        onclick: move |_| handle_start(d.clone()),
                                        if *is_starting.read() { "Starting..." } else { "{action_label}" }
                                    }
                                    Link { to: Route::Games {}, class: "app-btn", style: "justify-content: center;", "Back to Games" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
