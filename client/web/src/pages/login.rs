use dioxus::prelude::*;
use gloo_net::http::Request;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;

/// Simple percent-encoder for application/x-www-form-urlencoded values.
/// next-auth's credentials provider expects form-encoded, not JSON.
fn form_encode(pairs: &[(&str, &str)]) -> String {
    fn encode(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(b as char)
                }
                b' ' => out.push('+'),
                other => out.push_str(&format!("%{:02X}", other)),
            }
        }
        out
    }
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

#[derive(Deserialize)]
struct CsrfResponse {
    #[serde(rename = "csrfToken")]
    csrf_token: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    SignIn,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::SignIn => "Sign In",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![b.at(Panel::SignIn, 700.0, 60.0, 520.0, 640.0)]
}

#[component]
pub fn Login() -> Element {
    let email = use_signal(|| String::new());
    let password = use_signal(|| String::new());
    let email_touched = use_signal(|| false);
    let password_touched = use_signal(|| false);
    let loading = use_signal(|| false);
    let error = use_signal(|| String::new());

    // Derived validation
    let email_val = email.read().clone();
    let password_val = password.read().clone();

    let email_error = {
        let e = email_val.clone();
        if e.is_empty() {
            "Email address is required.".to_string()
        } else if !e.contains('@') || !e.contains('.') {
            "Please enter a valid email address.".to_string()
        } else {
            String::new()
        }
    };

    let password_error = if password_val.is_empty() {
        "Password is required.".to_string()
    } else {
        String::new()
    };

    let is_invalid = !email_error.is_empty() || !password_error.is_empty();

    let handle_submit = {
        let mut email_touched = email_touched.clone();
        let mut password_touched = password_touched.clone();
        let loading = loading.clone();
        let error = error.clone();
        let email_val = email_val.clone();
        let password_val = password_val.clone();

        move |evt: Event<FormData>| {
            evt.stop_propagation();
            email_touched.set(true);
            password_touched.set(true);

            if is_invalid {
                return;
            }

            let email_val = email_val.clone();
            let password_val = password_val.clone();
            let mut loading = loading.clone();
            let mut error = error.clone();

            spawn_local(async move {
                loading.set(true);
                error.set(String::new());

                // 1. Fetch CSRF token (sets the csrf cookie too)
                let csrf_result = async {
                    let resp = Request::get("/api/auth/csrf")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let data: CsrfResponse = resp.json().await.map_err(|e| e.to_string())?;
                    Ok::<String, String>(data.csrf_token)
                }
                .await;

                let csrf_token = match csrf_result {
                    Ok(t) => t,
                    Err(e) => {
                        error.set(format!("Failed to fetch CSRF token: {e}"));
                        loading.set(false);
                        return;
                    }
                };

                // 2. POST credentials
                let body = form_encode(&[
                    ("csrfToken", &csrf_token),
                    ("email", &email_val),
                    ("password", &password_val),
                    ("callbackUrl", "/"),
                    ("json", "true"),
                ]);

                let resp = Request::post("/api/auth/callback/credentials")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(body)
                    .unwrap()
                    .send()
                    .await;

                match resp {
                    Err(e) => {
                        error.set(e.to_string());
                        loading.set(false);
                    }
                    Ok(r) => {
                        // next-auth returns 200 with {url}. Check if url has error=
                        let text = r.text().await.unwrap_or_default();
                        let url_str = serde_json::from_str::<serde_json::Value>(&text)
                            .ok()
                            .and_then(|v| v["url"].as_str().map(|s| s.to_string()));

                        let is_error = url_str
                            .as_deref()
                            .map(|u| u.contains("error="))
                            .unwrap_or(false);

                        if is_error {
                            error.set("Invalid email or password.".to_string());
                            loading.set(false);
                        } else {
                            // Full-page reload so the session is re-fetched from scratch.
                            if let Some(win) = web_sys::window() {
                                let _ = win.location().set_href("/");
                            }
                        }
                    }
                }
            });
        }
    };

    let handle_keycloak = move |_| {
        if let Some(win) = web_sys::window() {
            let _ = win.location().set_href("/api/auth/signin/keycloak");
        }
    };

    // Dev bypass: signs in via the backend's `local-dev` credentials provider
    // (email-only admin, registered only in non-production). No-op in prod.
    let handle_dev_bypass = {
        let mut loading = loading;
        let mut error = error;
        move |_| {
            spawn_local(async move {
                loading.set(true);
                error.set(String::new());
                let csrf = async {
                    let resp = Request::get("/api/auth/csrf")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;
                    let d: CsrfResponse = resp.json().await.map_err(|e| e.to_string())?;
                    Ok::<String, String>(d.csrf_token)
                }
                .await;
                let csrf_token = match csrf {
                    Ok(t) => t,
                    Err(e) => {
                        error.set(format!("Failed to fetch CSRF token: {e}"));
                        loading.set(false);
                        return;
                    }
                };
                let body = form_encode(&[
                    ("csrfToken", &csrf_token),
                    ("email", "olive.casazza@gmail.com"),
                    ("callbackUrl", "/"),
                    ("json", "true"),
                ]);
                let resp = Request::post("/api/auth/callback/local-dev")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(body)
                    .unwrap()
                    .send()
                    .await;
                match resp {
                    Ok(r) => {
                        let text = r.text().await.unwrap_or_default();
                        let errored = serde_json::from_str::<serde_json::Value>(&text)
                            .ok()
                            .and_then(|v| v["url"].as_str().map(|s| s.contains("error=")))
                            .unwrap_or(false);
                        if errored {
                            error.set("Dev bypass unavailable (production build?).".into());
                            loading.set(false);
                        } else if let Some(win) = web_sys::window() {
                            let _ = win.location().set_href("/");
                        }
                    }
                    Err(e) => {
                        error.set(e.to_string());
                        loading.set(false);
                    }
                }
            });
        }
    };

    let ws = use_workspace("login_layout", default_layout);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::SignIn => rsx! {
                div {
                    class: "app-card",
                    style: "width: 100%; max-width: 28rem; padding: 2rem;",

                    // Header
                    div {
                        style: "display: flex; flex-direction: column; align-items: center; margin-bottom: 2rem;",
                        h1 {
                            style: "font-family: monospace; font-size: 1.5rem; font-weight: 700; text-transform: uppercase; letter-spacing: .1em; color: var(--text-primary); margin: 0 0 .25rem 0;",
                            "Sign In"
                        }
                        p {
                            class: "muted",
                            style: "font-size: .75rem; font-family: monospace; margin: 0; text-align: center;",
                            "Welcome back to the \"Definitely Not Crosswords\" experience"
                        }
                    }

                    // Form
                    form {
                        onsubmit: handle_submit.clone(),
                        style: "display: flex; flex-direction: column; gap: 1.25rem;",

                        // Error alert
                        if !error.read().is_empty() {
                            div {
                                class: "error",
                                style: "font-size: .75rem; font-family: monospace; padding: .75rem; border: 1px solid rgba(255,140,140,0.2); border-radius: .5rem; background: rgba(255,140,140,0.06);",
                                "{error}"
                            }
                        }

                        // Email field
                        div { style: "display: flex; flex-direction: column; gap: .375rem;",
                            label {
                                r#for: "email",
                                style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); font-family: monospace;",
                                "Email Address"
                            }
                            input {
                                id: "email",
                                class: "app-input",
                                style: "width: 100%; padding: .75rem 1rem;",
                                r#type: "email",
                                placeholder: "e.g. olive.casazza@gmail.com",
                                value: "{email}",
                                oninput: move |e| email.clone().set(e.value()),
                                onblur: move |_| email_touched.clone().set(true),
                            }
                            if *email_touched.read() && !email_error.is_empty() {
                                p {
                                    class: "error",
                                    style: "font-size: .69rem; font-family: monospace; margin: 0;",
                                    "{email_error}"
                                }
                            }
                        }

                        // Password field
                        div { style: "display: flex; flex-direction: column; gap: .375rem;",
                            label {
                                r#for: "password",
                                style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); font-family: monospace;",
                                "Password"
                            }
                            input {
                                id: "password",
                                class: "app-input",
                                style: "width: 100%; padding: .75rem 1rem;",
                                r#type: "password",
                                placeholder: "••••••••",
                                value: "{password}",
                                oninput: move |e| password.clone().set(e.value()),
                                onblur: move |_| password_touched.clone().set(true),
                            }
                            if *password_touched.read() && !password_error.is_empty() {
                                p {
                                    class: "error",
                                    style: "font-size: .69rem; font-family: monospace; margin: 0;",
                                    "{password_error}"
                                }
                            }
                        }

                        // Submit
                        button {
                            r#type: "submit",
                            class: "app-btn app-btn-active",
                            style: "width: 100%; padding: .75rem 1rem; font-weight: 600; font-size: .875rem; text-transform: uppercase; letter-spacing: .05em;",
                            disabled: *loading.read() || is_invalid,
                            if *loading.read() { "Signing in..." } else { "Sign In" }
                        }
                    }

                    // Divider
                    div {
                        style: "display: flex; align-items: center; gap: .75rem; margin: 1.25rem 0;",
                        div { style: "flex: 1; height: 1px; background: var(--border-app);" }
                        span {
                            class: "muted",
                            style: "font-size: .75rem; font-family: monospace;",
                            "or"
                        }
                        div { style: "flex: 1; height: 1px; background: var(--border-app);" }
                    }

                    // Keycloak SSO
                    button {
                        r#type: "button",
                        class: "app-btn",
                        style: "width: 100%; padding: .75rem 1rem; font-weight: 600; font-size: .875rem; text-transform: uppercase; letter-spacing: .05em;",
                        onclick: handle_keycloak.clone(),
                        "Continue with SSO"
                    }

                    // Dev-only bypass (no-op against a production backend).
                    button {
                        r#type: "button",
                        class: "app-btn",
                        style: "width: 100%; padding: .75rem 1rem; margin-top: .75rem; font-weight: 600; font-size: .8rem; text-transform: uppercase; letter-spacing: .05em;",
                        disabled: *loading.read(),
                        onclick: handle_dev_bypass.clone(),
                        "🔑 Developer Admin Bypass"
                    }

                    // Footer
                    div {
                        style: "margin-top: 1.5rem; padding-top: 1.5rem; border-top: 1px solid var(--border-app); text-align: center;",
                        p {
                            class: "muted",
                            style: "font-size: .75rem; font-family: monospace; margin: 0;",
                            "Don't have an account? "
                            Link {
                                to: crate::Route::Signup {},
                                style: "color: var(--pastel-yellow);",
                                "Sign Up"
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
            {ws.render(body)}
            {ws.dock()}
        }
    }
}
