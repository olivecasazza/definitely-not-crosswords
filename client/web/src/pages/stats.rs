use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::net;
use crate::store::use_app_state;
use crate::Route;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LeaderboardEntry {
    id: String,
    name: String,
    email: Option<String>,
    games_played: i64,
    total_score: i64,
    accuracy: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerStub {
    id: String,
    name: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CareerStats {
    games_played: i64,
    total_score: i64,
    total_correct: i64,
    total_incorrect: i64,
    accuracy: i64,
    global_rank: i64,
    total_players: i64,
    recent_games: Vec<Option<RecentGame>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecentGame {
    id: String,
    title: String,
    created_at: String,
    score: i64,
    correct_guesses: i64,
    incorrect_guesses: i64,
    rank: i64,
    total_participants: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct H2HData {
    opponent_name: String,
    games_played: i64,
    record: H2HRecord,
    scores: H2HScores,
    accuracy: H2HAccuracy,
    matches: Vec<H2HMatch>,
}

#[derive(Debug, Clone, Deserialize)]
struct H2HRecord {
    wins: i64,
    losses: i64,
    ties: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct H2HScores {
    user_total: i64,
    user_avg: i64,
    opponent_total: i64,
    opponent_avg: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct H2HAccuracy {
    user: i64,
    opponent: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct H2HMatch {
    game_id: String,
    title: String,
    created_at: String,
    user_score: i64,
    opponent_score: i64,
    result: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn date_short(s: &str) -> String {
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

/// Width percent for a comparison bar (returns 0.0-100.0).
fn bar_pct(a: i64, b: i64) -> f64 {
    let total = a + b;
    if total == 0 {
        50.0
    } else {
        a as f64 / total as f64 * 100.0
    }
}

// ---------------------------------------------------------------------------
// Panel kind
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Leaderboard,
    Career,
    Compare,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Leaderboard => "Leaderboard",
            Panel::Career => "Career",
            Panel::Compare => "Compare",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Leaderboard, 16.0, 16.0, 936.0, 948.0),
        b.at(Panel::Career, 968.0, 16.0, 936.0, 466.0),
        b.at(Panel::Compare, 968.0, 498.0, 936.0, 466.0),
    ]
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

#[component]
pub fn Stats() -> Element {
    let state = use_app_state();
    let selected_opponent_id = use_signal(|| String::new());

    // Global leaderboard — no session dependency
    let leaderboard_res = use_resource(move || async move {
        net::query_as::<Vec<LeaderboardEntry>>("stats.getGlobalLeaderboard", None).await
    });

    // Career stats — depends on session
    let career_res = use_resource(move || async move {
        let email = state.user().and_then(|u| u.email)?;
        Some(
            net::query_as::<CareerStats>("stats.getUserStats", Some(json!({ "email": email })))
                .await,
        )
    });

    // All players for H2H dropdown — depends on session (to exclude self)
    let players_res = use_resource(move || async move {
        // Omit excludeEmail entirely when unknown — the zod schema rejects an
        // explicit null (it's `string().optional()`, not nullable).
        let input = match state.user().and_then(|u| u.email) {
            Some(email) => json!({ "excludeEmail": email }),
            None => json!({}),
        };
        net::query_as::<Vec<PlayerStub>>("stats.getAllPlayers", Some(input)).await
    });

    // H2H — only fires when opponent is selected
    let h2h_res = use_resource(move || async move {
        let opp = selected_opponent_id.read().clone();
        if opp.is_empty() {
            return None;
        }
        Some(
            net::query_as::<H2HData>("stats.getHeadToHead", Some(json!({ "opponentId": opp })))
                .await,
        )
    });

    let ws = use_workspace("stats_layout", default_layout);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Leaderboard => {
                let current_email = state.user().and_then(|u| u.email);
                rsx! {
                    div { style: "display: flex; flex-direction: column; gap: 1.5rem; height: 100%; overflow-y: auto;",
                        match &*leaderboard_res.read_unchecked() {
                            None => rsx! { div { class: "muted st-loading", "Fetching rankings..." } },
                            Some(Err(e)) => rsx! { div { class: "error", style: "padding: 1rem; font-size: .75rem; font-family: monospace;", "{e}" } },
                            Some(Ok(entries)) if entries.is_empty() => rsx! {
                                div { class: "app-card", style: "padding: 3rem; text-align: center; font-size: .75rem; color: var(--text-secondary);",
                                    "No completed games or player statistics available yet."
                                }
                            },
                            Some(Ok(entries)) => rsx! {
                                div { style: "display: flex; flex-direction: column; gap: 1rem;",
                                    // Top 3 podium
                                    if entries.len() >= 1 {
                                        div { class: "st-podium",
                                            // 2nd
                                            if entries.len() >= 2 {
                                                div { class: "app-card st-podium-card",
                                                    div { class: "st-podium-badge", style: "background: #cbd5e1; color: #0f172a; border-color: #cbd5e1;", "2" }
                                                    span { style: "font-size: .875rem; font-weight: 700; color: var(--text-primary);", "{entries[1].name}" }
                                                    span { style: "font-size: .625rem; color: var(--pastel-yellow); font-weight: 900; text-transform: uppercase;", "{entries[1].total_score} pts" }
                                                    span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "{entries[1].games_played} games · {entries[1].accuracy}% Acc" }
                                                }
                                            }
                                            // 1st (bigger)
                                            div { class: "app-card st-podium-card st-podium-first",
                                                div { class: "st-podium-badge", style: "background: var(--pastel-yellow); color: #0f172a; border-color: var(--pastel-yellow); width: 3rem; height: 3rem; font-size: 1.25rem;", "👑" }
                                                span { style: "font-size: 1rem; font-weight: 900; color: var(--text-primary);", "{entries[0].name}" }
                                                span { style: "font-size: .875rem; color: var(--pastel-yellow); font-weight: 900; text-transform: uppercase;", "{entries[0].total_score} pts" }
                                                span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "{entries[0].games_played} games · {entries[0].accuracy}% Acc" }
                                            }
                                            // 3rd
                                            if entries.len() >= 3 {
                                                div { class: "app-card st-podium-card",
                                                    div { class: "st-podium-badge", style: "background: #d97706; color: #f8fafc; border-color: #d97706;", "3" }
                                                    span { style: "font-size: .875rem; font-weight: 700; color: var(--text-primary);", "{entries[2].name}" }
                                                    span { style: "font-size: .625rem; color: var(--pastel-yellow); font-weight: 900; text-transform: uppercase;", "{entries[2].total_score} pts" }
                                                    span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "{entries[2].games_played} games · {entries[2].accuracy}% Acc" }
                                                }
                                            }
                                        }
                                    }
                                    // Full table
                                    div { class: "app-card", style: "overflow: hidden;",
                                        div { style: "overflow-x: auto;",
                                            table { class: "st-table",
                                                thead {
                                                    tr { class: "st-thead-row",
                                                        th { "Rank" }
                                                        th { "Player" }
                                                        th { style: "text-align: center;", "Games" }
                                                        th { style: "text-align: center;", "Accuracy" }
                                                        th { style: "text-align: right;", "Career Score" }
                                                    }
                                                }
                                                tbody {
                                                    for (i, entry) in entries.iter().enumerate() {
                                                        {
                                                            let is_me = current_email.as_deref()
                                                                .zip(entry.email.as_deref())
                                                                .map(|(a, b)| a == b)
                                                                .unwrap_or(false);
                                                            let acc_color = if entry.accuracy >= 75 {
                                                                "color: var(--pastel-green);"
                                                            } else if entry.accuracy >= 45 {
                                                                "color: var(--pastel-yellow);"
                                                            } else {
                                                                "color: var(--pastel-red);"
                                                            };
                                                            let row_cls = if is_me { "st-table-row st-table-row-me" } else { "st-table-row" };
                                                            rsx! {
                                                                tr { class: "{row_cls}",
                                                                    td {
                                                                        span { class: "st-rank-badge", style: "{rank_badge_style(i)}", "{i+1}" }
                                                                    }
                                                                    td {
                                                                        span { style: "display: flex; align-items: center; gap: .375rem;",
                                                                            "{entry.name}"
                                                                            if is_me {
                                                                                span { style: "font-size: .5rem; font-weight: 900; border: 1px solid rgba(254,234,153,0.3); color: var(--pastel-yellow); padding: 0 .25rem; border-radius: .125rem; text-transform: uppercase;", "YOU" }
                                                                            }
                                                                        }
                                                                    }
                                                                    td { style: "text-align: center; color: var(--text-secondary);", "{entry.games_played}" }
                                                                    td { style: "text-align: center;",
                                                                        span { style: "{acc_color}", "{entry.accuracy}%" }
                                                                    }
                                                                    td { style: "text-align: right; font-weight: 900; color: var(--pastel-yellow); font-size: .875rem;", "{entry.total_score}" }
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
            }
            Panel::Career => rsx! {
                div { style: "display: flex; flex-direction: column; gap: 1.5rem; height: 100%; overflow-y: auto;",
                    match &*career_res.read_unchecked() {
                        None => rsx! { div { class: "muted st-loading", "Compiling career file..." } },
                        Some(None) => rsx! { div { class: "muted st-loading", "Sign in to view career stats." } },
                        Some(Some(Err(e))) => rsx! { div { class: "error", style: "padding: 1rem; font-size: .75rem; font-family: monospace;", "{e}" } },
                        Some(Some(Ok(stats))) if stats.games_played == 0 => rsx! {
                            div { class: "app-card", style: "padding: 3rem; text-align: center; font-size: .75rem; color: var(--text-secondary); display: flex; flex-direction: column; align-items: center; gap: 1rem;",
                                span { "No games played yet on this profile." }
                                Link { to: Route::Games {}, class: "app-btn", style: "font-weight: 700; text-transform: uppercase; letter-spacing: .05em; color: var(--pastel-yellow);", "Launch a Game" }
                            }
                        },
                        Some(Some(Ok(stats))) => rsx! {
                            div { style: "display: flex; flex-direction: column; gap: 2rem;",

                                // Stat cards row
                                div { style: "display: grid; grid-template-columns: repeat(2, 1fr); gap: 1rem;",
                                    div { class: "app-card st-stat-card",
                                        span { class: "muted st-stat-label", "Global Rank" }
                                        div { style: "display: flex; align-items: baseline; gap: .25rem;",
                                            span { style: "font-size: 1.5rem; font-weight: 900; color: var(--pastel-yellow);", "#{stats.global_rank}" }
                                            span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "of {stats.total_players}" }
                                        }
                                    }
                                    div { class: "app-card st-stat-card",
                                        span { class: "muted st-stat-label", "Career Score" }
                                        span { style: "font-size: 1.5rem; font-weight: 900; color: var(--pastel-yellow);", "{stats.total_score}" }
                                    }
                                    div { class: "app-card st-stat-card",
                                        span { class: "muted st-stat-label", "Games Played" }
                                        span { style: "font-size: 1.5rem; font-weight: 900; color: var(--text-primary);", "{stats.games_played}" }
                                    }
                                    div { class: "app-card st-stat-card",
                                        span { class: "muted st-stat-label", "Solve Accuracy" }
                                        span { style: "font-size: 1.5rem; font-weight: 900; color: var(--pastel-green);", "{stats.accuracy}%" }
                                    }
                                }

                                // Accuracy breakdown bar
                                div { class: "app-card", style: "padding: 1.25rem; display: flex; flex-direction: column; gap: 1rem;",
                                    span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase;", "Accuracy Breakdown" }
                                    div { style: "display: flex; flex-direction: column; gap: .5rem;",
                                        div { style: "display: flex; justify-content: space-between; font-size: .625rem; color: var(--text-secondary); text-transform: uppercase;",
                                            span { "Correct: {stats.total_correct}" }
                                            span { "Incorrect: {stats.total_incorrect}" }
                                        }
                                        div { class: "st-bar-track",
                                            div { class: "st-bar-segment", style: "width: {stats.accuracy}%; background: var(--pastel-green);" }
                                            div { class: "st-bar-segment", style: "width: {100 - stats.accuracy}%; background: var(--pastel-red);" }
                                        }
                                        span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase; line-height: 1.6;",
                                            "Your overall ratio is "
                                            span { style: "color: var(--pastel-green); font-weight: 700;", "{stats.total_correct} correct guesses" }
                                            " out of "
                                            span { style: "color: var(--text-primary); font-weight: 700;", "{stats.total_correct + stats.total_incorrect} total guesses" }
                                            "."
                                        }
                                    }
                                }

                                // Recent games
                                div { style: "display: flex; flex-direction: column; gap: .75rem;",
                                    span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Match Log History" }
                                    div { style: "display: flex; flex-direction: column; gap: .75rem;",
                                        for game_opt in stats.recent_games.iter() {
                                            if let Some(game) = game_opt {
                                                div { class: "app-card", style: "padding: 1rem 1.25rem; display: flex; flex-direction: column; gap: .75rem;",
                                                    div { style: "display: flex; flex-direction: row; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: .75rem;",
                                                        div { style: "display: flex; flex-direction: column; min-width: 0;",
                                                            span { style: "font-size: .875rem; font-weight: 700; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{game.title}" }
                                                            span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase; letter-spacing: .05em; margin-top: .25rem;", "Played on {date_short(&game.created_at)}" }
                                                        }
                                                        div { style: "display: flex; align-items: center; gap: 1.5rem; flex-shrink: 0; font-family: monospace;",
                                                            div { style: "display: flex; flex-direction: column;",
                                                                span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "Guesses" }
                                                                span { style: "font-size: .75rem; font-weight: 600; color: var(--text-primary);",
                                                                    span { style: "color: var(--pastel-green);", "{game.correct_guesses}" }
                                                                    " / "
                                                                    span { style: "color: var(--pastel-red);", "{game.incorrect_guesses}" }
                                                                }
                                                            }
                                                            div { style: "display: flex; flex-direction: column;",
                                                                span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "Place" }
                                                                span { style: "font-size: .75rem; font-weight: 700; color: var(--text-primary);",
                                                                    "#{game.rank} "
                                                                    span { class: "muted", style: "font-size: .5625rem; font-weight: 400; text-transform: uppercase;", "of {game.total_participants}" }
                                                                }
                                                            }
                                                            div { style: "display: flex; flex-direction: column; border-left: 1px solid var(--border-app); padding-left: 1rem; min-width: 3.125rem;",
                                                                span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "Score" }
                                                                span { style: "font-size: .875rem; font-weight: 900; color: var(--pastel-yellow);", "{game.score}" }
                                                            }
                                                            Link {
                                                                to: Route::GameCompleted { id: game.id.clone() },
                                                                class: "app-btn",
                                                                style: "font-size: .625rem; padding: .25rem .625rem; text-transform: uppercase; font-weight: 700;",
                                                                "Review"
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
            },
            Panel::Compare => rsx! {
                div { style: "display: flex; flex-direction: column; gap: 1.5rem; height: 100%; overflow-y: auto;",

                    // Selector
                    div { class: "app-card", style: "padding: 1.25rem; display: flex; flex-direction: column; gap: 1rem;",
                        div { style: "display: flex; flex-direction: column;",
                            span { style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; font-family: monospace;", "Select Opponent" }
                            span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase; margin-top: .125rem;", "Compare your career performance side-by-side" }
                        }
                        select {
                            class: "app-input",
                            style: "padding: .5rem .75rem; font-size: .75rem; font-family: monospace; text-transform: uppercase; font-weight: 700; max-width: 20rem;",
                            value: "{selected_opponent_id}",
                            onchange: move |e| selected_opponent_id.clone().set(e.value()),
                            option { value: "", disabled: true, "-- CHOOSE PLAYER --" }
                            match &*players_res.read_unchecked() {
                                Some(Ok(players)) => rsx! {
                                    for p in players {
                                        {
                                            let label = p.name.as_deref().or(p.email.as_deref()).unwrap_or("?");
                                            let pid = p.id.clone();
                                            rsx! {
                                                option { value: "{pid}", "{label}" }
                                            }
                                        }
                                    }
                                },
                                _ => rsx! {}
                            }
                        }
                    }

                    if selected_opponent_id.read().is_empty() {
                        div { class: "app-card", style: "padding: 3rem; text-align: center; font-size: .75rem; color: var(--text-secondary);",
                            "Select another player from the dropdown to unlock head-to-head comparison records."
                        }
                    } else {
                        match &*h2h_res.read_unchecked() {
                            None => rsx! { div { class: "muted st-loading", "Computing combat records..." } },
                            Some(None) => rsx! {},
                            Some(Some(Err(e))) => rsx! {
                                div { class: "error", style: "padding: 1rem; font-size: .75rem; font-family: monospace;", "{e}" }
                            },
                            Some(Some(Ok(h2h))) => rsx! {
                                div { style: "display: flex; flex-direction: column; gap: 2rem;",

                                    // Record banner
                                    div { class: "app-card", style: "padding: 1.5rem; text-align: center; display: flex; flex-direction: column; align-items: center; gap: .75rem; border-color: rgba(254,234,153,0.2);",
                                        h3 { class: "muted", style: "font-size: .625rem; text-transform: uppercase; letter-spacing: .1em; font-weight: 700; margin: 0;", "CO-OP MATCH RECORD" }
                                        div { style: "display: flex; align-items: center; gap: 1rem; font-size: 1.5rem; font-weight: 900;",
                                            span { style: "color: var(--pastel-green);", "{h2h.record.wins} W" }
                                            span { class: "muted", style: "font-size: 1rem; font-weight: 400; opacity: .3;", "—" }
                                            span { style: "color: var(--pastel-red);", "{h2h.record.losses} L" }
                                            span { class: "muted", style: "font-size: 1rem; font-weight: 400; opacity: .3;", "—" }
                                            span { class: "muted", "{h2h.record.ties} T" }
                                        }
                                        span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase; letter-spacing: .05em; border-top: 1px solid var(--border-app); padding-top: .625rem; width: 100%; max-width: 20rem;",
                                            "Total Shared Matches: "
                                            span { style: "color: var(--text-primary); font-weight: 700;", "{h2h.games_played}" }
                                        }
                                    }

                                    // Stat comparison
                                    div { style: "display: flex; flex-direction: column; gap: 1rem;",
                                        span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Stat Comparison" }
                                        div { class: "app-card", style: "padding: 1.5rem; display: flex; flex-direction: column; gap: 1.5rem; font-family: monospace; font-size: .75rem;",

                                            // Total score
                                            div { class: "st-stat-row",
                                                div { style: "display: flex; justify-content: space-between; font-weight: 700;",
                                                    span { style: "text-transform: uppercase;", "Total Shared Score" }
                                                    span { style: "display: flex; gap: 1rem;",
                                                        span { style: "color: var(--text-primary);", "You: {h2h.scores.user_total}" }
                                                        span { class: "muted", "Them: {h2h.scores.opponent_total}" }
                                                    }
                                                }
                                                div { class: "st-bar-track",
                                                    {
                                                        let pct = bar_pct(h2h.scores.user_total, h2h.scores.opponent_total);
                                                        rsx! {
                                                            div { class: "st-bar-segment", style: "width: {pct:.0}%; background: var(--pastel-yellow);" }
                                                            div { class: "st-bar-segment", style: "width: {100.0 - pct:.0}%; background: #64748b;" }
                                                        }
                                                    }
                                                }
                                                span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;",
                                                    "Yellow: You ({h2h.scores.user_total}) · Grey: Opponent ({h2h.scores.opponent_total})"
                                                }
                                            }

                                            // Avg score
                                            div { class: "st-stat-row",
                                                div { style: "display: flex; justify-content: space-between; font-weight: 700;",
                                                    span { style: "text-transform: uppercase;", "Average Match Score" }
                                                    span { style: "display: flex; gap: 1rem;",
                                                        span { style: "color: var(--text-primary);", "You: {h2h.scores.user_avg}" }
                                                        span { class: "muted", "Them: {h2h.scores.opponent_avg}" }
                                                    }
                                                }
                                                div { class: "st-bar-track",
                                                    {
                                                        let pct = bar_pct(h2h.scores.user_avg, h2h.scores.opponent_avg);
                                                        rsx! {
                                                            div { class: "st-bar-segment", style: "width: {pct:.0}%; background: var(--pastel-yellow);" }
                                                            div { class: "st-bar-segment", style: "width: {100.0 - pct:.0}%; background: #64748b;" }
                                                        }
                                                    }
                                                }
                                            }

                                            // Accuracy
                                            div { class: "st-stat-row",
                                                div { style: "display: flex; justify-content: space-between; font-weight: 700;",
                                                    span { style: "text-transform: uppercase;", "Shared Guess Accuracy" }
                                                    span { style: "display: flex; gap: 1rem;",
                                                        span { style: "color: var(--pastel-green);", "You: {h2h.accuracy.user}%" }
                                                        span { class: "muted", "Them: {h2h.accuracy.opponent}%" }
                                                    }
                                                }
                                                div { class: "st-bar-track",
                                                    {
                                                        let pct = bar_pct(h2h.accuracy.user, h2h.accuracy.opponent);
                                                        rsx! {
                                                            div { class: "st-bar-segment", style: "width: {pct:.0}%; background: var(--pastel-green);" }
                                                            div { class: "st-bar-segment", style: "width: {100.0 - pct:.0}%; background: #64748b;" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // Match log
                                    div { style: "display: flex; flex-direction: column; gap: 1rem;",
                                        span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Combat Match Log" }

                                        if h2h.matches.is_empty() {
                                            div { class: "app-card", style: "padding: 2rem; text-align: center; font-size: .75rem; color: var(--text-secondary);",
                                                "You haven't played any co-op crossword games with this player yet."
                                            }
                                        } else {
                                            div { style: "display: flex; flex-direction: column; gap: .75rem;",
                                                for m in h2h.matches.iter() {
                                                    {
                                                        let outcome_color = match m.result.as_str() {
                                                            "WIN" => "color: var(--pastel-green);",
                                                            "LOSS" => "color: var(--pastel-red);",
                                                            _ => "color: var(--text-secondary);",
                                                        };
                                                        rsx! {
                                                            div { class: "app-card", style: "padding: 1rem 1.25rem; display: flex; flex-direction: row; align-items: center; justify-content: space-between; gap: 1rem; flex-wrap: wrap; font-family: monospace;",
                                                                div { style: "display: flex; flex-direction: column; min-width: 0;",
                                                                    span { style: "font-size: .875rem; font-weight: 700; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{m.title}" }
                                                                    span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase; letter-spacing: .05em; margin-top: .25rem;", "Played on {date_short(&m.created_at)}" }
                                                                }
                                                                div { style: "display: flex; align-items: center; gap: 1.5rem; flex-shrink: 0;",
                                                                    div { style: "display: flex; flex-direction: column;",
                                                                        span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "Match Scores" }
                                                                        span { style: "font-size: .75rem; font-weight: 700; color: var(--text-primary);",
                                                                            "You "
                                                                            span { style: "color: var(--pastel-yellow);", "{m.user_score}" }
                                                                            " — "
                                                                            span { class: "muted", "{m.opponent_score}" }
                                                                            " Them"
                                                                        }
                                                                    }
                                                                    div { style: "display: flex; flex-direction: column; min-width: 4.375rem;",
                                                                        span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "Outcome" }
                                                                        span { style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em; {outcome_color}", "{m.result}" }
                                                                    }
                                                                    Link {
                                                                        to: Route::GameCompleted { id: m.game_id.clone() },
                                                                        class: "app-btn",
                                                                        style: "font-size: .625rem; padding: .25rem .625rem; text-transform: uppercase; font-weight: 700;",
                                                                        "Stats"
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
                }
            },
        }
    };

    rsx! {
        style { {STATS_CSS} }
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

const STATS_CSS: &str = r#"
.st-page {
    flex: 1;
    width: 100%;
    max-width: 64rem;
    margin: 0 auto;
    padding: 2rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 2rem;
    overflow-y: auto;
}
.st-header {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    border-bottom: 1px solid var(--border-app);
    padding-bottom: 1.5rem;
}
@media (min-width: 640px) {
    .st-header {
        flex-direction: row;
        align-items: center;
        justify-content: space-between;
    }
}
.st-title {
    font-size: 1.5rem;
    font-weight: 900;
    font-family: monospace;
    text-transform: uppercase;
    letter-spacing: .1em;
    color: var(--color-primary, var(--text-primary));
    margin: 0;
}
.st-tabs {
    display: flex;
    background: var(--bg-cell-empty);
    border: 1px solid var(--border-app);
    border-radius: .5rem;
    padding: .25rem;
    gap: 0;
    width: max-content;
    font-family: monospace;
    font-size: .75rem;
}
.st-tab {
    padding: .375rem .75rem;
    border-radius: .375rem;
    text-transform: uppercase;
    letter-spacing: .05em;
    font-weight: 700;
    transition: all .15s ease;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    cursor: pointer;
}
.st-tab:hover { color: var(--text-primary); }
.st-tab-active {
    background: var(--bg-card);
    border: 1px solid var(--border-app);
    color: var(--text-primary);
}
.st-loading {
    padding: 2rem;
    text-align: center;
    font-family: monospace;
    font-size: .75rem;
    text-transform: uppercase;
    letter-spacing: .05em;
}
.st-podium {
    display: grid;
    grid-template-columns: 1fr;
    gap: 1rem;
    margin-bottom: .5rem;
}
@media (min-width: 480px) {
    .st-podium { grid-template-columns: repeat(3, 1fr); }
}
.st-podium-card {
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: .5rem;
    font-family: monospace;
}
.st-podium-first {
    transform: scale(1.04);
}
.st-podium-badge {
    width: 2.5rem;
    height: 2.5rem;
    border-radius: .5rem;
    border: 1px solid;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: .875rem;
}
.st-table {
    width: 100%;
    text-align: left;
    border-collapse: collapse;
}
.st-thead-row {
    border-bottom: 1px solid var(--border-app);
    background: var(--bg-cell-empty);
}
.st-thead-row th {
    padding: .875rem 1rem;
    font-size: .625rem;
    text-transform: uppercase;
    letter-spacing: .05em;
    font-weight: 700;
    color: var(--text-secondary);
    font-family: monospace;
}
.st-table-row {
    border-bottom: 1px solid rgba(63,63,70,0.5);
    font-size: .75rem;
    font-family: monospace;
    transition: all .15s ease;
}
.st-table-row:hover { background: rgba(63,63,70,0.15); }
.st-table-row-me { background: rgba(254,234,153,0.02); font-weight: 700; }
.st-table-row td { padding: .875rem 1rem; }
.st-rank-badge {
    display: inline-flex;
    width: 1.5rem;
    height: 1.5rem;
    border-radius: .25rem;
    border: 1px solid;
    font-weight: 700;
    align-items: center;
    justify-content: center;
    font-size: .625rem;
}
.st-stat-card {
    padding: 1rem;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    min-height: 6rem;
    gap: .25rem;
}
.st-stat-label {
    font-size: .5625rem;
    text-transform: uppercase;
    letter-spacing: .05em;
}
.st-bar-track {
    height: .75rem;
    width: 100%;
    background: var(--bg-cell-empty);
    border: 1px solid var(--border-app);
    border-radius: 9999px;
    overflow: hidden;
    display: flex;
}
.st-bar-segment {
    height: 100%;
    transition: width .3s ease;
}
.st-stat-row {
    display: flex;
    flex-direction: column;
    gap: .5rem;
}
"#;
