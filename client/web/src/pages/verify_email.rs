use crate::components::auth_layout::AuthLayout;
use crate::components::ui::SquarePulse;
use crate::net;
use dioxus::prelude::*;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, PartialEq)]
enum VerifyState {
    Verifying,
    Success,
    Error(String),
}

#[component]
pub fn VerifyEmail() -> Element {
    let app = crate::store::use_app_state();
    let state = use_signal(|| VerifyState::Verifying);

    // Run verification once on mount
    use_effect(move || {
        let mut state = state.clone();
        spawn_local(async move {
            // Parse ?token= from the URL query string by hand
            // (web_sys UrlSearchParams isn't in the enabled feature set)
            let token = web_sys::window()
                .and_then(|w| w.location().search().ok())
                .and_then(|qs| {
                    // qs is like "?token=abc123" or "?foo=bar&token=abc"
                    let qs = qs.trim_start_matches('?');
                    qs.split('&').find_map(|pair| {
                        let mut kv = pair.splitn(2, '=');
                        let k = kv.next()?;
                        let v = kv.next()?;
                        if k == "token" {
                            Some(v.to_string())
                        } else {
                            None
                        }
                    })
                });

            let token = match token {
                Some(t) if !t.is_empty() => t,
                _ => {
                    state.set(VerifyState::Error(
                        "Missing verification token in URL query parameters.".to_string(),
                    ));
                    return;
                }
            };

            match net::mutation_as::<serde_json::Value>(
                "user.verifyEmail",
                Some(json!({ "token": token })),
            )
            .await
            {
                Ok(res) if res["success"].as_bool().unwrap_or(false) => {
                    state.set(VerifyState::Success);
                }
                Ok(_) => {
                    state.set(VerifyState::Error(
                        "Verification did not succeed. Please try again.".to_string(),
                    ));
                }
                Err(e) => {
                    state.set(VerifyState::Error(e));
                }
            }
        });
    });

    // Resend flow, shown in the error state so a dead link isn't a dead end.
    let mut resend_email = use_signal(String::new);
    let mut resend_loading = use_signal(|| false);
    let mut resend_done = use_signal(|| false);
    let mut resend_note = use_signal(String::new);

    // Prefill the resend email from the session once it resolves (a signed-in
    // user legitimately lands here from an email link). peek() keeps this
    // effect from re-running on every keystroke.
    use_effect(move || {
        if let Some(email) = app.user().and_then(|u| u.email) {
            if resend_email.peek().is_empty() {
                resend_email.set(email);
            }
        }
    });

    let resend_submit = move |e: Event<FormData>| {
        e.prevent_default();
        let email = resend_email.peek().trim().to_lowercase();
        if !email.contains('@') {
            resend_note.set("Enter the email you signed up with.".into());
            return;
        }
        spawn_local(async move {
            resend_loading.set(true);
            resend_note.set(String::new());
            match net::mutation_as::<serde_json::Value>(
                "user.resendVerification",
                Some(json!({ "email": email })),
            )
            .await
            {
                // Always-success copy: the backend never reveals whether an
                // account exists.
                Ok(_) => resend_done.set(true),
                Err(e) => {
                    // The mutation lands in a parallel change; until it does,
                    // point at the profile flow instead of a raw tRPC error.
                    let low = e.to_lowercase();
                    if low.contains("not implemented")
                        || low.contains("no procedure")
                        || low.contains("not found")
                        || low.contains("wasn't found")
                    {
                        resend_note.set("Sign in and re-request from your profile.".into());
                    } else {
                        resend_note.set(e);
                    }
                }
            }
            resend_loading.set(false);
        });
    };

    rsx! {
        AuthLayout {
            eyebrow: "VERIFY EMAIL",
            show_brand: false,
            subtitle: "",

            match state.read().clone() {
                VerifyState::Verifying => rsx! {
                    div { style: "display: flex; flex-direction: column; align-items: center; gap: 1.5rem; padding: 1rem 0;",
                        SquarePulse {}
                        p {
                            class: "muted auth-note",
                            style: "text-align: center;",
                            "Verifying your email token..."
                        }
                    }
                },
                VerifyState::Success => rsx! {
                    div { style: "display: flex; flex-direction: column; align-items: center; gap: 1.5rem; text-align: center;",
                        // Success icon square
                        div {
                            style: "
                                width: 4rem; height: 4rem;
                                border: 2px solid var(--pastel-green);
                                background: color-mix(in srgb, var(--pastel-green) 10%, transparent);
                                display: flex; align-items: center; justify-content: center;
                                color: var(--pastel-green);
                            ",
                            // Checkmark (inline SVG via raw rsx)
                            svg {
                                width: "2rem",
                                height: "2rem",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "3",
                                    d: "M5 13l4 4L19 7",
                                }
                            }
                        }
                        h2 {
                            id: "verification-success-title",
                            class: "success",
                            style: "font-family: var(--mono, monospace); font-size: 1.125rem; font-weight: 700; text-transform: uppercase; margin: 0;",
                            "Email Verified!"
                        }
                        p {
                            class: "muted",
                            style: "font-size: .75rem; line-height: 1.6; margin: 0;",
                            "Your email address has been successfully verified. You can now log into the application."
                        }
                        Link {
                            to: crate::Route::Login {},
                            class: "app-btn app-btn-active auth-submit",
                            style: "display: block;",
                            "Sign In"
                        }
                    }
                },
                VerifyState::Error(msg) => rsx! {
                    div { style: "display: flex; flex-direction: column; align-items: center; gap: 1.5rem;",
                        // Error icon square
                        div {
                            style: "
                                width: 4rem; height: 4rem;
                                border: 2px solid var(--pastel-red);
                                background: color-mix(in srgb, var(--pastel-red) 10%, transparent);
                                display: flex; align-items: center; justify-content: center;
                                color: var(--pastel-red);
                            ",
                            svg {
                                width: "2rem",
                                height: "2rem",
                                fill: "none",
                                view_box: "0 0 24 24",
                                stroke: "currentColor",
                                path {
                                    stroke_linecap: "round",
                                    stroke_linejoin: "round",
                                    stroke_width: "3",
                                    d: "M6 18L18 6M6 6l12 12",
                                }
                            }
                        }
                        h2 {
                            class: "error",
                            style: "font-family: var(--mono, monospace); font-size: 1.125rem; font-weight: 700; text-transform: uppercase; margin: 0; text-align: center;",
                            "Verification Failed"
                        }
                        p {
                            class: "muted",
                            style: "font-size: .75rem; line-height: 1.6; margin: 0; text-align: center;",
                            if msg.is_empty() {
                                "The verification token is invalid, expired, or has already been used."
                            } else {
                                "{msg}"
                            }
                        }

                        // Resend form (replaces the old dead-end to Signup)
                        if *resend_done.read() {
                            p {
                                class: "success auth-note",
                                style: "text-align: center;",
                                "If that address has an unverified account, a new link is on its way."
                            }
                        } else {
                            form {
                                class: "auth-form",
                                style: "width: 100%;",
                                onsubmit: resend_submit,
                                div { class: "auth-group",
                                    label { r#for: "resend-email", class: "auth-label", "Email" }
                                    input {
                                        id: "resend-email",
                                        class: "app-input auth-field",
                                        r#type: "email",
                                        autocomplete: "email",
                                        value: "{resend_email}",
                                        oninput: move |e| resend_email.set(e.value()),
                                    }
                                }
                                if !resend_note.read().is_empty() {
                                    p { class: "error auth-hint", "{resend_note}" }
                                }
                                button {
                                    r#type: "submit",
                                    class: "app-btn app-btn-active auth-submit",
                                    disabled: *resend_loading.read(),
                                    if *resend_loading.read() { "Sending..." } else { "Send New Link" }
                                }
                            }
                        }
                    }
                },
            }
        }
    }
}
