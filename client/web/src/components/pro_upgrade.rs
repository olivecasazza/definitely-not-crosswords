use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crate::{net, store::use_app_state};

// ── component-scoped CSS ──────────────────────────────────────────────────────

const CSS: &str = r#"
.pro-upgrade-card {
  padding: 1.5rem;
  border-radius: 1rem;
  background: rgba(24,24,27,0.6);
  backdrop-filter: blur(12px);
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}
.pro-upgrade-card .plan-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: .875rem;
  border-radius: .75rem;
  border: 1px solid var(--border-app);
  background: rgba(18,18,18,0.5);
}
.pro-upgrade-card .plan-label {
  font-size: .625rem;
  font-family: var(--mono);
  text-transform: uppercase;
  letter-spacing: .08em;
  color: var(--text-secondary);
}
.pro-upgrade-card .plan-name {
  font-size: .875rem;
  font-weight: 700;
}
.pro-upgrade-card .plan-name.is-pro {
  color: var(--pastel-green);
}
.pro-upgrade-card .pro-chip {
  display: inline-block;
  padding: .125rem .5rem;
  border-radius: .25rem;
  font-size: .625rem;
  font-weight: 700;
  text-transform: uppercase;
  background: var(--color-success);
  color: #0f172a;
}
.pro-upgrade-card .code-section {
  display: flex;
  flex-direction: column;
  gap: .375rem;
}
.pro-upgrade-card .code-label {
  font-size: .75rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: .08em;
  color: var(--text-secondary);
  font-family: var(--mono);
}
.pro-upgrade-card .code-label span {
  text-transform: none;
  opacity: .6;
  font-weight: 400;
}
.pro-upgrade-card .code-row {
  display: flex;
  gap: .5rem;
}
.pro-upgrade-card .code-input {
  flex: 1;
  text-transform: uppercase;
}
.pro-upgrade-card .upgrade-btn {
  width: 100%;
  padding: .75rem 1rem;
  border-radius: .75rem;
  font-weight: 600;
  font-size: .875rem;
  letter-spacing: .08em;
  text-transform: uppercase;
  background: linear-gradient(to right, var(--pastel-yellow), rgba(254,234,153,0.7));
  color: #0f172a;
  border: none;
  cursor: pointer;
  transition: transform .15s ease, opacity .15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: .5rem;
}
.pro-upgrade-card .upgrade-btn:hover { transform: scale(1.02); }
.pro-upgrade-card .upgrade-btn:active { transform: scale(0.98); }
.pro-upgrade-card .upgrade-btn:disabled { opacity: .5; cursor: not-allowed; transform: none; }
@keyframes pu-spin { to { transform: rotate(360deg); } }
.pro-upgrade-card .spin-ring {
  display: inline-block;
  width: 1rem;
  height: 1rem;
  border: 2px solid #0f172a;
  border-top-color: transparent;
  border-radius: 50%;
  animation: pu-spin .7s linear infinite;
}
"#;

// ── discount validate response ────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscountInfo {
    code: String,
    name: String,
    amount_type: String, // "PERCENT" | "FIXED"
    amount: i64,
    duration: String, // "ONCE" | "FOREVER" | "REPEATING"
}

#[derive(Debug, Clone, Deserialize)]
struct ValidateResponse {
    valid: bool,
    reason: Option<String>,
    discount: Option<DiscountInfo>,
}

// ── checkout response ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckoutResponse {
    checkout_url: String,
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn discount_label(d: &DiscountInfo) -> String {
    if d.amount_type == "PERCENT" {
        format!("{}% off", d.amount)
    } else {
        format!("${:.2} off", d.amount as f64 / 100.0)
    }
}

// ── component ─────────────────────────────────────────────────────────────────

#[component]
pub fn ProUpgrade() -> Element {
    let state = use_app_state();

    // Discount-code form
    let mut code = use_signal(String::new);
    let mut validating = use_signal(|| false);
    let mut code_error = use_signal(String::new);
    let mut applied_discount: Signal<Option<DiscountInfo>> = use_signal(|| None);

    // Checkout state
    let mut upgrading = use_signal(|| false);
    let mut checkout_error = use_signal(String::new);

    // ── apply-code handler ────────────────────────────────────────────────────
    let mut apply_code = move |_: ()| {
        let value = code.read().trim().to_uppercase();
        code_error.set(String::new());
        applied_discount.set(None);
        if value.is_empty() {
            return;
        }
        validating.set(true);
        spawn_local(async move {
            match net::query_as::<ValidateResponse>(
                "discount.validate",
                Some(json!({ "code": value })),
            )
            .await
            {
                Ok(res) => {
                    if res.valid {
                        if let Some(d) = res.discount {
                            code.set(d.code.clone());
                            applied_discount.set(Some(d));
                        }
                    } else {
                        code_error.set(
                            res.reason
                                .unwrap_or_else(|| "This code is not valid.".to_string()),
                        );
                    }
                }
                Err(e) => {
                    code_error.set(format!("Could not validate code: {e}"));
                }
            }
            validating.set(false);
        });
    };

    // ── upgrade handler ───────────────────────────────────────────────────────
    let upgrade = move |_| {
        checkout_error.set(String::new());
        upgrading.set(true);
        // Only send the validated discount code, not the raw typed value.
        let discount_code = applied_discount.read().as_ref().map(|d| d.code.clone());
        spawn_local(async move {
            let body = match discount_code {
                Some(ref c) => json!({ "discountCode": c }),
                None => json!({}),
            };

            let req = match gloo_net::http::Request::post("/api/checkout")
                .header("content-type", "application/json")
                .body(body.to_string())
            {
                Ok(r) => r,
                Err(e) => {
                    checkout_error.set(format!("Could not start checkout: {e}"));
                    upgrading.set(false);
                    return;
                }
            };

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    checkout_error.set(format!("Could not start checkout: {e}"));
                    upgrading.set(false);
                    return;
                }
            };

            if !resp.ok() {
                checkout_error.set(format!(
                    "Could not start checkout (HTTP {}).",
                    resp.status()
                ));
                upgrading.set(false);
                return;
            }

            match resp.json::<CheckoutResponse>().await {
                Ok(data) => {
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().set_href(&data.checkout_url);
                    }
                }
                Err(e) => {
                    checkout_error.set(format!("Could not parse checkout response: {e}"));
                }
            }

            upgrading.set(false);
        });
    };

    // ── read current sub state ────────────────────────────────────────────────
    let sub = state.sub.read();
    let is_pro = sub.as_ref().map(|s| s.is_pro).unwrap_or(false);
    let quota_used = sub.as_ref().map(|s| s.quota_used).unwrap_or(0);
    let quota_limit = sub.as_ref().and_then(|s| s.quota_limit);

    let quota_str = {
        let limit = match quota_limit {
            Some(l) => l.to_string(),
            None => "\u{221e}".to_string(), // ∞
        };
        format!("{quota_used} / {limit}")
    };

    let discount_msg = applied_discount.read().as_ref().map(|d| {
        let label = discount_label(d);
        let once_note = if d.duration == "ONCE" {
            " on your first payment"
        } else {
            ""
        };
        format!("{} applied — {}{once_note}.", d.name, label)
    });

    rsx! {
        style { {CSS} }

        div { class: "pro-upgrade-card app-card",
            // Header
            div { class: "col",
                h3 {
                    style: "margin:0; font-size: 1.125rem; font-weight: 700; font-family: var(--mono); text-transform: uppercase; letter-spacing: .08em;",
                    "Subscription"
                }
                p { class: "muted",
                    style: "margin:0; font-size: .75rem; font-family: var(--mono);",
                    "Unlock unlimited puzzle generation with Pro"
                }
            }

            // Current plan row
            div { class: "plan-row",
                div { class: "col", style: "gap: .125rem;",
                    span { class: "plan-label", "Current Plan" }
                    span {
                        class: if is_pro { "plan-name is-pro" } else { "plan-name" },
                        if is_pro { "Pro" } else { "Free" }
                    }
                }
                if is_pro {
                    span { class: "pro-chip", "active" }
                } else {
                    div { class: "col", style: "gap: .125rem; text-align: right; align-items: flex-end;",
                        span { class: "plan-label", "Generations" }
                        span {
                            style: "font-size: .875rem; font-family: var(--mono);",
                            "{quota_str}"
                        }
                    }
                }
            }

            // Upsell section — only when not Pro
            if !is_pro {
                // Discount code
                div { class: "code-section",
                    label { class: "code-label",
                        "Discount code "
                        span { "(optional)" }
                    }
                    div { class: "code-row",
                        input {
                            r#type: "text",
                            class: "app-input code-input",
                            placeholder: "LAUNCH50",
                            value: "{code}",
                            oninput: move |e| {
                                code.set(e.value());
                                code_error.set(String::new());
                                applied_discount.set(None);
                            },
                            onkeydown: move |e: KeyboardEvent| {
                                if e.key() == Key::Enter {
                                    apply_code(());
                                }
                            },
                        }
                        button {
                            r#type: "button",
                            class: "app-btn",
                            disabled: code.read().trim().is_empty() || *validating.read(),
                            onclick: move |_| apply_code(()),
                            if *validating.read() { "\u{2026}" } else { "Apply" }
                        }
                    }
                    if !code_error.read().is_empty() {
                        p { class: "error",
                            style: "margin: 0; font-size: .6875rem; font-family: var(--mono); padding-left: .25rem;",
                            "{code_error}"
                        }
                    } else if let Some(msg) = &discount_msg {
                        p { class: "success",
                            style: "margin: 0; font-size: .6875rem; font-family: var(--mono); padding-left: .25rem;",
                            "{msg}"
                        }
                    }
                }

                // Upgrade button
                button {
                    r#type: "button",
                    class: "upgrade-btn",
                    disabled: *upgrading.read(),
                    onclick: upgrade,
                    if *upgrading.read() {
                        span { class: "spin-ring" }
                        "Opening checkout\u{2026}"
                    } else {
                        "Upgrade to Pro"
                    }
                }
                if !checkout_error.read().is_empty() {
                    p { class: "error",
                        style: "margin: 0; font-size: .6875rem; font-family: var(--mono); padding-left: .25rem;",
                        "{checkout_error}"
                    }
                }
            }
        }
    }
}
