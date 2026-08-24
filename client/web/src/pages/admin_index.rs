//! The single admin view (GH-61): one panel-kit workspace at `/admin` whose
//! eight panels replace the old per-route admin pages. The dock is the
//! navigation — write surfaces (Parameters, Add User, Create) start minimized
//! as dock chips; per-source fetches only fire while a panel that needs them
//! is open.

use crate::components::admin::{
    job_status_accent, kpi_tile, mobile_banner, role_accent, status_badge, table_head,
    table_status_row, tag_badge, verified_badge, TileState,
};
use crate::components::generation_progress::{GenerationProgress, Progress};
use crate::components::identicon::Identicon;
use crate::components::ui::Drawer;
use crate::net::{mutation, query, subscribe, trpc_err_msg, Subscription};
use crate::store::{use_app_state, Severity};
use crate::Route;
use crossword_core::fmt::{format_date, format_datetime, rel_time};
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin, WinState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use wasm_bindgen_futures::spawn_local;

// ── data rows ────────────────────────────────────────────────────────────────

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminUser {
    id: String,
    email: Option<String>,
    username: Option<String>,
    name: Option<String>,
    role: String,
    vip_pass: bool,
    // nullable timestamp — presence = verified
    email_verified: Option<serde_json::Value>,
    // Additive server field — older servers omit it, so default to None.
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Clone, serde::Deserialize)]
struct RoleOption {
    role: String,
    capabilities: Vec<String>,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobRow {
    id: String,
    status: String,
    topic: String,
    width: i64,
    height: i64,
    created_at: String,
    result_game: Option<ResultGame>,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultGame {
    id: String,
    title: String,
    published: bool,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Discount {
    id: String,
    code: String,
    name: String,
    amount_type: String,
    amount: i64,
    duration: String,
    duration_in_months: Option<i64>,
    max_redemptions: Option<i64>,
    times_redeemed: i64,
    expires_at: Option<String>,
    is_active: bool,
    test_mode: bool,
}

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LeaderboardRow {
    games_played: i64,
}

// ── generator form state ─────────────────────────────────────────────────────

#[derive(Clone)]
struct GenForm {
    topic: String,
    width: i64,
    height: i64,
    min_word_length: i64,
    max_word_length: i64,
    target_words: i64,
    runs: i64,
    max_attempts: i64,
}

impl Default for GenForm {
    fn default() -> Self {
        Self {
            topic: "space exploration and planetary science".to_string(),
            width: 21,
            height: 21,
            min_word_length: 3,
            max_word_length: 12,
            target_words: 42,
            runs: 20,
            max_attempts: 180,
        }
    }
}

fn form_to_json(f: &GenForm) -> Value {
    json!({
        "params": {
            "topic": f.topic,
            "width": f.width,
            "height": f.height,
            "minWordLength": f.min_word_length,
            "maxWordLength": f.max_word_length,
            "targetWords": f.target_words,
            "runs": f.runs,
            "maxAttempts": f.max_attempts,
        }
    })
}

// ── discount formatting helpers ──────────────────────────────────────────────

fn format_amount(d: &Discount) -> String {
    if d.amount_type == "PERCENT" {
        format!("{}%", d.amount)
    } else {
        // stored as cents
        format!("${:.2}", d.amount as f64 / 100.0)
    }
}

fn format_duration(d: &Discount) -> &'static str {
    match d.duration.as_str() {
        "ONCE" => "Once",
        "FOREVER" => "Forever",
        _ => "Repeating",
    }
}

fn format_expiry(s: &Option<String>) -> String {
    match s {
        None => "—".to_string(),
        Some(v) => {
            // ISO date string: take the date part only (before 'T')
            v.split('T').next().unwrap_or(v).to_string()
        }
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

// ── fetch helpers ────────────────────────────────────────────────────────────

fn do_load_jobs(
    mut jobs: Signal<Vec<JobRow>>,
    mut jobs_loading: Signal<bool>,
    mut jobs_error: Signal<String>,
    take: i64,
) {
    jobs_loading.set(true);
    jobs_error.set(String::new());
    spawn_local(async move {
        match query("generator.listJobs", Some(json!({"take": take}))).await {
            Ok(v) => {
                let parsed: Vec<JobRow> = serde_json::from_value(v).unwrap_or_default();
                jobs.set(parsed);
            }
            Err(e) => jobs_error.set(e),
        }
        jobs_loading.set(false);
    });
}

// ── panels ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum AdminPanel {
    Overview,
    Parameters,
    Run,
    Jobs,
    Users,
    AddUser,
    Discounts,
    Create,
}

impl PanelKind for AdminPanel {
    fn title(self) -> &'static str {
        match self {
            AdminPanel::Overview => "Overview",
            AdminPanel::Parameters => "Parameters",
            AdminPanel::Run => "Run",
            AdminPanel::Jobs => "Jobs",
            AdminPanel::Users => "Users",
            AdminPanel::AddUser => "Add User",
            AdminPanel::Discounts => "Discounts",
            AdminPanel::Create => "Create",
        }
    }
}

fn default_layout() -> Vec<PanelWin<AdminPanel>> {
    let mut b = LayoutBuilder::new();

    let overview = b
        .at(AdminPanel::Overview, 16.0, 16.0, 1888.0, 880.0)
        .with_tile_flex(100.0, 1.0);
    let mut params = b
        .at(AdminPanel::Parameters, 16.0, 16.0, 640.0, 880.0)
        .with_tile_flex(34.0, 1.0);
    let run = b
        .at(AdminPanel::Run, 672.0, 16.0, 1232.0, 432.0)
        .with_tile_flex(66.0, 1.0);
    let jobs = b
        .at(AdminPanel::Jobs, 672.0, 464.0, 1232.0, 432.0)
        .with_tile_flex(66.0, 1.0);
    let users = b
        .at(AdminPanel::Users, 592.0, 16.0, 1312.0, 880.0)
        .with_tile_flex(66.0, 1.0);
    let mut add_user = b
        .at(AdminPanel::AddUser, 16.0, 16.0, 560.0, 880.0)
        .with_tile_flex(34.0, 1.0);
    let discounts = b
        .at(AdminPanel::Discounts, 592.0, 16.0, 1312.0, 880.0)
        .with_tile_flex(66.0, 1.0);
    let mut create = b
        .at(AdminPanel::Create, 16.0, 16.0, 560.0, 880.0)
        .with_tile_flex(34.0, 1.0);

    // Open on first load: the read/monitor surfaces.
    // Minimized → dock chips: the write surfaces.
    params.state = WinState::Minimized;
    add_user.state = WinState::Minimized;
    create.state = WinState::Minimized;

    vec![
        overview, params, run, jobs, users, add_user, discounts, create,
    ]
}

// ── component ────────────────────────────────────────────────────────────────

#[component]
pub fn AdminIndex() -> Element {
    let nav = use_navigator();
    let state = use_app_state();

    // ── users + roles ─────────────────────────────────────────────────────
    let mut users_res = use_signal(|| None::<Result<Vec<AdminUser>, String>>);
    let mut role_options = use_signal(Vec::<RoleOption>::new);
    let mut saving = use_signal(|| false);
    let mut saving_role_id = use_signal(|| None::<String>);
    let mut saving_vip_id = use_signal(|| None::<String>);
    let mut user_message = use_signal(String::new);
    let mut users_error = use_signal(String::new);

    // add-user form
    let mut new_email = use_signal(String::new);
    let mut new_name = use_signal(String::new);
    let mut new_role = use_signal(|| "ADMIN".to_string());

    // users search + filters (client-side, over the fetched list)
    let mut search = use_signal(String::new);
    let mut filter_role = use_signal(|| None::<&'static str>);
    let mut filter_verified = use_signal(|| None::<bool>);
    let mut filter_vip_only = use_signal(|| false);

    // detail drawer — holds the selected user's id; the row itself is looked
    // up live from `users_res` so refreshes keep the drawer in sync.
    let mut selected_id = use_signal(|| None::<String>);

    // set-password form (drawer)
    let mut pw = use_signal(String::new);
    let mut pw_confirm = use_signal(String::new);
    let mut pw_error = use_signal(String::new);
    let mut saving_pw = use_signal(|| false);

    // ── generator ─────────────────────────────────────────────────────────
    let mut form = use_signal(GenForm::default);
    let jobs = use_signal(Vec::<JobRow>::new);
    let jobs_loading = use_signal(|| false);
    let jobs_error = use_signal(String::new);
    let mut jobs_take = use_signal(|| 25i64);
    let mut job_search = use_signal(String::new);
    let mut status_filter = use_signal(String::new);

    let mut gen_log = use_signal(Vec::<Value>::new);
    let mut gen_progress = use_signal(|| None::<Progress>);
    // "idle" | "running" | "succeeded" | "failed"
    let mut gen_status = use_signal(|| "idle".to_string());
    let mut gen_error = use_signal(String::new);
    let mut elapsed_secs = use_signal(|| 0u64);
    // Subscription handle kept alive in a signal; drop = unsubscribe
    let mut sub_handle = use_signal(|| None::<Subscription>);
    let mut gen_game_id = use_signal(|| None::<String>);
    let mut gen_game_title = use_signal(|| None::<String>);
    let mut gen_game_published = use_signal(|| false);
    let mut publishing = use_signal(|| false);
    let mut publish_error = use_signal(String::new);

    // ── discounts ─────────────────────────────────────────────────────────
    let mut discounts = use_signal(Vec::<Discount>::new);
    let mut discounts_loading = use_signal(|| true);
    let mut discount_saving = use_signal(|| false);
    let mut saving_ids = use_signal(Vec::<String>::new);
    let mut discount_message = use_signal(String::new);
    let mut discount_error = use_signal(String::new);
    let mut pending_delete = use_signal(|| Option::<Discount>::None);

    // discount create form
    let mut f_code = use_signal(String::new);
    let mut f_name = use_signal(String::new);
    let mut f_amount_type = use_signal(|| "PERCENT".to_string());
    let mut f_amount = use_signal(String::new);
    let mut f_amount_err = use_signal(String::new);
    let mut f_duration = use_signal(|| "ONCE".to_string());
    let mut f_duration_months = use_signal(String::new);
    let mut f_max_redemptions = use_signal(String::new);
    let mut f_expires_at = use_signal(String::new); // YYYY-MM-DD from date input
    let mut f_test_mode = use_signal(|| false);

    // ── overview-only ─────────────────────────────────────────────────────
    let mut games_res = use_signal(|| None::<Result<i64, String>>);

    let refresh_users = move || {
        spawn_local(async move {
            let r = query("user.listForAdmin", None)
                .await
                .map_err(trpc_err_msg)
                .and_then(|v| {
                    serde_json::from_value::<Vec<AdminUser>>(v).map_err(|e| e.to_string())
                });
            users_res.set(Some(r));
        });
    };

    let refresh_discounts = move || {
        spawn_local(async move {
            match query("discount.listForAdmin", None).await {
                Ok(v) => {
                    let parsed: Vec<Discount> = serde_json::from_value(v).unwrap_or_default();
                    discounts.set(parsed);
                }
                Err(e) => discount_error.set(trpc_err_msg(e)),
            }
        });
    };

    // ── workspace ─────────────────────────────────────────────────────────

    let ws = use_workspace("admin_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    // Per-source fetches fire only while a panel that needs the source is
    // open (not minimized to the dock), and refetch on restore.

    // users + roles: Overview KPIs/feed, Users table, AddUser role select
    let mut users_seen_open = use_signal(|| false);
    use_effect(move || {
        let open = ws.panels.read().iter().any(|p| {
            matches!(
                p.kind,
                AdminPanel::Overview | AdminPanel::Users | AdminPanel::AddUser
            ) && p.state != WinState::Minimized
        });
        if open && !*users_seen_open.peek() {
            users_seen_open.set(true);
            refresh_users();
            spawn_local(async move {
                match query("user.roleOptions", None).await {
                    Ok(v) => {
                        let parsed: Vec<RoleOption> = serde_json::from_value(v).unwrap_or_default();
                        if let Some(first) = parsed.first() {
                            new_role.set(first.role.clone());
                        }
                        role_options.set(parsed);
                    }
                    Err(e) => users_error.set(e),
                }
            });
        } else if !open && *users_seen_open.peek() {
            users_seen_open.set(false);
        }
    });

    // jobs: Overview KPI/feed, Run last-run line, Jobs table. Also reactive
    // on `jobs_take` (Load more) while open.
    let mut jobs_seen_open = use_signal(|| false);
    let mut jobs_last_take = use_signal(|| 0i64);
    use_effect(move || {
        let take = *jobs_take.read();
        let open = ws.panels.read().iter().any(|p| {
            matches!(
                p.kind,
                AdminPanel::Overview | AdminPanel::Run | AdminPanel::Jobs
            ) && p.state != WinState::Minimized
        });
        if open && (!*jobs_seen_open.peek() || *jobs_last_take.peek() != take) {
            jobs_seen_open.set(true);
            jobs_last_take.set(take);
            do_load_jobs(jobs, jobs_loading, jobs_error, take);
        } else if !open && *jobs_seen_open.peek() {
            jobs_seen_open.set(false);
        }
    });

    // discounts: Overview KPI, Discounts table
    let mut discounts_seen_open = use_signal(|| false);
    use_effect(move || {
        let open = ws.panels.read().iter().any(|p| {
            matches!(p.kind, AdminPanel::Overview | AdminPanel::Discounts)
                && p.state != WinState::Minimized
        });
        if open && !*discounts_seen_open.peek() {
            discounts_seen_open.set(true);
            discounts_loading.set(true);
            spawn_local(async move {
                match query("discount.listForAdmin", None).await {
                    Ok(v) => {
                        let parsed: Vec<Discount> = serde_json::from_value(v).unwrap_or_default();
                        discounts.set(parsed);
                    }
                    Err(e) => discount_error.set(trpc_err_msg(e)),
                }
                discounts_loading.set(false);
            });
        } else if !open && *discounts_seen_open.peek() {
            discounts_seen_open.set(false);
        }
    });

    // games-played KPI: Overview only
    let mut games_seen_open = use_signal(|| false);
    use_effect(move || {
        let open = ws
            .panels
            .read()
            .iter()
            .any(|p| p.kind == AdminPanel::Overview && p.state != WinState::Minimized);
        if open && !*games_seen_open.peek() {
            games_seen_open.set(true);
            spawn_local(async move {
                // Sum of per-user gamesPlayed — the leaderboard's row count is
                // just the user count, so the aggregate is the meaningful total.
                let r = query("stats.getGlobalLeaderboard", None)
                    .await
                    .map_err(trpc_err_msg)
                    .and_then(|v| {
                        serde_json::from_value::<Vec<LeaderboardRow>>(v).map_err(|e| e.to_string())
                    })
                    .map(|rows| rows.iter().map(|x| x.games_played).sum());
                games_res.set(Some(r));
            });
        } else if !open && *games_seen_open.peek() {
            games_seen_open.set(false);
        }
    });

    if let Some(gate) = crate::store::use_auth_guard(crossword_core::auth::Role::Admin) {
        return gate;
    }

    // Mobile (< 760px, panel-kit's own threshold) is read-only: write
    // controls are not mounted at all.
    let mobile_ro = *ws.is_mobile.read();

    // ── shared user mutations (table + drawer) ────────────────────────────
    let mut set_role = move |uid: String, role: String| {
        saving_role_id.set(Some(uid.clone()));
        spawn_local(async move {
            match mutation("user.setRole", Some(json!({"userId": uid, "role": role}))).await {
                Ok(_) => state.toast(Severity::Success, "Role updated."),
                Err(e) => state.toast(Severity::Error, trpc_err_msg(e)),
            }
            refresh_users();
            saving_role_id.set(None);
        });
    };
    let mut set_vip = move |uid: String, vip: bool| {
        saving_vip_id.set(Some(uid.clone()));
        spawn_local(async move {
            match mutation(
                "user.setVipPass",
                Some(json!({"userId": uid, "vipPass": vip})),
            )
            .await
            {
                Ok(_) => state.toast(Severity::Success, "VIP status updated."),
                Err(e) => state.toast(Severity::Error, trpc_err_msg(e)),
            }
            refresh_users();
            saving_vip_id.set(None);
        });
    };

    let add_user = move |_: FormEvent| {
        let email = new_email.read().trim().to_string();
        let name = new_name.read().trim().to_string();
        let role = new_role.read().clone();
        if email.is_empty() {
            return;
        }
        saving.set(true);
        user_message.set(String::new());
        users_error.set(String::new());
        spawn_local(async move {
            let input = json!({
                "email": email,
                "name": if name.is_empty() { serde_json::Value::Null } else { json!(name) },
                "role": role,
            });
            match mutation("user.upsertFromAdmin", Some(input)).await {
                Ok(_) => {
                    user_message.set(format!("{email} is now {role}."));
                    new_email.set(String::new());
                    new_name.set(String::new());
                    new_role.set("ADMIN".to_string());
                    refresh_users();
                }
                Err(e) => users_error.set(e),
            }
            saving.set(false);
        });
    };

    // open the detail drawer for a user, resetting the password form
    let mut open_drawer = move |uid: String| {
        pw.set(String::new());
        pw_confirm.set(String::new());
        pw_error.set(String::new());
        selected_id.set(Some(uid));
    };

    // set-password submit (drawer). Validation errors stay inline; the
    // mutation outcome goes through toasts.
    let mut submit_password = move |uid: String| {
        let password = pw.read().clone();
        let confirm = pw_confirm.read().clone();
        if password.len() < 8 {
            pw_error.set("Password must be at least 8 characters.".to_string());
            return;
        }
        if password != confirm {
            pw_error.set("Passwords do not match.".to_string());
            return;
        }
        pw_error.set(String::new());
        saving_pw.set(true);
        spawn_local(async move {
            match mutation(
                "user.setPassword",
                Some(json!({"userId": uid, "password": password})),
            )
            .await
            {
                Ok(_) => {
                    state.toast(Severity::Success, "Password updated.");
                    pw.set(String::new());
                    pw_confirm.set(String::new());
                }
                Err(e) => state.toast(Severity::Error, trpc_err_msg(e)),
            }
            saving_pw.set(false);
        });
    };

    // capabilities for a role
    let capabilities_for_role = move |role: &str| -> Vec<String> {
        role_options
            .read()
            .iter()
            .find(|o| o.role == role)
            .map(|o| o.capabilities.clone())
            .unwrap_or_default()
    };

    // ── discount create submit ────────────────────────────────────────────
    let create_code = move |_: FormEvent| {
        let code = f_code.read().trim().to_uppercase();
        let name = f_name.read().trim().to_string();
        let amount_type = f_amount_type.read().clone();
        let duration = f_duration.read().clone();

        let raw_amount: f64 = match f_amount.read().parse() {
            Ok(v) => v,
            Err(_) => {
                f_amount_err.set("Enter an amount greater than 0".to_string());
                return;
            }
        };
        if !raw_amount.is_finite() || raw_amount <= 0.0 {
            f_amount_err.set("Enter an amount greater than 0".to_string());
            return;
        }
        f_amount_err.set(String::new());
        // FIXED: front-end enters dollars, server wants cents
        let amount = if amount_type == "FIXED" {
            (raw_amount * 100.0).round() as i64
        } else {
            raw_amount as i64
        };

        let duration_in_months: Option<i64> = if duration == "REPEATING" {
            f_duration_months
                .read()
                .parse::<i64>()
                .ok()
                .filter(|&n| n > 0)
        } else {
            None
        };
        let max_redemptions: Option<i64> = f_max_redemptions
            .read()
            .parse::<i64>()
            .ok()
            .filter(|&n| n > 0);
        // date input gives YYYY-MM-DD; zod wants RFC3339
        let expires_at: Option<String> = {
            let v = f_expires_at.read().trim().to_string();
            if v.is_empty() {
                None
            } else {
                Some(format!("{v}T00:00:00.000Z"))
            }
        };
        let test_mode = *f_test_mode.read();

        discount_saving.set(true);
        discount_message.set(String::new());
        discount_error.set(String::new());

        spawn_local(async move {
            let mut input = json!({
                "code": code,
                "name": name,
                "amountType": amount_type,
                "amount": amount,
                "duration": duration,
                "testMode": test_mode,
            });
            if let Some(dm) = duration_in_months {
                input["durationInMonths"] = json!(dm);
            }
            if let Some(mr) = max_redemptions {
                input["maxRedemptions"] = json!(mr);
            }
            if let Some(ea) = expires_at {
                input["expiresAt"] = json!(ea);
            }

            match mutation("discount.create", Some(input)).await {
                Ok(_) => {
                    discount_message.set(format!("Created code {code}."));
                    // reset form
                    f_code.set(String::new());
                    f_name.set(String::new());
                    f_amount_type.set("PERCENT".to_string());
                    f_amount.set(String::new());
                    f_duration.set("ONCE".to_string());
                    f_duration_months.set(String::new());
                    f_max_redemptions.set(String::new());
                    f_expires_at.set(String::new());
                    f_test_mode.set(false);
                    refresh_discounts();
                }
                Err(e) => discount_error.set(trpc_err_msg(e)),
            }
            discount_saving.set(false);
        });
    };

    // ── panel bodies ──────────────────────────────────────────────────────

    let body = move |kind: AdminPanel, _max: bool| -> Element {
        match kind {
            AdminPanel::Overview => {
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

                let jobs_list = jobs.read();
                let jobs_t: TileState = if !jobs_error.read().is_empty() {
                    Some(Err(jobs_error.read().clone()))
                } else if *jobs_loading.read() && jobs_list.is_empty() {
                    None
                } else {
                    let now = js_sys::Date::now();
                    let (mut ok, mut fail) = (0u32, 0u32);
                    for j in jobs_list.iter() {
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
                };

                let discounts_list = discounts.read();
                let discounts_t: TileState = if !discount_error.read().is_empty() {
                    Some(Err(discount_error.read().clone()))
                } else if *discounts_loading.read() && discounts_list.is_empty() {
                    None
                } else {
                    let active = discounts_list.iter().filter(|d| d.is_active).count();
                    let redeemed: i64 = discounts_list.iter().map(|d| d.times_redeemed).sum();
                    Some(Ok((
                        active.to_string(),
                        Some(format!("{redeemed} redemptions")),
                    )))
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
                for j in jobs_list.iter().take(5) {
                    let ms = js_sys::Date::parse(&j.created_at);
                    items.push(FeedItem {
                        ts: if ms.is_nan() { 0.0 } else { ms },
                        kind: FeedKind::Job {
                            status: j.status.clone(),
                            topic: j.topic.clone(),
                        },
                    });
                }
                if let Some(Ok(users)) = &*users_read {
                    let mut with_ts: Vec<(f64, &AdminUser)> = users
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

                let feed_loading =
                    users_read.is_none() && *jobs_loading.read() && jobs_list.is_empty();
                let jobs_err = {
                    let e = jobs_error.read();
                    (!e.is_empty()).then(|| e.clone())
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
                                            let status = status.clone();
                                            let topic = topic.clone();
                                            rsx! {
                                                button {
                                                    key: "{i}",
                                                    class: "row",
                                                    style: "width:100%;gap:0.75rem;align-items:center;padding:0.625rem 1rem;border:none;border-bottom:1px solid var(--border-app);background:transparent;color:inherit;text-align:left;cursor:pointer;font-size:0.875rem",
                                                    aria_label: "Open jobs: {topic}",
                                                    onclick: move |_| ws.restore(AdminPanel::Jobs),
                                                    {status_badge(status.clone(), job_status_accent(&status))}
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
                                                {tag_badge("user", "New user".to_string(), None)}
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
                    }
                }
            }

            AdminPanel::Parameters => {
                let status = gen_status.read().clone();
                let is_running = status == "running";
                let params_form = rsx! {
                    // topic + submit row
                    div { class: "row", style: "flex-wrap:wrap;align-items:flex-end;gap:0.75rem",
                        div { class: "col", style: "gap:0.375rem;flex:1;min-width:280px",
                            label {
                                r#for: "topic",
                                class: "muted",
                                style: "font-size:0.75rem;font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                "Topic"
                            }
                            input {
                                id: "topic",
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem;width:100%",
                                r#type: "text",
                                value: "{form.read().topic}",
                                oninput: move |e| form.write().topic = e.value(),
                            }
                        }
                        button {
                            class: "app-btn app-btn-active",
                            style: "height:38px;min-width:120px;font-weight:bold",
                            disabled: is_running,
                            onclick: move |_| {
                                // drop previous subscription
                                sub_handle.set(None);
                                gen_log.set(vec![]);
                                gen_progress.set(None);
                                gen_status.set("running".to_string());
                                gen_error.set(String::new());
                                gen_game_id.set(None);
                                gen_game_title.set(None);
                                gen_game_published.set(false);
                                elapsed_secs.set(0);
                                publish_error.set(String::new());

                                // tick elapsed every second until status != running
                                spawn_local(async move {
                                    loop {
                                        gloo_timers::future::TimeoutFuture::new(1_000).await;
                                        if gen_status.read().as_str() != "running" {
                                            break;
                                        }
                                        let cur = *elapsed_secs.read();
                                        elapsed_secs.set(cur + 1);
                                    }
                                });

                                let input = form_to_json(&form.read());
                                let handle = subscribe(
                                    "generator.runGeneration",
                                    Some(input),
                                    move |data: Value| {
                                        let etype = data["type"].as_str().unwrap_or("").to_string();
                                        if etype == "progress" {
                                            let stage = data["stage"].as_str().unwrap_or("").to_string();
                                            let current = data["current"].as_i64().unwrap_or(0);
                                            let total = data["total"].as_i64().unwrap_or(0);
                                            let message = data["message"].as_str().map(|s| s.to_string());
                                            gen_progress.set(Some(Progress { stage, current, total, message }));
                                            return;
                                        }

                                        gen_log.write().push(data.clone());

                                        match etype.as_str() {
                                            "completed" => {
                                                gen_status.set("succeeded".to_string());
                                                gen_progress.set(None);
                                                if let Some(gid) = data["gameId"].as_str() {
                                                    gen_game_id.set(Some(gid.to_string()));
                                                }
                                                if let Some(t) = data["title"].as_str() {
                                                    gen_game_title.set(Some(t.to_string()));
                                                }
                                                do_load_jobs(jobs, jobs_loading, jobs_error, *jobs_take.peek());
                                            }
                                            "failed" => {
                                                gen_status.set("failed".to_string());
                                                let err = data["error"].as_str().unwrap_or("unknown error").to_string();
                                                gen_error.set(err);
                                            }
                                            _ => {}
                                        }
                                    },
                                );
                                sub_handle.set(Some(handle));
                            },
                            if is_running { "Generating…" } else { "Generate" }
                        }
                    }

                    // preset chips — client-side sugar over the grid dimensions
                    div { class: "row", style: "gap:0.5rem;flex-wrap:wrap",
                        for (name, w, h, min_len, max_len) in [
                            ("Mini 5×5", 5i64, 5i64, 3i64, 5i64),
                            ("Daily 15×15", 15, 15, 3, 12),
                            ("Sunday 21×21", 21, 21, 3, 12),
                        ] {
                            button {
                                class: "app-btn",
                                style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                onclick: move |_| {
                                    let mut f = form.write();
                                    f.width = w;
                                    f.height = h;
                                    f.min_word_length = min_len;
                                    f.max_word_length = max_len;
                                },
                                {name}
                            }
                        }
                    }

                    // numeric params grid
                    div { style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(90px,1fr));gap:0.75rem",
                        for (label, value, min_val, max_val, setter) in [
                            ("Width", form.read().width, 3i64, 50i64, 0usize),
                            ("Height", form.read().height, 3, 50, 1),
                            ("Min Len", form.read().min_word_length, 2, 50, 2),
                            ("Max Len", form.read().max_word_length, 2, 50, 3),
                            ("Answers", form.read().target_words, 1, 250, 4),
                            ("Runs", form.read().runs, 1, 100, 5),
                            ("Attempts", form.read().max_attempts, 1, 1000, 6),
                        ] {
                            label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                                {label}
                                input {
                                    class: "app-input",
                                    style: "padding:0.375rem 0.5rem;font-size:0.875rem",
                                    r#type: "number",
                                    min: "{min_val}",
                                    max: "{max_val}",
                                    value: "{value}",
                                    oninput: move |e| {
                                        if let Ok(n) = e.value().parse::<i64>() {
                                            let mut f = form.write();
                                            match setter {
                                                0 => f.width = n,
                                                1 => f.height = n,
                                                2 => f.min_word_length = n,
                                                3 => f.max_word_length = n,
                                                4 => f.target_words = n,
                                                5 => f.runs = n,
                                                6 => f.max_attempts = n,
                                                _ => {}
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    }
                };

                rsx! {
                    div { class: "col", style: "gap:1.5rem;padding:1rem;overflow-y:auto;height:100%",
                        div { style: "border-bottom:1px solid var(--border-app);padding-bottom:1rem",
                            h1 { style: "font-size:1.125rem;font-weight:bold;letter-spacing:0.05em",
                                "CROSSWORD GENERATOR"
                            }
                        }
                        // Mobile: collapse the form behind a disclosure. Gated by
                        // rsx (not CSS) so only one branch mounts the controls.
                        if mobile_ro {
                            details { style: "border:1px solid var(--border-app)",
                                summary { class: "muted", style: "cursor:pointer;padding:0.75rem 1rem;font-size:0.75rem;font-weight:600;font-family:monospace;text-transform:uppercase;letter-spacing:0.05em",
                                    "New generation (desktop recommended)"
                                }
                                div { class: "col", style: "gap:1.5rem;padding:1rem;border-top:1px solid var(--border-app)",
                                    {params_form}
                                }
                            }
                        } else {
                            {params_form}
                        }
                    }
                }
            }

            AdminPanel::Run => {
                let status = gen_status.read().clone();
                let is_running = status == "running";
                // Idle summary of the most recent job from the already-fetched list.
                let last_run = jobs.read().first().map(|j| {
                    let ms = js_sys::Date::parse(&j.created_at);
                    let when = if ms.is_nan() {
                        format_datetime(&j.created_at)
                    } else {
                        rel_time(js_sys::Date::now(), ms)
                    };
                    format!("Last: {} · \"{}\" · {}", j.status, j.topic, when)
                });
                rsx! {
                    div { class: "col", style: "gap:1rem;padding:1rem;height:100%;overflow-y:auto",
                        // ── live progress ─────────────────────────────────────────
                        if status != "idle" {
                            GenerationProgress {
                                log: gen_log.read().clone(),
                                progress: gen_progress.read().clone(),
                                running: is_running,
                                status: status.clone(),
                                elapsed_secs: *elapsed_secs.read(),
                            }
                        } else if let Some(line) = last_run {
                            div { class: "col muted", style: "gap:0.5rem;text-align:center;padding:2rem 0",
                                div { style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                    {line}
                                }
                                div { style: "font-size:0.875rem", "Generation output will appear here." }
                            }
                        } else {
                            div { class: "muted", style: "font-size:0.875rem;text-align:center;padding:2rem 0",
                                "Generation output will appear here."
                            }
                        }

                        // ── gen error ─────────────────────────────────────────────
                        if !gen_error.read().is_empty() {
                            div { class: "app-card error", style: "padding:1rem;font-size:0.875rem",
                                {gen_error.read().clone()}
                            }
                        }

                        // ── completed game CTA ────────────────────────────────────
                        if let (Some(gid), Some(gtitle)) = (gen_game_id.read().clone(), gen_game_title.read().clone()) {
                            div { class: "app-card", style: "padding:1rem;border-color:var(--color-success)",
                                div { class: "row", style: "justify-content:space-between;align-items:center;gap:0.75rem;flex-wrap:wrap",
                                    div { class: "col", style: "gap:0.25rem",
                                        div { style: "font-size:0.875rem;font-weight:600", {gtitle.clone()} }
                                        if !publish_error.read().is_empty() {
                                            div { class: "error", style: "font-size:0.75rem", {publish_error.read().clone()} }
                                        }
                                    }
                                    div { class: "row", style: "gap:0.5rem",
                                        // Publish is desktop-only; mobile stays read-only.
                                        if !mobile_ro && !*gen_game_published.read() {
                                            button {
                                                class: "app-btn app-btn-active",
                                                style: "font-weight:bold",
                                                disabled: *publishing.read(),
                                                onclick: {
                                                    let gid = gid.clone();
                                                    move |_| {
                                                        let game_id = gid.clone();
                                                        publishing.set(true);
                                                        publish_error.set(String::new());
                                                        spawn_local(async move {
                                                            match mutation("generator.publishGeneratedGame", Some(json!({"gameId": game_id}))).await {
                                                                Ok(_) => {
                                                                    gen_game_published.set(true);
                                                                    do_load_jobs(jobs, jobs_loading, jobs_error, *jobs_take.peek());
                                                                }
                                                                Err(e) => publish_error.set(e),
                                                            }
                                                            publishing.set(false);
                                                        });
                                                    }
                                                },
                                                if *publishing.read() { "Publishing…" } else { "Publish" }
                                            }
                                        }
                                        button {
                                            class: "app-btn",
                                            onclick: {
                                                let gid = gid.clone();
                                                move |_| {
                                                    // Copy the game URL via the JS clipboard API (no extra Rust deps).
                                                    let origin = web_sys::window()
                                                        .and_then(|w| w.location().origin().ok())
                                                        .unwrap_or_default();
                                                    let url = format!("{origin}/game/{gid}/new");
                                                    let script = format!(
                                                        "navigator.clipboard && navigator.clipboard.writeText({})",
                                                        serde_json::to_string(&url).unwrap_or_default()
                                                    );
                                                    dioxus::document::eval(&script);
                                                    state.toast(crate::store::Severity::Success, "Link copied");
                                                }
                                            },
                                            "Copy game link"
                                        }
                                        button {
                                            class: "app-btn",
                                            onclick: move |_| { nav.push(Route::Games {}); },
                                            "View Games"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            AdminPanel::Jobs => {
                let search = job_search.read().to_lowercase();
                let filter = status_filter.read().clone();
                let filtered: Vec<JobRow> = jobs
                    .read()
                    .iter()
                    .filter(|j| {
                        (search.is_empty() || j.topic.to_lowercase().contains(&search))
                            && (filter.is_empty() || j.status == filter)
                    })
                    .cloned()
                    .collect();
                let shown = filtered.len();
                let any_loaded = !jobs.read().is_empty();
                // Server returned a full page — there may be more to fetch.
                let can_load_more = jobs.read().len() as i64 >= *jobs_take.read();
                rsx! {
                    div { style: "overflow:hidden;height:100%;display:flex;flex-direction:column",
                        div { class: "row", style: "padding:1rem;border-bottom:1px solid var(--border-app);justify-content:space-between;align-items:center",
                            h2 { style: "font-size:0.875rem;font-weight:bold;font-family:monospace;letter-spacing:0.05em",
                                "GENERATION JOBS"
                            }
                            button {
                                class: "app-btn",
                                style: "font-size:0.75rem;font-family:monospace;text-transform:uppercase",
                                disabled: *jobs_loading.read(),
                                onclick: move |_| do_load_jobs(jobs, jobs_loading, jobs_error, *jobs_take.peek()),
                                if *jobs_loading.read() { "Refreshing" } else { "Refresh" }
                            }
                        }

                        if !jobs_error.read().is_empty() {
                            div { class: "error", style: "padding:0.75rem 1rem;font-size:0.875rem;border-bottom:1px solid var(--border-app)",
                                {jobs_error.read().clone()}
                            }
                        }

                        // search + status filters (client-side, over the fetched page)
                        div { class: "row", style: "padding:0.75rem 1rem;border-bottom:1px solid var(--border-app);gap:0.5rem;align-items:center;flex-wrap:wrap",
                            input {
                                class: "app-input",
                                style: "padding:0.375rem 0.5rem;font-size:0.875rem;flex:1;min-width:160px",
                                r#type: "text",
                                placeholder: "Search topics…",
                                value: "{job_search}",
                                oninput: move |e| job_search.set(e.value()),
                            }
                            span {
                                class: "muted",
                                style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:bold;text-transform:uppercase;letter-spacing:0.05em;padding:0.125rem 0.5rem;border:1px solid var(--border-app);white-space:nowrap",
                                {format!("{shown} shown")}
                            }
                            for (chip_label, chip_val) in [
                                ("All", ""),
                                ("Succeeded", "SUCCEEDED"),
                                ("Failed", "FAILED"),
                                ("Running", "RUNNING"),
                            ] {
                                button {
                                    class: if *status_filter.read() == chip_val { "app-btn app-btn-active" } else { "app-btn" },
                                    style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                    onclick: move |_| status_filter.set(chip_val.to_string()),
                                    {chip_label}
                                }
                            }
                        }

                        div { style: "overflow-x:auto;flex:1",
                            table { style: "width:100%;text-align:left;font-size:0.875rem;border-collapse:collapse",
                                {table_head(vec!["Status", "Topic", "Grid", "Game", "Created"])}
                                tbody {
                                    for job in filtered.iter() {
                                        tr { style: "border-bottom:1px solid var(--border-app);font-family:monospace;font-size:0.75rem",
                                            td { style: "padding:0.75rem 1rem",
                                                {status_badge(job.status.clone(), job_status_accent(&job.status))}
                                            }
                                            td { style: "padding:0.75rem 1rem;font-family:sans-serif;font-size:0.875rem;font-weight:500",
                                                {job.topic.clone()}
                                            }
                                            td { class: "muted", style: "padding:0.75rem 1rem",
                                                {format!("{}x{}", job.width, job.height)}
                                            }
                                            td { style: "padding:0.75rem 1rem",
                                                if let Some(rg) = &job.result_game {
                                                    div { class: "col", style: "gap:0.125rem",
                                                        span { style: "font-family:sans-serif;font-size:0.875rem;font-weight:500", {rg.title.clone()} }
                                                        span { class: "muted", style: "font-size:0.625rem;font-weight:bold;text-transform:uppercase",
                                                            if rg.published { "published" } else { "draft" }
                                                        }
                                                    }
                                                } else {
                                                    span { class: "muted", "—" }
                                                }
                                            }
                                            td { class: "muted", style: "padding:0.75rem 1rem",
                                                {format_datetime(&job.created_at)}
                                            }
                                        }
                                    }
                                    if *jobs_loading.read() && jobs.read().is_empty() {
                                        {table_status_row("5", "Loading generation jobs…".to_string())}
                                    } else if filtered.is_empty() {
                                        {table_status_row(
                                            "5",
                                            if any_loaded { "No jobs match the current filters." } else { "No generation jobs found." }
                                                .to_string(),
                                        )}
                                    }
                                }
                            }
                        }

                        if can_load_more {
                            div { style: "padding:0.75rem 1rem;border-top:1px solid var(--border-app)",
                                button {
                                    class: "app-btn",
                                    style: "width:100%;font-size:0.75rem;font-family:monospace;text-transform:uppercase",
                                    disabled: *jobs_loading.read(),
                                    onclick: move |_| {
                                        let cur = *jobs_take.peek();
                                        jobs_take.set(cur + 25);
                                    },
                                    if *jobs_loading.read() { "Loading" } else { "Load more" }
                                }
                            }
                        }
                    }
                }
            }

            AdminPanel::AddUser if mobile_ro => rsx! {
                div { class: "col", style: "gap:0.75rem;padding:1rem",
                    div { class: "muted", style: "font-size:0.875rem",
                        "Adding users requires a desktop viewport."
                    }
                    // fetch errors still surface on mobile (read-only display)
                    if !users_error.read().is_empty() {
                        div { class: "app-card error", style: "padding:0.75rem;font-size:0.875rem",
                            {users_error.read().clone()}
                        }
                    }
                }
            },
            AdminPanel::AddUser => rsx! {
                div { class: "col", style: "gap:1rem;padding:1rem;overflow-y:auto",
                    // ── add user form ──────────────────────────────────────────
                    form {
                        style: "display:grid;gap:0.75rem;grid-template-columns:repeat(auto-fill,minmax(160px,1fr));align-items:end",
                        onsubmit: add_user,
                        label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                            "Email"
                            input {
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                                r#type: "email",
                                required: true,
                                value: "{new_email}",
                                oninput: move |e| new_email.set(e.value()),
                            }
                        }
                        label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                            "Name"
                            input {
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                                r#type: "text",
                                value: "{new_name}",
                                oninput: move |e| new_name.set(e.value()),
                            }
                        }
                        label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                            "Role"
                            select {
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem",
                                value: "{new_role}",
                                oninput: move |e| new_role.set(e.value()),
                                for opt in role_options.read().iter() {
                                    option { value: "{opt.role}", {opt.role.clone()} }
                                }
                            }
                        }
                        button {
                            class: "app-btn app-btn-active",
                            style: "height:38px;font-weight:bold",
                            r#type: "submit",
                            disabled: *saving.read(),
                            if *saving.read() { "Saving…" } else { "Add User" }
                        }
                    }

                    // ── feedback ───────────────────────────────────────────────
                    if !user_message.read().is_empty() {
                        div { class: "app-card success", style: "padding:0.75rem;font-size:0.875rem",
                            {user_message.read().clone()}
                        }
                    }
                    if !users_error.read().is_empty() {
                        div { class: "app-card error", style: "padding:0.75rem;font-size:0.875rem",
                            {users_error.read().clone()}
                        }
                    }
                }
            },

            AdminPanel::Users => {
                // client-side search + filters over the fetched list
                let users_read = users_res.read();
                let loading = users_read.is_none();
                let fetch_err = match &*users_read {
                    Some(Err(e)) => Some(e.clone()),
                    _ => None,
                };
                let all: Vec<AdminUser> = match &*users_read {
                    Some(Ok(list)) => list.clone(),
                    _ => Vec::new(),
                };
                let q = search.read().trim().to_lowercase();
                let role_f = *filter_role.read();
                let verified_f = *filter_verified.read();
                let vip_only = *filter_vip_only.read();
                let total = all.len();
                let filtered: Vec<AdminUser> = all
                    .iter()
                    .filter(|u| {
                        let text_match = q.is_empty()
                            || [&u.name, &u.email, &u.username].iter().any(|f| {
                                f.as_deref()
                                    .map(|s| s.to_lowercase().contains(&q))
                                    .unwrap_or(false)
                            });
                        let role_match = role_f.map(|r| u.role == r).unwrap_or(true);
                        let verified_match = verified_f
                            .map(|v| u.email_verified.is_some() == v)
                            .unwrap_or(true);
                        let vip_match = !vip_only || u.vip_pass;
                        text_match && role_match && verified_match && vip_match
                    })
                    .cloned()
                    .collect();
                let shown = filtered.len();

                rsx! {
                    div { class: "col", style: "gap:0.75rem;height:100%",
                        if let Some(e) = fetch_err {
                            div { class: "app-card error", style: "padding:0.75rem;font-size:0.875rem",
                                {e}
                            }
                        }
                        // ── search + result count ──────────────────────────────
                        div { class: "row", style: "gap:0.75rem;align-items:center;flex-wrap:wrap",
                            input {
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem;flex:1 1 200px;min-width:200px",
                                r#type: "search",
                                placeholder: "Search name, email, username…",
                                aria_label: "Search users",
                                value: "{search}",
                                oninput: move |e| search.set(e.value()),
                            }
                            span {
                                aria_live: "polite",
                                style: "border:1px solid var(--border-app);padding:0.25rem 0.625rem;font-family:monospace;font-size:0.6875rem;text-transform:uppercase;letter-spacing:0.05em;color:var(--text-secondary);white-space:nowrap",
                                "{shown} / {total} users"
                            }
                        }
                        // ── filter chips ───────────────────────────────────────
                        div { class: "row", style: "gap:0.75rem;flex-wrap:wrap",
                            div { class: "row", style: "gap:0.25rem", role: "group", aria_label: "Filter by role",
                                for (label, val) in [("All", None), ("Admin", Some("ADMIN")), ("User", Some("USER"))] {
                                    button {
                                        class: if role_f == val { "app-btn app-btn-active" } else { "app-btn" },
                                        style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                        aria_pressed: role_f == val,
                                        onclick: move |_| filter_role.set(val),
                                        {label}
                                    }
                                }
                            }
                            div { class: "row", style: "gap:0.25rem", role: "group", aria_label: "Filter by verification",
                                for (label, val) in [("All", None), ("Verified", Some(true)), ("Pending", Some(false))] {
                                    button {
                                        class: if verified_f == val { "app-btn app-btn-active" } else { "app-btn" },
                                        style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                        aria_pressed: verified_f == val,
                                        onclick: move |_| filter_verified.set(val),
                                        {label}
                                    }
                                }
                            }
                            div { class: "row", style: "gap:0.25rem", role: "group", aria_label: "Filter by VIP",
                                for (label, val) in [("All", false), ("VIP", true)] {
                                    button {
                                        class: if vip_only == val { "app-btn app-btn-active" } else { "app-btn" },
                                        style: "font-family:var(--mono);font-size:var(--fs-2xs);font-weight:600;text-transform:uppercase;letter-spacing:0.05em",
                                        aria_pressed: vip_only == val,
                                        onclick: move |_| filter_vip_only.set(val),
                                        {label}
                                    }
                                }
                            }
                        }
                        // ── users table ────────────────────────────────────────
                        div { style: "overflow-x:auto;flex:1;min-height:0",
                            table { style: "width:100%;text-align:left;font-size:0.875rem;border-collapse:collapse",
                                {table_head(vec!["User", "Username", "Verified", "Joined", "Role", "VIP Pass", "Capabilities"])}
                                tbody {
                                    for user in filtered.iter() {
                                        {
                                            let uid = user.id.clone();
                                            let user_role = user.role.clone();
                                            let vip = user.vip_pass;
                                            let caps = capabilities_for_role(&user.role);
                                            let verified = user.email_verified.is_some();
                                            let display_name = user.name.clone()
                                                .or_else(|| user.email.clone())
                                                .unwrap_or_else(|| "Unnamed user".to_string());
                                            let email_text = user.email.clone().unwrap_or_else(|| "—".to_string());
                                            let username_text = user.username.clone().unwrap_or_else(|| "—".to_string());
                                            let joined_text = user.created_at.as_deref().map(format_date).unwrap_or_else(|| "—".to_string());

                                            let uid_role = uid.clone();
                                            let uid_vip = uid.clone();
                                            let uid_open = uid.clone();
                                            let uid_key = uid.clone();

                                            rsx! {
                                                tr {
                                                    key: "{uid}",
                                                    style: "border-bottom:1px solid var(--border-app);cursor:pointer",
                                                    tabindex: "0",
                                                    aria_label: "View details for {display_name}",
                                                    onclick: move |_| open_drawer(uid_open.clone()),
                                                    onkeydown: move |e| {
                                                        // Enter / Space open the drawer, matching a native button.
                                                        let k = e.key();
                                                        if k == Key::Enter || k == Key::Character(" ".into()) {
                                                            e.prevent_default();
                                                            open_drawer(uid_key.clone());
                                                        }
                                                    },
                                                    td { style: "padding:0.75rem 1rem",
                                                        div { style: "font-weight:500", "{display_name}" }
                                                        div { class: "muted", style: "font-size:0.75rem", {email_text} }
                                                    }
                                                    td { class: "muted", style: "padding:0.75rem 1rem", {username_text} }
                                                    td { style: "padding:0.75rem 1rem", {verified_badge(verified)} }
                                                    td { class: "muted", style: "padding:0.75rem 1rem;white-space:nowrap", {joined_text} }
                                                    // inline controls own their cells — keep clicks and
                                                    // keystrokes from bubbling into the row's drawer-open.
                                                    // Mobile is read-only: badges, no editors mounted.
                                                    if mobile_ro {
                                                        td { style: "padding:0.75rem 1rem",
                                                            {tag_badge("role", user_role.clone(), Some(role_accent(&user_role)))}
                                                        }
                                                        td { class: "muted", style: "padding:0.75rem 1rem",
                                                            if vip { "VIP" } else { "—" }
                                                        }
                                                    } else {
                                                        td {
                                                            style: "padding:0.75rem 1rem",
                                                            onclick: move |e| e.stop_propagation(),
                                                            onkeydown: move |e| e.stop_propagation(),
                                                            {
                                                                let saving_id = saving_role_id.read().clone();
                                                                let is_saving = saving_id.as_deref() == Some(&uid_role);
                                                                let uid2 = uid_role.clone();
                                                                rsx! {
                                                                    select {
                                                                        class: "app-input",
                                                                        style: "padding:0.375rem 0.5rem;font-size:0.75rem",
                                                                        disabled: is_saving,
                                                                        value: "{user_role}",
                                                                        oninput: move |e| set_role(uid2.clone(), e.value()),
                                                                        for opt in role_options.read().iter() {
                                                                            option { value: "{opt.role}", {opt.role.clone()} }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                        td {
                                                            style: "padding:0.75rem 1rem",
                                                            onclick: move |e| e.stop_propagation(),
                                                            onkeydown: move |e| e.stop_propagation(),
                                                            {
                                                                let saving_id = saving_vip_id.read().clone();
                                                                let is_saving = saving_id.as_deref() == Some(&uid_vip);
                                                                let uid3 = uid_vip.clone();
                                                                rsx! {
                                                                    input {
                                                                        r#type: "checkbox",
                                                                        style: "width:1rem;height:1rem;cursor:pointer",
                                                                        checked: vip,
                                                                        disabled: is_saving,
                                                                        oninput: move |e| set_vip(uid3.clone(), e.value() == "true"),
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    td { style: "padding:0.75rem 1rem",
                                                        div { class: "row", style: "gap:0.25rem;flex-wrap:wrap",
                                                            for cap in caps.iter() {
                                                                span {
                                                                    style: "border:1px solid var(--border-app);padding:0.125rem 0.5rem;font-size:0.625rem;color:var(--text-secondary)",
                                                                    {cap.clone()}
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if filtered.is_empty() && !loading {
                                        {table_status_row(
                                            "7",
                                            if total == 0 { "No users found." } else { "No users match the current filters." }
                                                .to_string(),
                                        )}
                                    }
                                    if loading {
                                        {table_status_row("7", "Loading users…".to_string())}
                                    }
                                }
                            }
                        }
                    }
                }
            }

            AdminPanel::Create if mobile_ro => rsx! {
                div { class: "muted", style: "padding:1rem;font-size:0.875rem",
                    "Creating discount codes requires a desktop viewport."
                }
            },
            AdminPanel::Create => rsx! {
                form {
                    style: "display:grid;gap:0.75rem;grid-template-columns:repeat(auto-fill,minmax(160px,1fr));align-items:end;overflow-y:auto;padding:0.5rem",
                    onsubmit: create_code,

                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Code"
                        input {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;font-family:monospace;text-transform:uppercase",
                            r#type: "text",
                            placeholder: "LAUNCH50",
                            minlength: "3",
                            maxlength: "256",
                            required: true,
                            value: "{f_code}",
                            oninput: move |e| f_code.set(e.value()),
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Name"
                        input {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                            r#type: "text",
                            placeholder: "Launch promo",
                            minlength: "2",
                            maxlength: "120",
                            required: true,
                            value: "{f_name}",
                            oninput: move |e| f_name.set(e.value()),
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Amount type"
                        select {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem",
                            value: "{f_amount_type}",
                            oninput: move |e| f_amount_type.set(e.value()),
                            option { value: "PERCENT", "Percent" }
                            option { value: "FIXED", "Fixed" }
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Amount"
                        div { class: "row", style: "gap:0.25rem;align-items:center",
                            input {
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem;flex:1",
                                r#type: "number",
                                min: "1",
                                step: "1",
                                required: true,
                                value: "{f_amount}",
                                oninput: move |e| {
                                    f_amount.set(e.value());
                                    f_amount_err.set(String::new());
                                },
                            }
                            span { class: "muted", style: "font-size:0.75rem;white-space:nowrap",
                                if f_amount_type.read().as_str() == "PERCENT" { "%" } else { "USD" }
                            }
                        }
                        if !f_amount_err.read().is_empty() {
                            span { class: "error", style: "font-size:var(--fs-2xs);text-transform:none",
                                {f_amount_err.read().clone()}
                            }
                        }
                        if f_amount_type.read().as_str() == "FIXED" {
                            span { class: "muted", style: "font-size:0.625rem;text-transform:none", "Enter dollars (e.g. 10 = $10.00)" }
                        } else {
                            span { class: "muted", style: "font-size:0.625rem;text-transform:none", "1–100" }
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Duration"
                        select {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem",
                            value: "{f_duration}",
                            oninput: move |e| f_duration.set(e.value()),
                            option { value: "ONCE", "Once (first payment only)" }
                            option { value: "FOREVER", "Forever" }
                            option { value: "REPEATING", "Repeating" }
                        }
                    }
                    if f_duration.read().as_str() == "REPEATING" {
                        label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                            "Duration in months"
                            input {
                                class: "app-input",
                                style: "padding:0.5rem 0.75rem;font-size:0.875rem",
                                r#type: "number",
                                min: "1",
                                step: "1",
                                placeholder: "e.g. 3",
                                value: "{f_duration_months}",
                                oninput: move |e| f_duration_months.set(e.value()),
                            }
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Max redemptions"
                        input {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                            r#type: "number",
                            min: "1",
                            step: "1",
                            placeholder: "Unlimited",
                            value: "{f_max_redemptions}",
                            oninput: move |e| f_max_redemptions.set(e.value()),
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Expires at"
                        input {
                            class: "app-input",
                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                            r#type: "date",
                            value: "{f_expires_at}",
                            oninput: move |e| f_expires_at.set(e.value()),
                        }
                    }
                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                        "Test mode"
                        div { class: "row", style: "gap:0.5rem;align-items:center;padding:0.5rem 0",
                            input {
                                r#type: "checkbox",
                                style: "width:1rem;height:1rem;cursor:pointer",
                                checked: *f_test_mode.read(),
                                oninput: move |e| f_test_mode.set(e.value() == "true"),
                            }
                            span { class: "muted", style: "font-size:0.625rem;text-transform:none",
                                "Test-mode codes only work on test-mode checkouts."
                            }
                        }
                    }
                    div { style: "display:flex;align-items:flex-end",
                        button {
                            class: "app-btn app-btn-active",
                            style: "height:38px;font-weight:bold",
                            r#type: "submit",
                            disabled: *discount_saving.read(),
                            if *discount_saving.read() { "Saving…" } else { "Create code" }
                        }
                    }
                }
            },

            AdminPanel::Discounts => rsx! {
                div { class: "col", style: "gap:0.75rem;height:100%;overflow:hidden",
                    if !discount_message.read().is_empty() {
                        div { class: "app-card success", style: "padding:0.75rem;font-size:0.875rem",
                            {discount_message.read().clone()}
                        }
                    }
                    if !discount_error.read().is_empty() {
                        div { class: "app-card error", style: "padding:0.75rem;font-size:0.875rem",
                            {discount_error.read().clone()}
                        }
                    }
                    div { style: "overflow-x:auto;flex:1",
                        table { style: "width:100%;text-align:left;font-size:0.875rem;border-collapse:collapse",
                            {table_head(
                                if mobile_ro {
                                    vec!["Code", "Name", "Amount", "Duration", "Redemptions", "Expires", "Test", "Status"]
                                } else {
                                    vec!["Code", "Name", "Amount", "Duration", "Redemptions", "Expires", "Test", "Status", "Actions"]
                                },
                            )}
                            tbody {
                                for discount in discounts.read().iter() {
                                    {
                                        let did = discount.id.clone();
                                        let dcode = discount.code.clone();
                                        let is_active = discount.is_active;
                                        let amount_str = format_amount(discount);
                                        let duration_str = format_duration(discount);
                                        let expiry_str = format_expiry(&discount.expires_at);
                                        let test_mode = discount.test_mode;
                                        let times = discount.times_redeemed;
                                        let max_red = discount.max_redemptions;
                                        let dname = discount.name.clone();

                                        let did_active = did.clone();
                                        let d_for_delete = discount.clone();
                                        let dcode_msg = dcode.clone();

                                        rsx! {
                                            tr { style: "border-bottom:1px solid var(--border-app)",
                                                td { style: "padding:0.75rem 1rem;font-family:monospace;font-weight:bold",
                                                    {dcode.clone()}
                                                }
                                                td { class: "muted", style: "padding:0.75rem 1rem", {dname} }
                                                td { class: "muted", style: "padding:0.75rem 1rem", {amount_str} }
                                                td { class: "muted", style: "padding:0.75rem 1rem", {duration_str} }
                                                td { class: "muted", style: "padding:0.75rem 1rem",
                                                    {
                                                        match max_red {
                                                            Some(max) => {
                                                                let pct = if max > 0 {
                                                                    ((times as f64 / max as f64) * 100.0).clamp(0.0, 100.0)
                                                                } else {
                                                                    0.0
                                                                };
                                                                rsx! {
                                                                    div { class: "col", style: "gap:0.25rem;min-width:80px",
                                                                        span { {format!("{times} / {max}")} }
                                                                        div { style: "height:3px;width:100%;background:var(--bg-cell-empty)",
                                                                            div { style: format!("height:100%;width:{pct:.0}%;background:var(--pastel-yellow)") }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            None => rsx! {
                                                                span { {format!("{times} / ∞")} }
                                                            },
                                                        }
                                                    }
                                                }
                                                td { class: "muted", style: "padding:0.75rem 1rem", {expiry_str} }
                                                td { style: "padding:0.75rem 1rem",
                                                    if test_mode {
                                                        {tag_badge("mode", "Test".to_string(), Some("var(--text-secondary)"))}
                                                    } else {
                                                        span { class: "muted", "—" }
                                                    }
                                                }
                                                td { style: "padding:0.75rem 1rem",
                                                    {status_badge(
                                                        if is_active { "Active" } else { "Inactive" }.to_string(),
                                                        if is_active { "var(--color-success)" } else { "var(--color-warning)" },
                                                    )}
                                                }
                                                if !mobile_ro {
                                                    td { style: "padding:0.75rem 1rem",
                                                        {
                                                            let is_busy = saving_ids.read().contains(&did);
                                                            rsx! {
                                                                div { class: "row", style: "gap:0.5rem",
                                                                    button {
                                                                        class: "app-btn",
                                                                        style: "font-size:0.75rem;padding:0.25rem 0.5rem",
                                                                        disabled: is_busy,
                                                                        onclick: move |_| {
                                                                            let id = did_active.clone();
                                                                            let next_active = !is_active;
                                                                            let code = dcode_msg.clone();
                                                                            saving_ids.write().push(id.clone());
                                                                            discount_message.set(String::new());
                                                                            discount_error.set(String::new());
                                                                            spawn_local(async move {
                                                                                match mutation("discount.setActive", Some(json!({"id": id, "isActive": next_active}))).await {
                                                                                    Ok(_) => {
                                                                                        let state = if next_active { "active" } else { "inactive" };
                                                                                        discount_message.set(format!("{code} is now {state}."));
                                                                                        refresh_discounts();
                                                                                    }
                                                                                    Err(e) => {
                                                                                        discount_error.set(trpc_err_msg(e));
                                                                                        refresh_discounts();
                                                                                    }
                                                                                }
                                                                                saving_ids.write().retain(|x| x != &id);
                                                                            });
                                                                        },
                                                                        if is_active { "Deactivate" } else { "Activate" }
                                                                    }
                                                                    button {
                                                                        class: "app-btn error",
                                                                        style: "font-size:0.75rem;padding:0.25rem 0.5rem",
                                                                        disabled: is_busy,
                                                                        onclick: move |_| pending_delete.set(Some(d_for_delete.clone())),
                                                                        "Delete"
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
                                if discounts.read().is_empty() && !*discounts_loading.read() {
                                    {table_status_row(
                                        if mobile_ro { "8" } else { "9" },
                                        "No discount codes yet.".to_string(),
                                    )}
                                }
                                if *discounts_loading.read() {
                                    {table_status_row(
                                        if mobile_ro { "8" } else { "9" },
                                        "Loading discounts…".to_string(),
                                    )}
                                }
                            }
                        }
                    }
                }
            },
        }
    };

    // detail drawer — looked up live so a refresh keeps it current
    let drawer_user = selected_id
        .read()
        .as_ref()
        .and_then(|id| match &*users_res.read() {
            Some(Ok(list)) => list.iter().find(|u| &u.id == id).cloned(),
            _ => None,
        });

    rsx! {
        div { class: "col", style: "height:100%",
            if mobile_ro {
                {mobile_banner()}
            }
            div {
                class: ws.root_class(),
                tabindex: "0",
                onmousemove: move |e| ws.handle_mouse_move(&e),
                onmouseup: move |_| ws.handle_mouse_up(),
                onkeydown: move |e| {
                    // Keyboard restore shortcuts — inert while typing in a field.
                    if panel_kit::is_editing() {
                        return;
                    }
                    if let Key::Character(c) = e.key() {
                        match c.as_str() {
                            "1" => ws.restore(AdminPanel::Overview),
                            "2" => ws.restore(AdminPanel::Parameters),
                            "3" => ws.restore(AdminPanel::Users),
                            "4" => ws.restore(AdminPanel::Discounts),
                            _ => {}
                        }
                    }
                },
                {ws.render(body)}
                {ws.dock()}
            }
        }
        if let Some(u) = drawer_user {
            {
                let uid = u.id.clone();
                let verified = u.email_verified.is_some();
                let display_name = u.name.clone()
                    .or_else(|| u.email.clone())
                    .unwrap_or_else(|| "Unnamed user".to_string());
                let email_text = u.email.clone().unwrap_or_else(|| "—".to_string());
                let username_text = u.username.clone().unwrap_or_else(|| "—".to_string());
                let joined_text = u.created_at.as_deref().map(format_date).unwrap_or_else(|| "—".to_string());
                let user_role = u.role.clone();
                let role_saving = saving_role_id.read().as_deref() == Some(uid.as_str());
                let vip_saving = saving_vip_id.read().as_deref() == Some(uid.as_str());
                let uid_role = uid.clone();
                let uid_vip = uid.clone();
                let uid_pw = uid.clone();

                rsx! {
                    Drawer {
                        title: "User details".to_string(),
                        on_close: move |_| selected_id.set(None),
                        div { class: "col", style: "gap:1.25rem",
                            // ── identity ───────────────────────────────────────
                            div { class: "row", style: "gap:0.75rem;align-items:center",
                                Identicon { seed: uid.clone(), size: 48 }
                                div { class: "col", style: "gap:0.25rem;min-width:0",
                                    div { style: "font-weight:600", {display_name} }
                                    div { class: "muted", style: "font-size:0.8125rem;overflow-wrap:anywhere", {email_text} }
                                    div { class: "row", style: "gap:0.5rem;align-items:center",
                                        {verified_badge(verified)}
                                        span { class: "muted", style: "font-size:0.75rem", {username_text} }
                                    }
                                }
                            }
                            // ── role + vip (read-only values on mobile) ────────
                            if mobile_ro {
                                div { class: "col", style: "gap:0.25rem",
                                    span { class: "muted", style: "font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em", "Role" }
                                    {tag_badge("role", user_role.clone(), Some(role_accent(&user_role)))}
                                }
                                div { class: "col", style: "gap:0.25rem",
                                    span { class: "muted", style: "font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em", "VIP pass" }
                                    span { style: "font-size:0.875rem", if u.vip_pass { "Yes" } else { "No" } }
                                }
                                div { class: "col", style: "gap:0.25rem",
                                    span { class: "muted", style: "font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em", "Joined" }
                                    span { style: "font-size:0.875rem", {joined_text.clone()} }
                                }
                                div { class: "muted", style: "font-size:0.75rem;border-top:1px solid var(--border-app);padding-top:1rem",
                                    "Editing requires a desktop viewport."
                                }
                            } else {
                                label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                                    "Role"
                                    select {
                                        class: "app-input",
                                        style: "padding:0.5rem 0.75rem;font-size:0.875rem",
                                        disabled: role_saving,
                                        value: "{user_role}",
                                        oninput: move |e| set_role(uid_role.clone(), e.value()),
                                        for opt in role_options.read().iter() {
                                            option { value: "{opt.role}", {opt.role.clone()} }
                                        }
                                    }
                                }
                                label { class: "row", style: "gap:0.5rem;align-items:center;cursor:pointer",
                                    input {
                                        r#type: "checkbox",
                                        style: "width:1rem;height:1rem;cursor:pointer",
                                        checked: u.vip_pass,
                                        disabled: vip_saving,
                                        oninput: move |e| set_vip(uid_vip.clone(), e.value() == "true"),
                                    }
                                    span { style: "font-size:0.875rem", "VIP pass" }
                                }
                            }
                            // ── set password (desktop only) ────────────────────
                            if !mobile_ro {
                                form {
                                    class: "col",
                                    style: "gap:0.75rem;border-top:1px solid var(--border-app);padding-top:1rem",
                                    onsubmit: move |_| submit_password(uid_pw.clone()),
                                    span { class: "muted", style: "font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em;font-family:monospace",
                                        "Set password"
                                    }
                                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                                        "New password"
                                        input {
                                            class: "app-input",
                                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                                            r#type: "password",
                                            autocomplete: "new-password",
                                            value: "{pw}",
                                            oninput: move |e| pw.set(e.value()),
                                        }
                                    }
                                    label { class: "col muted", style: "gap:0.25rem;font-size:0.75rem;text-transform:uppercase;letter-spacing:0.05em",
                                        "Confirm password"
                                        input {
                                            class: "app-input",
                                            style: "padding:0.5rem 0.75rem;font-size:0.875rem;text-transform:none",
                                            r#type: "password",
                                            autocomplete: "new-password",
                                            value: "{pw_confirm}",
                                            oninput: move |e| pw_confirm.set(e.value()),
                                        }
                                    }
                                    if !pw_error.read().is_empty() {
                                        div { class: "app-card error", style: "padding:0.5rem 0.75rem;font-size:0.8125rem",
                                            {pw_error.read().clone()}
                                        }
                                    }
                                    button {
                                        class: "app-btn app-btn-active",
                                        style: "height:38px;font-weight:bold",
                                        r#type: "submit",
                                        disabled: *saving_pw.read(),
                                        if *saving_pw.read() { "Saving…" } else { "Set password" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        {
            pending_delete.read().clone().map(|d| {
                let del_id = d.id.clone();
                let del_code = d.code.clone();
                let busy = saving_ids.read().contains(&del_id);
                rsx! {
                    crate::components::ui::ConfirmModal {
                        title: format!("Delete {del_code}?"),
                        body: "This permanently deletes the code here and in Lemon Squeezy.".to_string(),
                        confirm_label: "Delete".to_string(),
                        busy,
                        on_confirm: move |_| {
                            let id = del_id.clone();
                            let code = del_code.clone();
                            saving_ids.write().push(id.clone());
                            discount_message.set(String::new());
                            discount_error.set(String::new());
                            spawn_local(async move {
                                match mutation("discount.remove", Some(json!({"id": id}))).await {
                                    Ok(_) => {
                                        discount_message.set(format!("Deleted code {code}."));
                                        refresh_discounts();
                                    }
                                    Err(e) => {
                                        discount_error.set(trpc_err_msg(e));
                                        refresh_discounts();
                                    }
                                }
                                saving_ids.write().retain(|x| x != &id);
                                pending_delete.set(None);
                            });
                        },
                        on_cancel: move |_| pending_delete.set(None),
                    }
                }
            })
        }
    }
}
