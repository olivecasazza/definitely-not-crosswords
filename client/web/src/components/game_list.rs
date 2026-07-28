//! The one game-list renderer, shared by every lobby panel (Available /
//! Active / Completed) and by anything else that lists games. Panels differ
//! only in the [`GameRow`]s they hand over — badge text, tone, and meta line.
//!
//! Callers own their state: hooks can't run inside a panel-kit `body` closure
//! (it's called once per panel, which would scramble hook order), so `filter`
//! is a signal created in the page component and passed down.

use dioxus::prelude::*;

/// Badge colour. Maps onto the app's pastel tokens; `Neutral` is outline-only.
#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Neutral,
    Active,
    Done,
}

impl Tone {
    fn badge_style(self) -> &'static str {
        match self {
            Tone::Neutral => "border: 1px solid var(--border-app); color: var(--text-secondary);",
            Tone::Active => "background: var(--pastel-yellow); color: var(--contrast-ink);",
            Tone::Done => "background: var(--pastel-green); color: var(--contrast-ink);",
        }
    }
}

/// One row, fully projected — no game-shape knowledge lives in this module.
#[derive(Clone, PartialEq)]
pub struct GameRow {
    pub id: String,
    pub title: String,
    /// Badge text. Kept as the e2e specs' handle on a row (`UNSTARTED` / `ACTIVE`
    /// / `COMPLETED`) — don't rename without updating `e2e/tests/`.
    pub badge: &'static str,
    pub tone: Tone,
    /// Pre-composed secondary line, e.g. `"12 clues · 2 players · 3h ago"`.
    pub meta: String,
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

/// Render `rows` as a keyboard-operable list. `filter` is `Some` on panels worth
/// searching; the filter is applied here so every caller gets it for free.
///
/// Linear in `rows` — one substring test per row, no per-row allocation beyond
/// the lowercased title the match needs.
pub fn game_list(
    rows: Vec<GameRow>,
    empty: &str,
    filter: Option<Signal<String>>,
    on_pick: impl FnMut(String) + Clone + 'static,
) -> Element {
    let needle = filter
        .map(|f| f.read().trim().to_lowercase())
        .unwrap_or_default();
    let shown: Vec<GameRow> = if needle.is_empty() {
        rows
    } else {
        rows.into_iter()
            .filter(|r| r.title.to_lowercase().contains(&needle))
            .collect()
    };

    let count = shown.len();
    let searching = !needle.is_empty();

    rsx! {
        div { class: "game-list",
            if let Some(mut f) = filter {
                div { class: "game-list-tools",
                    input {
                        class: "app-input game-list-search",
                        r#type: "search",
                        placeholder: "Search games…",
                        aria_label: "Search games",
                        value: f.read().clone(),
                        oninput: move |e| f.set(e.value()),
                    }
                    span { class: "game-list-count", "{count}" }
                }
            }

            if shown.is_empty() {
                {status("muted", if searching { "No games match that search." } else { empty }, false)}
            } else {
                for row in shown {
                    {
                        let id = row.id.clone();
                        let mut pick = on_pick.clone();
                        let mut pick_key = on_pick.clone();
                        let key_id = id.clone();
                        rsx! {
                            div {
                                key: "{row.id}",
                                class: "game-row",
                                // Inline `cursor: pointer` is also the e2e specs'
                                // row selector — keep it alongside the class.
                                style: "cursor: pointer;",
                                role: "button",
                                tabindex: "0",
                                aria_label: "{row.title} — {row.badge}",
                                onclick: move |_| pick(id.clone()),
                                onkeydown: move |e| {
                                    // Enter / Space activate, matching a native button.
                                    let k = e.key();
                                    if k == Key::Enter || k == Key::Character(" ".into()) {
                                        e.prevent_default();
                                        pick_key(key_id.clone());
                                    }
                                },
                                div { class: "game-row-body",
                                    span { class: "game-row-title", "{row.title}" }
                                    if !row.meta.is_empty() {
                                        span { class: "game-row-meta", "{row.meta}" }
                                    }
                                }
                                span { class: "game-row-badge", style: "{row.tone.badge_style()}", "{row.badge}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Injected once by the lobby page. Square corners throughout (house style).
pub const GAME_LIST_CSS: &str = "
.game-list { display: flex; flex-direction: column; }
.game-list-tools { display: flex; align-items: center; gap: .5rem; padding: .625rem .75rem;
  border-bottom: 1px solid var(--border-app); position: sticky; top: 0; background: var(--bg-card); z-index: 1; }
.game-list-search { flex: 1 1 auto; min-width: 0; font-family: var(--mono, monospace); font-size: var(--fs-xs); }
.game-list-count { font-family: var(--mono, monospace); font-size: var(--fs-2xs); font-weight: 700;
  color: var(--text-secondary); border: 1px solid var(--border-app); padding: .125rem .375rem; }

.game-row { display: flex; flex-direction: row; align-items: center; justify-content: space-between;
  gap: .75rem; padding: .875rem 1rem; border-bottom: 1px solid var(--border-app);
  border-left: 2px solid transparent; transition: background-color .12s ease, border-color .12s ease; }
.game-row:hover { background: var(--bg-cell-letter); border-left-color: var(--color-primary); }
.game-row:focus-visible { outline: none; background: var(--bg-cell-letter); border-left-color: var(--color-primary); }
.game-row:active { background: var(--bg-cell-empty); }
.game-row-body { display: flex; flex-direction: column; gap: .2rem; min-width: 0; }
.game-row-title { font-weight: 700; font-size: var(--fs-md); color: var(--text-primary);
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.game-row-meta { font-family: var(--mono, monospace); font-size: var(--fs-2xs); color: var(--text-secondary); }
.game-row-badge { flex-shrink: 0; padding: .25rem .625rem; font-size: var(--fs-xs);
  font-family: var(--mono, monospace); font-weight: 700; text-transform: uppercase; }


@media (max-width: 760px) {
  /* Comfortable tap target on touch, and no hover-stick on mobile Safari. */
  .game-list-tools { position: static; }
  .game-row { padding: 1rem; min-height: 3.25rem; }
  .game-row:hover { background: transparent; border-left-color: transparent; }
}
";
