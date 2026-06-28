use crate::components::admin_nav::AdminNav;
use crate::Route;
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Overview,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Overview => "Overview",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![b.at(Panel::Overview, 16.0, 16.0, 1888.0, 880.0)]
}

#[component]
pub fn AdminIndex() -> Element {
    let nav = use_navigator();
    let ws = use_workspace("admin_index_layout", default_layout);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Overview => {
                let nav_gen = nav.clone();
                let nav_users = nav.clone();
                rsx! {
                    div { style: "display:grid;gap:1rem;grid-template-columns:repeat(auto-fit,minmax(220px,1fr))",
                        button {
                            class: "app-card",
                            style: "padding:1.25rem;text-align:left;cursor:pointer",
                            onclick: move |_| { nav_gen.push(Route::AdminGenerator {}); },
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
                            onclick: move |_| { nav_users.push(Route::AdminUsers {}); },
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
    };

    rsx! {
        AdminNav {}
        div {
            class: ws.root_class(),
            tabindex: "0",
            onmousemove: move |e| ws.handle_mouse_move(&e),
            onmouseup: move |_| ws.handle_mouse_up(),
            {ws.render(body)}
            {ws.dock()}
        }
    }
}
