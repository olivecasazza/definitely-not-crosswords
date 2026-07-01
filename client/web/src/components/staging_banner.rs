//! Staging-only banner: warns users this is a beta/test environment (cheap Pro,
//! but expect data loss / unexpected changes) and links to a pre-tagged GitHub
//! issue for bug reports. Staging is detected at runtime from the host (the wasm
//! bundle is shared between environments).

use dioxus::prelude::*;

/// True when served from the staging host (crosswords-staging.casazza.io).
pub fn is_staging() -> bool {
    web_sys::window()
        .and_then(|w| w.location().host().ok())
        .map(|h| h.contains("staging"))
        .unwrap_or(false)
}

/// Pre-filled "new issue" URL, labelled `staging` so reports from here are
/// distinguishable from prod. KISS — just a link to GitHub's issue form.
const REPORT_BUG_URL: &str = "https://github.com/olivecasazza/definitely-not-crosswords/issues/new?labels=staging&title=%5Bstaging%5D+&body=%2A%2AEnvironment%3A%2A%2A+staging+%28reported+from+the+app%29%0A%0A%2A%2AWhat+happened%3F%2A%2A%0A%0A%2A%2ASteps+to+reproduce%3A%2A%2A%0A";

#[component]
pub fn StagingBanner() -> Element {
    if !is_staging() {
        return rsx! {};
    }
    rsx! {
        div {
            style: "background:#fde68a;color:#0f172a;font-size:0.8rem;line-height:1.4;\
                    padding:0.4rem 0.9rem;display:flex;gap:0.75rem;align-items:center;\
                    justify-content:center;flex-wrap:wrap;border-bottom:1px solid #0f172a",
            span {
                b { "STAGING (beta) — " }
                "Pro is $1 here, but this is a test environment: expect occasional data loss and unexpected changes. You're a beta tester. 🎈"
            }
            a {
                href: REPORT_BUG_URL,
                target: "_blank",
                rel: "noopener",
                style: "font-weight:bold;text-decoration:underline;white-space:nowrap",
                "Report a bug →"
            }
        }
    }
}
