use crate::store::use_app_state;
use crate::Route;
use dioxus::prelude::*;
use panel_kit::{use_workspace, LayoutBuilder, PanelKind, PanelWin};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum Panel {
    Welcome,
    Start,
}

impl PanelKind for Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Welcome => "Welcome",
            Panel::Start => "Get Started",
        }
    }
}

fn default_layout() -> Vec<PanelWin<Panel>> {
    let mut b = LayoutBuilder::new();
    vec![
        b.at(Panel::Welcome, 480.0, 170.0, 560.0, 560.0),
        b.at(Panel::Start, 1060.0, 170.0, 380.0, 560.0),
    ]
}

fn dimmed(x: i32, y: i32) -> bool {
    (x == 14 && (2..=18).contains(&y)) || (y == 10 && (6..=22).contains(&x))
}

#[component]
pub fn Home() -> Element {
    let state = use_app_state();
    let ws = use_workspace("home_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Welcome => rsx! {
                div { class: "home-welcome",
                    svg {
                        class: "home-logo",
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
                    h1 { class: "home-title", "definitely-not-crosswords" }
                    p { class: "home-tagline",
                        "Cooperative, real-time crosswords. Race the grid, fill the clues, and climb the leaderboard with friends."
                    }
                }
            },
            Panel::Start => {
                let signed_in = state.user().is_some();
                rsx! {
                    div { class: "home-start",
                        if signed_in {
                            Link { to: Route::Games {}, class: "app-btn app-btn-active home-cta", "Play a game" }
                            Link { to: Route::Stats {}, class: "app-btn home-cta", "Leaderboard" }
                            Link { to: Route::Profile {}, class: "app-btn home-cta", "Profile" }
                        } else {
                            Link { to: Route::Login {}, class: "app-btn app-btn-active home-cta", "Sign in" }
                            Link { to: Route::Signup {}, class: "app-btn home-cta", "Create account" }
                        }
                    }
                }
            }
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

const HOME_CSS: &str = "
.home-welcome { height: 100%; display: flex; flex-direction: column; align-items: center;
  justify-content: center; text-align: center; gap: 1.1rem; padding: 2rem; }
.home-logo { width: 5rem; height: 5rem; color: var(--pastel-yellow); }
.home-title { font-family: var(--mono, monospace); font-size: 1.6rem; font-weight: 800; margin: 0;
  letter-spacing: -.01em; }
.home-tagline { color: var(--text-secondary); max-width: 26rem; margin: 0; line-height: 1.7; font-size: .9rem; }
.home-start { height: 100%; display: flex; flex-direction: column; justify-content: center;
  gap: .75rem; padding: 2rem 1.75rem; }
.home-cta { padding: .7rem 1.25rem; font-weight: 600; text-align: center; }
";
