use crate::net;
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, PartialEq)]
enum VerifyState {
    Verifying,
    Success,
    Error(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    VerifyEmail,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::VerifyEmail => "Verify Email",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![b.at(Panel::VerifyEmail, 720.0, 120.0, 480.0, 440.0)]
}

#[component]
pub fn VerifyEmail() -> Element {
    let state = use_signal(|| VerifyState::Verifying);

    // Run verification once on mount
    use_effect(move || {
        let mut state = state.clone();
        wasm_bindgen_futures::spawn_local(async move {
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

    let ws = use_workspace("verify_email_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::VerifyEmail => rsx! {
                div {
                    style: "padding: 2rem; text-align: center; display: flex; flex-direction: column;",

                    // Header
                    div {
                        style: "display: flex; flex-direction: column; align-items: center; margin-bottom: 2rem;",
                        h1 {
                            style: "font-family: monospace; font-size: 1.5rem; font-weight: 700; text-transform: uppercase; letter-spacing: .1em; color: var(--text-primary); margin: 0;",
                            "Email Verification"
                        }
                    }

                    match state.read().clone() {
                        VerifyState::Verifying => rsx! {
                            div { style: "display: flex; flex-direction: column; align-items: center; gap: 1.5rem;",
                                // Spinner
                                div {
                                    style: "
                                        width: 3rem; height: 3rem;
                                        border-radius: 50%;
                                        border: 2px solid var(--border-app);
                                        border-top-color: var(--pastel-yellow);
                                        animation: spin 0.8s linear infinite;
                                    ",
                                }
                                style { "@keyframes spin {{ to {{ transform: rotate(360deg); }} }}" }
                                p {
                                    class: "muted",
                                    style: "font-size: .875rem; font-family: monospace; margin: 0;",
                                    "Verifying your email token..."
                                }
                            }
                        },
                        VerifyState::Success => rsx! {
                            div { style: "display: flex; flex-direction: column; align-items: center; gap: 1.5rem;",
                                // Success icon circle
                                div {
                                    style: "
                                        width: 4rem; height: 4rem; border-radius: 50%;
                                        border: 2px solid var(--pastel-green);
                                        background: rgba(168,230,207,0.1);
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
                                    style: "font-family: monospace; font-size: 1.125rem; font-weight: 700; text-transform: uppercase; margin: 0;",
                                    "Email Verified!"
                                }
                                p {
                                    class: "muted",
                                    style: "font-size: .75rem; line-height: 1.6; margin: 0;",
                                    "Your email address has been successfully verified. You can now log into the application."
                                }
                                Link {
                                    to: crate::Route::Login {},
                                    class: "app-btn app-btn-active",
                                    style: "width: 100%; padding: .75rem 1rem; font-weight: 600; font-size: .875rem; text-transform: uppercase; letter-spacing: .05em; display: block;",
                                    "Sign In"
                                }
                            }
                        },
                        VerifyState::Error(msg) => rsx! {
                            div { style: "display: flex; flex-direction: column; align-items: center; gap: 1.5rem;",
                                // Error icon circle
                                div {
                                    style: "
                                        width: 4rem; height: 4rem; border-radius: 50%;
                                        border: 2px solid var(--pastel-red);
                                        background: rgba(255,140,140,0.1);
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
                                    style: "font-family: monospace; font-size: 1.125rem; font-weight: 700; text-transform: uppercase; margin: 0;",
                                    "Verification Failed"
                                }
                                p {
                                    class: "muted",
                                    style: "font-size: .75rem; line-height: 1.6; margin: 0;",
                                    if msg.is_empty() {
                                        "The verification token is invalid, expired, or has already been used."
                                    } else {
                                        "{msg}"
                                    }
                                }
                                Link {
                                    to: crate::Route::Signup {},
                                    class: "app-btn",
                                    style: "width: 100%; padding: .75rem 1rem; font-weight: 600; font-size: .875rem; text-transform: uppercase; letter-spacing: .05em; display: block;",
                                    "Back to Signup"
                                }
                            }
                        },
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
            {ws.render(body)}
            {ws.dock()}
        }
    }
}
