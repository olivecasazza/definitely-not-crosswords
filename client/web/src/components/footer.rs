//! Static footer. Ported from `components/AppFooter.vue` (simplified).

use dioxus::prelude::*;

#[component]
pub fn AppFooter() -> Element {
    // Baked at build time (the workspace version release-plz bumps), so the
    // footer shows which release is deployed.
    let version = env!("CARGO_PKG_VERSION");
    rsx! {
        footer { class: "site-footer",
            span { class: "muted",
                "© definitely-not-crosswords "
                span { class: "app-version", "v{version}" }
            }
            nav { class: "site-footer-nav",
                a { class: "muted", href: "https://github.com/ocasazza", "GitHub" }
                a { class: "muted", href: "#", "Privacy" }
                a { class: "muted", href: "#", "Terms" }
            }
        }
        style { {FOOTER_CSS} }
    }
}

const FOOTER_CSS: &str = "
.site-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: .5rem 1rem;
  border-top: 1px solid var(--border-app);
  margin-top: auto;
  font-size: .75rem;
  color: var(--text-secondary);
}
.site-footer-nav {
  display: flex;
  align-items: center;
  gap: 1rem;
}
.site-footer-nav a { text-decoration: none; }
.site-footer-nav a:hover { color: var(--text-primary); }
";
