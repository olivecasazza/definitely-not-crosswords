use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    rsx! { div { class: "container", h1 { "Home" } p { class: "muted", "todo" } } }
}
