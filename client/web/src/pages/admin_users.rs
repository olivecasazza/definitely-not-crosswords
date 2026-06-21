use dioxus::prelude::*;

#[component]
pub fn AdminUsers() -> Element {
    rsx! { div { class: "container", h1 { "AdminUsers" } p { class: "muted", "todo" } } }
}
