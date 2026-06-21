use dioxus::prelude::*;

#[component]
pub fn GameCompleted(id: String) -> Element {
    rsx! { div { class: "container", h1 { "GameCompleted" } p { class: "muted", "game {id}" } } }
}
