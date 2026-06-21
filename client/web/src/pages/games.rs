use dioxus::prelude::*;

#[component]
pub fn Games() -> Element {
    rsx! { div { class: "container", h1 { "Games" } p { class: "muted", "todo" } } }
}
