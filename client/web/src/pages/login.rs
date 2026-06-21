use dioxus::prelude::*;

#[component]
pub fn Login() -> Element {
    rsx! { div { class: "container", h1 { "Login" } p { class: "muted", "todo" } } }
}
