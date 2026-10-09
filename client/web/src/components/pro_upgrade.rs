use dioxus::prelude::*;
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crate::store::use_app_state;

// ── component-scoped CSS ──────────────────────────────────────────────────────

const CSS: &str = r#"
.pro-upgrade {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
  height: 100%;
}
.pro-upgrade .plan-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: .875rem;
  border: 1px solid var(--border-app);
  background: color-mix(in srgb, var(--bg-app) 50%, transparent);
}
.pro-upgrade .plan-label {
  font-size: var(--fs-2xs);
  font-family: var(--mono);
  text-transform: uppercase;
  letter-spacing: .08em;
  color: var(--text-secondary);
}
.pro-upgrade .plan-name {
  font-size: var(--fs-sm);
  font-weight: 700;
}
.pro-upgrade .plan-name.is-pro {
  color: var(--pastel-green);
}
.pro-upgrade .pro-chip {
  display: inline-block;
  padding: .125rem .5rem;
  font-size: var(--fs-2xs);
  font-weight: 700;
  text-transform: uppercase;
  background: var(--color-success);
  color: var(--contrast-ink);
}
.pro-upgrade .upgrade-btn {
  width: 100%;
  padding: .75rem 1rem;
  font-weight: 600;
  font-size: var(--fs-md);
  letter-spacing: .08em;
  text-transform: uppercase;
  /* Solid, not the old two-stop gradient: the label is --contrast-ink, and
     the gradient's 70%-alpha end lifted the fill to #9a8348 in light mode —
     3.7:1 under white text. Every other accent button in the app is solid
     for exactly this reason. */
  background: var(--pastel-yellow);
  color: var(--contrast-ink);
  border: none;
  cursor: pointer;
  transition: transform .15s ease, opacity .15s ease;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: .5rem;
}
.pro-upgrade .upgrade-btn:hover { transform: scale(1.02); }
.pro-upgrade .upgrade-btn:active { transform: scale(0.98); }
.pro-upgrade .upgrade-btn:disabled { opacity: .5; cursor: not-allowed; transform: none; }
@keyframes pu-spin { to { transform: rotate(360deg); } }
.pro-upgrade .spin-ring {
  display: inline-block;
  width: 1rem;
  height: 1rem;
  border: 2px solid var(--contrast-ink);
  border-top-color: transparent;
  animation: pu-spin .7s linear infinite;
}
"#;

// ── checkout response ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckoutResponse {
    checkout_url: String,
}

// ── component ─────────────────────────────────────────────────────────────────

#[component]
pub fn ProUpgrade() -> Element {
    let state = use_app_state();

    // Checkout state
    let mut upgrading = use_signal(|| false);
    let mut checkout_error = use_signal(String::new);

    // ── upgrade handler ───────────────────────────────────────────────────────
    let upgrade = move |_| {
        checkout_error.set(String::new());
        upgrading.set(true);
        spawn_local(async move {
            let body = json!({});

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
                // Never surface the raw status. A 500 here is this deployment
                // having no Lemon Squeezy credentials, which reads as a bug
                // rather than the honest "unavailable" state it is.
                checkout_error
                    .set("Checkout is unavailable right now — please try again later.".to_string());
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
    // The purchase button needs a non-Pro subscriber AND a deployment that can
    // actually start a checkout. `proCheckout` mirrors the chart's
    // `billing.lemonSqueezy.enabled`, which gates the LEMONSQUEEZY_* injection
    // — so the button can no longer render where POST /api/checkout 500s
    // (DEF-166).
    let pro_checkout = state.feature(|f| f.pro_checkout);

    let quota_str = {
        let limit = match quota_limit {
            Some(l) => l.to_string(),
            None => "\u{221e}".to_string(), // ∞
        };
        format!("{quota_used} / {limit}")
    };

    rsx! {
        style { {CSS} }

        div { class: "pro-upgrade",
            // Header
            div { class: "col",
                p { class: "muted",
                    style: "margin:0; font-size: var(--fs-xs); font-family: var(--mono);",
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
                            style: "font-size: var(--fs-sm); font-family: var(--mono);",
                            "{quota_str}"
                        }
                    }
                }
            }

            // Upsell section — only when not Pro and checkout is possible
            if !is_pro && pro_checkout {
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
                        "Upgrade to Pro — $10/year"
                    }
                }
                if !checkout_error.read().is_empty() {
                    p { class: "error",
                        style: "margin: 0; font-size: var(--fs-xs); font-family: var(--mono); padding-left: .25rem;",
                        "{checkout_error}"
                    }
                }
            }
            // Same plan row, no purchase control: the price stays visible, the
            // offer does not pretend to be available.
            if !is_pro && !pro_checkout {
                p { class: "muted",
                    style: "margin: 0; font-size: var(--fs-xs); font-family: var(--mono);",
                    "Pro — $10/year: unlimited generation, teams of 10. Opening soon."
                }
            }
        }
    }
}
