//! Static footer. Ported from `components/AppFooter.vue` (simplified).

use dioxus::prelude::*;

#[component]
pub fn AppFooter() -> Element {
    rsx! {
        footer { class: "site-footer",
            span { class: "muted", "© definitely-not-crosswords" }
            nav { class: "row",
                a { class: "muted", href: "https://github.com/ocasazza", "GitHub" }
                a { class: "muted", href: "#", "Privacy" }
                a { class: "muted", href: "#", "Terms" }
            }
        }
        style { {FOOTER_CSS} }
    }
}

const FOOTER_CSS: &str = "
.site-footer { display: flex; align-items: center; justify-content: space-between;
  padding: 1rem 1.5rem; border-top: 1px solid var(--border-app); margin-top: 2rem; font-size: .8rem; }
";
