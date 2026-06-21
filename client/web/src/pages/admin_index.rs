use crate::components::admin_nav::AdminNav;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn AdminIndex() -> Element {
    let nav = use_navigator();

    rsx! {
        div { class: "container",
            div { class: "app-card col", style: "padding:1.5rem;gap:1.5rem",
                AdminNav {}

                div { style: "display:grid;gap:1rem;grid-template-columns:repeat(auto-fit,minmax(220px,1fr))",
                    button {
                        class: "app-card",
                        style: "padding:1.25rem;text-align:left;cursor:pointer",
                        onclick: move |_| { nav.push(Route::AdminGenerator {}); },
                        div { style: "font-size:0.875rem;font-weight:bold;text-transform:uppercase;letter-spacing:0.05em",
                            "Generator"
                        }
                        div { class: "muted", style: "margin-top:0.5rem;font-size:0.75rem;line-height:1.5",
                            "Create, review, and publish generated crossword drafts."
                        }
                    }
                    button {
                        class: "app-card",
                        style: "padding:1.25rem;text-align:left;cursor:pointer",
                        onclick: move |_| { nav.push(Route::AdminUsers {}); },
                        div { style: "font-size:0.875rem;font-weight:bold;text-transform:uppercase;letter-spacing:0.05em",
                            "Users"
                        }
                        div { class: "muted", style: "margin-top:0.5rem;font-size:0.75rem;line-height:1.5",
                            "Add admins and manage user roles."
                        }
                    }
                }
            }
        }
    }
}
