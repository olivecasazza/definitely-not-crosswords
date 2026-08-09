use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;

use wasm_bindgen_futures::spawn_local;

use crate::components::game_list::{error_status, status};
use crate::components::identicon::Identicon;
use crate::components::ui::{self, RankBadge, StatTile};
use crate::net;
use crate::store::{use_app_state, Severity};
use crate::Route;
use crossword_core::fmt::{format_date, plural};

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
// Personal performance (stats.getUserHistory)
// ---------------------------------------------------------------------------

/// Row from `stats.getUserHistory` (auth, completedAt DESC). The endpoint may
/// not be deployed yet — every consumer is gated on the fetch succeeding, so
/// the page keeps working (KPIs + legacy match log) while it's absent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryRow {
    #[allow(dead_code)]
    game_id: String,
    completed_game_id: String,
    title: String,
    completed_at: String,
    #[serde(default)]
    #[allow(dead_code)]
    started_at: Option<String>,
    score: i64,
    correct: i64,
    incorrect: i64,
    rank: i64,
    #[allow(dead_code)]
    participants: i64,
    grid_size: HistGridSize,
    #[serde(default)]
    duration_secs: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct HistGridSize {
    w: i64,
    h: i64,
}

const DAY_MS: f64 = 86_400_000.0;

/// Epoch-ms → UTC day index (days since 1970-01-01). None for NaN/∞ stamps
/// (a garbled `completedAt` drops out of the streak/heatmap, never panics).
fn utc_day(ms: f64) -> Option<i64> {
    ms.is_finite().then(|| (ms / DAY_MS).floor() as i64)
}

/// Current streak: consecutive UTC days with ≥1 completion, ending today or
/// yesterday (playing yesterday but not yet today keeps the streak alive).
fn current_streak(days: &std::collections::HashSet<i64>, today: i64) -> i64 {
    let anchor = if days.contains(&today) {
        today
    } else if days.contains(&(today - 1)) {
        today - 1
    } else {
        return 0;
    };
    let mut n = 0;
    let mut d = anchor;
    while days.contains(&d) {
        n += 1;
        d -= 1;
    }
    n
}

/// Longest run of consecutive days. Input must be sorted ascending + deduped.
/// Returns `(length, last day of the run)`.
fn longest_streak(sorted_days: &[i64]) -> (i64, Option<i64>) {
    let (mut best, mut best_end) = (0i64, None);
    let (mut run, mut prev) = (0i64, i64::MIN);
    for &d in sorted_days {
        run = if d == prev + 1 { run + 1 } else { 1 };
        if run > best {
            best = run;
            best_end = Some(d);
        }
        prev = d;
    }
    (best, best_end)
}

/// 4-step heat level for the activity grid: 0 (no games) → 4 (4+ games).
fn heat_level(n: i64) -> i64 {
    n.clamp(0, 4)
}

/// Cell background for a day count — an opacity ramp of the accent token.
fn heat_bg(n: i64) -> String {
    match heat_level(n) {
        0 => "var(--bg-cell-empty)".to_string(),
        l => format!(
            "color-mix(in srgb, var(--pastel-yellow) {}%, transparent)",
            l * 25
        ),
    }
}

/// UTC day index → civil (year, month, day). Howard Hinnant's algorithm —
/// pure integer math so it stays testable off-wasm.
fn civil_from_day(day: i64) -> (i64, u32, u32) {
    let z = day + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

const MONTHS_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// "Aug 9" — tooltip label for a heatmap day.
fn day_label(day: i64) -> String {
    let (_, m, d) = civil_from_day(day);
    format!("{} {d}", MONTHS_ABBR[(m - 1) as usize])
}

/// mm:ss, minutes unpadded ("12:05", "3:41") — matches game_completed.
fn fmt_mmss(secs: i64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Grid-size bucket by max(w,h): 0 Mini ≤9 · 1 Standard 10–15 · 2 Large 16+.
fn size_bucket(w: i64, h: i64) -> usize {
    let m = w.max(h);
    if m <= 9 {
        0
    } else if m <= 15 {
        1
    } else {
        2
    }
}

const BUCKET_LABELS: [&str; 3] = ["Mini · ≤9", "Standard · 10–15", "Large · 16+"];

#[derive(Default, Clone, Copy)]
struct BucketAgg {
    games: i64,
    score_sum: i64,
    correct: i64,
    incorrect: i64,
    dur_sum: i64,
    dur_n: i64,
}

impl BucketAgg {
    fn avg_score(&self) -> i64 {
        if self.games == 0 {
            0
        } else {
            self.score_sum / self.games
        }
    }
    fn avg_acc(&self) -> Option<i64> {
        let t = self.correct + self.incorrect;
        (t > 0).then(|| self.correct * 100 / t)
    }
    /// Average solve time — only rows that carried a duration count.
    fn avg_time(&self) -> Option<String> {
        (self.dur_n > 0).then(|| fmt_mmss(self.dur_sum / self.dur_n))
    }
}

fn bucket_stats(rows: &[HistoryRow]) -> [BucketAgg; 3] {
    let mut b = [BucketAgg::default(); 3];
    for r in rows {
        let a = &mut b[size_bucket(r.grid_size.w, r.grid_size.h)];
        a.games += 1;
        a.score_sum += r.score;
        a.correct += r.correct;
        a.incorrect += r.incorrect;
        if let Some(d) = r.duration_secs.filter(|&d| d > 0) {
            a.dur_sum += d;
            a.dur_n += 1;
        }
    }
    b
}

/// Per-row accuracy percent; None when the row has no guesses at all.
fn accuracy_pct(correct: i64, incorrect: i64) -> Option<i64> {
    let t = correct + incorrect;
    (t > 0).then(|| correct * 100 / t)
}

fn best_score(rows: &[HistoryRow]) -> Option<&HistoryRow> {
    rows.iter().max_by_key(|r| r.score)
}

fn best_accuracy(rows: &[HistoryRow]) -> Option<(&HistoryRow, i64)> {
    rows.iter()
        .filter_map(|r| accuracy_pct(r.correct, r.incorrect).map(|a| (r, a)))
        .max_by_key(|(_, a)| *a)
}

/// Fastest solve — only rows with a positive durationSecs qualify.
fn fastest_solve(rows: &[HistoryRow]) -> Option<(&HistoryRow, i64)> {
    rows.iter()
        .filter_map(|r| r.duration_secs.filter(|&s| s > 0).map(|s| (r, s)))
        .min_by_key(|(_, s)| *s)
}

fn score_bounds(scores: &[i64]) -> (i64, i64) {
    scores
        .iter()
        .fold((i64::MAX, i64::MIN), |(lo, hi), &s| (lo.min(s), hi.max(s)))
}

/// Map chronological scores onto SVG coords inside the plot rect
/// (x0/y0 top-left, w×h). Y auto-scales to the data; a flat series centers.
fn chart_points(scores: &[i64], x0: f64, y0: f64, w: f64, h: f64) -> Vec<(f64, f64)> {
    let n = scores.len();
    if n == 0 {
        return Vec::new();
    }
    let (lo, hi) = score_bounds(scores);
    let span = (hi - lo) as f64;
    scores
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let x = if n == 1 {
                x0 + w / 2.0
            } else {
                x0 + w * i as f64 / (n as f64 - 1.0)
            };
            let y = if span == 0.0 {
                y0 + h / 2.0
            } else {
                y0 + h - h * (s - lo) as f64 / span
            };
            (x, y)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Panel kind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TeamBoardEntry {
    id: String,
    name: String,
    member_count: i64,
    max_size: i64,
    visibility: String,
    total_score: i64,
    games_played: i64,
    accuracy: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MyTeam {
    id: String,
    name: String,
    member_count: i64,
    max_size: i64,
    visibility: String,
    is_owner: bool,
}

/// Row from `team.list` — public teams only (PRIVATE never leaves the server).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublicTeam {
    id: String,
    name: String,
    member_count: i64,
    max_size: i64,
    visibility: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MyInvite {
    id: String,
    team_name: String,
    invited_by: String,
}

/// Pull a human-readable message out of a tRPC error string (JSON), else raw.
fn trpc_err(e: &str) -> String {
    serde_json::from_str::<serde_json::Value>(e)
        .ok()
        .and_then(|v| v["message"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| e.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Leaderboard,
    Career,
    Compare,
    Teams,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Leaderboard => "Leaderboard",
            Panel::Career => "Career",
            Panel::Compare => "Compare",
            Panel::Teams => "Teams",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Leaderboard, 16.0, 16.0, 620.0, 948.0),
        b.at(Panel::Career, 652.0, 16.0, 620.0, 466.0),
        b.at(Panel::Compare, 652.0, 498.0, 620.0, 466.0),
        b.at(Panel::Teams, 1288.0, 16.0, 616.0, 948.0),
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

    // Full personal match history — depends on session. Built against the
    // stats.getUserHistory contract; the endpoint may not be deployed yet, so
    // everything derived from it is gated on Ok (see career_body).
    let history_res = use_resource(move || async move {
        if state.user().is_none() {
            return None;
        }
        Some(net::query_as::<Vec<HistoryRow>>("stats.getUserHistory", None).await)
    });
    // Match-log page (client-side, 10 rows/page).
    let hist_page = use_signal(|| 0usize);

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

    // Teams: anyone can create; max size is Pro-tiered. Public teams self-join,
    // private teams are invite-only.
    let team_name = use_signal(String::new);
    let team_private = use_signal(|| false);
    let team_refresh = use_signal(|| 0u32);
    // Per-team invite input text, keyed by team id.
    let invite_inputs = use_signal(std::collections::HashMap::<String, String>::new);
    let teams_board_res = use_resource(move || async move {
        let _ = team_refresh.read();
        net::query_as::<Vec<TeamBoardEntry>>("team.getTeamLeaderboard", None).await
    });
    let mut team_list_res = use_resource(move || async move {
        let _ = team_refresh.read();
        net::query_as::<Vec<PublicTeam>>("team.list", None).await
    });
    let mut my_teams_res = use_resource(move || async move {
        let _ = team_refresh.read();
        if state.user().is_none() {
            return None;
        }
        Some(net::query_as::<Vec<MyTeam>>("team.myTeams", None).await)
    });
    let mut my_invites_res = use_resource(move || async move {
        let _ = team_refresh.read();
        if state.user().is_none() {
            return None;
        }
        Some(net::query_as::<Vec<MyInvite>>("team.myInvites", None).await)
    });

    let ws = use_workspace("stats_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

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
                                                    div { class: "st-podium-badge", style: "background: var(--podium-silver); color: var(--contrast-ink); border-color: var(--podium-silver);", "2" }
                                                    span { style: "font-size: .875rem; font-weight: 700; color: var(--text-primary);", "{entries[1].name}" }
                                                    span { style: "font-size: .625rem; color: var(--pastel-yellow); font-weight: 900; text-transform: uppercase;", "{entries[1].total_score} pts" }
                                                    span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "{entries[1].games_played} games · {entries[1].accuracy}% Acc" }
                                                }
                                            }
                                            // 1st (bigger)
                                            div { class: "app-card st-podium-card st-podium-first",
                                                div { class: "st-podium-badge", style: "background: var(--pastel-yellow); color: var(--contrast-ink); border-color: var(--pastel-yellow); width: 3rem; height: 3rem; font-size: 1.25rem;", "👑" }
                                                span { style: "font-size: 1rem; font-weight: 900; color: var(--text-primary);", "{entries[0].name}" }
                                                span { style: "font-size: .875rem; color: var(--pastel-yellow); font-weight: 900; text-transform: uppercase;", "{entries[0].total_score} pts" }
                                                span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase;", "{entries[0].games_played} games · {entries[0].accuracy}% Acc" }
                                            }
                                            // 3rd
                                            if entries.len() >= 3 {
                                                div { class: "app-card st-podium-card",
                                                    // Bronze keeps a fixed near-white ink — a theme-flipping colour would
                                                // go dark-on-dark-orange in light mode (matches components/ui.rs).
                                                div { class: "st-podium-badge", style: "background: var(--podium-bronze); color: #f8fafc; border-color: var(--podium-bronze);", "3" }
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
                                                                        RankBadge { index: i, label: (i + 1).to_string() }
                                                                    }
                                                                    td {
                                                                        span { style: "display: flex; align-items: center; gap: .375rem;",
                                                                            Identicon { seed: entry.id.clone(), size: 20 }
                                                                            "{entry.name}"
                                                                            if is_me {
                                                                                span { style: "font-size: .5rem; font-weight: 900; border: 1px solid color-mix(in srgb, var(--pastel-yellow) 30%, transparent); color: var(--pastel-yellow); padding: 0 .25rem; text-transform: uppercase;", "YOU" }
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
                            {career_body(stats, history_res, hist_page)}
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
                                    div { class: "app-card", style: "padding: 1.5rem; text-align: center; display: flex; flex-direction: column; align-items: center; gap: .75rem; border-color: color-mix(in srgb, var(--pastel-yellow) 20%, transparent);",
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
                                                            div { class: "st-bar-segment", style: "width: {100.0 - pct:.0}%; background: var(--text-secondary);" }
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
                                                            div { class: "st-bar-segment", style: "width: {100.0 - pct:.0}%; background: var(--text-secondary);" }
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
                                                            div { class: "st-bar-segment", style: "width: {100.0 - pct:.0}%; background: var(--text-secondary);" }
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
                                                                    span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase; letter-spacing: .05em; margin-top: .25rem;", "Played on {format_date(&m.created_at)}" }
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
            Panel::Teams => {
                let signed_in = state.user().is_some();
                let mut team_name = team_name;
                let mut team_private = team_private;
                let mut team_refresh = team_refresh;
                let mut invite_inputs = invite_inputs;
                // Teams the user already belongs to — hides "Join" in the directory.
                let my_team_ids: std::collections::HashSet<String> =
                    match &*my_teams_res.read_unchecked() {
                        Some(Some(Ok(teams))) => teams.iter().map(|t| t.id.clone()).collect(),
                        _ => Default::default(),
                    };
                rsx! {
                    div { style: "display: flex; flex-direction: column; gap: 1.5rem; height: 100%; overflow-y: auto;",

                        // Create a team (anyone; size is Pro-tiered)
                        div { style: "display: flex; flex-direction: column; gap: .6rem;",
                            span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Create a Team" }
                            if !signed_in {
                                p { class: "muted", style: "font-size: .75rem; margin: 0;", "Sign in to create or join teams." }
                            } else {
                                div { style: "display: flex; gap: .5rem; align-items: center; flex-wrap: wrap;",
                                    input {
                                        class: "app-input", style: "flex: 1; min-width: 8rem; padding: .5rem .75rem; font-size: .75rem;",
                                        placeholder: "Team name", value: "{team_name}",
                                        oninput: move |e| team_name.set(e.value()),
                                    }
                                    label { class: "muted", style: "font-size: .65rem; display: flex; align-items: center; gap: .3rem; text-transform: uppercase; white-space: nowrap;",
                                        input { r#type: "checkbox", checked: "{team_private}", onchange: move |e| team_private.set(e.checked()) }
                                        "Private"
                                    }
                                    button {
                                        class: "app-btn app-btn-active", style: "font-size: .75rem;",
                                        onclick: move |_| {
                                            let name = team_name.peek().clone();
                                            if name.trim().len() < 2 {
                                                state.toast(Severity::Error, "Team name must be at least 2 characters.");
                                                return;
                                            }
                                            let visibility = if *team_private.peek() { "PRIVATE" } else { "PUBLIC" };
                                            spawn_local(async move {
                                                match net::mutation("team.create", Some(json!({ "name": name, "visibility": visibility }))).await {
                                                    Ok(_) => {
                                                        team_name.set(String::new());
                                                        state.toast(Severity::Success, "Team created!");
                                                        let n = *team_refresh.peek(); team_refresh.set(n + 1);
                                                    }
                                                    Err(e) => state.toast(Severity::Error, trpc_err(&e)),
                                                }
                                            });
                                        },
                                        "Create"
                                    }
                                }
                                p { class: "muted", style: "font-size: .625rem; margin: 0;", "Free teams hold 4 · Pro teams hold 10." }
                            }
                        }

                        // Pending invitations
                        if signed_in {
                            match &*my_invites_res.read_unchecked() {
                                Some(Some(Err(e))) => rsx! {
                                    div { style: "display: flex; flex-direction: column; gap: .5rem;",
                                        span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Invitations" }
                                        {error_status(&trpc_err(e), move |_| my_invites_res.restart())}
                                    }
                                },
                                Some(Some(Ok(invites))) if !invites.is_empty() => rsx! {
                                    div { style: "display: flex; flex-direction: column; gap: .5rem;",
                                        span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Invitations" }
                                        for inv in invites.clone() {
                                            {
                                                let iid_a = inv.id.clone();
                                                let iid_d = inv.id.clone();
                                                let tname = inv.team_name.clone();
                                                rsx! {
                                                    div { class: "app-card", style: "padding: .6rem .9rem; display: flex; align-items: center; justify-content: space-between; gap: .5rem;",
                                                        div { style: "display: flex; flex-direction: column;",
                                                            span { style: "font-weight: 700; font-size: .8rem;", "{inv.team_name}" }
                                                            span { class: "muted", style: "font-size: .6rem;", "invited by {inv.invited_by}" }
                                                        }
                                                        div { style: "display: flex; gap: .4rem;",
                                                            button {
                                                                class: "app-btn app-btn-active", style: "font-size: .65rem;",
                                                                onclick: move |_| {
                                                                    let id = iid_a.clone();
                                                                    let nm = tname.clone();
                                                                    spawn_local(async move {
                                                                        match net::mutation("team.respondToInvite", Some(json!({ "inviteId": id, "accept": true }))).await {
                                                                            Ok(_) => { state.toast(Severity::Success, format!("Joined {nm}!")); let n = *team_refresh.peek(); team_refresh.set(n + 1); }
                                                                            Err(e) => state.toast(Severity::Error, trpc_err(&e)),
                                                                        }
                                                                    });
                                                                },
                                                                "Accept"
                                                            }
                                                            button {
                                                                class: "app-btn", style: "font-size: .65rem;",
                                                                onclick: move |_| {
                                                                    let id = iid_d.clone();
                                                                    spawn_local(async move {
                                                                        match net::mutation("team.respondToInvite", Some(json!({ "inviteId": id, "accept": false }))).await {
                                                                            Ok(_) => { state.toast(Severity::Success, "Invite declined."); let n = *team_refresh.peek(); team_refresh.set(n + 1); }
                                                                            Err(e) => state.toast(Severity::Error, trpc_err(&e)),
                                                                        }
                                                                    });
                                                                },
                                                                "Decline"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                _ => rsx! {},
                            }
                        }

                        // My teams (invite, toggle visibility, leave)
                        if signed_in {
                            match &*my_teams_res.read_unchecked() {
                                Some(Some(Err(e))) => rsx! {
                                    div { style: "display: flex; flex-direction: column; gap: .5rem;",
                                        span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "My Teams" }
                                        {error_status(&trpc_err(e), move |_| my_teams_res.restart())}
                                    }
                                },
                                Some(Some(Ok(teams))) if !teams.is_empty() => rsx! {
                                    div { style: "display: flex; flex-direction: column; gap: .5rem;",
                                        span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "My Teams" }
                                        for t in teams.clone() {
                                            {
                                                let owner_tag = if t.is_owner { " · owner" } else { "" };
                                                let tid = t.id.clone();
                                                let tid_inv = t.id.clone();
                                                let tid_vis = t.id.clone();
                                                let tid_input = t.id.clone();
                                                let tname = t.name.clone();
                                                let vis = t.visibility.clone();
                                                let next_vis = if vis == "PRIVATE" { "PUBLIC" } else { "PRIVATE" };
                                                let full = t.member_count >= t.max_size;
                                                let cur_input = invite_inputs.read().get(&t.id).cloned().unwrap_or_default();
                                                rsx! {
                                                    div { class: "app-card", style: "padding: .6rem .9rem; display: flex; flex-direction: column; gap: .5rem;",
                                                        div { style: "display: flex; align-items: center; justify-content: space-between; gap: .5rem;",
                                                            div { style: "display: flex; flex-direction: column;",
                                                                span { style: "font-weight: 700; font-size: .8rem;", "{t.name}" }
                                                                span { class: "muted", style: "font-size: .6rem; text-transform: uppercase;", "{t.member_count}/{t.max_size} · {vis}{owner_tag}" }
                                                            }
                                                            div { style: "display: flex; gap: .4rem;",
                                                                if t.is_owner {
                                                                    button {
                                                                        class: "app-btn", style: "font-size: .6rem;",
                                                                        onclick: move |_| {
                                                                            let id = tid_vis.clone();
                                                                            spawn_local(async move {
                                                                                match net::mutation("team.setVisibility", Some(json!({ "teamId": id, "visibility": next_vis }))).await {
                                                                                    Ok(_) => { state.toast(Severity::Success, format!("Team is now {next_vis}.")); let n = *team_refresh.peek(); team_refresh.set(n + 1); }
                                                                                    Err(e) => state.toast(Severity::Error, trpc_err(&e)),
                                                                                }
                                                                            });
                                                                        },
                                                                        "Make {next_vis}"
                                                                    }
                                                                }
                                                                button {
                                                                    class: "app-btn", style: "font-size: .65rem;",
                                                                    onclick: move |_| {
                                                                        let id = tid.clone();
                                                                        let nm = tname.clone();
                                                                        spawn_local(async move {
                                                                            match net::mutation("team.leave", Some(json!({ "teamId": id }))).await {
                                                                                Ok(_) => { state.toast(Severity::Success, format!("Left {nm}.")); let n = *team_refresh.peek(); team_refresh.set(n + 1); }
                                                                                Err(e) => state.toast(Severity::Error, trpc_err(&e)),
                                                                            }
                                                                        });
                                                                    },
                                                                    "Leave"
                                                                }
                                                            }
                                                        }
                                                        if !full {
                                                            div { style: "display: flex; gap: .4rem;",
                                                                input {
                                                                    class: "app-input", style: "flex: 1; padding: .35rem .6rem; font-size: .7rem;",
                                                                    placeholder: "invite by username or email", value: "{cur_input}",
                                                                    oninput: move |e| { invite_inputs.write().insert(tid_input.clone(), e.value()); },
                                                                }
                                                                button {
                                                                    class: "app-btn", style: "font-size: .65rem;",
                                                                    onclick: move |_| {
                                                                        let id = tid_inv.clone();
                                                                        let who = invite_inputs.peek().get(&id).cloned().unwrap_or_default();
                                                                        if who.trim().is_empty() { return; }
                                                                        spawn_local(async move {
                                                                            match net::mutation("team.invite", Some(json!({ "teamId": id, "identifier": who }))).await {
                                                                                Ok(_) => { state.toast(Severity::Success, "Invite sent."); invite_inputs.write().remove(&id); let n = *team_refresh.peek(); team_refresh.set(n + 1); }
                                                                                Err(e) => state.toast(Severity::Error, trpc_err(&e)),
                                                                            }
                                                                        });
                                                                    },
                                                                    "Invite"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                _ => rsx! {},
                            }
                        }

                        // Team leaderboard
                        div { style: "display: flex; flex-direction: column; gap: .5rem;",
                            span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Team Leaderboard" }
                            match &*teams_board_res.read_unchecked() {
                                None => rsx! { div { class: "muted st-loading", "Loading teams..." } },
                                Some(Err(e)) => rsx! { div { class: "error", style: "font-size: .75rem;", "{e}" } },
                                Some(Ok(teams)) if teams.is_empty() => rsx! {
                                    div { class: "app-card", style: "padding: 2rem; text-align: center; font-size: .75rem; color: var(--text-secondary);", "No teams yet — create the first one!" }
                                },
                                Some(Ok(teams)) => rsx! {
                                    for (i, t) in teams.iter().enumerate() {
                                        {
                                            let t = t.clone();
                                            let badge = ui::rank_badge_style(i);
                                            let joinable = signed_in && t.visibility == "PUBLIC" && t.member_count < t.max_size && !my_team_ids.contains(&t.id);
                                            let is_private = t.visibility == "PRIVATE";
                                            rsx! {
                                                div { class: "app-card", style: "padding: .6rem .9rem; display: flex; align-items: center; gap: .75rem;",
                                                    span { style: "{badge} display: inline-flex; align-items: center; justify-content: center; width: 1.5rem; height: 1.5rem; font-size: .7rem; font-weight: 700; font-family: monospace; border: 1px solid;", "{i + 1}" }
                                                    div { style: "flex: 1; display: flex; flex-direction: column; min-width: 0;",
                                                        span { style: "font-weight: 700; font-size: .8rem;", "{t.name}" }
                                                        span { class: "muted", style: "font-size: .6rem; text-transform: uppercase;", "{t.member_count}/{t.max_size} · {t.games_played} games · {t.accuracy}% acc" }
                                                    }
                                                    span { style: "font-weight: 700; font-family: monospace; color: var(--pastel-yellow);", "{t.total_score}" }
                                                    if joinable {
                                                        button {
                                                            class: "app-btn", style: "font-size: .6875rem;",
                                                            onclick: move |_| {
                                                                let id = t.id.clone();
                                                                let nm = t.name.clone();
                                                                spawn_local(async move {
                                                                    match net::mutation("team.join", Some(json!({ "teamId": id }))).await {
                                                                        Ok(_) => {
                                                                            state.toast(Severity::Success, format!("Joined {nm}!"));
                                                                            let n = *team_refresh.peek(); team_refresh.set(n + 1);
                                                                        }
                                                                        Err(e) => state.toast(Severity::Error, trpc_err(&e)),
                                                                    }
                                                                });
                                                            },
                                                            "Join"
                                                        }
                                                    } else if is_private {
                                                        span { class: "muted", style: "font-size: .58rem; text-transform: uppercase;", "private" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        }

                        // Browse teams — public directory (team.list), joinable when open
                        div { style: "display: flex; flex-direction: column; gap: .5rem;",
                            span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Browse Teams" }
                            match &*team_list_res.read_unchecked() {
                                None => rsx! { div { class: "muted st-loading", "Loading team directory..." } },
                                Some(Err(e)) => rsx! { {error_status(&trpc_err(e), move |_| team_list_res.restart())} },
                                Some(Ok(teams)) if teams.is_empty() => rsx! {
                                    div { class: "app-card", style: "padding: 2rem; text-align: center; font-size: .75rem; color: var(--text-secondary);", "No public teams to browse yet." }
                                },
                                Some(Ok(teams)) => rsx! {
                                    for t in teams.iter() {
                                        {
                                            let t = t.clone();
                                            let is_member = my_team_ids.contains(&t.id);
                                            let full = t.member_count >= t.max_size;
                                            let joinable = signed_in && t.visibility == "PUBLIC" && !full && !is_member;
                                            rsx! {
                                                div { class: "app-card", style: "padding: .6rem .9rem; display: flex; align-items: center; gap: .75rem;",
                                                    div { style: "flex: 1; display: flex; flex-direction: column; min-width: 0;",
                                                        span { style: "font-weight: 700; font-size: .8rem;", "{t.name}" }
                                                        span { class: "muted", style: "font-size: .6rem; text-transform: uppercase;", "{t.member_count}/{t.max_size} members" }
                                                    }
                                                    span { class: "muted", style: "font-size: .58rem; text-transform: uppercase; border: 1px solid var(--border-app); padding: 0 .3rem;", "{t.visibility}" }
                                                    if is_member {
                                                        span { class: "muted", style: "font-size: .58rem; text-transform: uppercase;", "member" }
                                                    } else if full {
                                                        span { class: "muted", style: "font-size: .58rem; text-transform: uppercase;", "full" }
                                                    } else if joinable {
                                                        button {
                                                            class: "app-btn", style: "font-size: .6875rem;",
                                                            onclick: move |_| {
                                                                let id = t.id.clone();
                                                                let nm = t.name.clone();
                                                                spawn_local(async move {
                                                                    match net::mutation("team.join", Some(json!({ "teamId": id }))).await {
                                                                        Ok(_) => {
                                                                            state.toast(Severity::Success, format!("Joined {nm}!"));
                                                                            let n = *team_refresh.peek(); team_refresh.set(n + 1);
                                                                        }
                                                                        Err(e) => state.toast(Severity::Error, trpc_err(&e)),
                                                                    }
                                                                });
                                                            },
                                                            "Join"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }
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

// ---------------------------------------------------------------------------
// Career panel body — KPI row + personal-performance sections
// ---------------------------------------------------------------------------

type HistoryResource = Resource<Option<Result<Vec<HistoryRow>, String>>>;

/// Everything inside the Career panel once getUserStats is in. History-derived
/// sections (streak, heatmap, bests, trends, buckets, paginated match log)
/// only render when stats.getUserHistory answers Ok; until then the panel
/// falls back to the legacy recent-games log so nothing regresses.
fn career_body(
    stats: &CareerStats,
    mut history_res: HistoryResource,
    hist_page: Signal<usize>,
) -> Element {
    let hist = history_res.read_unchecked();
    let rows_ok: Option<&Vec<HistoryRow>> = match &*hist {
        Some(Some(Ok(rows))) => Some(rows),
        _ => None,
    };

    // Per-row UTC day indices (aligned with rows; unparsable stamps drop out).
    let today = utc_day(js_sys::Date::now()).unwrap_or(0);
    let day_of_row: Vec<Option<i64>> = rows_ok
        .map(|rows| {
            rows.iter()
                .map(|r| utc_day(js_sys::Date::parse(&r.completed_at)))
                .collect()
        })
        .unwrap_or_default();
    let mut day_counts = std::collections::HashMap::<i64, i64>::new();
    for d in day_of_row.iter().flatten() {
        *day_counts.entry(*d).or_insert(0) += 1;
    }
    let day_set: std::collections::HashSet<i64> = day_counts.keys().copied().collect();
    let mut sorted_days: Vec<i64> = day_set.iter().copied().collect();
    sorted_days.sort_unstable();

    // Streak shows "—" until history answers (endpoint may not exist yet).
    let has_history = rows_ok.is_some();
    let streak_val = if has_history {
        format!("{}d", current_streak(&day_set, today))
    } else {
        "—".to_string()
    };

    // History fetch state: loading → busy status, error → retry. Ok/absent-
    // session render nothing here.
    let hist_status: Element = match &*hist {
        None => status("muted", "Loading match history…", true),
        Some(Some(Err(e))) => error_status(&trpc_err(e), move |_| history_res.restart()),
        _ => rsx! {},
    };

    let (overview, trends): (Element, Element) = match rows_ok {
        Some(rows) if !rows.is_empty() => (
            rsx! {
                {heatmap_section(&day_counts, today)}
                {bests_section(rows, &day_of_row, &sorted_days)}
            },
            rsx! {
                {score_chart_section(rows)}
                {bucket_section(rows)}
                {match_log_section(rows, hist_page)}
            },
        ),
        _ => (rsx! {}, legacy_match_log(stats)),
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 2rem;",

            // KPI row — getUserStats data + streak derived from history
            div { class: "st-kpi-row",
                StatTile {
                    label: "Global Rank".to_string(),
                    value: format!("#{}", stats.global_rank),
                    sub: format!("of {}", stats.total_players),
                }
                StatTile { label: "Career Score".to_string(), value: stats.total_score.to_string() }
                StatTile { label: "Accuracy".to_string(), value: format!("{}%", stats.accuracy) }
                StatTile { label: "Games Played".to_string(), value: stats.games_played.to_string() }
                if has_history {
                    StatTile {
                        label: "Current Streak".to_string(),
                        value: streak_val.clone(),
                        sub: "consecutive days".to_string(),
                    }
                } else {
                    StatTile { label: "Current Streak".to_string(), value: streak_val.clone() }
                }
            }

            {hist_status}
            {overview}

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

            {trends}
        }
    }
}

/// GitHub-style activity grid: 26 columns (weeks) × 7 rows (Sun–Sat); the
/// last column is the current, partial week. Cell opacity ramps with games/day.
fn heatmap_section(day_counts: &std::collections::HashMap<i64, i64>, today: i64) -> Element {
    let weekday = (today + 4).rem_euclid(7); // epoch day 0 = Thursday; 0 = Sunday
    let start = today - weekday - 25 * 7;
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: .75rem;",
            span { class: "muted st-section-label", "Activity · Last 26 Weeks" }
            div { style: "overflow-x: auto;",
                div { class: "st-heat-grid",
                    for col in 0..26i64 {
                        for row in 0..7i64 {
                            {
                                let day = start + col * 7 + row;
                                if day > today {
                                    // Future days this week: invisible placeholders keep the column shape.
                                    rsx! { div { class: "st-heat-cell", style: "visibility: hidden;" } }
                                } else {
                                    let n = day_counts.get(&day).copied().unwrap_or(0);
                                    let bg = heat_bg(n);
                                    let tip = format!("{} · {}", plural(n, "game"), day_label(day));
                                    rsx! { div { class: "st-heat-cell", style: "background: {bg};", title: "{tip}" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Personal bests: best score / best accuracy / fastest solve / longest
/// streak, each linking to the game that set it.
fn bests_section(rows: &[HistoryRow], day_of_row: &[Option<i64>], sorted_days: &[i64]) -> Element {
    let bs = best_score(rows);
    let ba = best_accuracy(rows);
    let fs = fastest_solve(rows);
    let (streak_n, streak_end) = longest_streak(sorted_days);
    // The game that capped the longest streak (a completion on its last day).
    let streak_row = streak_end.and_then(|d| {
        day_of_row
            .iter()
            .position(|x| *x == Some(d))
            .and_then(|i| rows.get(i))
    });
    let sub_of = |r: &HistoryRow| format!("{} · {}", r.title, format_date(&r.completed_at));
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: .75rem;",
            span { class: "muted st-section-label", "Personal Bests" }
            div { class: "st-best-grid",
                {best_card("Best Score", bs.map(|r| r.score.to_string()).unwrap_or_else(|| "—".into()),
                    bs.map(sub_of), bs.map(|r| r.completed_game_id.clone()))}
                {best_card("Best Accuracy", ba.map(|(_, a)| format!("{a}%")).unwrap_or_else(|| "—".into()),
                    ba.map(|(r, _)| sub_of(r)), ba.map(|(r, _)| r.completed_game_id.clone()))}
                {best_card("Fastest Solve", fs.map(|(_, s)| fmt_mmss(s)).unwrap_or_else(|| "—".into()),
                    fs.map(|(r, _)| sub_of(r)), fs.map(|(r, _)| r.completed_game_id.clone()))}
                {best_card("Longest Streak",
                    if streak_n > 0 { format!("{streak_n}d") } else { "—".into() },
                    streak_row.map(sub_of), streak_row.map(|r| r.completed_game_id.clone()))}
            }
        }
    }
}

/// One personal-best tile. With a backing game it's a link to the completed-
/// game review; without one it's inert.
fn best_card(
    label: &'static str,
    value: String,
    sub: Option<String>,
    link: Option<String>,
) -> Element {
    let body = rsx! {
        span { class: "muted st-stat-label", "{label}" }
        span { class: "st-best-value", "{value}" }
        if let Some(s) = sub {
            span { class: "muted st-best-sub", "{s}" }
        }
    };
    match link {
        Some(id) => rsx! {
            Link { to: Route::GameCompleted { id }, class: "app-card st-best-card", {body} }
        },
        None => rsx! {
            div { class: "app-card st-best-card", {body} }
        },
    }
}

/// Inline SVG score trend over the last 30 matches (chronological, no chart
/// lib): a single yellow polyline with square markers, y auto-scaled.
fn score_chart_section(rows: &[HistoryRow]) -> Element {
    // Input is completedAt DESC → take the newest 30, then flip to chronological.
    let scores: Vec<i64> = rows.iter().take(30).rev().map(|r| r.score).collect();
    let n = scores.len();
    let (lo, hi) = score_bounds(&scores);
    const X0: f64 = 44.0;
    const Y0: f64 = 10.0;
    const W: f64 = 548.0;
    const H: f64 = 140.0;
    let pts = chart_points(&scores, X0, Y0, W, H);
    let poly = pts
        .iter()
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ");
    let x_end = X0 + W;
    let y_base = Y0 + H;
    let y_hi_lbl = Y0 + 4.0;
    let y_lo_lbl = y_base + 4.0;
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: .75rem;",
            span { class: "muted st-section-label", "Score Trend · Last {n} Matches" }
            div { class: "app-card", style: "padding: 1rem;",
                svg {
                    view_box: "0 0 600 165",
                    role: "img",
                    "aria-label": "Score per match, oldest to newest",
                    style: "display: block; width: 100%; height: auto;",
                    line { x1: "{X0}", y1: "{Y0}", x2: "{x_end}", y2: "{Y0}", stroke: "var(--border-app)", stroke_width: "1" }
                    line { x1: "{X0}", y1: "{y_base}", x2: "{x_end}", y2: "{y_base}", stroke: "var(--border-app)", stroke_width: "1" }
                    text { x: "38", y: "{y_hi_lbl}", class: "st-chart-label", "{hi}" }
                    text { x: "38", y: "{y_lo_lbl}", class: "st-chart-label", "{lo}" }
                    if pts.len() >= 2 {
                        polyline { points: "{poly}", fill: "none", stroke: "var(--pastel-yellow)", stroke_width: "2" }
                    }
                    for (x, y) in pts.iter() {
                        {
                            let rx = x - 3.0;
                            let ry = y - 3.0;
                            rsx! { rect { x: "{rx}", y: "{ry}", width: "6", height: "6", fill: "var(--pastel-yellow)" } }
                        }
                    }
                }
            }
        }
    }
}

/// Per-bucket aggregates by grid size (honest label — sizes, not difficulty).
fn bucket_section(rows: &[HistoryRow]) -> Element {
    let aggs = bucket_stats(rows);
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: .75rem;",
            span { class: "muted st-section-label", "By Grid Size" }
            div { class: "app-card", style: "overflow: hidden;",
                div { style: "overflow-x: auto;",
                    table { class: "st-table",
                        thead {
                            tr { class: "st-thead-row",
                                th { "Size" }
                                th { style: "text-align: center;", "Games" }
                                th { style: "text-align: center;", "Avg Score" }
                                th { style: "text-align: center;", "Avg Accuracy" }
                                th { style: "text-align: right;", "Avg Time" }
                            }
                        }
                        tbody {
                            for (i, a) in aggs.iter().enumerate() {
                                if a.games > 0 {
                                    {
                                        let label = BUCKET_LABELS[i];
                                        let avg_score = a.avg_score();
                                        let acc = a.avg_acc().map(|v| format!("{v}%")).unwrap_or_else(|| "—".into());
                                        let time = a.avg_time().unwrap_or_else(|| "—".into());
                                        rsx! {
                                            tr { class: "st-table-row",
                                                td { "{label}" }
                                                td { style: "text-align: center; color: var(--text-secondary);", "{a.games}" }
                                                td { style: "text-align: center; font-weight: 700; color: var(--pastel-yellow);", "{avg_score}" }
                                                td { style: "text-align: center;", "{acc}" }
                                                td { style: "text-align: right; color: var(--text-secondary);", "{time}" }
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

/// Paginated match log (10/page, client-side). Rows link to the completed-
/// game review.
fn match_log_section(rows: &[HistoryRow], mut page: Signal<usize>) -> Element {
    const PAGE: usize = 10;
    let total = rows.len();
    let pages = total.div_ceil(PAGE).max(1);
    let p = (*page.read()).min(pages - 1);
    let start = p * PAGE;
    let end = (start + PAGE).min(total);
    let from = start + 1;
    let at_start = p == 0;
    let at_end = end >= total;
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: .75rem;",
            span { class: "muted st-section-label", "Match Log" }
            div { style: "display: flex; flex-direction: column; gap: .5rem;",
                for r in rows[start..end].iter() {
                    {
                        let time = r.duration_secs.filter(|&s| s > 0).map(fmt_mmss).unwrap_or_else(|| "—".into());
                        let rank_idx = (r.rank.max(1) - 1) as usize;
                        let date = format_date(&r.completed_at);
                        rsx! {
                            Link {
                                to: Route::GameCompleted { id: r.completed_game_id.clone() },
                                class: "app-card st-mlog-row",
                                div { style: "flex: 1; display: flex; flex-direction: column; min-width: 0;",
                                    span { class: "st-mlog-title", "{r.title}" }
                                    span { class: "muted st-mlog-date", "{date}" }
                                }
                                span { class: "st-chip", "{r.grid_size.w}×{r.grid_size.h}" }
                                span { class: "st-mlog-time", "{time}" }
                                span { class: "st-mlog-score", "{r.score}" }
                                RankBadge { index: rank_idx, label: r.rank.to_string() }
                            }
                        }
                    }
                }
            }
            div { style: "display: flex; align-items: center; justify-content: space-between; gap: .75rem;",
                button {
                    class: "app-btn",
                    style: "font-size: .65rem;",
                    disabled: at_start,
                    onclick: move |_| {
                        let cur = *page.peek();
                        if cur > 0 {
                            page.set(cur - 1);
                        }
                    },
                    "Prev"
                }
                span { class: "muted", style: "font-size: .625rem; font-family: monospace; text-transform: uppercase;",
                    "{from}–{end} of {total}"
                }
                button {
                    class: "app-btn",
                    style: "font-size: .65rem;",
                    disabled: at_end,
                    onclick: move |_| {
                        let cur = *page.peek();
                        page.set((cur + 1).min(pages - 1));
                    },
                    "Next"
                }
            }
        }
    }
}

/// Pre-history fallback: the recent-games log fed by getUserStats. Rendered
/// while stats.getUserHistory is loading, erroring, or not yet deployed.
fn legacy_match_log(stats: &CareerStats) -> Element {
    rsx! {
        div { style: "display: flex; flex-direction: column; gap: .75rem;",
            span { class: "muted", style: "font-size: .75rem; font-weight: 700; text-transform: uppercase; letter-spacing: .05em;", "Match Log History" }
            div { style: "display: flex; flex-direction: column; gap: .75rem;",
                for game_opt in stats.recent_games.iter() {
                    if let Some(game) = game_opt {
                        div { class: "app-card", style: "padding: 1rem 1.25rem; display: flex; flex-direction: column; gap: .75rem;",
                            div { style: "display: flex; flex-direction: row; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: .75rem;",
                                div { style: "display: flex; flex-direction: column; min-width: 0;",
                                    span { style: "font-size: .875rem; font-weight: 700; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{game.title}" }
                                    span { class: "muted", style: "font-size: .5625rem; text-transform: uppercase; letter-spacing: .05em; margin-top: .25rem;", "Played on {format_date(&game.created_at)}" }
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

const STATS_CSS: &str = r#"
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
    border-bottom: 1px solid color-mix(in srgb, var(--border-hover) 50%, transparent);
    font-size: .75rem;
    font-family: monospace;
    transition: all .15s ease;
}
.st-table-row:hover { background: color-mix(in srgb, var(--border-hover) 15%, transparent); }
.st-table-row-me { background: color-mix(in srgb, var(--pastel-yellow) 2%, transparent); font-weight: 700; }
.st-table-row td { padding: .875rem 1rem; }
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
.st-kpi-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(8.5rem, 1fr));
    gap: 1rem;
}
.st-section-label {
    font-size: .75rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: .05em;
}
.st-heat-grid {
    display: grid;
    grid-auto-flow: column;
    grid-template-rows: repeat(7, .625rem);
    grid-auto-columns: .625rem;
    gap: 2px;
    width: max-content;
    padding: .125rem 0;
}
.st-heat-cell {
    width: .625rem;
    height: .625rem;
    border: 1px solid color-mix(in srgb, var(--border-app) 60%, transparent);
}
.st-best-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
    gap: 1rem;
}
.st-best-card {
    padding: .875rem 1rem;
    display: flex;
    flex-direction: column;
    gap: .25rem;
    font-family: monospace;
    color: inherit;
    text-decoration: none;
    min-width: 0;
}
a.st-best-card:hover { border-color: var(--border-hover); }
.st-best-value {
    font-size: 1.25rem;
    font-weight: 900;
    color: var(--pastel-yellow);
}
.st-best-sub {
    font-size: .5625rem;
    text-transform: uppercase;
    letter-spacing: .05em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.st-chart-label {
    fill: var(--text-secondary);
    font-size: 9px;
    font-family: monospace;
    text-anchor: end;
}
.st-chip {
    font-size: .5625rem;
    font-family: monospace;
    text-transform: uppercase;
    border: 1px solid var(--border-app);
    padding: .1rem .35rem;
    color: var(--text-secondary);
    flex-shrink: 0;
}
.st-mlog-row {
    display: flex;
    align-items: center;
    gap: .875rem;
    padding: .625rem .9rem;
    color: inherit;
    text-decoration: none;
    font-family: monospace;
}
.st-mlog-row:hover { border-color: var(--border-hover); }
.st-mlog-title {
    font-size: .8rem;
    font-weight: 700;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}
.st-mlog-date {
    font-size: .5625rem;
    text-transform: uppercase;
    letter-spacing: .05em;
    margin-top: .125rem;
}
.st-mlog-time {
    font-size: .6875rem;
    color: var(--text-secondary);
    min-width: 2.75rem;
    text-align: right;
    flex-shrink: 0;
}
.st-mlog-score {
    font-size: .875rem;
    font-weight: 900;
    color: var(--pastel-yellow);
    min-width: 2.25rem;
    text-align: right;
    flex-shrink: 0;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 2026-01-01T00:00:00Z in epoch ms (2025 is not a leap year).
    const JAN1_2026_MS: f64 = 1_767_225_600_000.0;

    #[test]
    fn utc_day_handles_day_boundaries_and_garbage() {
        let d0 = utc_day(JAN1_2026_MS).unwrap();
        // Last ms of the day is still the same UTC day; the next ms is not.
        assert_eq!(utc_day(JAN1_2026_MS + DAY_MS - 1.0), Some(d0));
        assert_eq!(utc_day(JAN1_2026_MS + DAY_MS), Some(d0 + 1));
        assert_eq!(utc_day(f64::NAN), None);
        // Sanity: the civil date round-trips.
        assert_eq!(civil_from_day(d0), (2026, 1, 1));
        assert_eq!(civil_from_day(0), (1970, 1, 1));
    }

    #[test]
    fn current_streak_anchors_on_today_or_yesterday() {
        let days: HashSet<i64> = [100, 99, 98, 95].into_iter().collect();
        assert_eq!(current_streak(&days, 100), 3); // ends today
        assert_eq!(current_streak(&days, 101), 3); // yesterday keeps it alive
        assert_eq!(current_streak(&days, 102), 0); // a full missed day breaks it
        assert_eq!(current_streak(&HashSet::new(), 100), 0);
    }

    #[test]
    fn streak_from_timestamps_respects_utc_midnight() {
        // Completions either side of UTC midnight land on consecutive days.
        let late = JAN1_2026_MS + DAY_MS - 60_000.0; // Jan 1, 23:59 UTC
        let early = JAN1_2026_MS + DAY_MS + 60_000.0; // Jan 2, 00:01 UTC
        let days: HashSet<i64> = [late, early].iter().filter_map(|&ms| utc_day(ms)).collect();
        assert_eq!(days.len(), 2);
        assert_eq!(current_streak(&days, utc_day(early).unwrap()), 2);
    }

    #[test]
    fn longest_streak_finds_run_and_end_day() {
        assert_eq!(longest_streak(&[1, 2, 3, 7, 8, 9, 10, 20]), (4, Some(10)));
        assert_eq!(longest_streak(&[5]), (1, Some(5)));
        assert_eq!(longest_streak(&[]), (0, None));
    }
}
