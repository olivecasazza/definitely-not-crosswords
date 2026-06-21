use dioxus::prelude::*;

#[component]
pub fn Signup() -> Element {
    rsx! { div { class: "container", h1 { "Signup" } p { class: "muted", "todo" } } }
}
