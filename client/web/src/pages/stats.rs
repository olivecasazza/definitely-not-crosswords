use dioxus::prelude::*;

#[component]
pub fn Stats() -> Element {
    rsx! { div { class: "container", h1 { "Stats" } p { class: "muted", "todo" } } }
}
