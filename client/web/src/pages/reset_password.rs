//! Password reset, both halves on one route:
//!   /auth/reset-password           → email form → user.requestPasswordReset
//!   /auth/reset-password?token=…   → new-password form → user.resetPassword
//!
//! The request form always reports success (the backend never reveals whether
//! an account exists). A dead token swaps the card to "Link expired" with a
//! one-click flip into request mode.

use crate::components::auth_layout::AuthLayout;
use crate::net;
use dioxus::prelude::*;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

/// `?name=` from the current URL (tiny hand parser; web_sys UrlSearchParams
/// isn't in the enabled feature set — same approach as verify_email).
fn query_param(name: &str) -> Option<String> {
    let qs = web_sys::window()?.location().search().ok()?;
    qs.trim_start_matches('?').split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name && !v.is_empty()).then(|| v.to_string())
    })
}

/// Does this `user.resetPassword` error mean the token itself is dead
/// (invalid / expired / already used), as opposed to a transport failure?
fn is_token_error(e: &str) -> bool {
    let e = e.to_lowercase();
    e.contains("expired") || (e.contains("token") && (e.contains("invalid") || e.contains("used")))
}

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
    // Set when resetPassword rejects the token itself → "Link expired" card.
    let mut expired = use_signal(|| false);
    // Set by the "Send me a new link" button: render the request form even
    // though the URL carries a (dead) token.
    let mut request_mode = use_signal(|| false);

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
                Err(e) => {
                    if is_token_error(&e) {
                        expired.set(true);
                    } else {
                        error.set(e);
                    }
                }
            }
            loading.set(false);
        });
    };

    let has_token = token.is_some();
    // True when the email-request form is (or should be) showing — either no
    // token in the URL, or the user flipped over after a dead token.
    let in_request = !has_token || *request_mode.read();

    rsx! {
        AuthLayout {
            eyebrow: "RESET PASSWORD",
            show_brand: false,
            subtitle: "",

            if !error.read().is_empty() {
                p { class: "error auth-note", "{error}" }
            }

            if *done.read() {
                div {
                    class: "success auth-note",
                    style: "display: flex; flex-direction: column; gap: 1rem;",
                    if in_request {
                        p { style: "margin: 0;", "If that address has an account, a reset link is on its way. It expires in 1 hour." }
                    } else {
                        p { style: "margin: 0;", "Password updated. Sign in with your new password." }
                    }
                    Link { to: crate::Route::Login {}, class: "app-btn app-btn-active", style: "text-align: center;", "Sign In" }
                }
            } else if *expired.read() && !in_request {
                div { style: "display: flex; flex-direction: column; gap: 1rem;",
                    h2 {
                        class: "error",
                        style: "font-family: var(--mono, monospace); font-size: 1.125rem; font-weight: 700; text-transform: uppercase; margin: 0;",
                        "Link Expired"
                    }
                    p { class: "muted auth-note",
                        "This reset link is invalid, expired, or has already been used."
                    }
                    button {
                        r#type: "button",
                        class: "app-btn app-btn-active auth-submit",
                        onclick: move |_| {
                            error.set(String::new());
                            request_mode.set(true);
                        },
                        "Send Me a New Link"
                    }
                }
            } else if !in_request {
                form {
                    class: "auth-form",
                    onsubmit: reset_submit,
                    div { class: "auth-group",
                        label { r#for: "new-password", class: "auth-label", "New Password" }
                        input {
                            id: "new-password",
                            class: "app-input auth-field",
                            r#type: "password",
                            autocomplete: "new-password",
                            value: "{password}",
                            oninput: move |e| password.set(e.value()),
                        }
                    }
                    div { class: "auth-group",
                        label { r#for: "confirm-password", class: "auth-label", "Confirm Password" }
                        input {
                            id: "confirm-password",
                            class: "app-input auth-field",
                            r#type: "password",
                            autocomplete: "new-password",
                            value: "{confirm}",
                            oninput: move |e| confirm.set(e.value()),
                        }
                    }
                    button {
                        r#type: "submit",
                        class: "app-btn app-btn-active auth-submit",
                        disabled: *loading.read(),
                        if *loading.read() { "Saving..." } else { "Set New Password" }
                    }
                }
            } else {
                form {
                    class: "auth-form",
                    onsubmit: request_submit,
                    p { class: "muted auth-note",
                        "Enter your email and we'll send you a link to choose a new password."
                    }
                    div { class: "auth-group",
                        label { r#for: "reset-email", class: "auth-label", "Email" }
                        input {
                            id: "reset-email",
                            class: "app-input auth-field",
                            r#type: "email",
                            autocomplete: "email",
                            value: "{email}",
                            oninput: move |e| email.set(e.value()),
                        }
                    }
                    button {
                        r#type: "submit",
                        class: "app-btn app-btn-active auth-submit",
                        disabled: *loading.read(),
                        if *loading.read() { "Sending..." } else { "Send Reset Link" }
                    }
                    Link {
                        to: crate::Route::Login {},
                        class: "muted auth-link",
                        "Back to sign in"
                    }
                }
            }
        }
    }
}
