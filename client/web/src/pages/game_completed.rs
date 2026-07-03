use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::rc::Rc;

use crate::components::identicon::Identicon;
use crate::net;
use crate::store::use_app_state;
use crate::Route;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompletedGameData {
    id: String,
    created_at: String,
    game: GameInfo,
    game_stats: GameStats,
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

fn format_date(s: &str) -> String {
    // minimal: take the date part from ISO string
    s.split('T').next().unwrap_or(s).to_string()
}

fn rank_badge_style(index: usize) -> &'static str {
    match index {
        0 => "background: var(--pastel-yellow); color: #0f172a; border-color: var(--pastel-yellow);",
        1 => "background: #cbd5e1; color: #0f172a; border-color: #cbd5e1;",
        2 => "background: #d97706; color: #f8fafc; border-color: #d97706;",
        _ => "background: var(--bg-cell-empty); color: var(--text-secondary); border-color: var(--border-app);",
    }
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
    Summary,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Rankings => "Rankings",
            Panel::Summary => "Summary",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Rankings, 16.0, 16.0, 1240.0, 948.0),
        b.at(Panel::Summary, 1272.0, 16.0, 632.0, 948.0),
    ]
}

#[component]
pub fn GameCompleted(id: String) -> Element {
    let state = use_app_state();

    let data_res = {
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

    let ws = use_workspace("game_completed_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    // Snapshot resource state via Rc so both panel arms of the body closure share it.
    // read_unchecked (not peek) keeps the component subscribed to the resource signal.
    let data_snap: Rc<Option<Result<Option<CompletedGameData>, String>>> =
        Rc::new((*data_res.read_unchecked()).clone());
    let current_email: Option<String> = state.user().and_then(|u| u.email);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Rankings => {
                match data_snap.as_ref() {
                    None => rsx! {
                        div { class: "muted", style: "padding: 2rem; text-align: center; font-family: monospace; font-size: .75rem;", "Analyzing results..." }
                    },
                    Some(Err(e)) => rsx! {
                        div { style: "display: flex; flex-direction: column; align-items: center; justify-content: center; padding: 3rem;",
                            div { class: "app-card", style: "max-width: 28rem; width: 100%; padding: 1.5rem; display: flex; flex-direction: column; gap: 1rem; font-family: monospace;",
                                span { class: "error", style: "font-size: .875rem; font-weight: 700; text-transform: uppercase;", "Error Loading Match Details" }
                                p { class: "muted", style: "font-size: .75rem;", "The requested game could not be found." }
                                p { class: "error", style: "font-size: .75rem;", "{e}" }
                                Link { to: Route::Games {}, class: "app-btn", style: "text-align: center; margin-top: .5rem;", "Back to Lobby" }
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
                                    style: "padding: 2rem; text-align: center; display: flex; flex-direction: column; align-items: center; gap: .75rem; border-color: rgba(168,230,207,0.3);",
                                    div {
                                        style: "width: 4rem; height: 4rem; border-radius: 50%; background: rgba(168,230,207,0.1); border: 1px solid rgba(168,230,207,0.3); display: flex; align-items: center; justify-content: center; font-size: 2rem;",
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
                                                        div {
                                                            class: "cg-rank-badge",
                                                            style: "{rank_badge_style(index)}",
                                                            "{rank_n}"
                                                        }
                                                        div { class: "cg-avatar",
                                                            Identicon { seed: display_name.to_string(), size: 30 }
                                                        }
                                                        div { style: "display: flex; flex-direction: column; min-width: 0;",
                                                            span {
                                                                style: "font-size: .875rem; font-weight: 700; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: flex; align-items: center; gap: .375rem;",
                                                                "{display_name}"
                                                                if is_me {
                                                                    span { style: "font-size: .5rem; font-weight: 700; letter-spacing: .1em; color: var(--pastel-yellow); border: 1px solid rgba(254,234,153,0.4); padding: 0 .25rem; border-radius: .125rem; text-transform: uppercase;", "YOU" }
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
                                                        div { style: "display: flex; flex-direction: column; text-align: right; border-left: 1px solid var(--border-app); padding-left: 1rem; min-width: 4.375rem;",
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

            Panel::Summary => {
                match data_snap.as_ref() {
                    None => rsx! {
                        div { class: "muted", style: "padding: 2rem; text-align: center; font-family: monospace; font-size: .75rem;", "Analyzing results..." }
                    },
                    Some(Err(_)) | Some(Ok(None)) => rsx! {
                        div { class: "muted", style: "padding: 2rem; text-align: center; font-family: monospace; font-size: .75rem;", "—" }
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

                        rsx! {
                            div { style: "display: flex; flex-direction: column; gap: 1.5rem; height: 100%; overflow-y: auto;",

                                div { class: "app-card", style: "padding: 1.5rem; display: flex; flex-direction: column; gap: 1.25rem; font-family: monospace;",
                                    h3 { style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; border-bottom: 1px solid var(--border-app); padding-bottom: .75rem; margin: 0;", "Crossword Metrics" }
                                    div { style: "display: flex; flex-direction: column; gap: 1rem;",
                                        div { style: "display: flex; justify-content: space-between; align-items: center; font-size: .75rem;",
                                            span { class: "muted", style: "text-transform: uppercase;", "Source Mode" }
                                            span { style: "font-weight: 700; text-transform: uppercase; background: var(--bg-cell-empty); border: 1px solid var(--border-app); padding: .125rem .5rem; border-radius: .25rem; font-size: .625rem;", "{data.game.source}" }
                                        }
                                        div { style: "display: flex; justify-content: space-between; align-items: center; font-size: .75rem;",
                                            span { class: "muted", style: "text-transform: uppercase;", "Total Clues" }
                                            span { style: "font-weight: 700; color: var(--text-primary);", "{total_questions} Clues" }
                                        }
                                        div { style: "display: flex; justify-content: space-between; align-items: center; font-size: .75rem;",
                                            span { class: "muted", style: "text-transform: uppercase;", "Total Guesses" }
                                            span { style: "font-weight: 700; color: var(--text-primary);", "{total_guesses}" }
                                        }
                                        div { style: "display: flex; justify-content: space-between; align-items: center; font-size: .75rem;",
                                            span { class: "muted", style: "text-transform: uppercase;", "Solve Precision" }
                                            span { style: "font-weight: 700; color: var(--pastel-green);", "{solve_precision}%" }
                                        }
                                    }

                                    div { style: "height: 1px; background: var(--border-app); width: 100%;" }

                                    div { style: "display: flex; flex-direction: column; gap: .625rem;",
                                        Link {
                                            to: Route::Stats {},
                                            class: "app-btn",
                                            style: "display: flex; align-items: center; justify-content: center; gap: .5rem; padding: .625rem 1rem; text-align: center; font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; color: var(--pastel-yellow); border-color: rgba(254,234,153,0.3);",
                                            "Career Stats Dashboard"
                                        }
                                        Link {
                                            to: Route::Games {},
                                            class: "app-btn",
                                            style: "display: flex; align-items: center; justify-content: center; gap: .5rem; padding: .625rem 1rem; text-align: center; font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;",
                                            "Back to Lobby"
                                        }
                                    }
                                }

                                // Scoring info
                                div {
                                    class: "app-card",
                                    style: "padding: 1.25rem; border-color: rgba(254,234,153,0.1); background: rgba(254,234,153,0.02); font-family: monospace; font-size: .625rem; line-height: 1.6; color: var(--text-secondary); display: flex; gap: .5rem;",
                                    span { style: "font-size: .875rem;", "💡" }
                                    div {
                                        span { style: "color: var(--text-primary); font-weight: 700; text-transform: uppercase; display: block; margin-bottom: .25rem;", "Scoring Mechanics" }
                                        "Each correct guess gives "
                                        span { style: "color: var(--pastel-green); font-weight: 700;", "+10 pts" }
                                        ". Every incorrect guess subtracts "
                                        span { style: "color: var(--pastel-red); font-weight: 700;", "-2 pts" }
                                        ". Aim for perfect precision!"
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
    border-color: rgba(254,234,153,0.4);
    background: rgba(254,234,153,0.02);
}
.cg-rank-badge {
    width: 2rem;
    height: 2rem;
    border-radius: .5rem;
    border: 1px solid;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: .75rem;
    flex-shrink: 0;
}
.cg-avatar {
    width: 2.25rem;
    height: 2.25rem;
    border-radius: 50%;
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
