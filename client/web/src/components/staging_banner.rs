//! Staging-only banner: warns users this is a beta/test environment (cheap Pro,
//! but expect data loss / unexpected changes) and links to a pre-tagged GitHub
//! issue for bug reports. Gated on the `stagingBanner` feature flag from
//! `/api/config` (the server's APP_ENV), since the wasm bundle is shared across
//! environments.
//!
//! Dismissible: the ✕ persists to localStorage and collapses the strip into the
//! header's BETA chip (see `header.rs`), which reopens this copy in a popover.

use crate::store::use_app_state;
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};

/// Pre-filled "new issue" URL, labelled `staging` so reports from here are
/// distinguishable from prod. KISS — just a link to GitHub's issue form.
pub const REPORT_BUG_URL: &str = "https://github.com/olivecasazza/definitely-not-crosswords/issues/new?labels=staging&title=%5Bstaging%5D+&body=%2A%2AEnvironment%3A%2A%2A+staging+%28reported+from+the+app%29%0A%0A%2A%2AWhat+happened%3F%2A%2A%0A%0A%2A%2ASteps+to+reproduce%3A%2A%2A%0A";

/// The dismiss control. At the banner's type sizes a bare ✕ is ~14px tall, so
/// the button is a real 44px box (the strip grows to match) rather than a
/// 14px target.
const STAGING_CSS: &str = r#"
.staging-banner { background: var(--color-warning); color: var(--contrast-ink);
  font-size: var(--fs-sm); line-height: 1.4; padding: 0.4rem 0.9rem;
  display: flex; gap: 0.75rem; align-items: center; justify-content: center;
  flex-wrap: wrap; border-bottom: 1px solid var(--contrast-ink); }
.staging-dismiss { position: relative; background: none; border: none; color: var(--contrast-ink);
  font-weight: 700; cursor: pointer; padding: 0 .5rem; font-size: var(--fs-sm); min-height: 44px; }
/* Mobile: the full beta-price sentence plus a 44px dismiss wrap the banner to
   five lines (~130px) and push the play board past a phone viewport. Collapse
   to one short warning; the price detail stays on desktop and in the BETA
   popover (header.rs) once the banner is dismissed. */
@media (max-width: 760px) {
  .staging-banner { padding: 0.2rem 0.5rem; gap: 0.5rem; }
  .staging-long, .staging-report { display: none; }
}
"#;

#[component]
pub fn StagingBanner() -> Element {
    let state = use_app_state();
    if !state.feature(|f| f.staging_banner) || *state.banner_dismissed.read() {
        return rsx! {};
    }
    rsx! {
        style { {STAGING_CSS} }
        div {
            class: "staging-banner",
            span {
                b { "STAGING (beta) — " }
                "test environment, data may be lost."
            }
            span { class: "staging-long",
                "Beta price: Pro is $1 here (production is $10/year). You're a beta tester. 🎈"
            }
            a {
                class: "staging-report",
                href: REPORT_BUG_URL,
                target: "_blank",
                rel: "noopener",
                style: "font-weight:bold;text-decoration:underline;white-space:nowrap",
                "Report a bug →"
            }
            button {
                class: "staging-dismiss",
                aria_label: "Dismiss",
                onclick: move |_| {
                    let _ = LocalStorage::set("staging_dismissed", true);
                    let mut dismissed = state.banner_dismissed;
                    dismissed.set(true);
                },
                "✕"
            }
        }
    }
}
