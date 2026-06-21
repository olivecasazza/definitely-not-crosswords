use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::json;

use crate::net;
use crate::store::use_app_state;
use crate::Route;

/// Raw items returned by gameList.get — a heterogeneous array discriminated by `type`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum GameListItem {
    Game(UnstartedGame),
    ActiveGame(ActiveGameItem),
    CompletedGame(CompletedGameItem),
}

#[derive(Debug, Clone, Deserialize)]
struct UnstartedGame {
    id: String,
    title: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NestedGame {
    title: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ActiveGameItem {
    id: String,
    game: NestedGame,
}

#[derive(Debug, Clone, Deserialize)]
struct CompletedGameItem {
    id: String,
    game: NestedGame,
}

#[component]
pub fn Games() -> Element {
    let state = use_app_state();

    let games_res = use_resource(move || async move {
        let email = state.user().and_then(|u| u.email)?;
        let result = net::query_as::<Vec<serde_json::Value>>(
            "gameList.get",
            Some(json!({ "email": email })),
        )
        .await;
        Some(result)
    });

    let nav = use_navigator();

    let handle_click = move |item: GameListItem| match item {
        GameListItem::Game(g) => {
            nav.push(Route::GameNew { id: g.id });
        }
        GameListItem::ActiveGame(g) => {
            nav.push(Route::GamePlay { id: g.id });
        }
        GameListItem::CompletedGame(g) => {
            nav.push(Route::GameCompleted { id: g.id });
        }
    };

    rsx! {
        div {
            style: "flex: 1; padding: 1.5rem; display: flex; flex-direction: column; max-width: 36rem; margin: 0 auto; width: 100%;",

            div { class: "app-card", style: "overflow: hidden;",
                div {
                    style: "padding: 1rem; border-bottom: 1px solid var(--border-app);",
                    h1 {
                        style: "font-size: 1.125rem; font-weight: 700; font-family: monospace; letter-spacing: .1em; margin: 0;",
                        "AVAILABLE GAMES"
                    }
                }

                match &*games_res.read_unchecked() {
                    None => rsx! {
                        div { class: "muted", style: "padding: 2rem; text-align: center; font-size: .75rem; font-family: monospace;", "Loading..." }
                    },
                    Some(None) => rsx! {
                        div { class: "muted", style: "padding: 2rem; text-align: center; font-size: .75rem; font-family: monospace;", "Sign in to see your games." }
                    },
                    Some(Some(Err(e))) => rsx! {
                        div { class: "error", style: "padding: 1rem; font-size: .75rem; font-family: monospace;", "Error: {e}" }
                    },
                    Some(Some(Ok(raw_items))) => {
                        let items: Vec<GameListItem> = raw_items
                            .iter()
                            .filter_map(|v| serde_json::from_value(v.clone()).ok())
                            .collect();
                        if items.is_empty() {
                            rsx! {
                                div {
                                    class: "muted",
                                    style: "padding: 3rem; text-align: center; font-size: .75rem; font-family: monospace;",
                                    "No games available yet."
                                }
                            }
                        } else {
                            rsx! {
                                div { style: "divide-y: 1px solid var(--border-app);",
                                    for item in items {
                                        {
                                            let (title, badge_text, badge_style) = match &item {
                                                GameListItem::Game(g) => (
                                                    g.title.clone(),
                                                    "UNSTARTED",
                                                    "padding: .25rem .625rem; font-size: .75rem; font-family: monospace; font-weight: 700; border-radius: .25rem; text-transform: uppercase; border: 1px solid var(--border-app); color: var(--text-secondary);",
                                                ),
                                                GameListItem::ActiveGame(g) => (
                                                    g.game.title.clone(),
                                                    "ACTIVE",
                                                    "padding: .25rem .625rem; font-size: .75rem; font-family: monospace; font-weight: 700; border-radius: .25rem; text-transform: uppercase; background: var(--pastel-yellow); color: #0f172a;",
                                                ),
                                                GameListItem::CompletedGame(g) => (
                                                    g.game.title.clone(),
                                                    "COMPLETED",
                                                    "padding: .25rem .625rem; font-size: .75rem; font-family: monospace; font-weight: 700; border-radius: .25rem; text-transform: uppercase; background: var(--pastel-green); color: #0f172a;",
                                                ),
                                            };
                                            let item_clone = item.clone();
                                            let hc = handle_click.clone();
                                            rsx! {
                                                div {
                                                    style: "display: flex; flex-direction: row; align-items: center; justify-content: space-between; padding: 1rem; border-bottom: 1px solid var(--border-app); cursor: pointer;",
                                                    onclick: move |_| hc(item_clone.clone()),
                                                    span {
                                                        style: "font-weight: 700; font-size: .875rem; color: var(--text-primary);",
                                                        "{title}"
                                                    }
                                                    span { style: "{badge_style}", "{badge_text}" }
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
    }
}
