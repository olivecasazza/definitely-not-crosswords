use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn AdminNav() -> Element {
    let route = use_route::<Route>();

    let tabs = [
        ("Overview", Route::AdminIndex {}),
        ("Generator", Route::AdminGenerator {}),
        ("Users", Route::AdminUsers {}),
        ("Discounts", Route::AdminDiscounts {}),
    ];

    rsx! {
        style { {ADMIN_NAV_CSS} }
        div { class: "admin-nav",
            div { class: "admin-nav-chrome",
                span { class: "admin-nav-label", "ADMIN" }
                span { class: "admin-nav-desc muted",
                    "Operational controls for puzzles, users, and roles."
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
.admin-nav-label { font-size: 10px; font-weight: 700; font-family: monospace; letter-spacing: 0.1em; color: var(--text-secondary); text-transform: uppercase; }
.admin-nav-desc { font-size: 10px; }
.admin-nav-tabs { display: flex; flex-wrap: wrap; }
.admin-tab { padding: 6px 14px; font-size: 10px; font-weight: 700; font-family: monospace; text-transform: uppercase; letter-spacing: 0.05em; border: none; border-right: 1px solid var(--border-app); background: transparent; color: var(--text-secondary); cursor: pointer; text-decoration: none; display: inline-flex; align-items: center; transition: color .12s, background .12s; }
.admin-tab:first-child { border-left: none; }
.admin-tab:hover { color: var(--text-primary); background: rgba(255,255,255,0.03); }
.admin-tab-active { background: var(--pastel-yellow); color: #18181b; }
.admin-tab-active:hover { background: var(--pastel-yellow); color: #18181b; }
"#;
