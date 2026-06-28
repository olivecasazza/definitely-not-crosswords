use crate::components::admin_nav::AdminNav;
use crate::net::{mutation, query};
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

#[component]
pub fn AdminUsers() -> Element {
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

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
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
            Panel::Users => rsx! {
                // ── users table ────────────────────────────────────────────────
                div { style: "overflow-x:auto;height:100%",
                    table { style: "width:100%;text-align:left;font-size:0.875rem;border-collapse:collapse",
                        thead {
                            tr { style: "font-size:0.75rem;text-transform:uppercase;font-family:monospace",
                                for col in ["User", "Username", "Verified", "Role", "VIP Pass", "Capabilities"] {
                                    th { class: "muted", style: "padding:0.75rem 1rem;border-bottom:1px solid var(--border-app)", {col} }
                                }
                            }
                        }
                        tbody {
                            for user in users.read().iter() {
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

                                    let uid_role = uid.clone();
                                    let uid_vip = uid.clone();

                                    rsx! {
                                        tr { style: "border-bottom:1px solid var(--border-app)",
                                            td { style: "padding:0.75rem 1rem",
                                                div { style: "font-weight:500", {display_name} }
                                                div { class: "muted", style: "font-size:0.75rem", {email_text} }
                                            }
                                            td { class: "muted", style: "padding:0.75rem 1rem", {username_text} }
                                            td { style: "padding:0.75rem 1rem",
                                                span {
                                                    style: if verified {
                                                        "padding:0.125rem 0.5rem;border-radius:0.25rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;background:var(--color-success);color:#0f172a"
                                                    } else {
                                                        "padding:0.125rem 0.5rem;border-radius:0.25rem;font-size:0.625rem;font-weight:bold;text-transform:uppercase;background:var(--border-app);color:var(--text-secondary)"
                                                    },
                                                    if verified { "Verified" } else { "Pending" }
                                                }
                                            }
                                            td { style: "padding:0.75rem 1rem",
                                                {
                                                    let saving_id = saving_role_id.read().clone();
                                                    let is_saving = saving_id.as_deref() == Some(&uid_role);
                                                    rsx! {
                                                        select {
                                                            class: "app-input",
                                                            style: "padding:0.375rem 0.5rem;font-size:0.75rem",
                                                            disabled: is_saving,
                                                            value: "{user_role}",
                                                            oninput: move |e| {
                                                                let new_role_val = e.value();
                                                                let uid2 = uid_role.clone();
                                                                saving_role_id.set(Some(uid2.clone()));
                                                                message.set(String::new());
                                                                error_msg.set(String::new());
                                                                spawn_local(async move {
                                                                    match mutation("user.setRole", Some(json!({"userId": uid2, "role": new_role_val}))).await {
                                                                        Ok(_) => {
                                                                            message.set("Role updated.".to_string());
                                                                            refresh_users();
                                                                        }
                                                                        Err(e) => {
                                                                            error_msg.set(e);
                                                                            refresh_users();
                                                                        }
                                                                    }
                                                                    saving_role_id.set(None);
                                                                });
                                                            },
                                                            for opt in role_options.read().iter() {
                                                                option { value: "{opt.role}", {opt.role.clone()} }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            td { style: "padding:0.75rem 1rem",
                                                {
                                                    let saving_id = saving_vip_id.read().clone();
                                                    let is_saving = saving_id.as_deref() == Some(&uid_vip);
                                                    rsx! {
                                                        input {
                                                            r#type: "checkbox",
                                                            style: "width:1rem;height:1rem;cursor:pointer",
                                                            checked: vip,
                                                            disabled: is_saving,
                                                            oninput: move |e| {
                                                                let checked = e.value() == "true";
                                                                let uid3 = uid_vip.clone();
                                                                saving_vip_id.set(Some(uid3.clone()));
                                                                message.set(String::new());
                                                                error_msg.set(String::new());
                                                                spawn_local(async move {
                                                                    match mutation("user.setVipPass", Some(json!({"userId": uid3, "vipPass": checked}))).await {
                                                                        Ok(_) => {
                                                                            message.set("VIP status updated.".to_string());
                                                                            refresh_users();
                                                                        }
                                                                        Err(e) => {
                                                                            error_msg.set(e);
                                                                            refresh_users();
                                                                        }
                                                                    }
                                                                    saving_vip_id.set(None);
                                                                });
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                            td { style: "padding:0.75rem 1rem",
                                                div { class: "row", style: "gap:0.25rem;flex-wrap:wrap",
                                                    for cap in caps.iter() {
                                                        span {
                                                            style: "border:1px solid var(--border-app);padding:0.125rem 0.5rem;border-radius:0.25rem;font-size:0.625rem;color:var(--text-secondary)",
                                                            {cap.clone()}
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if users.read().is_empty() && !*loading.read() {
                                tr {
                                    td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "6",
                                        "No users found."
                                    }
                                }
                            }
                            if *loading.read() {
                                tr {
                                    td { class: "muted", style: "padding:1.5rem 1rem;text-align:center", colspan: "6",
                                        "Loading users…"
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
        div {
            class: ws.root_class(),
            tabindex: "0",
            onmousemove: move |e| ws.handle_mouse_move(&e),
            onmouseup: move |_| ws.handle_mouse_up(),
            AdminNav {}
            {ws.render(body)}
            {ws.dock()}
        }
    }
}
