//! Top navigation bar. Auth-aware via `AppState`: active-route underline,
//! ▶ Resume for the cached in-progress game, BETA chip (once the staging strip
//! is dismissed), generation-quota chip for free users, theme toggle. While the
//! session loads it shows a small skeleton instead of flashing the signed-out
//! chrome. Under 760px the nav links vanish (the bottom TabBar owns primary
//! navigation) and the wordmark collapses to the logo.

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
    let admin_active = matches!(route, Route::AdminIndex {});
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

    // Environment chip (admins only): the one allowed header carries the
    // env signal now that the admin tab strip is gone (GH-61).
    let admin_env = state
        .config
        .read()
        .as_ref()
        .map(|c| c.environment.clone())
        .filter(|e| !e.is_empty() && state.is_admin());

    rsx! {
        header { class: "site-header",
            Link { to: Route::Home {}, class: "brand",
                BrandLogo { size: 20 }
                span { class: "brand-word", "definitely-not-crosswords" }
            }
            nav { class: "row",
                Link { to: Route::Games {}, class: navlink(games_active), "Games" }
                Link { to: Route::Stats {}, class: navlink(stats_active), "Stats" }
                if state.is_admin() {
                    Link { to: Route::AdminIndex {}, class: navlink(admin_active), "Admin" }
                }
                if let Some(g) = resume {
                    Link {
                        to: Route::GamePlay { id: g.id.clone() },
                        class: "app-btn app-btn-active resume-btn",
                        title: "Resume \"{g.title}\"",
                        "▶ "
                        span { class: "resume-label", "Resume" }
                    }
                }
                if let Some(env) = admin_env {
                    {crate::components::admin::env_badge(&env)}
                }
                if show_beta_chip {
                    div { class: "beta-wrap",
                        button {
                            class: "beta-chip",
                            onclick: move |_| {
                                let next = !beta_open();
                                beta_open.set(next);
                            },
                            "BETA"
                        }
                        if beta_open() {
                            div { class: "beta-pop app-card",
                                p { class: "muted",
                                    "Staging environment — Pro is $1 here, but expect occasional data loss and unexpected changes. You're a beta tester. 🎈"
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
                    class: "app-btn",
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
                        class: if used >= limit - 1 { "quota-chip quota-chip-warn" } else { "quota-chip" },
                        title: "Puzzle generations this month",
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
                            a { class: "app-btn signout-btn", href: "/api/auth/signout", "Sign out" }
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
  font-family: var(--mono); font-size: .8rem; letter-spacing: .01em;
}
.site-header .brand {
  font-weight: 700; display: inline-flex; align-items: center; gap: .45rem;
  color: var(--fg);
}
.site-header .brand svg { transition: transform .2s ease; }
.site-header .brand:hover svg { transform: scale(1.1); }
.site-header .brand span { color: var(--dim); }
.site-header .brand:hover span { color: var(--fg); }
.site-header nav.row { gap: .25rem; }
.site-header .navlink {
  color: var(--dim); padding: .25rem .5rem;
  border-bottom: 2px solid transparent;
  transition: color .15s ease;
}
.site-header .navlink:hover { color: var(--fg); }
.site-header .navlink-active { color: var(--fg); border-bottom-color: var(--pastel-yellow); }
.site-header .navlink-user { display: inline-flex; align-items: center; gap: .4rem; }
.resume-btn { white-space: nowrap; }
.beta-wrap { position: relative; }
.beta-chip {
  font-family: var(--mono); font-size: var(--fs-2xs); font-weight: 700;
  letter-spacing: .08em; padding: .2rem .45rem; cursor: pointer;
  background: var(--color-warning); color: var(--contrast-ink);
  border: 1px solid var(--contrast-ink);
}
.beta-pop {
  position: absolute; top: calc(100% + .5rem); right: 0; z-index: 60;
  width: 17rem; padding: .75rem .9rem; font-size: var(--fs-2xs);
  display: flex; flex-direction: column; gap: .5rem;
}
.beta-pop p { margin: 0; line-height: 1.5; }
.beta-pop a { text-decoration: underline; font-weight: 700; }
.quota-chip {
  font-family: var(--mono); font-size: var(--fs-2xs); font-weight: 700;
  letter-spacing: .05em; padding: .25rem .45rem;
  border: 1px solid var(--border-app); color: var(--text-secondary);
}
.quota-chip:hover { color: var(--text-primary); border-color: var(--border-hover); }
.quota-chip-warn { color: var(--contrast-ink); background: var(--color-warning); border-color: var(--color-warning); }
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
