use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Available,
    Active,
    Completed,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Available => "Available",
            Panel::Active => "Active",
            Panel::Completed => "Completed",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Available, 16.0, 16.0, 620.0, 948.0),
        b.at(Panel::Active, 652.0, 16.0, 620.0, 948.0),
        b.at(Panel::Completed, 1288.0, 16.0, 616.0, 948.0),
    ]
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

    let ws = use_workspace("games_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        // Distinguish session-loading from signed-out: while `session` is None the
        // request is still in flight (show Loading), and only `Some(None)` is a
        // genuine signed-out state. Collapsing both to None flashed a wrong
        // "Sign in" message on every load.
        match &*state.session.read() {
            None => {
                return rsx! {
                    div {
                        class: "muted",
                        style: "padding: 2rem; text-align: center; font-size: .75rem; font-family: monospace;",
                        "Loading..."
                    }
                };
            }
            Some(None) => {
                return rsx! {
                    div {
                        class: "muted",
                        style: "padding: 2rem; text-align: center; font-size: .75rem; font-family: monospace;",
                        "Sign in to see your games."
                    }
                };
            }
            Some(Some(_)) => {}
        }

        // Parse items once; early-return for loading / error states
        // so all three panels show a consistent status.
        let all_items: Vec<GameListItem> = {
            let res = games_res.read_unchecked();
            match &*res {
                // Signed in, but the game list is still being fetched (or the
                // edge case of a session with no email) — keep showing Loading,
                // never the sign-in prompt (handled above).
                None | Some(None) => {
                    return rsx! {
                        div {
                            class: "muted",
                            style: "padding: 2rem; text-align: center; font-size: .75rem; font-family: monospace;",
                            "Loading..."
                        }
                    };
                }
                Some(Some(Err(e))) => {
                    return rsx! {
                        div {
                            class: "error",
                            style: "padding: 1rem; font-size: .75rem; font-family: monospace;",
                            "Error: {e}"
                        }
                    };
                }
                Some(Some(Ok(raw_items))) => raw_items
                    .iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect(),
            }
        };

        match kind {
            Panel::Available => {
                let items: Vec<_> = all_items
                    .into_iter()
                    .filter(|i| matches!(i, GameListItem::Game(_)))
                    .collect();
                if items.is_empty() {
                    return rsx! {
                        div {
                            class: "muted",
                            style: "padding: 3rem; text-align: center; font-size: .75rem; font-family: monospace;",
                            "No games available yet."
                        }
                    };
                }
                rsx! {
                    div { style: "divide-y: 1px solid var(--border-app);",
                        for item in items {
                            {
                                let title = match &item {
                                    GameListItem::Game(g) => g.title.clone(),
                                    _ => String::new(),
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
                                        span {
                                            style: "padding: .25rem .625rem; font-size: .75rem; font-family: monospace; font-weight: 700; border-radius: .25rem; text-transform: uppercase; border: 1px solid var(--border-app); color: var(--text-secondary);",
                                            "UNSTARTED"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Panel::Active => {
                let items: Vec<_> = all_items
                    .into_iter()
                    .filter(|i| matches!(i, GameListItem::ActiveGame(_)))
                    .collect();
                if items.is_empty() {
                    return rsx! {
                        div {
                            class: "muted",
                            style: "padding: 3rem; text-align: center; font-size: .75rem; font-family: monospace;",
                            "No active games."
                        }
                    };
                }
                rsx! {
                    div { style: "divide-y: 1px solid var(--border-app);",
                        for item in items {
                            {
                                let title = match &item {
                                    GameListItem::ActiveGame(g) => g.game.title.clone(),
                                    _ => String::new(),
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
                                        span {
                                            style: "padding: .25rem .625rem; font-size: .75rem; font-family: monospace; font-weight: 700; border-radius: .25rem; text-transform: uppercase; background: var(--pastel-yellow); color: #0f172a;",
                                            "ACTIVE"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Panel::Completed => {
                let items: Vec<_> = all_items
                    .into_iter()
                    .filter(|i| matches!(i, GameListItem::CompletedGame(_)))
                    .collect();
                if items.is_empty() {
                    return rsx! {
                        div {
                            class: "muted",
                            style: "padding: 3rem; text-align: center; font-size: .75rem; font-family: monospace;",
                            "No completed games."
                        }
                    };
                }
                rsx! {
                    div { style: "divide-y: 1px solid var(--border-app);",
                        for item in items {
                            {
                                let title = match &item {
                                    GameListItem::CompletedGame(g) => g.game.title.clone(),
                                    _ => String::new(),
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
                                        span {
                                            style: "padding: .25rem .625rem; font-size: .75rem; font-family: monospace; font-weight: 700; border-radius: .25rem; text-transform: uppercase; background: var(--pastel-green); color: #0f172a;",
                                            "COMPLETED"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    rsx! {
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
