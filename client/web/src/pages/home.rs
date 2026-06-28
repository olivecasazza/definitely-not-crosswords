use crate::store::use_app_state;
use crate::Route;
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Welcome,
    Start,
    HowTo,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Welcome => "Welcome",
            Panel::Start => "Get Started",
            Panel::HowTo => "How to Play",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Welcome, 24.0, 24.0, 420.0, 280.0),
        b.at(Panel::Start, 464.0, 24.0, 320.0, 280.0),
        b.at(Panel::HowTo, 24.0, 324.0, 420.0, 240.0),
    ]
}

#[component]
pub fn Home() -> Element {
    let state = use_app_state();
    let ws = use_workspace("home_layout", default_layout);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Welcome => rsx! {
                div { class: "home-logo",
                    svg {
                        view_box: "0 0 24 24",
                        fill: "none",
                        xmlns: "http://www.w3.org/2000/svg",
                        for y in [2, 6, 10, 14, 18, 22] {
                            for x in [2, 6, 10, 14, 18, 22] {
                                circle {
                                    cx: "{x}",
                                    cy: "{y}",
                                    r: "1.2",
                                    fill: "currentColor",
                                    opacity: if dimmed(x, y) { "0.3" } else { "1" },
                                }
                            }
                        }
                    }
                }
                p { "Cooperative, real-time crosswords. Race the grid, fill the clues, climb the leaderboard." }
            },
            Panel::Start => {
                let signed_in = state.user().is_some();
                rsx! {
                    div { class: "col",
                        if signed_in {
                            Link { to: Route::Games {}, class: "app-btn app-btn-active", "Play a game" }
                            Link { to: Route::Stats {}, class: "app-btn", "Leaderboard" }
                            Link { to: Route::Profile {}, class: "app-btn", "Profile" }
                        } else {
                            Link { to: Route::Login {}, class: "app-btn app-btn-active", "Sign in" }
                            Link { to: Route::Signup {}, class: "app-btn", "Create account" }
                        }
                    }
                }
            }
            Panel::HowTo => rsx! {
                ul { class: "home-list",
                    li { "Pick a clue from the list or click a square on the board." }
                    li { "Type letters — they auto-advance; arrows and backspace work too." }
                    li { "Submit a guess; correct words lock in green, wrong ones flash red." }
                    li { "Drag panels around, minimize to the dock, or tile them." }
                }
            },
        }
    };

    rsx! {
        style { {HOME_CSS} }
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

/// Same dimmed-cross pattern as the header logo.
fn dimmed(x: i32, y: i32) -> bool {
    (x == 14 && (2..=18).contains(&y)) || (y == 10 && (6..=22).contains(&x))
}

const HOME_CSS: &str = "
.home-logo { width: 4rem; height: 4rem; color: var(--pastel-yellow); margin-bottom: .75rem; }
.home-list { margin: 0; padding-left: 1.1rem; color: var(--text-secondary); line-height: 1.7; }
.home-list li { margin-bottom: .25rem; }
";
