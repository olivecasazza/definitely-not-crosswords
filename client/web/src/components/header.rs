//! Top navigation bar. Auth-aware via `AppState`: active-route underline,
//! ▶ Resume for the cached in-progress game, BETA tag (once the staging strip
//! is dismissed), generation-quota readout for free users, theme toggle, and
//! the one real action — Sign in for guests. While the session loads it shows
//! a small skeleton instead of flashing the signed-out chrome. Under 760px the
//! nav links vanish (the bottom TabBar owns primary navigation) and the
//! wordmark collapses to the logo.
//!
//! Admin access and the environment signal live in the footer, not the header:
//! the header carries exactly one filled control at a time (Sign in, or Resume
//! for a signed-in player mid-game — the two are mutually exclusive), and
//! everything else is text, a tag, or a glyph.

use crate::components::brand::BrandLogo;
use crate::components::identicon::Identicon;
use crate::components::staging_banner::REPORT_BUG_URL;
use crate::store::use_app_state;
use crate::{set_light_class, Route};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};

#[component]
pub fn AppHeader() -> Element {
    let state = use_app_state();
    let route = use_route::<Route>();
    let mut light = use_signal(|| {
        LocalStorage::get::<String>("theme")
            .map(|t| t == "light")
            .unwrap_or(false)
    });
    let mut beta_open = use_signal(|| false);
    let user = state.user();

    let games_active = matches!(route, Route::Games {});
    let stats_active = matches!(route, Route::Stats {});
    let navlink = |active: bool| {
        if active {
            "navlink navlink-active"
        } else {
            "navlink"
        }
    };

    // ▶ Resume: cached most-recent active game, hidden while playing it.
    let resume = state
        .active_game
        .read()
        .clone()
        .filter(|g| !matches!(&route, Route::GamePlay { id } if *id == g.id));

    // GEN n/m quota chip: signed-in free users only (Pro / unlimited hides it).
    let quota = state
        .sub
        .read()
        .clone()
        .filter(|s| !s.is_pro && user.is_some())
        .and_then(|s| s.quota_limit.map(|l| (s.quota_used, l)));

    let show_beta_chip = state.feature(|f| f.staging_banner) && *state.banner_dismissed.read();

    rsx! {
        header { class: "site-header",
            Link { to: Route::Home {}, class: "brand",
                BrandLogo { size: 20 }
                span { class: "brand-word", "definitely-not-crosswords" }
            }
            nav { class: "row",
                Link { to: Route::Games {}, class: navlink(games_active), "Games" }
                Link { to: Route::Stats {}, class: navlink(stats_active), "Stats" }
                // Admin access lives in the footer, not here: it is a
                // once-per-session destination for one role, and it cost the
                // header a third nav slot plus an env badge next to it.
                if let Some(g) = resume {
                    Link {
                        to: Route::GamePlay { id: g.id.clone() },
                        class: "app-btn app-btn-active resume-btn",
                        title: "Resume \"{g.title}\"",
                        "▶ "
                        span { class: "resume-label", "Resume" }
                    }
                }
                if show_beta_chip {
                    div { class: "beta-wrap",
                        button {
                            class: "beta-chip tap-44",
                            onclick: move |_| {
                                let next = !beta_open();
                                beta_open.set(next);
                            },
                            "BETA"
                        }
                        if beta_open() {
                            div { class: "beta-pop app-card",
                                p { class: "muted",
                                    "Staging environment — beta price: Pro is $1 here (production is $10/year). Expect occasional data loss and unexpected changes. You're a beta tester. 🎈"
                                }
                                a {
                                    href: REPORT_BUG_URL,
                                    target: "_blank",
                                    rel: "noopener",
                                    "Report a bug →"
                                }
                            }
                        }
                    }
                }
                button {
                    class: "icon-btn",
                    // Icon-only control: the glyph is the whole content, so
                    // without a name a screen reader announces "sun" or
                    // "moon" (or nothing) and never the action.
                    aria_label: if light() { "Switch to light theme" } else { "Switch to dark theme" },
                    title: if light() { "Switch to light theme" } else { "Switch to dark theme" },
                    onclick: move |_| {
                        let next = !light();
                        light.set(next);
                        set_light_class(next);
                    },
                    if light() { "☾" } else { "☀" }
                }
                if let Some((used, limit)) = quota {
                    Link {
                        to: Route::Profile {},
                        class: if used >= limit - 1 { "quota-chip quota-chip-warn tap-44" } else { "quota-chip tap-44" },
                        title: "Puzzle generations this month",
                        // "GEN" is the compact visual; the accessible name
                        // spells the noun out — an abbreviation with only a
                        // title leaves screen readers announcing "gen".
                        aria_label: "Puzzle generations: {used} of {limit} used this month",
                        "GEN {used}/{limit}"
                    }
                }
                if state.is_loading() {
                    // Session still resolving — don't flash the signed-out chrome.
                    div { class: "session-skeleton", aria_busy: "true" }
                } else {
                    match user {
                        Some(u) => rsx! {
                            Link { to: Route::Profile {}, class: "navlink navlink-user",
                                Identicon { seed: u.id.clone(), size: 22 }
                                span { class: "user-name",
                                    "{u.name.clone().or(u.email.clone()).unwrap_or_default()}"
                                }
                            }
                            a { class: "navlink signout-btn", href: "/api/auth/signout", "Sign out" }
                        },
                        None => rsx! {
                            Link { to: Route::Login {}, class: "app-btn app-btn-active", "Sign in" }
                        },
                    }
                }
            }
        }
        style { {HEADER_CSS} }
    }
}

const HEADER_CSS: &str = "
.site-header {
  position: sticky; top: 0; z-index: 50;
  display: flex; align-items: center; justify-content: space-between;
  padding: .35rem 1rem;
  background: var(--bg); border-bottom: 1px solid var(--line);
  font-family: var(--mono); font-size: var(--fs-sm); letter-spacing: .01em;
}
.site-header .brand {
  font-weight: 700; display: inline-flex; align-items: center; gap: .45rem;
  color: var(--fg);
}
.site-header .brand svg { transition: transform .2s ease; }
.site-header .brand:hover svg { transform: scale(1.1); }
.site-header .brand span { color: var(--dim); }
.site-header .brand:hover span { color: var(--fg); }
.site-header nav.row { gap: .5rem; }
.site-header .navlink {
  color: var(--dim); padding: .5rem .5rem; min-height: 44px;
  display: inline-flex; align-items: center;
  /* 2px, not 1px: the reserved active underline (see .navlink-active below) is
     2px, and it is transparent-but-present on EVERY navlink so toggling the
     active route cannot reflow the row. Reserving it on all links is the point
     — only the colour differs — so this is NOT the source of the uneven box;
     the box itself is normalised to .app-btn's 44px here instead. */
  border-bottom: 2px solid transparent;
  transition: color .15s ease;
}
.site-header .navlink:hover { color: var(--fg); }
.site-header .navlink-active { color: var(--fg); border-bottom-color: var(--pastel-yellow); }
.site-header .navlink-user { display: inline-flex; align-items: center; gap: .4rem; }
.resume-btn { white-space: nowrap; }
.beta-wrap { position: relative; }
/* Outlined tag, not the old filled black-bordered box: it was the loudest
   thing in a strip whose only real action is Sign in. Hue stays --pastel-
   yellow (14.7:1 dark / 6.1:1 light), so it reads as the env warning without
   shouting. */
.beta-chip {
  font-family: var(--mono); font-size: var(--fs-2xs); font-weight: 700;
  letter-spacing: .08em; padding: .2rem .45rem; cursor: pointer;
  background: transparent; color: var(--color-warning);
  border: 1px solid var(--color-warning);
}
.beta-pop {
  position: absolute; top: calc(100% + .5rem); right: 0; z-index: 60;
  width: 17rem; padding: .75rem .9rem; font-size: var(--fs-2xs);
  display: flex; flex-direction: column; gap: .5rem;
}
.beta-pop p { margin: 0; line-height: 1.5; }
.beta-pop a { text-decoration: underline; font-weight: 700; }
/* Plain mono text, not a boxed chip: the header carries exactly one real
   control (Sign in / Resume) and everything else is information. The warn
   state keeps the filled pill because that one time it is an alert. */
.quota-chip {
  font-family: var(--mono); font-size: var(--fs-2xs); font-weight: 700;
  letter-spacing: .05em; padding: .25rem .35rem;
  color: var(--text-secondary); text-decoration: none;
}
.quota-chip:hover { color: var(--text-primary); }
.quota-chip-warn { color: var(--contrast-ink); background: var(--color-warning); border-color: var(--color-warning); }
/* Borderless 44px icon control for the theme toggle — the glyph carries the
   meaning, a box around it only added weight. */
.site-header .icon-btn {
  min-width: 44px; min-height: 44px; display: inline-flex; align-items: center;
  justify-content: center; background: none; border: none; border-radius: 0;
  color: var(--dim); cursor: pointer; font-size: var(--fs-md); padding: 0;
  transition: color .15s ease;
}
.site-header .icon-btn:hover { color: var(--fg); }
.site-header .icon-btn:focus-visible { outline: 2px solid var(--fg); outline-offset: 2px; }
.session-skeleton {
  width: 22px; height: 22px; background: var(--bg-cell-letter);
  animation: square-pulse 1.2s ease-in-out infinite;
}
@media (max-width: 760px) {
  /* TabBar owns primary navigation; header keeps logo + status chips. */
  .site-header .navlink { display: none; }
  .site-header .navlink-user { display: inline-flex; }
  .site-header .user-name { display: none; }
  .site-header .brand-word { display: none; }
  .site-header .signout-btn { display: none; }
  .resume-label { display: none; }
}
";
