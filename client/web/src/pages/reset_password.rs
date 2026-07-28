//! Password reset, both halves on one route:
//!   /auth/reset-password           → email form → user.requestPasswordReset
//!   /auth/reset-password?token=…   → new-password form → user.resetPassword
//!
//! The request form always reports success (the backend never reveals whether
//! an account exists).

use crate::net;
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Reset,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        "Reset Password"
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![b.at(Panel::Reset, 720.0, 120.0, 480.0, 480.0)]
}

/// `?name=` from the current URL (tiny hand parser; web_sys UrlSearchParams
/// isn't in the enabled feature set — same approach as verify_email).
fn query_param(name: &str) -> Option<String> {
    let qs = web_sys::window()?.location().search().ok()?;
    qs.trim_start_matches('?').split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name && !v.is_empty()).then(|| v.to_string())
    })
}

const LABEL: &str = "font-size: .75rem; font-family: monospace; font-weight: 700; \
    text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary);";
const FIELD: &str = "width: 100%; padding: .625rem .75rem; font-size: .875rem;";
const SUBMIT: &str = "width: 100%; padding: .75rem 1rem; font-weight: 600; \
    font-size: .875rem; text-transform: uppercase; letter-spacing: .05em;";

#[component]
pub fn ResetPassword() -> Element {
    // Read once on mount: which half of the flow is this?
    let token = use_hook(|| query_param("token"));

    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(String::new);
    let mut done = use_signal(|| false);

    let request_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let email = email.peek().trim().to_lowercase();
        if !email.contains('@') {
            error.set("Enter the email you signed up with.".into());
            return;
        }
        spawn_local(async move {
            loading.set(true);
            error.set(String::new());
            match net::mutation_as::<serde_json::Value>(
                "user.requestPasswordReset",
                Some(json!({ "email": email })),
            )
            .await
            {
                Ok(_) => done.set(true),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    let token_for_submit = token.clone();
    let reset_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let pw = password.peek().clone();
        if pw.len() < 8 {
            error.set("Password must be at least 8 characters.".into());
            return;
        }
        if pw != *confirm.peek() {
            error.set("Passwords don't match.".into());
            return;
        }
        let token = token_for_submit.clone().unwrap_or_default();
        spawn_local(async move {
            loading.set(true);
            error.set(String::new());
            match net::mutation_as::<serde_json::Value>(
                "user.resetPassword",
                Some(json!({ "token": token, "password": pw })),
            )
            .await
            {
                Ok(_) => done.set(true),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    let ws = use_workspace("reset_password_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let has_token = token.is_some();
    let body = move |_: Panel, _max: bool| -> Element {
        rsx! {
            div {
                style: "padding: 2rem; display: flex; flex-direction: column; gap: 1.25rem;",
                h1 {
                    style: "font-family: monospace; font-size: 1.25rem; font-weight: 700; text-transform: uppercase; letter-spacing: .1em; color: var(--text-primary); margin: 0; text-align: center;",
                    "Reset Password"
                }

                if !error.read().is_empty() {
                    p { class: "error", style: "font-size: .75rem; font-family: monospace; margin: 0;", "{error}" }
                }

                if *done.read() {
                    div { class: "success", style: "font-size: .8rem; font-family: monospace; display: flex; flex-direction: column; gap: 1rem;",
                        if has_token {
                            p { style: "margin: 0;", "Password updated. Sign in with your new password." }
                        } else {
                            p { style: "margin: 0;", "If that address has an account, a reset link is on its way. It expires in 1 hour." }
                        }
                        Link { to: crate::Route::Login {}, class: "app-btn app-btn-active", style: "text-align: center;", "Sign In" }
                    }
                } else if has_token {
                    form {
                        onsubmit: reset_submit.clone(),
                        style: "display: flex; flex-direction: column; gap: 1rem;",
                        div { style: "display: flex; flex-direction: column; gap: .375rem;",
                            label { r#for: "new-password", style: LABEL, "New Password" }
                            input {
                                id: "new-password",
                                class: "app-input",
                                style: FIELD,
                                r#type: "password",
                                autocomplete: "new-password",
                                value: "{password}",
                                oninput: move |e| password.set(e.value()),
                            }
                        }
                        div { style: "display: flex; flex-direction: column; gap: .375rem;",
                            label { r#for: "confirm-password", style: LABEL, "Confirm Password" }
                            input {
                                id: "confirm-password",
                                class: "app-input",
                                style: FIELD,
                                r#type: "password",
                                autocomplete: "new-password",
                                value: "{confirm}",
                                oninput: move |e| confirm.set(e.value()),
                            }
                        }
                        button {
                            r#type: "submit",
                            class: "app-btn app-btn-active",
                            style: SUBMIT,
                            disabled: *loading.read(),
                            if *loading.read() { "Saving..." } else { "Set New Password" }
                        }
                    }
                } else {
                    form {
                        onsubmit: request_submit,
                        style: "display: flex; flex-direction: column; gap: 1rem;",
                        p { class: "muted", style: "font-size: .75rem; font-family: monospace; margin: 0;",
                            "Enter your email and we'll send you a link to choose a new password."
                        }
                        div { style: "display: flex; flex-direction: column; gap: .375rem;",
                            label { r#for: "reset-email", style: LABEL, "Email" }
                            input {
                                id: "reset-email",
                                class: "app-input",
                                style: FIELD,
                                r#type: "email",
                                autocomplete: "email",
                                value: "{email}",
                                oninput: move |e| email.set(e.value()),
                            }
                        }
                        button {
                            r#type: "submit",
                            class: "app-btn app-btn-active",
                            style: SUBMIT,
                            disabled: *loading.read(),
                            if *loading.read() { "Sending..." } else { "Send Reset Link" }
                        }
                        Link {
                            to: crate::Route::Login {},
                            class: "muted",
                            style: "font-size: .75rem; font-family: monospace; text-align: center; text-decoration: underline;",
                            "Back to sign in"
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
