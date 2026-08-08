//! Shared UI atoms. Classes live in `styles.rs` (injected once at the root);
//! these components only compose markup.

use crate::store::use_app_state;
use dioxus::prelude::*;

/// Global toast stack, rendered once in the shell. Desktop: top-right.
/// Mobile: bottom-center. Queue via `AppState::toast(severity, msg)`;
/// toasts auto-dismiss after 5s or on click.
#[component]
pub fn ToastHost() -> Element {
    let state = use_app_state();
    let toasts = state.toasts.read().clone();
    rsx! {
        div { class: "toast-host",
            for t in toasts {
                div {
                    key: "{t.id}",
                    class: "toast {t.severity.class()}",
                    role: "status",
                    onclick: move |_| {
                        let mut toasts = state.toasts;
                        toasts.write().retain(|x| x.id != t.id);
                    },
                    "{t.msg}"
                }
            }
        }
    }
}

/// Podium colours come from the palette tokens in `styles.rs` so they flip with
/// the theme instead of hardcoding hex per rank.
pub fn rank_badge_style(index: usize) -> &'static str {
    match index {
        0 => "background: var(--pastel-yellow); color: var(--contrast-ink); border-color: var(--pastel-yellow);",
        1 => "background: var(--podium-silver); color: var(--contrast-ink); border-color: var(--podium-silver);",
        // Bronze keeps a fixed near-white ink — a theme-flipping colour would go
        // dark-on-dark-orange in light mode.
        2 => "background: var(--podium-bronze); color: #f8fafc; border-color: var(--podium-bronze);",
        _ => "background: var(--bg-cell-empty); color: var(--text-secondary); border-color: var(--border-app);",
    }
}

/// Square rank badge: gold/silver/bronze for the podium, neutral below.
/// `label` is the display text (usually the 1-based rank; ties may repeat).
#[component]
pub fn RankBadge(index: usize, label: String) -> Element {
    rsx! {
        span { class: "rank-badge", style: "{rank_badge_style(index)}", "{label}" }
    }
}

/// Square stat tile: uppercase mono eyebrow, big tabular number, optional sub.
#[component]
pub fn StatTile(label: String, value: String, sub: Option<String>) -> Element {
    rsx! {
        div { class: "app-card stat-tile",
            span { class: "stat-tile-label", "{label}" }
            span { class: "stat-tile-value", "{value}" }
            if let Some(s) = sub {
                span { class: "stat-tile-sub muted", "{s}" }
            }
        }
    }
}

/// Square segmented tab strip. Uppercase mono labels, active tab in yellow.
/// Full-width segments under 760px (CSS).
#[component]
pub fn SectionTabs(tabs: Vec<String>, active: usize, on_select: EventHandler<usize>) -> Element {
    rsx! {
        div { class: "section-tabs", role: "tablist",
            for (i, t) in tabs.into_iter().enumerate() {
                button {
                    class: if i == active { "section-tab section-tab-active" } else { "section-tab" },
                    role: "tab",
                    aria_selected: i == active,
                    onclick: move |_| on_select.call(i),
                    "{t}"
                }
            }
        }
    }
}

/// Square dot-grid pulse spinner (5×5, identicon idiom). The app's only
/// spinner — circles are off-brand.
#[component]
pub fn SquarePulse() -> Element {
    rsx! {
        div { class: "square-pulse", role: "status", aria_label: "Loading",
            for i in 0..25usize {
                // diagonal wave: delay grows with manhattan distance from the corner
                div {
                    class: "square-pulse-cell",
                    style: "animation-delay: {(i % 5 + i / 5) * 80}ms;",
                }
            }
        }
    }
}
