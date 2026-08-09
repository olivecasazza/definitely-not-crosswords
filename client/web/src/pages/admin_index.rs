use crate::net::{query, trpc_err_msg};
use crate::Route;
use crossword_core::fmt::rel_time;
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

// ── data rows (only the fields the dashboard aggregates) ─────────────────────

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserRow {
    name: Option<String>,
    email: Option<String>,
    role: String,
    vip_pass: bool,
    email_verified: Option<serde_json::Value>,
    // Additive server field — older servers simply omit it.
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobRow {
    status: String,
    topic: String,
    created_at: String,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscountRow {
    is_active: bool,
    times_redeemed: i64,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LeaderboardRow {
    games_played: i64,
}

// ── KPI tile ─────────────────────────────────────────────────────────────────

/// Per-source tile state: None = loading, Err = fetch/parse failure.
/// Ok carries (value, optional sub-line).
type TileState = Option<Result<(String, Option<String>), String>>;

/// Square stat tile that renders "—" while its source loads and "!" (with the
/// error in a title tooltip) when it fails — each tile degrades independently.
fn kpi_tile(label: &'static str, state: &TileState) -> Element {
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

// ── activity feed ────────────────────────────────────────────────────────────

enum FeedKind {
    Job { status: String, topic: String },
    User { display: String, email: String },
}

struct FeedItem {
    ts: f64,
    kind: FeedKind,
}

/// Status badge colours match the generator jobs table.
fn job_badge_style(status: &str) -> &'static str {
    match status {
        "SUCCEEDED" => "background:var(--color-success);color:var(--contrast-ink)",
        "FAILED" => "background:var(--color-error);color:var(--contrast-ink)",
        _ => "background:var(--pastel-yellow);color:var(--contrast-ink)",
    }
}

// ── panels ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Overview,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Overview => "Overview",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![b.at(Panel::Overview, 16.0, 16.0, 1888.0, 880.0)]
}

// ── component ────────────────────────────────────────────────────────────────

#[component]
pub fn AdminIndex() -> Element {
    let nav = use_navigator();

    // One independent fetch per source so each tile loads / fails on its own.
    let mut users_res = use_signal(|| None::<Result<Vec<UserRow>, String>>);
    let mut jobs_res = use_signal(|| None::<Result<Vec<JobRow>, String>>);
    let mut discounts_res = use_signal(|| None::<Result<Vec<DiscountRow>, String>>);
    let mut games_res = use_signal(|| None::<Result<i64, String>>);

    use_effect(move || {
        spawn_local(async move {
            let r = query("user.listForAdmin", None)
                .await
                .map_err(trpc_err_msg)
                .and_then(|v| serde_json::from_value::<Vec<UserRow>>(v).map_err(|e| e.to_string()));
            users_res.set(Some(r));
        });
        spawn_local(async move {
            let r = query("generator.listJobs", Some(json!({"take": 25})))
                .await
                .map_err(trpc_err_msg)
                .and_then(|v| serde_json::from_value::<Vec<JobRow>>(v).map_err(|e| e.to_string()));
            jobs_res.set(Some(r));
        });
        spawn_local(async move {
            let r = query("discount.listForAdmin", None)
                .await
                .map_err(trpc_err_msg)
                .and_then(|v| {
                    serde_json::from_value::<Vec<DiscountRow>>(v).map_err(|e| e.to_string())
                });
            discounts_res.set(Some(r));
        });
        spawn_local(async move {
            // Sum of per-user gamesPlayed — the leaderboard's row count is just
            // the user count, so the aggregate is the meaningful total.
            let r = query("stats.getGlobalLeaderboard", None)
                .await
                .map_err(trpc_err_msg)
                .and_then(|v| {
                    serde_json::from_value::<Vec<LeaderboardRow>>(v).map_err(|e| e.to_string())
                })
                .map(|rows| rows.iter().map(|x| x.games_played).sum());
            games_res.set(Some(r));
        });
    });

    let ws = use_workspace("admin_index_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);
    if let Some(gate) = crate::store::use_auth_guard(crossword_core::auth::Role::Admin) {
        return gate;
    }

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Overview => {
                // ── KPI tile states, derived per source ────────────────────
                let users_read = users_res.read();
                let (users_t, verified_t, admins_t, vip_t): (
                    TileState,
                    TileState,
                    TileState,
                    TileState,
                ) = match &*users_read {
                    None => (None, None, None, None),
                    Some(Err(e)) => (
                        Some(Err(e.clone())),
                        Some(Err(e.clone())),
                        Some(Err(e.clone())),
                        Some(Err(e.clone())),
                    ),
                    Some(Ok(list)) => {
                        let total = list.len();
                        let verified = list.iter().filter(|u| u.email_verified.is_some()).count();
                        let admins = list.iter().filter(|u| u.role == "ADMIN").count();
                        let vip = list.iter().filter(|u| u.vip_pass).count();
                        (
                            Some(Ok((total.to_string(), None))),
                            Some(Ok((verified.to_string(), Some(format!("of {total}"))))),
                            Some(Ok((admins.to_string(), None))),
                            Some(Ok((vip.to_string(), None))),
                        )
                    }
                };

                let jobs_read = jobs_res.read();
                let jobs_t: TileState = match &*jobs_read {
                    None => None,
                    Some(Err(e)) => Some(Err(e.clone())),
                    Some(Ok(jobs)) => {
                        let now = js_sys::Date::now();
                        let (mut ok, mut fail) = (0u32, 0u32);
                        for j in jobs.iter() {
                            let ms = js_sys::Date::parse(&j.created_at);
                            if ms.is_nan() || now - ms > 86_400_000.0 {
                                continue;
                            }
                            match j.status.as_str() {
                                "SUCCEEDED" => ok += 1,
                                "FAILED" => fail += 1,
                                _ => {}
                            }
                        }
                        Some(Ok((
                            format!("{ok}/{fail}"),
                            Some("ok / failed".to_string()),
                        )))
                    }
                };

                let discounts_read = discounts_res.read();
                let discounts_t: TileState = match &*discounts_read {
                    None => None,
                    Some(Err(e)) => Some(Err(e.clone())),
                    Some(Ok(list)) => {
                        let active = list.iter().filter(|d| d.is_active).count();
                        let redeemed: i64 = list.iter().map(|d| d.times_redeemed).sum();
                        Some(Ok((
                            active.to_string(),
                            Some(format!("{redeemed} redemptions")),
                        )))
                    }
                };

                let games_read = games_res.read();
                let games_t: TileState = match &*games_read {
                    None => None,
                    Some(Err(e)) => Some(Err(e.clone())),
                    Some(Ok(total)) => {
                        Some(Ok((total.to_string(), Some("all players".to_string()))))
                    }
                };

                // ── merged activity feed, newest first ─────────────────────
                let now = js_sys::Date::now();
                let mut items: Vec<FeedItem> = Vec::new();
                if let Some(Ok(jobs)) = &*jobs_read {
                    for j in jobs.iter().take(5) {
                        let ms = js_sys::Date::parse(&j.created_at);
                        items.push(FeedItem {
                            ts: if ms.is_nan() { 0.0 } else { ms },
                            kind: FeedKind::Job {
                                status: j.status.clone(),
                                topic: j.topic.clone(),
                            },
                        });
                    }
                }
                if let Some(Ok(users)) = &*users_read {
                    let mut with_ts: Vec<(f64, &UserRow)> = users
                        .iter()
                        .filter_map(|u| {
                            let ms = js_sys::Date::parse(u.created_at.as_deref()?);
                            (!ms.is_nan()).then_some((ms, u))
                        })
                        .collect();
                    with_ts
                        .sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                    for (ms, u) in with_ts.into_iter().take(5) {
                        let display = u
                            .name
                            .clone()
                            .or_else(|| u.email.clone())
                            .unwrap_or_else(|| "Unnamed user".to_string());
                        items.push(FeedItem {
                            ts: ms,
                            kind: FeedKind::User {
                                display,
                                email: u.email.clone().unwrap_or_default(),
                            },
                        });
                    }
                }
                items.sort_by(|a, b| b.ts.partial_cmp(&a.ts).unwrap_or(std::cmp::Ordering::Equal));

                let feed_loading = users_read.is_none() && jobs_read.is_none();
                let jobs_err = match &*jobs_read {
                    Some(Err(e)) => Some(e.clone()),
                    _ => None,
                };
                let users_err = match &*users_read {
                    Some(Err(e)) => Some(e.clone()),
                    _ => None,
                };

                rsx! {
                    div { class: "col", style: "gap:1.5rem;height:100%;overflow-y:auto",
                        // ── KPI strip ──────────────────────────────────────
                        div { style: "display:grid;gap:0.75rem;grid-template-columns:repeat(auto-fit,minmax(150px,1fr))",
                            {kpi_tile("Users", &users_t)}
                            {kpi_tile("Verified", &verified_t)}
                            {kpi_tile("Admins", &admins_t)}
                            {kpi_tile("VIP", &vip_t)}
                            {kpi_tile("Jobs 24h", &jobs_t)}
                            {kpi_tile("Active discounts", &discounts_t)}
                            {kpi_tile("Games played", &games_t)}
                        }

                        // ── recent activity ────────────────────────────────
                        div { class: "app-card", style: "padding:0",
                            div { style: "padding:0.75rem 1rem;border-bottom:1px solid var(--border-app)",
                                h2 { class: "muted", style: "font-size:0.75rem;font-weight:bold;font-family:monospace;text-transform:uppercase;letter-spacing:0.05em",
                                    "Recent activity"
                                }
                            }
                            if let Some(e) = jobs_err {
                                div { class: "error", style: "padding:0.5rem 1rem;font-size:0.75rem;border-bottom:1px solid var(--border-app)",
                                    "Jobs unavailable: {e}"
                                }
                            }
                            if let Some(e) = users_err {
                                div { class: "error", style: "padding:0.5rem 1rem;font-size:0.75rem;border-bottom:1px solid var(--border-app)",
                                    "Users unavailable: {e}"
                                }
                            }
                            if feed_loading {
                                div { class: "muted", style: "padding:1.5rem 1rem;text-align:center;font-size:0.875rem",
                                    "Loading activity…"
                                }
                            } else if items.is_empty() {
                                div { class: "muted", style: "padding:1.5rem 1rem;text-align:center;font-size:0.875rem",
                                    "No recent activity."
                                }
                            }
                            for (i, item) in items.iter().enumerate() {
                                {
                                    let when = if item.ts > 0.0 { rel_time(now, item.ts) } else { "—".to_string() };
                                    match &item.kind {
                                        FeedKind::Job { status, topic } => {
                                            let badge = job_badge_style(status);
                                            let status = status.clone();
                                            let topic = topic.clone();
                                            rsx! {
                                                button {
                                                    key: "{i}",
                                                    class: "row",
                                                    style: "width:100%;gap:0.75rem;align-items:center;padding:0.625rem 1rem;border:none;border-bottom:1px solid var(--border-app);background:transparent;color:inherit;text-align:left;cursor:pointer;font-size:0.875rem",
                                                    aria_label: "Open generator: {topic}",
                                                    onclick: move |_| { nav.push(Route::AdminGenerator {}); },
                                                    span {
                                                        style: "padding:0.125rem 0.5rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;{badge}",
                                                        {status.clone()}
                                                    }
                                                    span { style: "flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap",
                                                        {topic.clone()}
                                                    }
                                                    span { class: "muted", style: "font-size:0.75rem;white-space:nowrap", {when} }
                                                }
                                            }
                                        }
                                        FeedKind::User { display, email } => rsx! {
                                            div {
                                                key: "{i}",
                                                class: "row",
                                                style: "gap:0.75rem;align-items:center;padding:0.625rem 1rem;border-bottom:1px solid var(--border-app);font-size:0.875rem",
                                                span {
                                                    style: "padding:0.125rem 0.5rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;border:1px solid var(--border-app);color:var(--text-secondary)",
                                                    "New user"
                                                }
                                                span { style: "flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap",
                                                    {display.clone()}
                                                    if !email.is_empty() && email != display {
                                                        span { class: "muted", style: "font-size:0.75rem;margin-left:0.5rem", {email.clone()} }
                                                    }
                                                }
                                                span { class: "muted", style: "font-size:0.75rem;white-space:nowrap", {when} }
                                            }
                                        },
                                    }
                                }
                            }
                        }

                        // ── shortcuts ──────────────────────────────────────
                        div { style: "display:grid;gap:1rem;grid-template-columns:repeat(auto-fit,minmax(220px,1fr))",
                            for (title, blurb, route) in [
                                ("Generator", "Create, review, and publish generated crossword drafts.", Route::AdminGenerator {}),
                                ("Users", "Add admins and manage user roles.", Route::AdminUsers {}),
                                ("Discounts", "Create and manage discount codes.", Route::AdminDiscounts {}),
                                ("View lobby as player", "See the games list the way players do.", Route::Games {}),
                            ] {
                                button {
                                    class: "app-card",
                                    style: "padding:1.25rem;text-align:left;cursor:pointer",
                                    onclick: move |_| { nav.push(route.clone()); },
                                    div { style: "font-size:0.875rem;font-weight:bold;text-transform:uppercase;letter-spacing:0.05em",
                                        {title}
                                    }
                                    div { class: "muted", style: "margin-top:0.5rem;font-size:0.75rem;line-height:1.5",
                                        {blurb}
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
