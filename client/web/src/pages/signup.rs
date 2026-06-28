use crate::net;
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Welcome,
    CreateAccount,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Welcome => "Welcome",
            Panel::CreateAccount => "Create Account",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Welcome, 480.0, 110.0, 380.0, 700.0),
        b.at(Panel::CreateAccount, 880.0, 110.0, 540.0, 700.0),
    ]
}

fn dimmed(x: i32, y: i32) -> bool {
    (x == 14 && (2..=18).contains(&y)) || (y == 10 && (6..=22).contains(&x))
}

fn brand_panel(subtitle: &str) -> Element {
    rsx! {
        div { style: "display:flex; flex-direction:column; align-items:center; justify-content:center; height:100%; text-align:center; gap:1rem; padding:1.5rem;",
            svg {
                width: "72", height: "72", view_box: "0 0 24 24", fill: "none",
                xmlns: "http://www.w3.org/2000/svg", style: "color: var(--pastel-yellow);",
                for y in [2, 6, 10, 14, 18, 22] {
                    for x in [2, 6, 10, 14, 18, 22] {
                        circle { cx: "{x}", cy: "{y}", r: "1.2", fill: "currentColor",
                            opacity: if dimmed(x, y) { "0.3" } else { "1" } }
                    }
                }
            }
            h1 { style: "font-family: var(--mono, monospace); font-size: 1.3rem; font-weight: 800; margin: 0;",
                "definitely-not-crosswords" }
            p { class: "muted", style: "font-size: .8rem; line-height: 1.6; max-width: 16rem;", "{subtitle}" }
        }
    }
}

#[component]
pub fn Signup() -> Element {
    let name = use_signal(|| String::new());
    let username = use_signal(|| String::new());
    let email = use_signal(|| String::new());
    let password = use_signal(|| String::new());

    let name_touched = use_signal(|| false);
    let username_touched = use_signal(|| false);
    let email_touched = use_signal(|| false);
    let password_touched = use_signal(|| false);

    let loading = use_signal(|| false);
    let error = use_signal(|| String::new());
    let success = use_signal(|| false);
    let verification_token = use_signal(|| String::new());

    // Uniqueness state (checked on blur — ponytail: skip debounce, blur is fine)
    let username_unique = use_signal(|| true);
    let email_unique = use_signal(|| true);
    let checking_username = use_signal(|| false);
    let checking_email = use_signal(|| false);

    // Derived validation
    let name_val = name.read().clone();
    let username_val = username.read().clone();
    let email_val = email.read().clone();
    let password_val = password.read().clone();

    let name_error = if name_val.trim().is_empty() {
        "Full Name is required.".to_string()
    } else if name_val.trim().len() < 2 {
        "Full Name must be at least 2 characters.".to_string()
    } else {
        String::new()
    };

    let username_error = if username_val.trim().is_empty() {
        "Username is required.".to_string()
    } else if username_val.trim().len() < 3 {
        "Username must be at least 3 characters.".to_string()
    } else {
        String::new()
    };

    let email_error = if email_val.is_empty() {
        "Email address is required.".to_string()
    } else if !email_val.contains('@') || !email_val.contains('.') {
        "Please enter a valid email address.".to_string()
    } else {
        String::new()
    };

    let password_error = if password_val.is_empty() {
        "Password is required.".to_string()
    } else if password_val.len() < 6 {
        "Password must be at least 6 characters.".to_string()
    } else {
        String::new()
    };

    let is_invalid = !name_error.is_empty()
        || !username_error.is_empty()
        || !email_error.is_empty()
        || !password_error.is_empty()
        || !*username_unique.read()
        || !*email_unique.read()
        || *checking_username.read()
        || *checking_email.read();

    // On-blur uniqueness check for username
    let on_username_blur = {
        let mut username_touched = username_touched.clone();
        let username_unique = username_unique.clone();
        let checking_username = checking_username.clone();
        let username_val = username_val.clone();

        move |_| {
            username_touched.set(true);
            if username_val.trim().len() < 3 {
                return;
            }
            let uname = username_val.clone();
            let mut unique = username_unique.clone();
            let mut checking = checking_username.clone();
            spawn_local(async move {
                checking.set(true);
                match net::query_as::<serde_json::Value>(
                    "user.isUsernameUnique",
                    Some(json!({ "username": uname })),
                )
                .await
                {
                    Ok(v) => unique.set(v["unique"].as_bool().unwrap_or(true)),
                    Err(_) => unique.set(true), // fail open
                }
                checking.set(false);
            });
        }
    };

    // On-blur uniqueness check for email
    let on_email_blur = {
        let mut email_touched = email_touched.clone();
        let email_unique = email_unique.clone();
        let checking_email = checking_email.clone();
        let email_val = email_val.clone();

        move |_| {
            email_touched.set(true);
            if !email_val.contains('@') || !email_val.contains('.') {
                return;
            }
            let ev = email_val.clone();
            let mut unique = email_unique.clone();
            let mut checking = checking_email.clone();
            spawn_local(async move {
                checking.set(true);
                match net::query_as::<serde_json::Value>(
                    "user.isEmailUnique",
                    Some(json!({ "email": ev })),
                )
                .await
                {
                    Ok(v) => unique.set(v["unique"].as_bool().unwrap_or(true)),
                    Err(_) => unique.set(true),
                }
                checking.set(false);
            });
        }
    };

    let handle_submit = {
        let name = name.clone();
        let username = username.clone();
        let email = email.clone();
        let password = password.clone();
        let mut name_touched = name_touched.clone();
        let mut username_touched = username_touched.clone();
        let mut email_touched = email_touched.clone();
        let mut password_touched = password_touched.clone();
        let loading = loading.clone();
        let error = error.clone();
        let success = success.clone();
        let verification_token = verification_token.clone();
        let name_val = name_val.clone();
        let username_val = username_val.clone();
        let email_val = email_val.clone();
        let password_val = password_val.clone();

        move |evt: Event<FormData>| {
            evt.stop_propagation();
            name_touched.set(true);
            username_touched.set(true);
            email_touched.set(true);
            password_touched.set(true);

            if is_invalid {
                return;
            }

            let name_v = name_val.clone();
            let user_v = username_val.clone();
            let email_v = email_val.clone();
            let pass_v = password_val.clone();
            let mut name = name.clone();
            let mut username = username.clone();
            let mut email = email.clone();
            let mut password = password.clone();
            let mut loading = loading.clone();
            let mut error = error.clone();
            let mut success = success.clone();
            let mut verification_token = verification_token.clone();

            spawn_local(async move {
                loading.set(true);
                error.set(String::new());

                match net::mutation_as::<serde_json::Value>(
                    "user.signup",
                    Some(json!({
                        "email": email_v,
                        "name": name_v,
                        "username": user_v,
                        "password": pass_v,
                    })),
                )
                .await
                {
                    Ok(res) => {
                        if res["success"].as_bool().unwrap_or(false) {
                            verification_token
                                .set(res["verificationToken"].as_str().unwrap_or("").to_string());
                            success.set(true);
                            name.set(String::new());
                            username.set(String::new());
                            email.set(String::new());
                            password.set(String::new());
                        }
                    }
                    Err(e) => {
                        error.set(e);
                    }
                }
                loading.set(false);
            });
        }
    };

    // Username availability hint
    let uname_len = username_val.trim().len();
    let uname_available = *username_unique.read();
    let uname_checking = *checking_username.read();
    let uname_touched_val = *username_touched.read();

    // Email availability hint
    let email_len_ok = email_val.contains('@') && email_val.contains('.');
    let email_available = *email_unique.read();
    let email_checking = *checking_email.read();
    let email_touched_val = *email_touched.read();

    let ws = use_workspace("signup_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Welcome => brand_panel("Join the cooperative crossword race. Create an account to play, compete, and climb the leaderboard."),
            Panel::CreateAccount => rsx! {
                div {
                    style: "height: 100%; display: flex; flex-direction: column; justify-content: center; gap: 1.25rem; padding: 1.5rem 1.75rem; overflow-y: auto;",

                    // Header
                    div {
                        style: "display: flex; flex-direction: column; align-items: center; margin-bottom: 2rem;",
                        h1 {
                            style: "font-family: monospace; font-size: 1.5rem; font-weight: 700; text-transform: uppercase; letter-spacing: .1em; color: var(--text-primary); margin: 0 0 .25rem 0;",
                            "Create Account"
                        }
                        p {
                            class: "muted",
                            style: "font-size: .75rem; font-family: monospace; margin: 0; text-align: center;",
                            "Join the \"Definitely Not Crosswords\" experience"
                        }
                    }

                    // Success state
                    if *success.read() {
                        div {
                            class: "success",
                            style: "font-size: .75rem; font-family: monospace; padding: .75rem; border: 1px solid rgba(168,230,207,0.2); border-radius: .5rem; background: rgba(168,230,207,0.06); display: flex; flex-direction: column; gap: .5rem;",
                            p { style: "margin: 0;", "Registration successful! Please verify your email." }
                            if !verification_token.read().is_empty() {
                                div {
                                    style: "padding-top: .5rem; border-top: 1px solid rgba(168,230,207,0.15);",
                                    p {
                                        class: "muted",
                                        style: "font-size: .625rem; margin: 0 0 .25rem 0;",
                                        "Testing verification link:"
                                    }
                                    Link {
                                        to: crate::Route::VerifyEmail {},
                                        // Note: ideally we'd append ?token=... but Route::VerifyEmail
                                        // has no query param; using a raw href instead.
                                        onclick: {
                                            let tok = verification_token.read().clone();
                                            move |_| {
                                                if let Some(win) = web_sys::window() {
                                                    let href = format!("/auth/verify-email?token={}", tok);
                                                    let _ = win.location().set_href(&href);
                                                }
                                            }
                                        },
                                        style: "color: var(--pastel-yellow); font-weight: 600; font-size: .75rem;",
                                        "Verify Email ({verification_token.read().chars().take(8).collect::<String>()}...)"
                                    }
                                }
                            }
                        }
                    }

                    // Form (hidden after success)
                    if !*success.read() {
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

                            // Name field
                            div { style: "display: flex; flex-direction: column; gap: .375rem;",
                                label {
                                    r#for: "name",
                                    style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); font-family: monospace;",
                                    "Full Name"
                                }
                                input {
                                    id: "name",
                                    class: "app-input",
                                    style: "width: 100%; padding: .75rem 1rem;",
                                    r#type: "text",
                                    placeholder: "e.g. Olive Casazza",
                                    value: "{name}",
                                    oninput: move |e| name.clone().set(e.value()),
                                    onblur: move |_| name_touched.clone().set(true),
                                }
                                if *name_touched.read() && !name_error.is_empty() {
                                    p {
                                        class: "error",
                                        style: "font-size: .69rem; font-family: monospace; margin: 0;",
                                        "{name_error}"
                                    }
                                }
                            }

                            // Username field
                            div { style: "display: flex; flex-direction: column; gap: .375rem;",
                                label {
                                    r#for: "username",
                                    style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); font-family: monospace;",
                                    "Username"
                                }
                                input {
                                    id: "username",
                                    class: "app-input",
                                    style: "width: 100%; padding: .75rem 1rem;",
                                    r#type: "text",
                                    placeholder: "e.g. olivepasta",
                                    value: "{username}",
                                    oninput: move |e| username.clone().set(e.value()),
                                    onblur: on_username_blur.clone(),
                                }
                                if uname_touched_val {
                                    if !username_error.is_empty() {
                                        p { class: "error", style: "font-size: .69rem; font-family: monospace; margin: 0;", "{username_error}" }
                                    } else if uname_checking {
                                        p { class: "muted", style: "font-size: .69rem; font-family: monospace; margin: 0;", "Checking availability..." }
                                    } else if uname_len >= 3 && !uname_available {
                                        p { class: "error", style: "font-size: .69rem; font-family: monospace; margin: 0;", "Username is already taken." }
                                    } else if uname_len >= 3 && uname_available {
                                        p { class: "success", style: "font-size: .69rem; font-family: monospace; margin: 0;", "Username is available!" }
                                    }
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
                                    onblur: on_email_blur.clone(),
                                }
                                if email_touched_val {
                                    if !email_error.is_empty() {
                                        p { class: "error", style: "font-size: .69rem; font-family: monospace; margin: 0;", "{email_error}" }
                                    } else if email_checking {
                                        p { class: "muted", style: "font-size: .69rem; font-family: monospace; margin: 0;", "Checking availability..." }
                                    } else if email_len_ok && !email_available {
                                        p { class: "error", style: "font-size: .69rem; font-family: monospace; margin: 0;", "Email is already registered." }
                                    } else if email_len_ok && email_available {
                                        p { class: "success", style: "font-size: .69rem; font-family: monospace; margin: 0;", "Email is available!" }
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
                                if *loading.read() { "Creating..." } else { "Sign Up" }
                            }
                        }
                    }

                    // Footer
                    div {
                        style: "margin-top: 1.5rem; padding-top: 1.5rem; border-top: 1px solid var(--border-app); text-align: center;",
                        p {
                            class: "muted",
                            style: "font-size: .75rem; font-family: monospace; margin: 0;",
                            "Already have an account? "
                            Link {
                                to: crate::Route::Login {},
                                style: "color: var(--pastel-yellow);",
                                "Sign In"
                            }
                        }
                    }
                }
            },
        }
    };

    rsx! {
        style { {SIGNUP_CSS} }
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

const SIGNUP_CSS: &str = "";
