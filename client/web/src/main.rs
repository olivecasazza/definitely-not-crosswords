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

use components::{footer::AppFooter, header::AppHeader};
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
    #[route("/admin")]
    AdminIndex {},
    #[route("/admin/generator")]
    AdminGenerator {},
    #[route("/admin/users")]
    AdminUsers {},
    #[route("/admin/discounts")]
    AdminDiscounts {},
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

// Re-export page components into scope for the `Routable` derive.
pub use pages::admin_discounts::AdminDiscounts;
pub use pages::admin_generator::AdminGenerator;
pub use pages::admin_index::AdminIndex;
pub use pages::admin_users::AdminUsers;
pub use pages::game_completed::GameCompleted;
pub use pages::game_new::GameNew;
pub use pages::game_play::GamePlay;
pub use pages::games::Games;
pub use pages::home::Home;
pub use pages::login::Login;
pub use pages::profile::Profile;
pub use pages::signup::Signup;
pub use pages::stats::Stats;
pub use pages::verify_email::VerifyEmail;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    provide_app_state();
    // Theme: `.light` on <html>, persisted to localStorage (ported from app.vue).
    use_hook(|| {
        let light = LocalStorage::get::<String>("theme")
            .map(|t| t == "light")
            .unwrap_or(false);
        set_light_class(light);
    });

    rsx! {
        style { {PANEL_CSS} }
        style { {DESIGN} }
        Router::<Route> {}
    }
}

/// Layout shell wrapping every route: sticky header, routed `Outlet`, footer.
#[component]
fn Shell() -> Element {
    rsx! {
        div { class: "app-shell",
            AppHeader {}
            main { class: "app-main", Outlet::<Route> {} }
            AppFooter {}
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

/// Toggle the `.light` class on `<html>` and persist. Shared by the header.
pub fn set_light_class(light: bool) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(html) = doc.document_element() {
            let list = html.class_list();
            let _ = if light {
                list.add_1("light")
            } else {
                list.remove_1("light")
            };
        }
    }
    let _ = LocalStorage::set("theme", if light { "light" } else { "dark" });
}
