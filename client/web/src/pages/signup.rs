use crate::components::auth_layout::AuthLayout;
use crate::net;
use dioxus::prelude::*;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

#[component]
pub fn Signup() -> Element {
    // Already signed in? Straight to the stashed destination (or Home).
    crate::store::use_guest_only();

    let name = use_signal(|| String::new());
    let username = use_signal(|| String::new());
    let email = use_signal(|| String::new());
    let password = use_signal(|| String::new());
    let confirm = use_signal(|| String::new());

    let name_touched = use_signal(|| false);
    let username_touched = use_signal(|| false);
    let email_touched = use_signal(|| false);
    let password_touched = use_signal(|| false);
    let confirm_touched = use_signal(|| false);

    let loading = use_signal(|| false);
    let error = use_signal(|| String::new());
    let success = use_signal(|| false);

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
    let confirm_val = confirm.read().clone();

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
    } else if password_val.len() < 8 {
        "Password must be at least 8 characters.".to_string()
    } else {
        String::new()
    };

    // A typo here means a trip through the email reset flow — confirming is
    // the cheap guard against that.
    let confirm_error = if confirm_val.is_empty() {
        "Please re-enter your password.".to_string()
    } else if confirm_val != password_val {
        "Passwords do not match.".to_string()
    } else {
        String::new()
    };

    let is_invalid = !name_error.is_empty()
        || !username_error.is_empty()
        || !email_error.is_empty()
        || !password_error.is_empty()
        || !confirm_error.is_empty()
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
        let confirm = confirm.clone();
        let mut name_touched = name_touched.clone();
        let mut username_touched = username_touched.clone();
        let mut email_touched = email_touched.clone();
        let mut password_touched = password_touched.clone();
        let mut confirm_touched = confirm_touched.clone();
        let loading = loading.clone();
        let error = error.clone();
        let success = success.clone();
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
            confirm_touched.set(true);

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
            let mut confirm = confirm.clone();
            let mut loading = loading.clone();
            let mut error = error.clone();
            let mut success = success.clone();

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
                            success.set(true);
                            name.set(String::new());
                            username.set(String::new());
                            email.set(String::new());
                            password.set(String::new());
                            confirm.set(String::new());
                        } else {
                            // success:false must never be a silent no-op —
                            // surface the server's message, or a generic one.
                            let msg = res["message"].as_str().unwrap_or("").to_string();
                            error.set(if msg.is_empty() {
                                "Something went wrong — please try again.".to_string()
                            } else {
                                msg
                            });
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

    rsx! {
        AuthLayout {
            eyebrow: "CREATE ACCOUNT",
            show_brand: true,
            subtitle: "Solve together.",

            p {
                class: "muted auth-note",
                style: "text-align: center;",
                "Join the \"Definitely Not Crosswords\" experience"
            }

            // Success state
            if *success.read() {
                div {
                    class: "success auth-banner auth-banner-ok",
                    style: "display: flex; flex-direction: column; gap: .5rem;",
                    // Login isn't gated on verification, so make both
                    // halves clear: link sent, but sign-in works already.
                    p { style: "margin: 0;", "Account created! We've sent a verification link to your email — you can sign in right away." }
                    Link {
                        to: crate::Route::Login {},
                        class: "app-btn app-btn-active",
                        style: "text-align: center; margin-top: .25rem;",
                        "Sign In"
                    }
                }
            }

            // Form (hidden after success)
            if !*success.read() {
                form {
                    class: "auth-form",
                    onsubmit: handle_submit,

                    // Error alert
                    if !error.read().is_empty() {
                        div { class: "error auth-banner", "{error}" }
                    }

                    // Name field
                    div { class: "auth-group",
                        label { r#for: "name", class: "auth-label", "Full Name" }
                        input {
                            id: "name",
                            class: "app-input auth-field",
                            r#type: "text",
                            placeholder: "e.g. Olive Casazza",
                            value: "{name}",
                            oninput: move |e| name.clone().set(e.value()),
                            onblur: move |_| name_touched.clone().set(true),
                        }
                        if *name_touched.read() && !name_error.is_empty() {
                            p { class: "error auth-hint", "{name_error}" }
                        }
                    }

                    // Username field
                    div { class: "auth-group",
                        label { r#for: "username", class: "auth-label", "Username" }
                        input {
                            id: "username",
                            class: "app-input auth-field",
                            r#type: "text",
                            placeholder: "e.g. olivepasta",
                            value: "{username}",
                            oninput: move |e| username.clone().set(e.value()),
                            onblur: on_username_blur,
                        }
                        if uname_touched_val {
                            if !username_error.is_empty() {
                                p { class: "error auth-hint", "{username_error}" }
                            } else if uname_checking {
                                p { class: "muted auth-hint", "Checking availability..." }
                            } else if uname_len >= 3 && !uname_available {
                                p { class: "error auth-hint", "Username is already taken." }
                            } else if uname_len >= 3 && uname_available {
                                p { class: "success auth-hint", "Username is available!" }
                            }
                        }
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
                            onblur: on_email_blur,
                        }
                        if email_touched_val {
                            if !email_error.is_empty() {
                                p { class: "error auth-hint", "{email_error}" }
                            } else if email_checking {
                                p { class: "muted auth-hint", "Checking availability..." }
                            } else if email_len_ok && !email_available {
                                p { class: "error auth-hint", "Email is already registered." }
                            } else if email_len_ok && email_available {
                                p { class: "success auth-hint", "Email is available!" }
                            }
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

                    // Confirm password field
                    div { class: "auth-group",
                        label { r#for: "confirm-password", class: "auth-label", "Confirm Password" }
                        input {
                            id: "confirm-password",
                            class: "app-input auth-field",
                            r#type: "password",
                            placeholder: "••••••••",
                            value: "{confirm}",
                            oninput: move |e| confirm.clone().set(e.value()),
                            onblur: move |_| confirm_touched.clone().set(true),
                        }
                        if *confirm_touched.read() && !confirm_error.is_empty() {
                            p { class: "error auth-hint", "{confirm_error}" }
                        }
                    }

                    // Submit
                    button {
                        r#type: "submit",
                        class: "app-btn app-btn-active auth-submit",
                        disabled: *loading.read() || is_invalid,
                        if *loading.read() { "Creating..." } else { "Sign Up" }
                    }
                }
            }

            // Footer
            div { class: "auth-foot",
                p { class: "muted auth-note",
                    "Already have an account? "
                    Link {
                        to: crate::Route::Login {},
                        style: "color: var(--pastel-yellow);",
                        "Sign In"
                    }
                }
            }
        }
    }
}
