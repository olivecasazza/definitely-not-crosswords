//! Crossword web client — Dioxus 0.6 SPA, frontend rewrite of the Nuxt app.
//!
//! Keeps the Nuxt backend; talks to it over plain tRPC JSON (`net`) and the
//! tRPC WebSocket protocol for subscriptions. `crossword_core` holds the ported
//! game logic and wire format. Each page/component lives in its own file under
//! `pages/`/`components/`; this file owns only the router, layout, and theme.

mod components;
mod net;
mod pages;
mod store;
mod styles;
mod workspace;

use components::{
    footer::AppFooter, header::AppHeader, staging_banner::StagingBanner, tab_bar::TabBar,
    ui::ToastHost,
};
use dioxus::prelude::*;
use gloo_storage::{LocalStorage, Storage};
use panel_kit::CSS as PANEL_CSS;
use store::provide_app_state;
use styles::DESIGN;

/// The app route table. Path params arrive as component props.
#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[layout(Shell)]
    #[route("/")]
    Home {},
    #[route("/games")]
    Games {},
    #[route("/game/:id")]
    GamePlay { id: String },
    #[route("/game/:id/new")]
    GameNew { id: String },
    #[route("/game/:id/completed")]
    GameCompleted { id: String },
    #[route("/profile")]
    Profile {},
    #[route("/stats")]
    Stats {},
    #[route("/auth/login")]
    Login {},
    #[route("/auth/signup")]
    Signup {},
    #[route("/auth/verify-email")]
    VerifyEmail {},
    #[route("/auth/reset-password")]
    ResetPassword {},
    #[route("/admin")]
    AdminIndex {},
    #[redirect("/admin/generator", || Route::AdminIndex {})]
    #[redirect("/admin/users", || Route::AdminIndex {})]
    #[redirect("/admin/discounts", || Route::AdminIndex {})]
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

// Re-export page components into scope for the `Routable` derive.
pub use pages::admin_index::AdminIndex;
pub use pages::game_completed::GameCompleted;
pub use pages::game_new::GameNew;
pub use pages::game_play::GamePlay;
pub use pages::games::Games;
pub use pages::home::Home;
pub use pages::login::Login;
pub use pages::profile::Profile;
pub use pages::reset_password::ResetPassword;
pub use pages::signup::Signup;
pub use pages::stats::Stats;
pub use pages::verify_email::VerifyEmail;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    provide_app_state();
    // Theme: `.light-mode` on <html>, persisted to localStorage (ported from app.vue).
    use_hook(|| {
        let light = LocalStorage::get::<String>("theme")
            .map(|t| t == "light")
            .unwrap_or(false);
        set_light_class(light);
    });

    // DEF-148 backstop: the shell's boot card (#boot, injected by
    // client/flake.nix) is removed by a MutationObserver on the first mutation of
    // #main. That observer is JS in a <script> block, so it can fail to attach for
    // reasons this app can't see. If it did, the app would boot *underneath* the
    // card and the user would stare at "Loading" with a working app behind it.
    // We get here only after the app has rendered, so removing the card now is
    // always the right answer and is a no-op in the normal case.
    use_effect(|| {
        if let Some(boot) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("boot"))
        {
            let _ = boot.remove();
        }
    });

    rsx! {
        style { {PANEL_CSS} }
        style { {DESIGN} }
        Router::<Route> {}
    }
}

/// Layout shell wrapping every route: sticky header, routed `Outlet`, toasts,
/// footer+dock (desktop), tab bar (mobile). The admin view owns its own workspace
/// chrome (panels + dock) — no extra nav strips here.
#[component]
fn Shell() -> Element {
    // DEF-148: bumped by the error panel's "Try again" / "Go home" to make this
    // scope — and with it the ErrorBoundary below — re-render after the errors are
    // cleared. Nothing else can: while the boundary's fallback is up its children
    // (the Outlet) are not mounted, so a navigation never reaches them, and
    // `ErrorContext::clear_errors` does not mark the boundary's own scope dirty.
    // The read is the subscription; the value itself is unused.
    let recover = use_signal(|| 0_u32);
    let _ = recover.read();

    rsx! {
        div { class: "app-shell",
            StagingBanner {}
            AppHeader {}
            main { class: "app-main",
                // DEF-148 §7: a render error in any page must not unmount the
                // whole app. Kept INSIDE Shell so the header, footer and tab bar
                // stay mounted — losing the nav on one bad page is worse than the
                // error. `panic!()` is not catchable in WASM (dioxus-core 0.6.3
                // `CapturedPanic` is never constructed there), so a page has to
                // bubble an `Err` with `?` to reach this boundary.
                ErrorBoundary {
                    handle_error: move |errors: ErrorContext| rsx! {
                        PageErrorPanel { errors, recover }
                    },
                    Outlet::<Route> {}
                }
            }
            AppFooter {}
            ToastHost {}
            TabBar {}
        }
    }
}

/// DEF-148 §7 post-mount render-error fallback. Rendered by the `ErrorBoundary`
/// in `Shell` in place of the routed page; the shell chrome around it is
/// untouched.
#[component]
fn PageErrorPanel(errors: ErrorContext, recover: Signal<u32>) -> Element {
    // Move focus onto the panel heading so a keyboard or screen-reader user lands
    // on the recovery controls instead of at the top of the document. `eval` runs
    // after the render's DOM mutations are flushed, so the node exists by then.
    use_effect(|| {
        dioxus::document::eval("document.getElementById('page-error')?.focus()");
    });

    // `ErrorContext` is a shared handle (Clone + PartialEq, not Copy), so each
    // handler needs its own copy.
    let retry_errors = errors.clone();

    rsx! {
        div {
            class: "app-card",
            role: "alert",
            style: "margin:auto;max-width:34rem;padding:1.5rem;display:flex;flex-direction:column;gap:.75rem",
            p {
                class: "muted",
                style: "margin:0;font-family:var(--mono);font-size:.625rem;letter-spacing:.05em;text-transform:uppercase",
                "Page error"
            }
            h1 {
                id: "page-error",
                tabindex: "-1",
                style: "margin:0;font-size:1.125rem;font-weight:700;color:var(--color-error)",
                "Something broke on this page"
            }
            p { class: "muted", style: "margin:0", "The rest of the app still works." }
            div { style: "display:flex;gap:.5rem;flex-wrap:wrap",
                button {
                    class: "app-btn app-btn-active",
                    r#type: "button",
                    onclick: move |_| {
                        let _ = retry_errors.clear_errors();
                        let next = *recover.peek() + 1;
                        recover.set(next);
                    },
                    "Try again"
                }
                Link {
                    to: Route::Home {},
                    class: "app-btn",
                    onclick: move |_| {
                        let _ = errors.clear_errors();
                        let next = *recover.peek() + 1;
                        recover.set(next);
                    },
                    "Go home"
                }
            }
        }
    }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    rsx! {
        div { class: "container",
            h1 { "404" }
            p { class: "muted", "No page at /{segments.join(\"/\")}" }
            Link { to: Route::Home {}, class: "app-btn", "Go home" }
        }
    }
}

/// Toggle the `.light-mode` class on `<html>` and persist. Shared by the header.
pub fn set_light_class(light: bool) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(html) = doc.document_element() {
            let list = html.class_list();
            // NOTE: "light-mode", not "light" — panel-kit uses `.light` for its
            // traffic-light dots; putting `.light` on <html> applied those button
            // styles to the whole document and broke the page.
            let _ = if light {
                list.add_1("light-mode")
            } else {
                list.remove_1("light-mode")
            };
        }
    }
    let _ = LocalStorage::set("theme", if light { "light" } else { "dark" });
}
