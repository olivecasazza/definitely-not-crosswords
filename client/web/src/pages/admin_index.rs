use dioxus::prelude::*;

#[component]
pub fn AdminIndex() -> Element {
    rsx! { div { class: "container", h1 { "AdminIndex" } p { class: "muted", "todo" } } }
}
