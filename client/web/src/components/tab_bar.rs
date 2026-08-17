//! Mobile bottom tab bar (<760px): 3–4 flat destinations in thumb reach, no
//! hamburger. Hidden on the play screen (the board needs full height) and on
//! desktop (CSS). A spacer keeps page content clear of the fixed bar.

use crate::components::identicon::Identicon;
use crate::store::use_app_state;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn TabBar() -> Element {
    let state = use_app_state();
    let route = use_route::<Route>();
    if matches!(route, Route::GamePlay { .. }) {
        return rsx! {};
    }

    let games_active = matches!(route, Route::Games {});
    let stats_active = matches!(route, Route::Stats {});
    let profile_active = matches!(
        route,
        Route::Profile {} | Route::Login {} | Route::Signup {}
    );
    let admin_active = matches!(route, Route::AdminIndex {});
    let tab = |active: bool| {
        if active {
            "tab-bar-tab tab-bar-tab-active"
        } else {
            "tab-bar-tab"
        }
    };
    let user = state.user();

    rsx! {
        div { class: "tab-bar-spacer" }
        nav { class: "tab-bar",
            Link { to: Route::Games {}, class: tab(games_active),
                span { class: "tab-bar-icon", "▦" }
                span { "Games" }
            }
            Link { to: Route::Stats {}, class: tab(stats_active),
                span { class: "tab-bar-icon", "▲" }
                span { "Stats" }
            }
            match user {
                Some(u) => rsx! {
                    Link { to: Route::Profile {}, class: tab(profile_active),
                        span { class: "tab-bar-icon", Identicon { seed: u.id.clone(), size: 16 } }
                        span { "Profile" }
                    }
                },
                None => rsx! {
                    Link { to: Route::Login {}, class: tab(profile_active),
                        span { class: "tab-bar-icon", "●" }
                        span { "Sign in" }
                    }
                },
            }
            if state.is_admin() {
                Link { to: Route::AdminIndex {}, class: tab(admin_active),
                    span { class: "tab-bar-icon", "⚙" }
                    span { "Admin" }
                }
            }
        }
        style { {TAB_BAR_CSS} }
    }
}

const TAB_BAR_CSS: &str = "
.tab-bar, .tab-bar-spacer { display: none; }
@media (max-width: 760px) {
  .tab-bar {
    display: flex; position: fixed; left: 0; right: 0; bottom: 0; z-index: 100;
    background: var(--bg-card); border-top: 1px solid var(--border-app);
    padding-bottom: env(safe-area-inset-bottom);
  }
  .tab-bar-spacer { display: block; height: calc(3.1rem + env(safe-area-inset-bottom)); }
  .tab-bar-tab {
    flex: 1 1 0; display: flex; flex-direction: column; align-items: center;
    gap: .15rem; padding: .45rem 0 .35rem;
    font-family: var(--mono); font-size: var(--fs-2xs); text-transform: uppercase;
    letter-spacing: .05em; color: var(--text-secondary);
    border-top: 2px solid transparent;
  }
  .tab-bar-icon { font-size: .9rem; line-height: 1; display: inline-flex; }
  .tab-bar-tab-active { color: var(--pastel-yellow); border-top-color: var(--pastel-yellow); }
}
";
