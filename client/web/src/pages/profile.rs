//! Account surface: Identity, Security, Subscription, Preferences, Danger zone.
//!
//! Built against backend contracts that may land after this page ships
//! (`user.changePassword`, `user.resendVerification`, `subscription.stop`,
//! `username` on `user.getProfile`, `currentPeriodEnd` on
//! `subscription.getStatus`). Everything new is Option-gated and a
//! "procedure not implemented" error renders the feature as unavailable
//! rather than broken.

use crossword_core::auth::Role;
use crossword_core::fmt::format_date;
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use panel_kit::{use_workspace, LayoutBuilder, Mode, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crate::components::identicon::Identicon;
use crate::components::pro_upgrade::ProUpgrade;
use crate::components::ui::{ConfirmModal, SectionTabs};
use crate::net;
use crate::store::{use_app_state, Severity, SubStatus};
use crate::Route;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Identity,
    Security,
    Subscription,
    Preferences,
    Danger,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Identity => "Identity",
            Panel::Security => "Security",
            Panel::Subscription => "Subscription",
            Panel::Preferences => "Preferences",
            Panel::Danger => "Danger Zone",
        }
    }
}

/// Vec order is the mobile stack order: Identity → Security → Subscription →
/// Preferences → Danger.
fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Identity, 16.0, 16.0, 640.0, 460.0),
        b.at(Panel::Security, 16.0, 492.0, 640.0, 472.0),
        b.at(Panel::Subscription, 672.0, 16.0, 608.0, 460.0),
        b.at(Panel::Preferences, 672.0, 492.0, 608.0, 472.0),
        b.at(Panel::Danger, 1296.0, 16.0, 608.0, 948.0),
    ]
}

/// The router's fallthrough for procedures that haven't landed yet
/// (see client/backend/server/src/routers/mod.rs). Substring-matched so the
/// UI degrades to "unavailable" instead of surfacing a raw error.
fn proc_missing(msg: &str) -> bool {
    msg.contains("procedure not implemented")
}

const FIELD_ERR: &str =
    "color: var(--pastel-red); font-size: .6875rem; font-family: monospace; margin: 0;";
const INLINE_OK: &str = "font-size: .75rem; font-family: monospace; padding: .875rem; border: 1px solid color-mix(in srgb, var(--pastel-green) 20%, transparent); background: color-mix(in srgb, var(--pastel-green) 6%, transparent);";
const INLINE_ERR: &str = "font-size: .75rem; font-family: monospace; padding: .875rem; border: 1px solid color-mix(in srgb, var(--pastel-red) 20%, transparent); background: color-mix(in srgb, var(--pastel-red) 6%, transparent);";

#[component]
pub fn Profile() -> Element {
    let state = use_app_state();

    // ── Identity ─────────────────────────────────────────────────────────────
    let mut name_input = use_signal(String::new);
    let mut display_name = use_signal(String::new);
    let mut updating = use_signal(|| false);
    let mut name_success = use_signal(String::new);
    let mut name_error = use_signal(String::new);
    // From `user.getProfile` (Option-gated: the field lands in parallel).
    let mut username: Signal<Option<String>> = use_signal(|| None);
    let mut resend_busy = use_signal(|| false);

    // ── Security ─────────────────────────────────────────────────────────────
    let mut cur_pw = use_signal(String::new);
    let mut new_pw = use_signal(String::new);
    let mut confirm_pw = use_signal(String::new);
    let mut show_cur = use_signal(|| false);
    let mut show_new = use_signal(|| false);
    let mut show_conf = use_signal(|| false);
    let mut pw_busy = use_signal(|| false);
    let mut pw_new_error = use_signal(String::new);
    let mut pw_confirm_error = use_signal(String::new);
    let mut pw_form_error = use_signal(String::new);
    let mut pw_unavailable = use_signal(|| false);

    // ── Subscription ─────────────────────────────────────────────────────────
    // Fresh `currentPeriodEnd` from `subscription.getStatus` (Option-gated:
    // AppState's copy doesn't carry it, so it's fetched raw here).
    let mut period_end: Signal<Option<String>> = use_signal(|| None);
    let mut cancel_open = use_signal(|| false);
    let mut cancel_busy = use_signal(|| false);

    // ── Preferences (client-only) ────────────────────────────────────────────
    let mut light = use_signal(|| {
        LocalStorage::get::<String>("theme")
            .map(|t| t == "light")
            .unwrap_or(false)
    });

    // ── Danger zone ──────────────────────────────────────────────────────────
    let mut danger_open = use_signal(|| false);
    let mut delete_input = use_signal(String::new);
    let mut deleting = use_signal(|| false);
    let mut delete_error = use_signal(String::new);

    // Backfill the name form from the session once it resolves (the signals
    // initialise on first render, which can happen before the session loads).
    {
        let session = state.session;
        use_effect(move || {
            if let Some(Some(u)) = session.read().clone() {
                if let Some(n) = u.name {
                    if display_name.peek().is_empty() {
                        display_name.set(n.clone());
                    }
                    if name_input.peek().is_empty() {
                        name_input.set(n);
                    }
                }
            }
        });
    }

    // One-shot fetch: fresh profile (username) + fresh subscription status
    // (currentPeriodEnd). Both best-effort — on error or missing fields the
    // page just omits the extras.
    use_hook(move || {
        spawn_local(async move {
            if let Ok(v) = net::query("user.getProfile", None).await {
                if let Some(u) = v.get("username").and_then(|x| x.as_str()) {
                    if !u.is_empty() {
                        username.set(Some(u.to_string()));
                    }
                }
                if let Some(n) = v.get("name").and_then(|x| x.as_str()) {
                    if !n.is_empty() {
                        display_name.set(n.to_string());
                        if name_input.peek().is_empty() {
                            name_input.set(n.to_string());
                        }
                    }
                }
            }
            if let Ok(v) = net::query("subscription.getStatus", None).await {
                if let Some(p) = v.get("currentPeriodEnd").and_then(|x| x.as_str()) {
                    period_end.set(Some(p.to_string()));
                }
            }
        });
    });

    let ws = use_workspace("profile_layout_v2", default_layout);
    crate::store::sync_panel_mode(ws.mode);
    // After all hooks so the hook order is stable across guard states.
    if let Some(gate) = crate::store::use_auth_guard(Role::User) {
        return gate;
    }

    // Guard passed ⇒ signed in.
    let Some(user) = state.user() else {
        return rsx! {};
    };
    let user_id = user.id.clone();
    let user_email = user.email.clone().unwrap_or_default();
    let email_verified = user.email_verified;
    let role_label = match user.role {
        Role::Admin => "Admin",
        _ => "Player",
    };

    let mut ws_mode = ws.mode;

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            // ── Identity ──────────────────────────────────────────────────────
            Panel::Identity => {
                let uid = user_id.clone();
                let email_label = user_email.clone();
                let email_for_resend = user_email.clone();
                let email_for_update = user_email.clone();
                let dn = display_name.read().clone();
                let uname = username.read().clone();
                rsx! {
                    div { class: "pf-panel",
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
                                div { class: "pf-avatar-circle",
                                    Identicon { seed: uid, size: 72 }
                                }
                                if email_verified {
                                    div {
                                        class: "pf-verified-badge",
                                        title: "Email Verified",
                                        "✓"
                                    }
                                }
                            }
                            div { style: "text-align: center;",
                                h2 { style: "font-weight: 700; font-size: 1.125rem; color: var(--text-primary); margin: 0 0 .25rem 0;", "{dn}" }
                                if let Some(u) = uname {
                                    p { class: "muted", style: "font-size: .75rem; font-family: monospace; margin: 0 0 .25rem 0;", "@{u}" }
                                }
                                p { class: "muted", style: "font-size: .75rem; font-family: monospace; margin: 0;", "{email_label}" }
                            }
                            div { class: "pf-meta-list",
                                div { class: "pf-meta-row",
                                    span { class: "muted pf-meta-label", "Account Type:" }
                                    span { style: "font-size: .625rem; font-family: monospace; font-weight: 600; text-transform: uppercase; color: var(--pastel-yellow);", "{role_label}" }
                                }
                                div { class: "pf-meta-row",
                                    span { class: "muted pf-meta-label", "Status:" }
                                    span {
                                        style: if email_verified {
                                            "font-size: .625rem; font-family: monospace; font-weight: 600; text-transform: uppercase; color: var(--pastel-green);"
                                        } else {
                                            "font-size: .625rem; font-family: monospace; font-weight: 600; text-transform: uppercase; color: var(--text-secondary);"
                                        },
                                        if email_verified { "Verified" } else { "Unverified" }
                                    }
                                }
                            }
                        }

                        // Unverified email → warning row + resend
                        if !email_verified {
                            div { class: "pf-warn-row",
                                span { "Your email address isn't verified yet." }
                                button {
                                    class: "app-btn",
                                    style: "font-size: .6875rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; white-space: nowrap;",
                                    disabled: resend_busy(),
                                    onclick: move |_| {
                                        let email = email_for_resend.clone();
                                        spawn_local(async move {
                                            resend_busy.set(true);
                                            match net::mutation(
                                                "user.resendVerification",
                                                Some(json!({ "email": email })),
                                            )
                                            .await
                                            {
                                                Ok(_) => state.toast(
                                                    Severity::Success,
                                                    "Verification email sent.",
                                                ),
                                                Err(e) => {
                                                    let msg = net::trpc_err_msg(e);
                                                    if proc_missing(&msg) {
                                                        state.toast(
                                                            Severity::Warning,
                                                            "Resending verification emails isn't available yet.",
                                                        );
                                                    } else {
                                                        state.toast(Severity::Error, msg);
                                                    }
                                                }
                                            }
                                            resend_busy.set(false);
                                        });
                                    },
                                    if resend_busy() { "Sending…" } else { "Resend verification" }
                                }
                            }
                        }

                        // Display-name edit
                        div { class: "app-card pf-form-card",
                            div {
                                h3 { class: "pf-heading", "Profile Settings" }
                                p { class: "muted pf-subheading", "Update your public identity details" }
                            }
                            form {
                                onsubmit: move |evt: Event<FormData>| {
                                    evt.stop_propagation();
                                    let email = email_for_update.clone();
                                    let name = name_input.read().trim().to_string();
                                    name_success.set(String::new());
                                    name_error.set(String::new());
                                    if name.is_empty() {
                                        name_error.set("Display name can't be empty.".into());
                                        return;
                                    }
                                    spawn_local(async move {
                                        updating.set(true);
                                        match net::mutation(
                                            "user.updateProfile",
                                            Some(json!({ "email": email, "name": name })),
                                        )
                                        .await
                                        {
                                            Ok(res) => {
                                                if let Some(new_name) =
                                                    res.get("name").and_then(|v| v.as_str())
                                                {
                                                    display_name.set(new_name.to_string());
                                                    // Keep the shared session in sync so the
                                                    // header reflects the new name without a
                                                    // full reload.
                                                    let mut session = state.session;
                                                    let mut guard = session.write();
                                                    if let Some(Some(u)) = guard.as_mut() {
                                                        u.name = Some(new_name.to_string());
                                                    }
                                                    drop(guard);
                                                }
                                                name_success
                                                    .set("Profile updated successfully!".into());
                                            }
                                            Err(e) => name_error.set(net::trpc_err_msg(e)),
                                        }
                                        updating.set(false);
                                    });
                                },
                                style: "display: flex; flex-direction: column; gap: 1rem;",
                                div { class: "pf-field",
                                    label { r#for: "profile-name", class: "pf-label", "Display Name" }
                                    input {
                                        id: "profile-name",
                                        class: "app-input",
                                        style: "width: 100%; padding: .75rem 1rem;",
                                        r#type: "text",
                                        required: true,
                                        placeholder: "e.g. Olive Casazza",
                                        value: "{name_input}",
                                        oninput: move |e| name_input.set(e.value()),
                                    }
                                    if !name_success.read().is_empty() {
                                        div { class: "success", style: INLINE_OK, "{name_success}" }
                                    }
                                    if !name_error.read().is_empty() {
                                        div { class: "error", style: INLINE_ERR, "{name_error}" }
                                    }
                                }
                                button {
                                    r#type: "submit",
                                    class: "app-btn app-btn-active",
                                    style: "width: 100%; justify-content: center; font-size: .875rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; padding: .75rem 1rem;",
                                    disabled: updating(),
                                    if updating() { "Saving..." } else { "Update Profile" }
                                }
                            }
                        }
                    }
                }
            }

            // ── Security ──────────────────────────────────────────────────────
            Panel::Security => rsx! {
                div { class: "pf-panel",
                    div {
                        h3 { class: "pf-heading", "Change Password" }
                        p { class: "muted pf-subheading", "Update the password you use to sign in" }
                    }
                    if pw_unavailable() {
                        div { class: "pf-warn-row",
                            span { "Changing your password isn't available yet. Check back soon." }
                        }
                    } else {
                        form {
                            onsubmit: move |evt: Event<FormData>| {
                                evt.stop_propagation();
                                pw_new_error.set(String::new());
                                pw_confirm_error.set(String::new());
                                pw_form_error.set(String::new());
                                let cur = cur_pw.read().clone();
                                let new = new_pw.read().clone();
                                let conf = confirm_pw.read().clone();
                                let mut bad = false;
                                if new.len() < 8 {
                                    pw_new_error
                                        .set("New password must be at least 8 characters.".into());
                                    bad = true;
                                }
                                if conf != new {
                                    pw_confirm_error.set("Passwords don't match.".into());
                                    bad = true;
                                }
                                if bad {
                                    return;
                                }
                                spawn_local(async move {
                                    pw_busy.set(true);
                                    match net::mutation(
                                        "user.changePassword",
                                        Some(json!({ "current": cur, "new": new })),
                                    )
                                    .await
                                    {
                                        Ok(_) => {
                                            state.toast(Severity::Success, "Password changed.");
                                            cur_pw.set(String::new());
                                            new_pw.set(String::new());
                                            confirm_pw.set(String::new());
                                        }
                                        Err(e) => {
                                            let msg = net::trpc_err_msg(e);
                                            if proc_missing(&msg) {
                                                pw_unavailable.set(true);
                                            } else if msg.to_lowercase().contains("no password") {
                                                pw_form_error.set(format!(
                                                    "{msg} Use \u{201c}Forgot password\u{201d} on the sign-in page to set one via an email reset instead."
                                                ));
                                            } else {
                                                pw_form_error.set(msg);
                                            }
                                        }
                                    }
                                    pw_busy.set(false);
                                });
                            },
                            style: "display: flex; flex-direction: column; gap: 1rem;",
                            div { class: "pf-field",
                                label { r#for: "pw-current", class: "pf-label", "Current Password" }
                                div { class: "pf-input-row",
                                    input {
                                        id: "pw-current",
                                        class: "app-input",
                                        style: "flex: 1; padding: .75rem 1rem;",
                                        r#type: if show_cur() { "text" } else { "password" },
                                        autocomplete: "current-password",
                                        value: "{cur_pw}",
                                        oninput: move |e| cur_pw.set(e.value()),
                                    }
                                    button {
                                        r#type: "button",
                                        class: "app-btn pf-show-btn",
                                        aria_label: "Toggle current password visibility",
                                        onclick: move |_| {
                                            let v = !show_cur();
                                            show_cur.set(v);
                                        },
                                        if show_cur() { "HIDE" } else { "SHOW" }
                                    }
                                }
                            }
                            div { class: "pf-field",
                                label { r#for: "pw-new", class: "pf-label", "New Password" }
                                div { class: "pf-input-row",
                                    input {
                                        id: "pw-new",
                                        class: "app-input",
                                        style: "flex: 1; padding: .75rem 1rem;",
                                        r#type: if show_new() { "text" } else { "password" },
                                        autocomplete: "new-password",
                                        value: "{new_pw}",
                                        oninput: move |e| {
                                            new_pw.set(e.value());
                                            pw_new_error.set(String::new());
                                        },
                                    }
                                    button {
                                        r#type: "button",
                                        class: "app-btn pf-show-btn",
                                        aria_label: "Toggle new password visibility",
                                        onclick: move |_| {
                                            let v = !show_new();
                                            show_new.set(v);
                                        },
                                        if show_new() { "HIDE" } else { "SHOW" }
                                    }
                                }
                                if !pw_new_error.read().is_empty() {
                                    p { style: FIELD_ERR, "{pw_new_error}" }
                                }
                            }
                            div { class: "pf-field",
                                label { r#for: "pw-confirm", class: "pf-label", "Confirm New Password" }
                                div { class: "pf-input-row",
                                    input {
                                        id: "pw-confirm",
                                        class: "app-input",
                                        style: "flex: 1; padding: .75rem 1rem;",
                                        r#type: if show_conf() { "text" } else { "password" },
                                        autocomplete: "new-password",
                                        value: "{confirm_pw}",
                                        oninput: move |e| {
                                            confirm_pw.set(e.value());
                                            pw_confirm_error.set(String::new());
                                        },
                                    }
                                    button {
                                        r#type: "button",
                                        class: "app-btn pf-show-btn",
                                        aria_label: "Toggle confirmation password visibility",
                                        onclick: move |_| {
                                            let v = !show_conf();
                                            show_conf.set(v);
                                        },
                                        if show_conf() { "HIDE" } else { "SHOW" }
                                    }
                                }
                                if !pw_confirm_error.read().is_empty() {
                                    p { style: FIELD_ERR, "{pw_confirm_error}" }
                                }
                            }
                            if !pw_form_error.read().is_empty() {
                                div { class: "error", style: INLINE_ERR, "{pw_form_error}" }
                            }
                            button {
                                r#type: "submit",
                                class: "app-btn app-btn-active",
                                style: "width: 100%; justify-content: center; font-size: .875rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; padding: .75rem 1rem;",
                                disabled: pw_busy(),
                                if pw_busy() { "Changing…" } else { "Change Password" }
                            }
                        }
                    }
                }
            },

            // ── Subscription ──────────────────────────────────────────────────
            Panel::Subscription => {
                let sub = state.sub.read().clone();
                let is_pro = sub.as_ref().map(|s| s.is_pro).unwrap_or(false);
                if !is_pro {
                    rsx! {
                        ProUpgrade {}
                    }
                } else {
                    let quota_used = sub.as_ref().map(|s| s.quota_used).unwrap_or(0);
                    let renew_label = period_end.read().as_deref().map(format_date);
                    let confirm_body = match &renew_label {
                        Some(d) => format!(
                            "Your Pro access stays active until {d}, then you'll move to the Free plan. No further charges."
                        ),
                        None => "Your Pro access stays active until the end of the current billing period, then you'll move to the Free plan.".to_string(),
                    };
                    rsx! {
                        div { class: "pf-panel",
                            div { class: "pf-row",
                                div { style: "display: flex; flex-direction: column; gap: .25rem;",
                                    span { class: "pf-meta-label muted", "Current Plan" }
                                    span { class: "pf-chip", "PRO · ACTIVE" }
                                }
                                div { style: "display: flex; flex-direction: column; gap: .25rem; align-items: flex-end; text-align: right;",
                                    span { class: "pf-meta-label muted", "Generations" }
                                    span { style: "font-size: .875rem; font-family: monospace;", "{quota_used} / \u{221e}" }
                                }
                            }
                            if let Some(d) = renew_label.clone() {
                                div { class: "pf-row",
                                    span { class: "pf-meta-label muted", "Renews" }
                                    span { style: "font-size: .875rem; font-family: monospace;", "{d}" }
                                }
                            }
                            button {
                                class: "app-btn",
                                style: "width: max-content; font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; border-color: var(--pastel-red); color: var(--pastel-red);",
                                onclick: move |_| cancel_open.set(true),
                                "Cancel subscription"
                            }
                            if cancel_open() {
                                ConfirmModal {
                                    title: "Cancel Pro?",
                                    body: confirm_body.clone(),
                                    confirm_label: "Cancel subscription",
                                    busy: cancel_busy(),
                                    on_confirm: move |_| {
                                        if cancel_busy() {
                                            return;
                                        }
                                        cancel_busy.set(true);
                                        spawn_local(async move {
                                            match net::mutation(
                                                "subscription.stop",
                                                Some(json!({})),
                                            )
                                            .await
                                            {
                                                Ok(_) => {
                                                    // Refetch: AppState.sub for the whole app,
                                                    // currentPeriodEnd for this page.
                                                    if let Ok(v) = net::query(
                                                        "subscription.getStatus",
                                                        None,
                                                    )
                                                    .await
                                                    {
                                                        if let Ok(s) = serde_json::from_value::<
                                                            SubStatus,
                                                        >(
                                                            v.clone()
                                                        ) {
                                                            let mut sub_sig = state.sub;
                                                            sub_sig.set(Some(s));
                                                        }
                                                        period_end.set(
                                                            v.get("currentPeriodEnd")
                                                                .and_then(|x| x.as_str())
                                                                .map(String::from),
                                                        );
                                                    }
                                                    state.toast(
                                                        Severity::Success,
                                                        "Subscription cancelled. Pro stays active until the end of the paid period.",
                                                    );
                                                }
                                                Err(e) => {
                                                    let msg = net::trpc_err_msg(e);
                                                    if proc_missing(&msg) {
                                                        state.toast(
                                                            Severity::Warning,
                                                            "Cancelling from the app isn't available yet.",
                                                        );
                                                    } else {
                                                        // May be a portal instruction —
                                                        // show it verbatim.
                                                        state.toast(Severity::Error, msg);
                                                    }
                                                }
                                            }
                                            cancel_busy.set(false);
                                            cancel_open.set(false);
                                        });
                                    },
                                    on_cancel: move |_| cancel_open.set(false),
                                }
                            }
                        }
                    }
                }
            }

            // ── Preferences (client-only) ─────────────────────────────────────
            Panel::Preferences => {
                let theme_active = if light() { 1 } else { 0 };
                let mode_active = match *ws_mode.read() {
                    Mode::Tiling => 0,
                    Mode::Floating => 1,
                };
                rsx! {
                    div { class: "pf-panel",
                        div { class: "pf-field",
                            span { class: "pf-label", "Theme" }
                            SectionTabs {
                                tabs: vec!["Dark".to_string(), "Light".to_string()],
                                active: theme_active,
                                on_select: move |i: usize| {
                                    let l = i == 1;
                                    light.set(l);
                                    crate::set_light_class(l);
                                },
                            }
                            p { class: "muted pf-subheading", "Applies immediately and persists on this device." }
                        }
                        div { class: "pf-field",
                            span { class: "pf-label", "Default Panel Mode" }
                            SectionTabs {
                                tabs: vec!["Tiling".to_string(), "Floating".to_string()],
                                active: mode_active,
                                on_select: move |i: usize| {
                                    let m = if i == 0 { Mode::Tiling } else { Mode::Floating };
                                    ws_mode.set(m);
                                    let _ = LocalStorage::set(
                                        "panel_mode",
                                        if i == 0 { "tiling" } else { "floating" },
                                    );
                                },
                            }
                            p { class: "muted pf-subheading", "How workspace panels arrange, here and on every other view." }
                        }
                    }
                }
            }

            // ── Danger zone ───────────────────────────────────────────────────
            Panel::Danger => {
                let email_label = user_email.clone();
                let email_for_delete = user_email.clone();
                let ready = delete_input
                    .read()
                    .trim()
                    .eq_ignore_ascii_case(email_label.trim());
                rsx! {
                    div { class: "pf-panel",
                        div { class: "app-card pf-danger-card",
                            div {
                                h3 { class: "pf-heading", style: "color: var(--pastel-red);", "Danger Zone" }
                                p { class: "muted pf-subheading", "Permanently remove your account and all associated data" }
                            }
                            if !danger_open() {
                                button {
                                    class: "app-btn",
                                    style: "width: max-content; font-size: .75rem; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; border-color: var(--pastel-red); color: var(--pastel-red);",
                                    onclick: move |_| danger_open.set(true),
                                    "Delete account…"
                                }
                            } else {
                                div { class: "pf-danger-confirm",
                                    p { class: "muted", style: "font-size: .75rem; line-height: 1.6; margin: 0;",
                                        "This action is irreversible. All of your stats, generation jobs, and account references will be deleted forever."
                                    }
                                    div { class: "pf-field",
                                        label { r#for: "pf-delete-confirm", class: "pf-label",
                                            "Type your email to confirm"
                                        }
                                        p { class: "muted", style: "font-size: .6875rem; font-family: monospace; margin: 0;", "{email_label}" }
                                        input {
                                            id: "pf-delete-confirm",
                                            class: "app-input",
                                            style: "width: 100%; padding: .75rem 1rem;",
                                            r#type: "email",
                                            autocomplete: "off",
                                            placeholder: "you@example.com",
                                            value: "{delete_input}",
                                            oninput: move |e| delete_input.set(e.value()),
                                        }
                                    }
                                    if !delete_error.read().is_empty() {
                                        div { class: "error", style: INLINE_ERR, "{delete_error}" }
                                    }
                                    div { style: "display: flex; flex-wrap: wrap; gap: .75rem;",
                                        button {
                                            class: "app-btn app-btn-active",
                                            style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; padding: .625rem 1rem; background: var(--pastel-red); border-color: var(--pastel-red); color: var(--contrast-ink);",
                                            disabled: !ready || deleting(),
                                            onclick: move |_| {
                                                let email = email_for_delete.clone();
                                                spawn_local(async move {
                                                    deleting.set(true);
                                                    delete_error.set(String::new());
                                                    match net::mutation(
                                                        "user.deleteAccount",
                                                        Some(json!({ "email": email })),
                                                    )
                                                    .await
                                                    {
                                                        Ok(_) => {
                                                            if let Some(win) = web_sys::window() {
                                                                let _ = win
                                                                    .location()
                                                                    .set_href("/auth/signup");
                                                            }
                                                        }
                                                        Err(e) => {
                                                            delete_error
                                                                .set(net::trpc_err_msg(e));
                                                            deleting.set(false);
                                                        }
                                                    }
                                                });
                                            },
                                            if deleting() { "Deleting..." } else { "Delete Account" }
                                        }
                                        button {
                                            class: "app-btn",
                                            style: "font-size: .75rem; font-weight: 600; text-transform: uppercase; padding: .625rem 1rem;",
                                            disabled: deleting(),
                                            onclick: move |_| {
                                                danger_open.set(false);
                                                delete_input.set(String::new());
                                                delete_error.set(String::new());
                                            },
                                            "Cancel"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
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
.pf-panel {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    height: 100%;
    overflow-y: auto;
}
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
    background: linear-gradient(to top right, var(--pastel-yellow), color-mix(in srgb, var(--pastel-yellow) 30%, transparent));
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 1.875rem;
    color: var(--contrast-ink);
    border: 4px solid color-mix(in srgb, var(--text-primary) 5%, transparent);
    text-transform: uppercase;
    user-select: none;
}
.pf-verified-badge {
    position: absolute;
    bottom: 0;
    right: 0;
    width: 1.5rem;
    height: 1.5rem;
    background: var(--pastel-green);
    color: var(--contrast-ink);
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
.pf-meta-label {
    font-size: .625rem;
    font-family: monospace;
    text-transform: uppercase;
    letter-spacing: .05em;
}
.pf-heading {
    font-size: 1.125rem;
    font-weight: 700;
    font-family: monospace;
    text-transform: uppercase;
    letter-spacing: .05em;
    color: var(--text-primary);
    margin: 0 0 .25rem 0;
}
.pf-subheading {
    font-size: .75rem;
    font-family: monospace;
    margin: 0;
}
.pf-form-card {
    padding: 1.5rem 2rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
}
.pf-field {
    display: flex;
    flex-direction: column;
    gap: .375rem;
}
.pf-label {
    font-size: .75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: .05em;
    color: var(--text-secondary);
    font-family: monospace;
}
.pf-input-row {
    display: flex;
    gap: .5rem;
}
.pf-show-btn {
    font-size: .625rem;
    font-weight: 600;
    font-family: monospace;
    letter-spacing: .05em;
    white-space: nowrap;
}
.pf-warn-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: .75rem;
    padding: .75rem .875rem;
    border: 1px solid color-mix(in srgb, var(--pastel-yellow) 35%, transparent);
    background: color-mix(in srgb, var(--pastel-yellow) 8%, transparent);
    font-size: .75rem;
    font-family: monospace;
}
.pf-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: .875rem;
    border: 1px solid var(--border-app);
    background: color-mix(in srgb, var(--bg-app) 50%, transparent);
}
.pf-chip {
    display: inline-block;
    width: max-content;
    padding: .25rem .625rem;
    font-size: .625rem;
    font-weight: 700;
    font-family: monospace;
    text-transform: uppercase;
    letter-spacing: .08em;
    background: color-mix(in srgb, var(--pastel-green) 12%, transparent);
    color: var(--pastel-green);
    border: 1px solid var(--pastel-green);
}
.pf-danger-card {
    padding: 1.5rem 2rem;
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    border-color: color-mix(in srgb, var(--pastel-red) 15%, transparent);
    background: color-mix(in srgb, var(--pastel-red) 3%, transparent);
}
.pf-danger-confirm {
    padding: 1rem;
    border: 1px solid color-mix(in srgb, var(--pastel-red) 20%, transparent);
    background: color-mix(in srgb, var(--pastel-red) 5%, transparent);
    display: flex;
    flex-direction: column;
    gap: 1rem;
}
"#;
