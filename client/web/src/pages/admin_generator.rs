use dioxus::prelude::*;

#[component]
pub fn AdminGenerator() -> Element {
    rsx! { div { class: "container", h1 { "AdminGenerator" } p { class: "muted", "todo" } } }
}
