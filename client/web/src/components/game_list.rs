//! The one game-list renderer for the games library, plus the shared status /
//! error / progress atoms other pages reuse. Rows are fully projected by the
//! caller into [`GameRow`]s — no wire-shape knowledge lives in this module.
//!
//! Callers own their state: hooks can't run inside a panel-kit `body` closure
//! (it's called once per panel, which would scramble hook order), so filtering,
//! sorting, and searching all happen in the page component before rows arrive.

use crossword_core::fmt::plural;
use dioxus::prelude::*;

/// Play-state of a game row. Drives the status square, the action label, and
/// (in the page) which route a click lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    New,
    Active,
    Solved,
}

impl GameStatus {
    fn glyph(self) -> &'static str {
        match self {
            GameStatus::New => "▢",
            GameStatus::Active => "▶",
            GameStatus::Solved => "✓",
        }
    }

    /// Status-square tone: 2px border + glyph in the status colour. Green =
    /// in-progress everywhere in the app; yellow = score/solved.
    fn square_style(self) -> &'static str {
        match self {
            GameStatus::New => "border-color: var(--border-app); color: var(--text-secondary);",
            GameStatus::Active => "border-color: var(--pastel-green); color: var(--pastel-green);",
            GameStatus::Solved => {
                "border-color: var(--pastel-yellow); color: var(--pastel-yellow);"
            }
        }
    }

    /// Accessible state name (also the e2e specs' handle on a row's state).
    pub fn label(self) -> &'static str {
        match self {
            GameStatus::New => "NEW",
            GameStatus::Active => "IN PROGRESS",
            GameStatus::Solved => "SOLVED",
        }
    }

    fn action(self) -> &'static str {
        match self {
            GameStatus::New => "Play",
            GameStatus::Active => "Resume",
            GameStatus::Solved => "Review",
        }
    }
}

/// One row, fully projected.
#[derive(Clone, PartialEq)]
pub struct GameRow {
    pub id: String,
    pub title: String,
    pub status: GameStatus,
    /// Clue count; 0 hides the fact.
    pub clues: i64,
    /// Grid dimensions chip `(w, h)`, when the server sends them.
    pub size: Option<(i64, i64)>,
    /// Pre-formatted relative date ("3h ago").
    pub when: Option<String>,
    /// Final score, Solved rows only.
    pub score: Option<i32>,
    /// `(filled, total)` cell counts, Active rows only — the mini progress bar
    /// renders only when the server sends both. Never faked.
    pub progress: Option<(i64, i64)>,
}

/// A centred status message (loading / signed-out / empty / error). Replaces the
/// six near-identical inline blocks the lobby used to carry.
pub fn status(class: &str, msg: &str, busy: bool) -> Element {
    rsx! {
        div {
            class: "game-status {class}",
            aria_busy: if busy { "true" } else { "false" },
            role: "status",
            "{msg}"
        }
    }
}

/// Error status with a retry button — a dead-end error message is not shippable.
pub fn error_status(msg: &str, on_retry: impl FnMut(Event<MouseData>) + 'static) -> Element {
    rsx! {
        div { class: "game-status error",
            div { "{msg}" }
            button { class: "app-btn", style: "margin-top: .75rem;", onclick: on_retry, "Retry" }
        }
    }
}

/// 0–100 fill percentage; `None` unless both counts are present and sane, so
/// callers can gate the bar on real data instead of faking one.
pub fn progress_pct(filled: Option<i64>, total: Option<i64>) -> Option<i64> {
    let (f, t) = (filled?, total?);
    if t <= 0 {
        return None;
    }
    Some(f.clamp(0, t) * 100 / t)
}

/// Thin square progress bar (green fill, house-style square corners).
pub fn progress_bar(pct: i64) -> Element {
    rsx! {
        div {
            class: "game-progress",
            role: "progressbar",
            aria_valuenow: "{pct}",
            aria_valuemin: "0",
            aria_valuemax: "100",
            div { class: "game-progress-fill", style: "width: {pct}%;" }
        }
    }
}

/// Render `rows` as a keyboard-operable list. The caller has already filtered,
/// searched, and sorted; empty states are the caller's too (they vary by
/// filter). `on_pick` receives the row's status so the page can route
/// New → start, Active → play, Solved → review.
///
/// Linear in `rows`; nothing beyond per-row string formatting.
pub fn game_list(
    rows: Vec<GameRow>,
    on_pick: impl FnMut(GameStatus, String) + Clone + 'static,
) -> Element {
    rsx! {
        div { class: "game-list",
            for row in rows {
                {
                    let id = row.id.clone();
                    let mut pick = on_pick.clone();
                    let mut pick_key = on_pick.clone();
                    let key_id = id.clone();
                    let st = row.status;
                    let pct = row
                        .progress
                        .and_then(|(f, t)| progress_pct(Some(f), Some(t)));
                    rsx! {
                        div {
                            key: "{row.id}",
                            class: "game-row",
                            // Inline `cursor: pointer` is also the e2e specs'
                            // row selector — keep it alongside the class.
                            style: "cursor: pointer;",
                            role: "button",
                            tabindex: "0",
                            aria_label: "{row.title} — {st.label()}",
                            onclick: move |_| pick(st, id.clone()),
                            onkeydown: move |e| {
                                // Enter / Space activate, matching a native button.
                                let k = e.key();
                                if k == Key::Enter || k == Key::Character(" ".into()) {
                                    e.prevent_default();
                                    pick_key(st, key_id.clone());
                                }
                            },
                            span {
                                class: "game-row-square",
                                style: "{st.square_style()}",
                                aria_hidden: "true",
                                "{st.glyph()}"
                            }
                            div { class: "game-row-body",
                                span { class: "game-row-title", "{row.title}" }
                                div { class: "game-row-meta",
                                    if row.clues > 0 {
                                        span { {plural(row.clues, "clue")} }
                                    }
                                    if let Some((w, h)) = row.size {
                                        span { class: "game-row-chip", "{w}×{h}" }
                                    }
                                    if let Some(when) = &row.when {
                                        span { "{when}" }
                                    }
                                }
                            }
                            div { class: "game-row-side",
                                if let Some(p) = pct {
                                    {progress_bar(p)}
                                }
                                if let Some(s) = row.score {
                                    span { class: "game-row-score", "{s} pts" }
                                }
                                // Affordance only — the whole row is the button,
                                // so this stays out of the tab order.
                                span { class: "game-row-action", aria_hidden: "true", "{st.action()}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Injected once by the games page. Square corners throughout (house style).
pub const GAME_LIST_CSS: &str = "
.game-list { display: flex; flex-direction: column; }

.game-row { display: flex; flex-direction: row; align-items: center; gap: .75rem;
  padding: .875rem 1rem; border-bottom: 1px solid var(--border-app);
  border-left: 2px solid transparent; transition: background-color .12s ease, border-color .12s ease; }
.game-row:hover { background: var(--bg-cell-letter); border-left-color: var(--color-primary); }
.game-row:focus-visible { outline: none; background: var(--bg-cell-letter); border-left-color: var(--color-primary); }
.game-row:active { background: var(--bg-cell-empty); }

.game-row-square { flex-shrink: 0; width: 2rem; height: 2rem; border: 2px solid;
  display: flex; align-items: center; justify-content: center; font-size: var(--fs-sm); }

.game-row-body { flex: 1 1 auto; display: flex; flex-direction: column; gap: .2rem; min-width: 0; }
.game-row-title { font-weight: 700; font-size: var(--fs-md); color: var(--text-primary);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.game-row-meta { display: flex; align-items: center; gap: .5rem;
  font-family: var(--mono, monospace); font-size: var(--fs-2xs); color: var(--text-secondary); }
.game-row-chip { border: 1px solid var(--border-app); padding: .06rem .3rem; font-weight: 700; }

.game-row-side { flex-shrink: 0; display: flex; align-items: center; gap: .625rem; }
.game-row-score { font-family: var(--mono, monospace); font-size: var(--fs-xs); font-weight: 700;
  color: var(--pastel-yellow); }
.game-row-action { border: 1px solid var(--border-app); padding: .25rem .625rem;
  font-family: var(--mono, monospace); font-size: var(--fs-2xs); font-weight: 700;
  text-transform: uppercase; color: var(--text-secondary); }
.game-row:hover .game-row-action, .game-row:focus-visible .game-row-action {
  border-color: var(--color-primary); color: var(--text-primary); }

.game-progress { width: 72px; height: 6px; border: 1px solid var(--border-app);
  background: var(--bg-cell-empty); }
.game-progress-fill { height: 100%; background: var(--pastel-green); }

@media (max-width: 760px) {
  /* Comfortable tap target on touch, and no hover-stick on mobile Safari. */
  .game-row { padding: 1rem; min-height: 3.25rem; }
  .game-row:hover { background: transparent; border-left-color: transparent; }
  /* Progress bar yields first when a small row runs out of width. */
  .game-progress { width: 48px; }
}
";
