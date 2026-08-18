//! Static footer. Ported from `components/AppFooter.vue` (simplified).
//!
//! On desktop this is `position: absolute` inside `.app-main`, sitting on top of
//! the panel-kit dock — combining two footer-like strips into one visual element.
//! On mobile the TabBar owns the bottom edge; this component is hidden there.

use dioxus::prelude::*;

#[component]
pub fn AppFooter() -> Element {
    let version = env!("CARGO_PKG_VERSION");
    rsx! {
        footer { class: "site-footer",
            span { class: "muted",
                "© definitely-not-crosswords "
                span { class: "app-version", "v{version}" }
            }
            nav { class: "site-footer-nav",
                a { class: "muted", href: "https://github.com/olivecasazza/definitely-not-crosswords", "GitHub" }
            }
        }
        style { {FOOTER_CSS} }
    }
}

const FOOTER_CSS: &str = "
.site-footer {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  z-index: 50;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: .5rem 1rem;
  border-top: 1px solid var(--border-app);
  font-size: .75rem;
  color: var(--text-secondary);
  pointer-events: none;
}
/* The dock chips need to be clickable above the footer. */
.site-footer + .dock-empty,
.dock-empty { pointer-events: auto; }
.site-footer-nav {
  display: flex;
  align-items: center;
  gap: 1rem;
}
.site-footer-nav a { text-decoration: none; pointer-events: auto; }
.site-footer-nav a:hover { color: var(--text-primary); }
/* Mobile: the TabBar owns the bottom edge; hide the footer strip entirely. */
@media (max-width: 760px) { .site-footer { display: none; } }
";
