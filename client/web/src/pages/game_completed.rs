use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

use crate::components::game_list::status;
use crate::components::identicon::Identicon;
use crate::components::ui::{RankBadge, StatTile};
use crate::net;
use crate::store::use_app_state;
use crate::Route;
use crossword_core::fmt::format_date;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompletedGameData {
    id: String,
    /// Source `Game` id — powers the rematch button. Optional so responses from
    /// an older server (which doesn't send it) still parse; we then hide rematch.
    #[serde(default)]
    game_id: Option<String>,
    created_at: String,
    game: GameInfo,
    game_stats: GameStats,
}

/// The slice of a `gameList.get` item the "Up next" card needs. Items are
/// discriminated by `type`; we only deserialize `type == "Game"` (unstarted).
#[derive(Debug, Clone, Deserialize)]
struct NextUpGame {
    id: String,
    title: String,
    #[serde(default)]
    clues: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameInfo {
    title: String,
    source: String,
    questions: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameStats {
    member_scores: Vec<MemberScore>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemberScore {
    id: String,
    score: i64,
    correct_guesses: i64,
    incorrect_guesses: i64,
    member: GameMember,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameMember {
    is_owner: bool,
    user: MemberUser,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemberUser {
    name: Option<String>,
    email: Option<String>,
}

fn rank_name(index: usize) -> &'static str {
    match index {
        0 => "1ST PLACE",
        1 => "2ND PLACE",
        2 => "3RD PLACE",
        _ => "...",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Rankings,
    // Alias keeps layouts persisted under the old "Summary" name deserializing.
    #[serde(alias = "Summary")]
    NextUp,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Rankings => "Rankings",
            Panel::NextUp => "Next Up",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Rankings, 16.0, 16.0, 1240.0, 948.0),
        b.at(Panel::NextUp, 1272.0, 16.0, 632.0, 948.0),
    ]
}

#[component]
pub fn GameCompleted(id: String) -> Element {
    let state = use_app_state();

    let mut data_res = {
        let id = id.clone();
        use_resource(move || {
            let id = id.clone();
            async move {
                // getCompletedGame can return null (findUnique) → parse as Option
                let raw = net::query("stats.getCompletedGame", Some(json!({ "id": id }))).await?;
                if raw.is_null() {
                    return Ok::<Option<CompletedGameData>, String>(None);
                }
                let parsed: CompletedGameData =
                    serde_json::from_value(raw).map_err(|e| e.to_string())?;
                Ok(Some(parsed))
            }
        })
    };

    // First unstarted puzzle from the lobby list — best-effort: any failure or
    // empty list simply hides the "Up next" card.
    let next_res = use_resource(move || async move {
        let raw = net::query("gameList.get", None).await.ok()?;
        raw.as_array()?
            .iter()
            .find(|v| v.get("type").and_then(|t| t.as_str()) == Some("Game"))
            .and_then(|v| serde_json::from_value::<NextUpGame>(v.clone()).ok())
    });

    let is_starting = use_signal(|| false);
    let start_error = use_signal(String::new);
    let nav = use_navigator();

    let ws = use_workspace("game_completed_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    // Snapshot resource state via Rc so both panel arms of the body closure share it.
    // read_unchecked (not peek) keeps the component subscribed to the resource signal.
    let data_snap: Rc<Option<Result<Option<CompletedGameData>, String>>> =
        Rc::new((*data_res.read_unchecked()).clone());
    let next_up: Option<NextUpGame> = (*next_res.read_unchecked()).clone().flatten();
    let current_email: Option<String> = state.user().and_then(|u| u.email);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Rankings => {
                match data_snap.as_ref() {
                    None => status("muted", "Analyzing results…", true),
                    Some(Err(e)) => rsx! {
                        div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 3rem;",
                            div { class: "app-card", style: "max-width: 28rem; width: 100%; padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; font-family: monospace;",
                                span { class: "error", style: "font-size: .875rem; font-weight: 700; text-transform: uppercase;", "Error Loading Match Details" }
                                p { class: "muted", style: "font-size: .75rem;", "The requested game could not be found." }
                                p { class: "error", style: "font-size: .75rem;", "{e}" }
                                div { class: "row",
                                    button { class: "app-btn", onclick: move |_| data_res.restart(), "Retry" }
                                    Link { to: Route::Games {}, class: "app-btn", "Back to Lobby" }
                                }
                            }
                        }
                    },
                    Some(Ok(None)) => rsx! {
                        div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 3rem;",
                            div { class: "app-card", style: "max-width: 28rem; width: 100%; padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; font-family: monospace;",
                                span { class: "error", style: "font-size: .875rem; font-weight: 700; text-transform: uppercase;", "Game Not Found" }
                                p { class: "muted", style: "font-size: .75rem;", "This completed game could not be found." }
                                Link { to: Route::Games {}, class: "app-btn", style: "text-align: center; margin-top: .5rem;", "Back to Lobby" }
                            }
                        }
                    },
                    Some(Ok(Some(data))) => {
                        let mut rankings = data.game_stats.member_scores.clone();
                        rankings.sort_by(|a, b| b.score.cmp(&a.score));
                        let rankings_len = rankings.len();

                        rsx! {
                            div { style: "display: flex; flex-direction: column; gap: 2rem; height: 100%; overflow-y: auto;",

                                // Victory banner
                                div {
                                    class: "app-card",
                                    style: "padding: 2rem; text-align: center; display: flex; flex-direction: column; align-items: center; gap: .75rem; border-color: color-mix(in srgb, var(--pastel-green) 30%, transparent);",
                                    div {
                                        style: "width: 4rem; height: 4rem; background: color-mix(in srgb, var(--pastel-green) 10%, transparent); border: 1px solid color-mix(in srgb, var(--pastel-green) 30%, transparent); display: flex; align-items: center; justify-content: center; font-size: 2rem;",
                                        "🎉"
                                    }
                                    h1 {
                                        style: "font-size: 1.5rem; font-weight: 700; text-transform: uppercase; letter-spacing: .1em; color: var(--pastel-green); margin: 0;",
                                        "Crossword Solved!"
                                    }
                                    p {
                                        class: "muted",
                                        style: "font-size: .75rem; font-family: monospace; text-transform: uppercase; margin: 0;",
                                        "Game Room: "
                                        span { style: "color: var(--text-primary); font-weight: 700;", "{data.game.title}" }
                                    }
                                    span {
                                        class: "muted",
                                        style: "font-size: .625rem; font-family: monospace; text-transform: uppercase; border-top: 1px solid var(--border-app); padding-top: .75rem; width: 100%; max-width: 20rem;",
                                        "COMPLETED: "
                                        span { style: "color: var(--text-primary); font-weight: 700;", "{format_date(&data.created_at)}" }
                                    }
                                }

                                // Standings header
                                div { style: "display: flex; align-items: center; justify-content: space-between; font-family: monospace; padding: 0 .25rem;",
                                    h2 { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; margin: 0;", "Match Standings" }
                                    span { class: "muted", style: "font-size: .625rem; text-transform: uppercase;", "{rankings_len} Players" }
                                }

                                // Rank cards
                                div { style: "display: flex; flex-direction: column; gap: .75rem;",
                                    for (index, score_record) in rankings.iter().enumerate() {
                                        {
                                            let is_me = current_email.as_deref()
                                                .zip(score_record.member.user.email.as_deref())
                                                .map(|(a, b)| a == b)
                                                .unwrap_or(false);

                                            let display_name = score_record.member.user.name.as_deref()
                                                .or(score_record.member.user.email.as_deref())
                                                .unwrap_or("Anonymous");

                                            let total = score_record.correct_guesses + score_record.incorrect_guesses;
                                            let accuracy = if total > 0 {
                                                score_record.correct_guesses * 100 / total
                                            } else { 0 };

                                            let card_style = if is_me {
                                                "app-card cg-rank-card cg-rank-card-me"
                                            } else {
                                                "app-card cg-rank-card"
                                            };

                                            let rank_n = if index > 2 {
                                                format!("{}", index + 1)
                                            } else {
                                                (index + 1).to_string()
                                            };

                                            rsx! {
                                                div { class: "{card_style}",
                                                    // Rank badge + avatar + name
                                                    div { style: "display: flex; align-items: center; gap: 1rem; min-width: 0;",
                                                        RankBadge { index, label: rank_n }
                                                        div { class: "cg-avatar",
                                                            Identicon { seed: display_name.to_string(), size: 30 }
                                                        }
                                                        div { style: "display: flex; flex-direction: column; min-width: 0;",
                                                            span {
                                                                style: "font-size: .875rem; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: flex; align-items: center; gap: .375rem;",
                                                                "{display_name}"
                                                                if is_me {
                                                                    span { style: "font-size: .5rem; font-weight: 700; letter-spacing: .1em; color: var(--pastel-yellow); border: 1px solid color-mix(in srgb, var(--pastel-yellow) 40%, transparent); padding: 0 .25rem; text-transform: uppercase;", "YOU" }
                                                                }
                                                                if score_record.member.is_owner {
                                                                    span { style: "font-size: .5rem; color: var(--text-secondary); opacity: .6;", "👑" }
                                                                }
                                                            }
                                                            span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase; letter-spacing: .05em;", "{rank_name(index)}" }
                                                        }
                                                    }

                                                    // Scores
                                                    div { style: "display: flex; align-items: center; gap: 1.5rem; flex-shrink: 0;",
                                                        div { style: "display: flex; flex-direction: column; text-align: right;",
                                                            span { class: "muted", style: "font-size: .625rem; text-transform: uppercase;", "Accuracy" }
                                                            span { style: "font-size: .75rem; font-weight: 700; color: var(--text-primary);", "{accuracy}%" }
                                                        }
                                                        div { style: "display: flex; flex-direction: column; text-align: right;",
                                                            span { class: "muted", style: "font-size: .625rem; text-transform: uppercase;", "Guesses" }
                                                            span { style: "font-size: .75rem; font-weight: 700; display: flex; align-items: center; gap: .25rem;",
                                                                span { style: "color: var(--pastel-green);", "{score_record.correct_guesses}" }
                                                                span { class: "muted", "/" }
                                                                span { style: "color: var(--pastel-red);", "{score_record.incorrect_guesses}" }
                                                            }
                                                        }
                                                        div {
                                                            style: "display: flex; flex-direction: column; text-align: right; border-left: 1px solid var(--border-app); padding-left: 1rem; min-width: 4.375rem;",
                                                            title: "Scoring: each correct guess gives +10 pts, every incorrect guess subtracts -2 pts.",
                                                            span { class: "muted", style: "font-size: .625rem; text-transform: uppercase;", "Score" }
                                                            span { style: "font-size: 1rem; font-weight: 900; color: var(--pastel-yellow);", "{score_record.score}" }
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

            Panel::NextUp => {
                match data_snap.as_ref() {
                    None => status("muted", "Analyzing results…", true),
                    Some(Err(_)) | Some(Ok(None)) => rsx! {
                        div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 3rem;",
                            div { class: "app-card", style: "max-width: 28rem; width: 100%; padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; font-family: monospace;",
                                span { class: "error", style: "font-size: .875rem; font-weight: 700; text-transform: uppercase;", "Game Not Found" }
                                p { class: "muted", style: "font-size: .75rem;", "This completed game could not be found." }
                                Link { to: Route::Games {}, class: "app-btn", style: "text-align: center; margin-top: .5rem;", "← Back to Lobby" }
                            }
                        }
                    },
                    Some(Ok(Some(data))) => {
                        let total_questions =
                            data.game.questions.as_ref().map(|q| q.len()).unwrap_or(0);
                        let total_guesses: i64 = data
                            .game_stats
                            .member_scores
                            .iter()
                            .map(|r| r.correct_guesses + r.incorrect_guesses)
                            .sum();
                        let total_correct: i64 = data
                            .game_stats
                            .member_scores
                            .iter()
                            .map(|r| r.correct_guesses)
                            .sum();
                        let solve_precision = if total_guesses > 0 {
                            (total_correct * 100 / total_guesses) as i64
                        } else {
                            0
                        };
                        let rematch_id = data.game_id.clone();

                        rsx! {
                            div { style: "display: flex; flex-direction: column; gap: 1.5rem; height: 100%; overflow-y: auto;",

                                // Rematch — only when the server sent the source gameId
                                // (older servers don't; hide rather than guess).
                                if let Some(game_id) = rematch_id {
                                    div { class: "app-card", style: "padding: 1.5rem; display: flex; flex-direction: column; gap: .75rem; font-family: monospace;",
                                        h3 { style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; margin: 0;", "Rematch" }
                                        if !start_error.read().is_empty() {
                                            p { class: "error", style: "font-size: .75rem; margin: 0;", "{start_error}" }
                                        }
                                        button {
                                            class: "app-btn app-btn-active",
                                            style: "justify-content: center;",
                                            disabled: *is_starting.read(),
                                            onclick: move |_| {
                                                let game_id = game_id.clone();
                                                let mut is_starting = is_starting;
                                                let mut start_error = start_error;
                                                let nav = nav;
                                                spawn_local(async move {
                                                    is_starting.set(true);
                                                    start_error.set(String::new());
                                                    match net::mutation(
                                                        "activeGame.start",
                                                        Some(json!({ "gameId": game_id })),
                                                    )
                                                    .await
                                                    {
                                                        Ok(res) => {
                                                            if let Some(new_id) = res.get("id").and_then(|v| v.as_str()) {
                                                                nav.push(Route::GamePlay { id: new_id.to_string() });
                                                            } else {
                                                                start_error.set("Unexpected response from server.".into());
                                                            }
                                                        }
                                                        Err(e) => start_error.set(net::trpc_err_msg(e)),
                                                    }
                                                    is_starting.set(false);
                                                });
                                            },
                                            if *is_starting.read() { "Starting…" } else { "Play this puzzle again" }
                                        }
                                    }
                                }

                                // Up next — first unstarted puzzle from the lobby list.
                                if let Some(next) = next_up.clone() {
                                    Link {
                                        to: Route::GameNew { id: next.id.clone() },
                                        class: "app-card",
                                        style: "padding: 1.25rem 1.5rem; display: flex; flex-direction: column; gap: .25rem; font-family: monospace; text-decoration: none; color: inherit;",
                                        span { class: "muted", style: "font-size: .625rem; text-transform: uppercase; letter-spacing: .05em;", "Up Next" }
                                        span { style: "font-size: .875rem; font-weight: 700; color: var(--text-primary);",
                                            "{next.title} · {next.clues} clues"
                                        }
                                    }
                                }

                                // Metrics mini-card
                                div { class: "app-card", style: "padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; font-family: monospace;",
                                    h3 { style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; border-bottom: 1px solid var(--border-app); padding-bottom: .75rem; margin: 0;", "Crossword Metrics" }
                                    div { style: "display: grid; grid-template-columns: repeat(3, 1fr); gap: .75rem;",
                                        StatTile { label: "Source".to_string(), value: data.game.source.clone() }
                                        StatTile { label: "Clues".to_string(), value: total_questions.to_string() }
                                        StatTile { label: "Precision".to_string(), value: format!("{solve_precision}%") }
                                    }
                                }

                                // Links row
                                div { style: "display: flex; gap: .75rem;",
                                    Link {
                                        to: Route::Stats {},
                                        class: "app-btn",
                                        style: "flex: 1; justify-content: center; text-align: center; font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; color: var(--pastel-yellow); border-color: color-mix(in srgb, var(--pastel-yellow) 30%, transparent);",
                                        "Career Stats →"
                                    }
                                    Link {
                                        to: Route::Games {},
                                        class: "app-btn",
                                        style: "flex: 1; justify-content: center; text-align: center; font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;",
                                        "← Back to Lobby"
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
        style { {COMPLETED_CSS} }
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

const COMPLETED_CSS: &str = r#"
.cg-rank-card {
    padding: 1rem 1.25rem;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    font-family: monospace;
    transition: all .2s ease;
}
.cg-rank-card-me {
    border-color: color-mix(in srgb, var(--pastel-yellow) 40%, transparent);
    background: color-mix(in srgb, var(--pastel-yellow) 2%, transparent);
}
.cg-avatar {
    width: 2.25rem;
    height: 2.25rem;
    background: var(--bg-cell-empty);
    border: 1px solid var(--border-app);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: .75rem;
    font-weight: 700;
    color: var(--text-secondary);
    flex-shrink: 0;
}
"#;
