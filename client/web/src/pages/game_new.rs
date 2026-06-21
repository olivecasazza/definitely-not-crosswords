use dioxus::prelude::*;

#[component]
pub fn GameNew(id: String) -> Element {
    rsx! { div { class: "container", h1 { "GameNew" } p { class: "muted", "game {id}" } } }
}
