//! Shared admin building blocks (GH-61): KPI tiles, status/metadata chips
//! rendered through panel-kit's `Badge`, table scaffolding, and the mobile
//! read-only banner. Extracted from the four former admin pages so the
//! merged `/admin` view stays DRY.

use dioxus::prelude::*;
use panel_kit::badge::{Badge, BadgeKind};

/// Per-source KPI tile state: `None` = loading, `Err` = fetch/parse failure.
/// `Ok` carries (value, optional sub-line).
pub type TileState = Option<Result<(String, Option<String>), String>>;

/// Square stat tile that renders "—" while its source loads and "!" (with the
/// error in a title tooltip) when it fails — each tile degrades independently.
pub fn kpi_tile(label: &'static str, state: &TileState) -> Element {
    match state {
        None => rsx! {
            div { class: "app-card stat-tile",
                span { class: "stat-tile-label", {label} }
                span { class: "stat-tile-value muted", "—" }
            }
        },
        Some(Err(e)) => rsx! {
            div { class: "app-card stat-tile", title: "{e}",
                span { class: "stat-tile-label", {label} }
                span { class: "stat-tile-value error", "!" }
            }
        },
        Some(Ok((value, sub))) => rsx! {
            div { class: "app-card stat-tile",
                span { class: "stat-tile-label", {label} }
                span { class: "stat-tile-value", {value.clone()} }
                if let Some(s) = sub {
                    span { class: "stat-tile-sub muted", {s.clone()} }
                }
            }
        },
    }
}

/// Accent token for a generator-job status value.
pub fn job_status_accent(status: &str) -> &'static str {
    match status {
        "SUCCEEDED" => "var(--color-success)",
        "FAILED" => "var(--color-error)",
        _ => "var(--color-warning)",
    }
}

/// Status chip (job status, discount active/inactive, verified/pending)
/// rendered through panel-kit's `Badge`.
pub fn status_badge(value: String, accent: &'static str) -> Element {
    rsx! {
        Badge {
            field: "status".to_string(),
            value,
            kind: BadgeKind::Status,
            small: true,
            accent_color: Some(accent.to_string()),
            on_action: move |_| {},
        }
    }
}

/// Small tag chip (role, test-mode, new-user, env) rendered through
/// panel-kit's `Badge`.
pub fn tag_badge(field: &'static str, value: String, accent: Option<&'static str>) -> Element {
    rsx! {
        Badge {
            field: field.to_string(),
            value,
            kind: BadgeKind::Tag,
            small: true,
            accent_color: accent.map(|a| a.to_string()),
            on_action: move |_| {},
        }
    }
}

/// Verified/pending pill shared by the users table and the detail drawer.
pub fn verified_badge(verified: bool) -> Element {
    status_badge(
        if verified { "Verified" } else { "Pending" }.to_string(),
        if verified {
            "var(--color-success)"
        } else {
            "var(--text-secondary)"
        },
    )
}

/// Accent token for a role tag chip.
pub fn role_accent(role: &str) -> &'static str {
    match role {
        "ADMIN" => "var(--color-warning)",
        "VIP" => "var(--color-success)",
        _ => "var(--text-secondary)",
    }
}

/// Environment chip: PRODUCTION is red, everything else (staging, dev) is
/// yellow. Rendered in the shared `AppHeader` for admins.
pub fn env_badge(environment: &str) -> Element {
    let accent = if environment == "production" {
        "var(--color-error)"
    } else {
        "var(--color-warning)"
    };
    tag_badge("env", environment.to_uppercase(), Some(accent))
}

/// Shared table header row: monospace uppercase column labels.
pub fn table_head(cols: Vec<&'static str>) -> Element {
    rsx! {
        thead {
            tr { style: "font-size:0.75rem;text-transform:uppercase;font-family:monospace",
                for col in cols {
                    th { class: "muted", style: "padding:0.75rem 1rem;border-bottom:1px solid var(--border-app)", {col} }
                }
            }
        }
    }
}

/// Shared full-width status row for loading/empty table states.
pub fn table_status_row(colspan: &'static str, text: String) -> Element {
    rsx! {
        tr {
            td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan, {text} }
        }
    }
}

/// Mobile read-only banner shared by the admin write surfaces.
pub fn mobile_banner() -> Element {
    rsx! {
        div { class: "muted", style: "font-size:0.75rem;padding:0.5rem 1rem;border-bottom:1px solid var(--border-app)",
            "Editing requires a desktop viewport."
        }
    }
}
