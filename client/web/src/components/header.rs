//! Top navigation bar. Ported from `components/AppHeader.vue` (simplified: a
//! flat nav instead of a dropdown). Auth-aware via `AppState`; the Admin link
//! is gated on the `admin:access` capability; toggles the light/dark theme.

use crate::components::identicon::Identicon;
use crate::store::use_app_state;
use crate::{set_light_class, Route};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};

#[component]
pub fn AppHeader() -> Element {
    let state = use_app_state();
    let mut light = use_signal(|| {
        LocalStorage::get::<String>("theme")
            .map(|t| t == "light")
            .unwrap_or(false)
    });
    let user = state.user();

    rsx! {
        header { class: "site-header",
            Link { to: Route::Home {}, class: "brand",
                svg {
                    class: "logo",
                    view_box: "0 0 24 24",
                    fill: "none",
                    xmlns: "http://www.w3.org/2000/svg",
                    // 6×6 dot grid; the cross of dots is dimmed (opacity 0.3),
                    // ported faithfully from the original AppHeader.vue logo.
                    for y in [2, 6, 10, 14, 18, 22] {
                        for x in [2, 6, 10, 14, 18, 22] {
                            circle {
                                cx: "{x}",
                                cy: "{y}",
                                r: "1.2",
                                fill: "currentColor",
                                opacity: if logo_dot_dimmed(x, y) { "0.3" } else { "1" },
                            }
                        }
                    }
                }
                span { "definitely-not-crosswords" }
            }
            nav { class: "row",
                Link { to: Route::Games {}, class: "navlink", "Games" }
                Link { to: Route::Stats {}, class: "navlink", "Stats" }
                if state.is_admin() {
                    Link { to: Route::AdminIndex {}, class: "navlink", "Admin" }
                }
                button {
                    class: "app-btn",
                    onclick: move |_| {
                        let next = !light();
                        light.set(next);
                        set_light_class(next);
                    },
                    if light() { "☾" } else { "☀" }
                }
                match user {
                    Some(u) => rsx! {
                        Link { to: Route::Profile {}, class: "navlink",
                            style: "display: inline-flex; align-items: center; gap: .4rem;",
                            Identicon { seed: u.id.clone(), size: 22 }
                            span { "{u.name.clone().or(u.email.clone()).unwrap_or_default()}" }
                        }
                        a { class: "app-btn", href: "/api/auth/signout", "Sign out" }
                    },
                    None => rsx! {
                        Link { to: Route::Login {}, class: "app-btn app-btn-active", "Sign in" }
                    },
                }
            }
        }
        style { {HEADER_CSS} }
    }
}

/// The dimmed dots form a plus/cross in the grid (matches the original logo's
/// `opacity-30` circles): the column x=14 (rows 2–18) and the row y=10 (x=6–22).
fn logo_dot_dimmed(x: i32, y: i32) -> bool {
    (x == 14 && (2..=18).contains(&y)) || (y == 10 && (6..=22).contains(&x))
}

const HEADER_CSS: &str = "
.site-header {
  position: sticky; top: 0; z-index: 50;
  display: flex; align-items: center; justify-content: space-between;
  padding: .35rem 1rem;
  background: var(--bg); border-bottom: 1px solid var(--line);
  font-family: var(--mono); font-size: .8rem; letter-spacing: .01em;
}
.site-header .brand {
  font-weight: 700; display: inline-flex; align-items: center; gap: .45rem;
  color: var(--fg);
}
.site-header .brand .logo {
  width: 1.25rem; height: 1.25rem; color: var(--accent);
  transition: transform .2s ease;
}
.site-header .brand:hover .logo { transform: scale(1.1); }
.site-header .brand span { color: var(--dim); }
.site-header .brand:hover span { color: var(--fg); }
.site-header nav.row { gap: .25rem; }
.site-header .navlink {
  color: var(--dim); padding: .25rem .5rem;
  transition: color .15s ease;
}
.site-header .navlink:hover { color: var(--fg); }
";
