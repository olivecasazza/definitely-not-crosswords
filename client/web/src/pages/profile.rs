use dioxus::prelude::*;

#[component]
pub fn Profile() -> Element {
    rsx! { div { class: "container", h1 { "Profile" } p { class: "muted", "todo" } } }
}
