use crate::components::identicon::Identicon;
use crate::components::ui::Drawer;
use crate::net::{mutation, query, trpc_err_msg};
use crate::store::{use_app_state, Severity};
use crossword_core::fmt::format_date;
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

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
    // Additive server field — older servers omit it, so default to None
    // and render "—".
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Clone, serde::Deserialize)]
struct RoleOption {
    role: String,
    capabilities: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    AddUser,
    Users,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::AddUser => "Add User",
            Panel::Users => "Users",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::AddUser, 16.0, 16.0, 560.0, 880.0),
        b.at(Panel::Users, 592.0, 16.0, 1312.0, 880.0),
    ]
}

/// Square verified/pending status pill, shared by the table and the drawer.
fn verified_badge(verified: bool) -> Element {
    rsx! {
        span {
            style: if verified {
                "padding:0.125rem 0.5rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;background:var(--color-success);color:var(--contrast-ink)"
            } else {
                "padding:0.125rem 0.5rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;background:var(--border-app);color:var(--text-secondary)"
            },
            if verified { "Verified" } else { "Pending" }
        }
    }
}

#[component]
pub fn AdminUsers() -> Element {
    let state = use_app_state();
    let mut users = use_signal(Vec::<AdminUser>::new);
    let mut role_options = use_signal(Vec::<RoleOption>::new);
    let mut loading = use_signal(|| true);
    let mut saving = use_signal(|| false);
    let mut saving_role_id = use_signal(|| None::<String>);
    let mut saving_vip_id = use_signal(|| None::<String>);
    let mut message = use_signal(String::new);
    let mut error_msg = use_signal(String::new);

    // add-user form
    let mut new_email = use_signal(String::new);
    let mut new_name = use_signal(String::new);
    let mut new_role = use_signal(|| "ADMIN".to_string());

    // search + filters (client-side, over the fetched list)
    let mut search = use_signal(String::new);
    let mut filter_role = use_signal(|| None::<&'static str>);
    let mut filter_verified = use_signal(|| None::<bool>);
    let mut filter_vip_only = use_signal(|| false);

    // detail drawer — holds the selected user's id; the row itself is looked
    // up live from `users` so refreshes keep the drawer in sync.
    let mut selected_id = use_signal(|| None::<String>);

    // set-password form (drawer)
    let mut pw = use_signal(String::new);
    let mut pw_confirm = use_signal(String::new);
    let mut pw_error = use_signal(String::new);
    let mut saving_pw = use_signal(|| false);

    let refresh_users = move || {
        spawn_local(async move {
            match query("user.listForAdmin", None).await {
                Ok(v) => {
                    let parsed: Vec<AdminUser> = serde_json::from_value(v).unwrap_or_default();
                    users.set(parsed);
                }
                Err(e) => error_msg.set(e),
            }
        });
    };

    // initial load
    use_effect(move || {
        loading.set(true);
        error_msg.set(String::new());
        spawn_local(async move {
            // fetch roles and users in parallel — fire-and-forget style
            let roles_fut = query("user.roleOptions", None);
            let users_fut = query("user.listForAdmin", None);
            let (roles_res, users_res) = futures::join!(roles_fut, users_fut);
            match roles_res {
                Ok(v) => {
                    let parsed: Vec<RoleOption> = serde_json::from_value(v).unwrap_or_default();
                    // seed role selector with first option if not yet set
                    if let Some(first) = parsed.first() {
                        new_role.set(first.role.clone());
                    }
                    role_options.set(parsed);
                }
                Err(e) => error_msg.set(e),
            }
            match users_res {
                Ok(v) => {
                    let parsed: Vec<AdminUser> = serde_json::from_value(v).unwrap_or_default();
                    users.set(parsed);
                }
                Err(e) => error_msg.set(e),
            }
            loading.set(false);
        });
    });

    // add user submit
    let add_user = move |_: FormEvent| {
        let email = new_email.read().trim().to_string();
        let name = new_name.read().trim().to_string();
        let role = new_role.read().clone();
        if email.is_empty() {
            return;
        }
        saving.set(true);
        message.set(String::new());
        error_msg.set(String::new());
        spawn_local(async move {
            let input = json!({
                "email": email,
                "name": if name.is_empty() { serde_json::Value::Null } else { json!(name) },
                "role": role,
            });
            match mutation("user.upsertFromAdmin", Some(input)).await {
                Ok(_) => {
                    message.set(format!("{email} is now {role}."));
                    new_email.set(String::new());
                    new_name.set(String::new());
                    new_role.set("ADMIN".to_string());
                    refresh_users();
                }
                Err(e) => error_msg.set(e),
            }
            saving.set(false);
        });
    };

    // row-level mutations, shared by the table and the drawer. Outcomes go
    // through toasts; guard-rail errors (not-self, last-admin) surface via
    // trpc_err_msg. Refresh either way so a rejected change snaps back.
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

    let ws = use_workspace("admin_users_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);
    if let Some(gate) = crate::store::use_auth_guard(crossword_core::auth::Role::Admin) {
        return gate;
    }

    // Mobile (< 760px, panel-kit's own threshold) is read-only: row data still
    // renders, but mutation controls are not mounted at all.
    let mobile_ro = *ws.is_mobile.read();

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::AddUser if mobile_ro => rsx! {
                div { class: "col", style: "gap:0.75rem;padding:1rem",
                    div { class: "muted", style: "font-size:0.875rem",
                        "Adding users requires a desktop viewport."
                    }
                    // fetch errors still surface on mobile (read-only display)
                    if !error_msg.read().is_empty() {
                        div { class: "app-card error", style: "padding:0.75rem;font-size:0.875rem",
                            {error_msg.read().clone()}
                        }
                    }
                }
            },
            Panel::AddUser => rsx! {
                div { class: "col", style: "gap:1rem",
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
                    if !message.read().is_empty() {
                        div { class: "app-card success", style: "padding:0.75rem;font-size:0.875rem",
                            {message.read().clone()}
                        }
                    }
                    if !error_msg.read().is_empty() {
                        div { class: "app-card error", style: "padding:0.75rem;font-size:0.875rem",
                            {error_msg.read().clone()}
                        }
                    }
                }
            },
            Panel::Users => {
                // client-side search + filters over the fetched list
                let q = search.read().trim().to_lowercase();
                let role_f = *filter_role.read();
                let verified_f = *filter_verified.read();
                let vip_only = *filter_vip_only.read();
                let total = users.read().len();
                let filtered: Vec<AdminUser> = users
                    .read()
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
                            div { class: "section-tabs", role: "group", aria_label: "Filter by role",
                                for (label, val) in [("All", None), ("Admin", Some("ADMIN")), ("User", Some("USER"))] {
                                    button {
                                        class: if role_f == val { "section-tab section-tab-active" } else { "section-tab" },
                                        aria_pressed: role_f == val,
                                        onclick: move |_| filter_role.set(val),
                                        {label}
                                    }
                                }
                            }
                            div { class: "section-tabs", role: "group", aria_label: "Filter by verification",
                                for (label, val) in [("All", None), ("Verified", Some(true)), ("Pending", Some(false))] {
                                    button {
                                        class: if verified_f == val { "section-tab section-tab-active" } else { "section-tab" },
                                        aria_pressed: verified_f == val,
                                        onclick: move |_| filter_verified.set(val),
                                        {label}
                                    }
                                }
                            }
                            div { class: "section-tabs", role: "group", aria_label: "Filter by VIP",
                                for (label, val) in [("All", false), ("VIP", true)] {
                                    button {
                                        class: if vip_only == val { "section-tab section-tab-active" } else { "section-tab" },
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
                                thead {
                                    tr { style: "font-size:0.75rem;text-transform:uppercase;font-family:monospace",
                                        for col in ["User", "Username", "Verified", "Joined", "Role", "VIP Pass", "Capabilities"] {
                                            th { class: "muted", style: "padding:0.75rem 1rem;border-bottom:1px solid var(--border-app)", {col} }
                                        }
                                    }
                                }
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
                                                    // Mobile is read-only: plain text, no editors mounted.
                                                    if mobile_ro {
                                                        td { style: "padding:0.75rem 1rem", {user_role.clone()} }
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
                                    if filtered.is_empty() && !*loading.read() {
                                        tr {
                                            td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "7",
                                                if total == 0 { "No users found." } else { "No users match the current filters." }
                                            }
                                        }
                                    }
                                    if *loading.read() {
                                        tr {
                                            td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "7",
                                                "Loading users…"
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
    };

    // detail drawer — looked up live so a refresh keeps it current
    let drawer_user = selected_id
        .read()
        .as_ref()
        .and_then(|id| users.read().iter().find(|u| &u.id == id).cloned());

    rsx! {
        div { class: "col", style: "height:100%",
            if mobile_ro {
                div { class: "muted", style: "font-size:0.75rem;padding:0.5rem 1rem;border-bottom:1px solid var(--border-app)",
                    "Editing requires a desktop viewport."
                }
            }
            div {
                class: ws.root_class(),
                tabindex: "0",
                onmousemove: move |e| ws.handle_mouse_move(&e),
                onmouseup: move |_| ws.handle_mouse_up(),
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
                                    span { style: "font-size:0.875rem", {user_role.clone()} }
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
    }
}
