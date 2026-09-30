//! Static footer. Ported from `components/AppFooter.vue` (simplified).
//!
//! On desktop this is `position: absolute` inside `.app-main`, sitting on top of
//! the panel-kit dock — combining two footer-like strips into one visual element.
//! On mobile the TabBar owns the bottom edge; this component is hidden there.

use dioxus::prelude::*;

#[component]
pub fn AppFooter() -> Element {
    let version = env!("CARGO_PKG_VERSION");
    let build_sha = option_env!("BUILD_SHA").unwrap_or("unknown");
    rsx! {
        footer { class: "site-footer",
            span { class: "muted",
                "\u{00A9} definitely-not-crosswords"
                span { class: "app-version", "data-build": build_sha, "v{version}" }
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
/* The version used to sit directly after the title with nothing between them:
   a bare space, an inline child one step smaller in font-size inside the
   title's taller line box. That read as smushed/overlapping even though the
   boxes never intersected. Give it a real gap plus a middle-dot separator, and
   size it one step down the token ladder (--fs-2xs) so it stays legible but
   clearly secondary. */
.app-version {
  margin-left: .5rem;
  font-size: var(--fs-2xs);
  line-height: 1;
  font-family: var(--mono, ui-monospace, monospace);
  white-space: nowrap;
}
.app-version::before { content: '·'; margin-right: .5rem; }
/* Mobile: the TabBar owns the bottom edge; hide the footer strip entirely. */
@media (max-width: 760px) { .site-footer { display: none; } }
";
