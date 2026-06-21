//! Top navigation bar. Ported from `components/AppHeader.vue` (simplified: a
//! flat nav instead of a dropdown). Auth-aware via `AppState`; the Admin link
//! is gated on the `admin:access` capability; toggles the light/dark theme.

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
            Link { to: Route::Home {}, class: "brand", "definitely-not-crosswords" }
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
                            "{u.name.clone().or(u.email.clone()).unwrap_or_default()}"
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

const HEADER_CSS: &str = "
.site-header { position: sticky; top: 0; z-index: 50; display: flex; align-items: center;
  justify-content: space-between; padding: .6rem 1.5rem; background: var(--bg-card);
  border-bottom: 1px solid var(--border-app); }
.site-header .brand { font-weight: 700; }
.site-header .navlink { color: var(--text-secondary); padding: .35rem .5rem; border-radius: .375rem; }
.site-header .navlink:hover { color: var(--text-primary); }
";
