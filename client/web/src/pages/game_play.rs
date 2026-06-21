use dioxus::prelude::*;

#[component]
pub fn GamePlay(id: String) -> Element {
    rsx! { div { class: "container", h1 { "GamePlay" } p { class: "muted", "game {id}" } } }
}
