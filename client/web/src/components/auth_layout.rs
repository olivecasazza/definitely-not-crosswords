//! Static centered layout shared by the auth pages (login, signup, verify
//! email, reset password). First impressions want stillness — no panel-kit
//! workspaces, no draggable windows, just a centered card.
//!
//! Owns the shared auth form styles (`.auth-*` classes below), seeded from the
//! style consts that used to live in `reset_password.rs`. Square corners,
//! tokens only.

use crate::components::brand::{brand_panel, BrandLogo};
use dioxus::prelude::*;

/// Injected once per auth page (only one `AuthLayout` is ever mounted at a
/// time). Layout + the shared field/label/error/button rules.
const AUTH_CSS: &str = r#"
.auth-shell { flex: 1 1 auto; min-height: 100%; overflow-y: auto;
  display: flex; align-items: center; justify-content: center; padding: 2rem 1rem; }
.auth-frame { display: flex; flex-direction: row; align-items: center; justify-content: center;
  gap: 3rem; width: 100%; max-width: 60rem; margin: 0 auto; }
.auth-brand-pc { flex: 1 1 0; min-width: 0; }
.auth-brand-mobile { display: none; }
.auth-wordmark { font-family: var(--mono, monospace); font-size: 1rem; font-weight: 800;
  color: var(--text-primary); }
.auth-card { width: 24rem; max-width: 100%; flex: 0 0 auto; padding: 1.5rem;
  display: flex; flex-direction: column; gap: 1.25rem; }
.auth-card-solo { width: 26rem; }
.auth-eyebrow { font-family: var(--mono, monospace); font-size: var(--fs-2xs); font-weight: 700;
  text-transform: uppercase; letter-spacing: .12em; color: var(--text-secondary); margin: 0; }

.auth-form { display: flex; flex-direction: column; gap: 1rem; }
.auth-group { display: flex; flex-direction: column; gap: .375rem; }
.auth-label { font-size: .75rem; font-family: var(--mono, monospace); font-weight: 700;
  text-transform: uppercase; letter-spacing: .05em; color: var(--text-secondary); }
.auth-field { width: 100%; padding: .625rem .75rem; font-size: .875rem; min-height: 44px; }
.auth-submit { width: 100%; padding: .75rem 1rem; font-weight: 600; font-size: .875rem;
  text-transform: uppercase; letter-spacing: .05em; min-height: 44px; }
.auth-hint { font-size: .69rem; font-family: var(--mono, monospace); margin: 0; }
.auth-note { font-size: .75rem; font-family: var(--mono, monospace); margin: 0; }
.auth-banner { font-size: .75rem; font-family: var(--mono, monospace); padding: .75rem;
  border: 1px solid color-mix(in srgb, var(--pastel-red) 20%, transparent);
  background: color-mix(in srgb, var(--pastel-red) 6%, transparent); }
.auth-banner-ok { border-color: color-mix(in srgb, var(--pastel-green) 20%, transparent);
  background: color-mix(in srgb, var(--pastel-green) 6%, transparent); }
.auth-link { font-size: .75rem; font-family: var(--mono, monospace); text-align: center;
  text-decoration: underline; }
.auth-divider { display: flex; align-items: center; gap: .75rem;
  font-size: .75rem; font-family: var(--mono, monospace); }
.auth-divider::before, .auth-divider::after { content: ""; flex: 1; height: 1px;
  background: var(--border-app); }
.auth-foot { padding-top: 1.25rem; border-top: 1px solid var(--border-app); text-align: center; }

@media (max-width: 760px) {
  .auth-shell { padding: 1.5rem 1rem; align-items: flex-start; }
  .auth-frame { flex-direction: column; gap: 1.25rem; }
  .auth-brand-pc { display: none; }
  .auth-brand-mobile { display: flex; align-items: center; justify-content: center; gap: .625rem; }
  .auth-card, .auth-card-solo { width: 100%; }
}
"#;

/// Centered auth frame. PC: brand column (when `show_brand`) beside a single
/// `.app-card` form column whose first element is the uppercase mono eyebrow.
/// Mobile: brand collapses to a logo + wordmark row above a full-width card.
/// `subtitle` feeds the brand panel; pass `""` when `show_brand` is false.
#[component]
pub fn AuthLayout(
    eyebrow: String,
    show_brand: bool,
    subtitle: String,
    children: Element,
) -> Element {
    rsx! {
        style { {AUTH_CSS} }
        div { class: "auth-shell",
            div { class: "auth-frame",
                if show_brand {
                    div { class: "auth-brand-pc", {brand_panel(&subtitle)} }
                    div { class: "auth-brand-mobile",
                        BrandLogo { size: 40 }
                        span { class: "auth-wordmark", "definitely-not-crosswords" }
                    }
                }
                div {
                    class: if show_brand { "app-card auth-card" } else { "app-card auth-card auth-card-solo" },
                    h1 { class: "auth-eyebrow", "{eyebrow}" }
                    {children}
                }
            }
        }
    }
}
