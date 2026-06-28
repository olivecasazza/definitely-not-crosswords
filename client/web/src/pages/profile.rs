use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crate::components::pro_upgrade::ProUpgrade;
use crate::net;
use crate::store::use_app_state;
use crate::Route;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Profile,
    Subscription,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Profile => "Profile",
            Panel::Subscription => "Subscription",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Profile, 16.0, 16.0, 1120.0, 948.0),
        b.at(Panel::Subscription, 1152.0, 16.0, 752.0, 948.0),
    ]
}

#[component]
pub fn Profile() -> Element {
    let state = use_app_state();

    let user = state.user();
    let user_email = user
        .as_ref()
        .and_then(|u| u.email.clone())
        .unwrap_or_default();
    let user_name_init = user
        .as_ref()
        .and_then(|u| u.name.clone())
        .unwrap_or_default();
    let user_role = format!(
        "{:?}",
        user.as_ref().map(|u| u.role.clone()).unwrap_or_default()
    );

    let name_input = use_signal(|| user_name_init.clone());
    let updating = use_signal(|| false);
    let success_msg = use_signal(|| String::new());
    let error_msg = use_signal(|| String::new());

    let show_delete_confirm = use_signal(|| false);
    let deleting = use_signal(|| false);

    // The displayed name may change after a successful update
    let display_name = use_signal(|| user_name_init.clone());

    let handle_update = {
        let email = user_email.clone();
        let name_input = name_input.clone();
        let updating = updating.clone();
        let success_msg = success_msg.clone();
        let error_msg = error_msg.clone();
        let display_name = display_name.clone();
        move |evt: Event<FormData>| {
            evt.stop_propagation();
            let email = email.clone();
            let name = name_input.read().clone();
            let mut updating = updating.clone();
            let mut success_msg = success_msg.clone();
            let mut error_msg = error_msg.clone();
            let mut display_name = display_name.clone();
            spawn_local(async move {
                updating.set(true);
                success_msg.set(String::new());
                error_msg.set(String::new());
                match net::query_as::<serde_json::Value>(
                    "user.updateProfile",
                    Some(json!({ "email": email, "name": name })),
                )
                .await
                {
                    Ok(res) => {
                        if let Some(new_name) = res.get("name").and_then(|v| v.as_str()) {
                            display_name.set(new_name.to_string());
                        }
                        success_msg.set("Profile updated successfully!".into());
                    }
                    Err(e) => {
                        error_msg.set(e);
                    }
                }
                updating.set(false);
            });
        }
    };

    let handle_delete = {
        let email = user_email.clone();
        let deleting = deleting.clone();
        let error_msg = error_msg.clone();
        let show_delete_confirm = show_delete_confirm.clone();
        move |_| {
            let email = email.clone();
            let mut deleting = deleting.clone();
            let mut error_msg = error_msg.clone();
            let mut show_delete_confirm = show_delete_confirm.clone();
            spawn_local(async move {
                deleting.set(true);
                error_msg.set(String::new());
                match net::query_as::<serde_json::Value>(
                    "user.deleteAccount",
                    Some(json!({ "email": email })),
                )
                .await
                {
                    Ok(_) => {
                        if let Some(win) = web_sys::window() {
                            let _ = win.location().set_href("/auth/signup");
                        }
                    }
                    Err(e) => {
                        error_msg.set(e);
                        deleting.set(false);
                        show_delete_confirm.set(false);
                    }
                }
            });
        }
    };

    let ws = use_workspace("profile_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Profile => {
                let first_char = display_name
                    .read()
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_else(|| "U".to_string());
                rsx! {
                    div { style: "display: flex; flex-direction: column; gap: 1.5rem; height: 100%; overflow-y: auto;",

                        // Back button
                        div {
                            Link {
                                to: Route::Games {},
                                class: "app-btn",
                                style: "width: max-content; font-size: .75rem; font-family: monospace; text-transform: uppercase; letter-spacing: .05em;",
                                "← Back to Lobby"
                            }
                        }

                        // Avatar card
                        div { class: "app-card pf-avatar-card",
                            div {
                                style: "position: relative; display: inline-block;",
                                div { class: "pf-avatar-circle", "{first_char}" }
                                div {
                                    class: "pf-verified-badge",
                                    title: "Email Verified",
                                    "✓"
                                }
                            }
                            div { style: "text-align: center;",
                                h2 { style: "font-weight: 700; font-size: 1.125rem; color: var(--text-primary); margin: 0 0 .25rem 0;", "{display_name}" }
                                p { class: "muted", style: "font-size: .75rem; font-family: monospace; margin: 0;", "{user_email}" }
                            }
                            div { class: "pf-meta-list",
                                div { class: "pf-meta-row",
                                    span { class: "muted", style: "font-size: .625rem; font-family: monospace; text-transform: uppercase; letter-spacing: .05em;", "Account Type:" }
                                    span { style: "font-size: .625rem; font-family: monospace; font-weight: 600; text-transform: uppercase; color: var(--pastel-yellow);", "{user_role}" }
                                }
                                div { class: "pf-meta-row",
                                    span { class: "muted", style: "font-size: .625rem; font-family: monospace; text-transform: uppercase; letter-spacing: .05em;", "Status:" }
                                    span { style: "font-size: .625rem; font-family: monospace; font-weight: 600; text-transform: uppercase; color: var(--pastel-green);", "Verified" }
                                }
                            }
                        }

                        // Profile settings
                        div { class: "app-card", style: "padding: 1.5rem 2rem; display: flex; flex-direction: column; gap: 1.5rem;",
                            div {
                                h3 { style: "font-size: 1.125rem; font-weight: 700; font-family: monospace; text-transform: uppercase; letter-spacing: .05em; color: var(--text-primary); margin: 0 0 .25rem 0;", "Profile Settings" }
                                p { class: "muted", style: "font-size: .75rem; font-family: monospace; margin: 0;", "Update your public identity details" }
                            }

                            if !success_msg.read().is_empty() {
                                div { class: "success", style: "font-size: .75rem; font-family: monospace; padding: .875rem; border-radius: .75rem; border: 1px solid rgba(168,230,207,0.2); background: rgba(168,230,207,0.06);",
                                    "{success_msg}"
                                }
                            }
                            if !error_msg.read().is_empty() {
                                div { class: "error", style: "font-size: .75rem; font-family: monospace; padding: .875rem; border-radius: .75rem; border: 1px solid rgba(255,140,140,0.2); background: rgba(255,140,140,0.06);",
                                    "{error_msg}"
                                }
                            }

                            form {
                                onsubmit: handle_update.clone(),
                                style: "display: flex; flex-direction: column; gap: 1rem;",
                                div { style: "display: flex; flex-direction: column; gap: .375rem;",
                                    label {
                                        r#for: "profile-name",
                                        style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); font-family: monospace;",
                                        "Display Name"
                                    }
                                    input {
                                        id: "profile-name",
                                        class: "app-input",
                                        style: "width: 100%; padding: .75rem 1rem;",
                                        r#type: "text",
                                        required: true,
                                        placeholder: "e.g. Olive Casazza",
                                        value: "{name_input}",
                                        oninput: move |e| name_input.clone().set(e.value()),
                                    }
                                }
                                button {
                                    r#type: "submit",
                                    class: "app-btn app-btn-active",
                                    style: "width: 100%; justify-content: center; font-size: .875rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; padding: .75rem 1rem;",
                                    disabled: *updating.read(),
                                    if *updating.read() { "Saving..." } else { "Update Profile" }
                                }
                            }
                        }

                        // Danger zone
                        div {
                            class: "app-card",
                            style: "padding: 1.5rem 2rem; display: flex; flex-direction: column; gap: 1.5rem; border-color: rgba(255,140,140,0.15); background: rgba(255,140,140,0.03);",
                            div {
                                h3 { style: "font-size: 1.125rem; font-weight: 700; font-family: monospace; text-transform: uppercase; letter-spacing: .05em; color: var(--pastel-red); margin: 0 0 .25rem 0;", "Danger Zone" }
                                p { class: "muted", style: "font-size: .75rem; font-family: monospace; margin: 0;", "Permanently remove your account and all associated data" }
                            }

                            if !*show_delete_confirm.read() {
                                button {
                                    class: "app-btn",
                                    style: "width: max-content; font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; border-color: var(--pastel-red); color: var(--pastel-red);",
                                    onclick: move |_| show_delete_confirm.clone().set(true),
                                    "Delete Account"
                                }
                            } else {
                                div {
                                    style: "padding: 1rem; border-radius: .75rem; border: 1px solid rgba(255,140,140,0.2); background: rgba(255,140,140,0.05); display: flex; flex-direction: column; gap: 1rem;",
                                    h4 { style: "font-size: .875rem; font-weight: 700; color: var(--pastel-red); text-transform: uppercase; font-family: monospace; margin: 0;", "Are you absolutely sure?" }
                                    p { class: "muted", style: "font-size: .75rem; line-height: 1.6; margin: 0;",
                                        "This action is irreversible. All of your stats, generation jobs, and account references will be deleted forever."
                                    }
                                    div { style: "display: flex; flex-wrap: wrap; gap: .75rem;",
                                        button {
                                            class: "app-btn app-btn-active",
                                            style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; padding: .625rem 1rem; background: var(--pastel-red); border-color: var(--pastel-red); color: #0f172a;",
                                            disabled: *deleting.read(),
                                            onclick: handle_delete.clone(),
                                            if *deleting.read() { "Deleting..." } else { "Yes, Delete Account" }
                                        }
                                        button {
                                            class: "app-btn",
                                            style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; padding: .625rem 1rem;",
                                            disabled: *deleting.read(),
                                            onclick: move |_| show_delete_confirm.clone().set(false),
                                            "Cancel"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Panel::Subscription => rsx! {
                ProUpgrade {}
            },
        }
    };

    rsx! {
        style { {PROFILE_CSS} }
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

const PROFILE_CSS: &str = r#"
.pf-avatar-card {
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 1rem;
}
.pf-avatar-circle {
    width: 6rem;
    height: 6rem;
    border-radius: 50%;
    background: linear-gradient(to top right, var(--pastel-yellow), rgba(254,234,153,0.3));
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 1.875rem;
    color: #0f172a;
    border: 4px solid rgba(255,255,255,0.05);
    text-transform: uppercase;
    user-select: none;
}
.pf-verified-badge {
    position: absolute;
    bottom: 0;
    right: 0;
    width: 1.5rem;
    height: 1.5rem;
    border-radius: 50%;
    background: var(--pastel-green);
    color: #0f172a;
    border: 2px solid var(--bg-card);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: .625rem;
    font-weight: 700;
}
.pf-meta-list {
    border-top: 1px solid var(--border-app);
    padding-top: 1rem;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: .5rem;
}
.pf-meta-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
}
"#;
