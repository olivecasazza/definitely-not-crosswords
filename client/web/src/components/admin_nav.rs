//! Admin chrome strip, rendered by the Shell on every `/admin*` route (pages
//! don't own their nav). Eyebrow + section tabs + an environment chip so an
//! admin always knows whether they're touching staging or production.

use crate::store::use_app_state;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn AdminStrip() -> Element {
    let state = use_app_state();
    let route = use_route::<Route>();

    let tabs = [
        ("Overview", Route::AdminIndex {}),
        ("Generator", Route::AdminGenerator {}),
        ("Users", Route::AdminUsers {}),
        ("Discounts", Route::AdminDiscounts {}),
    ];

    let environment = state
        .config
        .read()
        .as_ref()
        .map(|c| c.environment.clone())
        .unwrap_or_default();
    let env_chip_class = match environment.as_str() {
        "production" => "admin-env-chip admin-env-prod",
        _ => "admin-env-chip",
    };

    rsx! {
        style { {ADMIN_NAV_CSS} }
        div { class: "admin-nav",
            div { class: "admin-nav-chrome",
                span { class: "admin-nav-label", "ADMIN" }
                span { class: "admin-nav-desc muted",
                    "Operational controls for puzzles, users, and roles."
                }
                if !environment.is_empty() {
                    span { class: "{env_chip_class}", "{environment.to_uppercase()}" }
                }
            }
            nav { class: "admin-nav-tabs",
                for (label , dest) in tabs {
                    {
                        let is_active = match (&route, &dest) {
                            (Route::AdminIndex {}, Route::AdminIndex {}) => true,
                            (Route::AdminGenerator {}, Route::AdminGenerator {}) => true,
                            (Route::AdminUsers {}, Route::AdminUsers {}) => true,
                            (Route::AdminDiscounts {}, Route::AdminDiscounts {}) => true,
                            _ => false,
                        };
                        rsx! {
                            Link {
                                to: dest,
                                class: if is_active { "admin-tab admin-tab-active" } else { "admin-tab" },
                                {label}
                            }
                        }
                    }
                }
            }
        }
    }
}

const ADMIN_NAV_CSS: &str = r#"
.admin-nav { display: flex; flex-direction: column; border-bottom: 1px solid var(--border-app); }
.admin-nav-chrome { display: flex; align-items: baseline; gap: 0.75rem; padding: 6px 12px; background: var(--bg-titlebar, var(--bg-card)); border-bottom: 1px solid var(--border-app); }
.admin-nav-label { font-size: var(--fs-2xs); font-weight: 600; font-family: var(--font-sans); letter-spacing: 0.1em; color: var(--pastel-yellow); text-transform: uppercase; }
.admin-nav-desc { font-size: var(--fs-2xs); }
.admin-env-chip { margin-left: auto; font-family: var(--mono); font-size: var(--fs-2xs); font-weight: 700; letter-spacing: .08em; padding: .1rem .45rem; background: var(--pastel-yellow); color: var(--contrast-ink); border: 1px solid var(--pastel-yellow); }
.admin-env-prod { background: transparent; color: var(--pastel-red); border-color: var(--pastel-red); }
.admin-nav-tabs { display: flex; flex-wrap: nowrap; overflow-x: auto; }
.admin-tab { padding: 6px 14px; font-size: var(--fs-2xs); font-weight: 600; font-family: var(--font-sans); text-transform: uppercase; letter-spacing: 0.05em; border: none; border-right: 1px solid var(--border-app); background: transparent; color: var(--text-secondary); cursor: pointer; text-decoration: none; display: inline-flex; align-items: center; transition: color .12s, background .12s; white-space: nowrap; }
.admin-tab:first-child { border-left: none; }
.admin-tab:hover { color: var(--text-primary); background: color-mix(in srgb, var(--text-primary) 3%, transparent); }
.admin-tab-active { background: var(--pastel-yellow); color: var(--contrast-ink); }
.admin-tab-active:hover { background: var(--pastel-yellow); color: var(--contrast-ink); }
"#;
