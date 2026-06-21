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
        div { class: "col",
            style: "gap:0.75rem;border-bottom:1px solid var(--border-app);padding-bottom:1rem",
            div { class: "col", style: "gap:0.25rem",
                h1 { style: "font-size:1.125rem;font-weight:bold;letter-spacing:0.05em", "ADMIN" }
                p { class: "muted", style: "font-size:0.75rem",
                    "Operational controls for puzzles, users, and roles."
                }
            }
            nav { class: "row", style: "gap:0.5rem;flex-wrap:wrap",
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
                                class: if is_active { "app-btn app-btn-active" } else { "app-btn" },
                                style: "font-size:0.75rem;font-family:monospace;text-transform:uppercase;letter-spacing:0.05em",
                                {label}
                            }
                        }
                    }
                }
            }
        }
    }
}
