use crossword_core::fmt::{plural, rel_time};
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::components::game_list::{error_status, game_list, status, GameRow, Tone, GAME_LIST_CSS};
use crate::net;
use crate::store::use_app_state;
use crate::Route;

/// Raw items returned by gameList.get — a heterogeneous array discriminated by `type`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type")]
enum GameListItem {
    Game(UnstartedGame),
    ActiveGame(PlayedGame),
    CompletedGame(PlayedGame),
}

/// Lobby metadata the server aggregates alongside each row. All optional so an
/// older/leaner server response still deserializes.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct UnstartedGame {
    id: String,
    title: String,
    #[serde(default)]
    clues: i64,
    #[serde(default)]
    at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct NestedGame {
    title: String,
}

/// ActiveGame and CompletedGame have the same shape on the wire; `score` is only
/// populated for completed rows.
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

/// Route a row id to the right destination — the only place that mapping lives.
fn route_for(kind: Panel, id: String) -> Route {
    match kind {
        Panel::Available => Route::GameNew { id },
        Panel::Active => Route::GamePlay { id },
        Panel::Completed => Route::GameCompleted { id },
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

/// Join the non-empty parts of a meta line with the app's separator.
fn meta_line(parts: [Option<String>; 3]) -> String {
    parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// Project the parsed list into rows for one panel. Single pass over `items`
/// (`O(n)`), and the only place a panel's badge/meta/tone is decided.
fn rows_for(kind: Panel, items: &[GameListItem]) -> Vec<GameRow> {
    items
        .iter()
        .filter_map(|item| match (kind, item) {
            (Panel::Available, GameListItem::Game(g)) => Some(GameRow {
                id: g.id.clone(),
                title: g.title.clone(),
                badge: "UNSTARTED",
                tone: Tone::Neutral,
                meta: meta_line([
                    (g.clues > 0).then(|| plural(g.clues, "clue")),
                    age(&g.at).map(|a| format!("added {a}")),
                    None,
                ]),
            }),
            (Panel::Active, GameListItem::ActiveGame(g)) => Some(GameRow {
                id: g.id.clone(),
                title: g.game.title.clone(),
                badge: "ACTIVE",
                tone: Tone::Active,
                meta: meta_line([
                    (g.clues > 0).then(|| plural(g.clues, "clue")),
                    (g.players > 1).then(|| plural(g.players, "player")),
                    age(&g.at).map(|a| format!("played {a}")),
                ]),
            }),
            (Panel::Completed, GameListItem::CompletedGame(g)) => Some(GameRow {
                id: g.id.clone(),
                title: g.game.title.clone(),
                badge: "COMPLETED",
                tone: Tone::Done,
                meta: meta_line([
                    g.score.map(|s| format!("{s} pts")),
                    (g.players > 1).then(|| plural(g.players, "player")),
                    age(&g.at).map(|a| format!("finished {a}")),
                ]),
            }),
            _ => None,
        })
        .collect()
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

    let search = use_signal(String::new);
    let nav = use_navigator();

    let ws = use_workspace("games_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        // Distinguish session-loading from signed-out: while `session` is None the
        // request is still in flight (show Loading), and only `Some(None)` is a
        // genuine signed-out state. Collapsing both to None flashed a wrong
        // "Sign in" message on every load.
        match &*state.session.read() {
            None => return status("muted", "Loading…", true),
            Some(None) => return status("muted", "Sign in to see your games.", false),
            Some(Some(_)) => {}
        }

        // Signed in, but no parsed items yet: either still fetching (or a session
        // with no email — same treatment) or the request failed.
        let Some(items) = items() else {
            let err = match &*games_res.read_unchecked() {
                Some(Some(Err(e))) => Some(e.clone()),
                _ => None,
            };
            return match err {
                Some(msg) => error_status(&msg, move |_| games_res.restart()),
                None => status("muted", "Loading…", true),
            };
        };

        let (empty, filter) = match kind {
            // Only the shared pool can grow large enough to need searching.
            Panel::Available => ("No games available yet. Check back soon.", Some(search)),
            Panel::Active => ("No games in progress. Start one from Available.", None),
            Panel::Completed => ("No finished games yet. Solve one to see it here.", None),
        };

        game_list(rows_for(kind, &items), empty, filter, move |id| {
            nav.push(route_for(kind, id));
        })
    };

    rsx! {
        style { {GAME_LIST_CSS} }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<GameListItem> {
        serde_json::from_value(json!([
            { "type": "Game", "id": "g1", "title": "Monday Mini", "clues": 1 },
            { "type": "ActiveGame", "id": "a1", "game": { "title": "Co-op Cryptic" }, "clues": 12, "players": 2 },
            { "type": "CompletedGame", "id": "c1", "game": { "title": "Sunday Giant" }, "clues": 40, "players": 1, "score": 340 },
        ]))
        .unwrap()
    }

    #[test]
    fn each_panel_sees_only_its_own_kind() {
        let items = fixture();
        for (kind, id, badge) in [
            (Panel::Available, "g1", "UNSTARTED"),
            (Panel::Active, "a1", "ACTIVE"),
            (Panel::Completed, "c1", "COMPLETED"),
        ] {
            let rows = rows_for(kind, &items);
            assert_eq!(rows.len(), 1, "{badge} panel");
            assert_eq!(rows[0].id, id);
            assert_eq!(rows[0].badge, badge);
        }
    }

    #[test]
    fn meta_lines_skip_absent_and_singleton_facts() {
        let items = fixture();
        // no `at` in the fixture, and 1 clue stays singular
        assert_eq!(rows_for(Panel::Available, &items)[0].meta, "1 clue");
        // solo completed games don't advertise a player count
        assert_eq!(rows_for(Panel::Completed, &items)[0].meta, "340 pts");
        assert_eq!(
            rows_for(Panel::Active, &items)[0].meta,
            "12 clues · 2 players"
        );
    }
}
