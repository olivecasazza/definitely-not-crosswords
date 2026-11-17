//! App meta for the workspace dock band. Ported from `components/AppFooter.vue`
//! (simplified), then folded into the panel-kit dock through the dock's
//! host-owned `extras` slot: copyright, version, environment, and links ride in
//! the same band as the dock chips instead of a second strip below it.
//!
//! The dock band is the only bottom chrome on desktop; on mobile the TabBar
//! owns the bottom edge and the meta is hidden.

use crate::store::use_app_state;
use crate::Route;
use dioxus::prelude::*;

/// Copyright, build identity, environment, and links, right-aligned inside the
/// dock band. Rendered through `dock::dock(.., Some(rsx! { FooterMeta {} }))`.
#[component]
pub fn FooterMeta() -> Element {
    let state = use_app_state();
    let version = env!("CARGO_PKG_VERSION");
    let build_sha = option_env!("BUILD_SHA").unwrap_or("unknown");
    // The environment signal moved here from the header (it was an admin-only
    // badge competing with Sign in). /api/config reports it for everyone, so
    // the band answers "which build am I on" in one place: copyright, version,
    // environment. Staging is tinted — a beta host should be recognizable at a
    // glance — and every other environment stays muted text.
    let environment = state
        .config
        .read()
        .as_ref()
        .map(|c| c.environment.clone())
        .filter(|e| !e.is_empty());
    rsx! {
        div { class: "dock-meta",
            span { class: "dock-meta-copy",
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
            nav { class: "dock-meta-nav",
                a { class: "muted", href: "https://github.com/olivecasazza/definitely-not-crosswords", "GitHub" }
                // Admin access, out of the header: one role's once-per-session
                // destination does not earn a nav slot beside Games/Stats. A
                // quiet right-aligned link keeps it findable without a button
                // competing with Sign in.
                if state.is_admin() {
                    Link { to: Route::AdminIndex {}, class: "dock-meta-link", "Admin" }
                }
            }
        }
        style { {META_CSS} }
    }
}

const META_CSS: &str = "
/* Right-aligned guest of the dock band: pushed to the far edge with
   margin-left auto, never overlapping the chips (flex, not the old absolute
   overlay whose version text collided with them), and truncating rather than
   wrapping if a long dock squeezes it. */
.dock-meta {
  margin-left: auto;
  flex-shrink: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 1rem;
  font-size: var(--fs-xs);
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
}
.dock-meta-copy { min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.dock-meta-nav {
  display: flex;
  align-items: center;
  gap: 1rem;
  flex-shrink: 0;
}
/* WCAG 2.5.8 wants 24px and the strip is too short for a 44px box without
   swallowing the row beside it, so the link's own padding carries the target. */
.dock-meta-nav a, .dock-meta-link { text-decoration: none;
  padding: .25rem .5rem; margin: -.25rem -.5rem; }
.dock-meta-nav a:hover, .dock-meta-link:hover { color: var(--text-primary); }
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
/* The quiet link idiom — used by GitHub and Admin. */
.dock-meta-link {
  color: var(--text-secondary);
  min-height: 24px;
  display: inline-flex;
  align-items: center;
}
/* Mobile: the TabBar owns the bottom edge; the meta lives only on wide docks. */
@media (max-width: 760px) { .dock-meta { display: none; } }
";
