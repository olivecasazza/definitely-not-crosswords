use crate::components::auth_layout::AuthLayout;
use dioxus::prelude::*;
use gloo_net::http::Request;
use serde::Deserialize;
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

#[component]
pub fn Login() -> Element {
    let nav = use_navigator();
    let state = crate::store::use_app_state();
    // Already signed in? Straight to the stashed destination (or Home).
    crate::store::use_guest_only();

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
                            // Refresh the shared session in place and router-nav
                            // back to wherever the guard bounced us from.
                            crate::store::refresh_session(state).await;
                            let dest =
                                crate::store::take_return_to().unwrap_or(crate::Route::Home {});
                            nav.push(dest);
                        }
                    }
                }
            });
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
                // No email: the backend's local-dev provider falls back to
                // LOCAL_ADMIN_EMAIL, so the bypass identity stays server-owned.
                let body = form_encode(&[
                    ("csrfToken", &csrf_token),
                    ("email", ""),
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
                        } else {
                            crate::store::refresh_session(state).await;
                            let dest =
                                crate::store::take_return_to().unwrap_or(crate::Route::Home {});
                            nav.push(dest);
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

    rsx! {
        AuthLayout {
            eyebrow: "SIGN IN",
            show_brand: true,
            subtitle: "Cooperative, real-time crosswords.",

            p {
                class: "muted auth-note",
                style: "text-align: center;",
                "Welcome back to the \"Definitely Not Crosswords\" experience"
            }

            form {
                class: "auth-form",
                onsubmit: handle_submit,

                // Error alert
                if !error.read().is_empty() {
                    div { class: "error auth-banner", "{error}" }
                }

                // Email field
                div { class: "auth-group",
                    label { r#for: "email", class: "auth-label", "Email Address" }
                    input {
                        id: "email",
                        class: "app-input auth-field",
                        r#type: "email",
                        placeholder: "you@example.com",
                        value: "{email}",
                        oninput: move |e| email.clone().set(e.value()),
                        onblur: move |_| email_touched.clone().set(true),
                    }
                    if *email_touched.read() && !email_error.is_empty() {
                        p { class: "error auth-hint", "{email_error}" }
                    }
                }

                // Password field
                div { class: "auth-group",
                    label { r#for: "password", class: "auth-label", "Password" }
                    input {
                        id: "password",
                        class: "app-input auth-field",
                        r#type: "password",
                        placeholder: "••••••••",
                        value: "{password}",
                        oninput: move |e| password.clone().set(e.value()),
                        onblur: move |_| password_touched.clone().set(true),
                    }
                    if *password_touched.read() && !password_error.is_empty() {
                        p { class: "error auth-hint", "{password_error}" }
                    }
                }

                // Submit
                button {
                    r#type: "submit",
                    class: "app-btn app-btn-active auth-submit",
                    disabled: *loading.read() || is_invalid,
                    if *loading.read() { "Signing in..." } else { "Sign In" }
                }

                Link {
                    to: crate::Route::ResetPassword {},
                    class: "muted auth-link",
                    "Forgot your password?"
                }
            }

            // Local-only dev bypass: the backend unregisters the local-dev
            // route outside local, and this button is hidden via the
            // devLoginBypass feature flag from /api/config.
            //
            // There is no SSO button here: the Rust backend never ported
            // next-auth's OAuth sign-in flow, so "Continue with SSO" only
            // ever navigated to /api/auth/signin/keycloak — an unrouted
            // path the SPA answered with its 404 page. Credentials login
            // is the only real path. Re-add SSO alongside a genuine
            // authorize/callback route + JWKS verification, not before.
            if state.feature(|f| f.dev_login_bypass) {
                div { class: "auth-divider",
                    span { class: "muted", "or" }
                }
                button {
                    r#type: "button",
                    class: "app-btn auth-submit",
                    disabled: *loading.read(),
                    onclick: handle_dev_bypass,
                    "🔑 Developer Admin Bypass"
                }
            }

            // Footer
            div { class: "auth-foot",
                p { class: "muted auth-note",
                    "Don't have an account? "
                    Link {
                        to: crate::Route::Signup {},
                        style: "color: var(--pastel-yellow);",
                        "Sign Up"
                    }
                }
            }
        }
    }
}
