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

#[component]
pub fn Home() -> Element {
    let state = use_app_state();
    let ws = use_workspace("home_layout", default_layout);
    crate::store::sync_panel_mode(ws.mode);

    let body = move |kind: Panel, _max: bool| -> Element {
        match kind {
            Panel::Welcome => {
                let user_name = state
                    .user()
                    .as_ref()
                    .and_then(|u| u.name.clone())
                    .unwrap_or_else(|| "friend".to_string());
                rsx! {
                    div { class: "home-welcome",
                        crate::components::brand::BrandLogo { size: 64 }
                        h1 { class: "home-title",
                            if state.user().is_some() { "Welcome back, {user_name}" } else { "definitely-not-crosswords" }
                        }
                        p { class: "home-tagline",
                            "Cooperative, real-time crosswords. Race the grid, fill the clues, and climb the leaderboard with friends."
                        }
                        div { class: "home-features",
                            div { class: "home-feature",
                                span { class: "home-feature-icon", "◀▶" }
                                div { class: "home-feature-body",
                                    p { class: "home-feature-title", "Real-time co-op" }
                                    p { class: "home-feature-desc", "Solve together. See every move as it happens." }
                                }
                            }
                            div { class: "home-feature",
                                span { class: "home-feature-icon", "●" }
                                div { class: "home-feature-body",
                                    p { class: "home-feature-title", "Live presence" }
                                    p { class: "home-feature-desc", "Know who's on which clue, right now." }
                                }
                            }
                            div { class: "home-feature",
                                span { class: "home-feature-icon", "▲" }
                                div { class: "home-feature-body",
                                    p { class: "home-feature-title", "Stats & rankings" }
                                    p { class: "home-feature-desc", "Track solve times. Compare head-to-head. Climb." }
                                }
                            }
                        }
                    }
                }
            }
            Panel::Start => {
                let signed_in = state.user().is_some();
                rsx! {
                    div { class: "home-start",
                        p { class: "home-start-eyebrow", "Ready when you are" }
                        if signed_in {
                            Link { to: Route::Games {}, class: "app-btn app-btn-active home-cta", "Play a game" }
                            Link { to: Route::Stats {}, class: "app-btn home-cta", "Leaderboard" }
                            Link { to: Route::Profile {}, class: "app-btn home-cta", "Your profile" }
                        } else {
                            Link { to: Route::Login {}, class: "app-btn app-btn-active home-cta", "Sign in" }
                            Link { to: Route::Signup {}, class: "app-btn home-cta", "Create account" }
                        }
                        p { class: "home-start-foot",
                            if signed_in { "Pick up where you left off." } else { "Free to play. No card required." }
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
  justify-content: center; text-align: center; gap: 1rem; padding: 1.75rem 1.5rem; }
.home-title { font-family: var(--mono, monospace); font-size: 1.4rem; font-weight: 800; margin: 0;
  letter-spacing: -.01em; color: var(--text-primary, var(--fg, #ededed)); }
.home-tagline { color: var(--text-secondary, var(--dim, #7a7a7a)); max-width: 28rem; margin: 0;
  line-height: 1.6; font-size: .88rem; }
.home-features { display: flex; flex-direction: column; gap: .65rem; margin-top: .75rem;
  width: 100%; max-width: 26rem; }
.home-feature { display: flex; align-items: flex-start; gap: .65rem; text-align: left;
  padding: .55rem .7rem; border: 1px solid var(--border-app, var(--line2, #3a3a3a));
  background: var(--bg-card, transparent); }
.home-feature-icon { font-size: .8rem; color: var(--pastel-yellow, #ffbd2e); line-height: 1.4;
  flex-shrink: 0; min-width: 1.2rem; text-align: center; }
.home-feature-body { flex: 1; min-width: 0; }
.home-feature-title { margin: 0; font-size: .82rem; font-weight: 700; color: var(--text-primary, var(--fg)); }
.home-feature-desc { margin: .15rem 0 0; font-size: .72rem; color: var(--text-secondary, var(--dim));
  line-height: 1.45; }

.home-start { height: 100%; display: flex; flex-direction: column; justify-content: center;
  gap: .6rem; padding: 1.75rem 1.5rem; }
.home-start-eyebrow { margin: 0 0 .35rem; font-size: .68rem; font-weight: 700; text-transform: uppercase;
  letter-spacing: .08em; color: var(--text-secondary, var(--dim)); }
.home-cta { padding: .65rem 1.1rem; font-weight: 600; text-align: center; font-size: .88rem; }
.home-start-foot { margin: .5rem 0 0; font-size: .72rem; color: var(--text-secondary, var(--dim));
  text-align: center; }

@media (max-width: 760px) {
  .home-welcome { padding: 1.25rem 1rem; gap: .75rem; }
  .home-title { font-size: 1.2rem; }
  .home-tagline { font-size: .82rem; }
  .home-features { gap: .5rem; margin-top: .5rem; }
  .home-feature { padding: .45rem .55rem; }
  .home-start { padding: 1.25rem 1rem; gap: .55rem; }
  .home-cta { padding: .75rem 1rem; font-size: .9rem; }
}
";
