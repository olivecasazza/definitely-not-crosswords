use crate::store::use_app_state;
use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    let state = use_app_state();
    let user = state.user();

    rsx! {
        style { {HOME_CSS} }
        div { class: "hero",
            div { class: "hero-logo",
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
            h1 { class: "hero-title", "definitely-not-crosswords" }
            p { class: "hero-tagline",
                "Cooperative, real-time crosswords. Race the grid, fill the clues, climb the leaderboard."
            }
            div { class: "hero-cta",
                match user {
                    Some(_) => rsx! {
                        Link { to: Route::Games {}, class: "app-btn app-btn-active hero-btn", "Play a game" }
                        Link { to: Route::Stats {}, class: "app-btn hero-btn", "Leaderboard" }
                    },
                    None => rsx! {
                        Link { to: Route::Login {}, class: "app-btn app-btn-active hero-btn", "Sign in" }
                        Link { to: Route::Signup {}, class: "app-btn hero-btn", "Create account" }
                    },
                }
            }
        }
    }
}

/// Same dimmed-cross pattern as the header logo.
fn dimmed(x: i32, y: i32) -> bool {
    (x == 14 && (2..=18).contains(&y)) || (y == 10 && (6..=22).contains(&x))
}

const HOME_CSS: &str = "
.hero { max-width: 40rem; margin: 0 auto; padding: 5rem 1.5rem; display: flex; flex-direction: column;
  align-items: center; text-align: center; gap: 1rem; }
.hero-logo { width: 5rem; height: 5rem; color: var(--pastel-yellow); }
.hero-title { font-family: var(--mono, monospace); font-size: 2rem; font-weight: 800; margin: 0;
  letter-spacing: -.02em; }
.hero-tagline { color: var(--text-secondary); max-width: 30rem; margin: 0 0 1rem; line-height: 1.5; }
.hero-cta { display: flex; gap: .75rem; flex-wrap: wrap; justify-content: center; }
.hero-btn { padding: .6rem 1.25rem; font-weight: 600; }
";
