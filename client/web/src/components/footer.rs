//! Static footer. Ported from `components/AppFooter.vue` (simplified).
//!
//! A flow row at the bottom of `.app-shell`, below `.app-main`. It used to be
//! `position: absolute; bottom: 0` inside `.app-main`, layered over the panel-kit
//! dock, and the two strips' glyphs collided (pixel-tested: the version text
//! overlapped the dock chips). As a flex child it takes its own row and the
//! workspace clamps to whatever height remains — the same mechanism the staging
//! banner and the header already ride on. On mobile the TabBar owns the bottom
//! edge; this component is hidden there.

use crate::store::use_app_state;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn AppFooter() -> Element {
    let state = use_app_state();
    let version = env!("CARGO_PKG_VERSION");
    let build_sha = option_env!("BUILD_SHA").unwrap_or("unknown");
    // The environment signal moved here from the header (it was an admin-only
    // badge competing with Sign in). /api/config reports it for everyone, so
    // the strip answers "which build am I on" in one place: copyright, version,
    // environment. Staging is tinted — a beta host should be recognizable at a
    // glance — and every other environment stays muted text.
    let environment = state
        .config
        .read()
        .as_ref()
        .map(|c| c.environment.clone())
        .filter(|e| !e.is_empty());
    rsx! {
        footer { class: "site-footer",
            span { class: "muted",
                "\u{00A9} definitely-not-crosswords"
                span { class: "app-version", "data-build": build_sha, "v{version}" }
                if let Some(env) = environment {
                    span {
                        class: if env == "production" { "site-env" } else { "site-env site-env-warn" },
                        "data-env": "{env}",
                        "{env}"
                    }
                }
            }
            nav { class: "site-footer-nav",
                a { class: "muted", href: "https://github.com/olivecasazza/definitely-not-crosswords", "GitHub" }
                // Admin access, out of the header: one role's once-per-session
                // destination does not earn a nav slot beside Games/Stats. A
                // quiet right-aligned link keeps it findable without a button
                // competing with Sign in.
                if state.is_admin() {
                    Link { to: Route::AdminIndex {}, class: "site-footer-link", "Admin" }
                }
            }
        }
        style { {FOOTER_CSS} }
    }
}

const FOOTER_CSS: &str = "
/* A flex row in .app-shell, NOT an overlay: the absolute positioning used to
   share the panel-kit dock's band and the two strips' glyphs collided. As a
   flow element .app-main shrinks by this row's height and the workspace
   re-clamps to it — nothing overlaps, nothing needs pointer-event surgery
   (the old `pointer-events: none` + `.dock-empty` override existed only to
   click through the overlay). */
.site-footer {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: .35rem 1rem;
  border-top: 1px solid var(--border-app);
  font-size: var(--fs-xs);
  color: var(--text-secondary);
}
.site-footer-nav {
  display: flex;
  align-items: center;
  gap: 1rem;
}
/* WCAG 2.5.8 wants 24px and the strip is too short for a 44px box without
   swallowing the row beside it, so the link's own padding carries the target. */
.site-footer-nav a { text-decoration: none;
  padding: .25rem .5rem; margin: -.25rem -.5rem; }
.site-footer-nav a:hover { color: var(--text-primary); }
/* Environment segment beside the version — same mono ladder, one step down.
   Non-production hosts take the warning hue so staging reads at a glance;
   production stays muted (you are on production, obviously). */
.site-env {
  margin-left: .5rem;
  font-size: var(--fs-2xs);
  line-height: 1;
  font-family: var(--mono, ui-monospace, monospace);
  text-transform: uppercase;
  letter-spacing: .05em;
  white-space: nowrap;
}
.site-env::before { content: '·'; margin-right: .5rem; }
.site-env-warn { color: var(--pastel-yellow); }
/* The footer's quiet link idiom — used by GitHub and Admin. */
.site-footer-link {
  text-decoration: none;
  color: var(--text-secondary);
  padding: .25rem .5rem;
  margin: -.25rem -.5rem;
  min-height: 24px;
  display: inline-flex;
  align-items: center;
}
.site-footer-link:hover { color: var(--text-primary); }
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
