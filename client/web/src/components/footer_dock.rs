//! Footer+dock consolidator (GH-58).
//!
//! On DESKTOP: the panel-kit dock is the visible bottom chrome. The separate
//! `AppFooter` strip is hidden via CSS so there is no double bottom bar.
//! On MOBILE (<761px): `TabBar` owns the bottom edge and `AppFooter` stays
//! hidden via its existing media query — no double bottom bar.

use dioxus::prelude::*;

#[component]
pub fn FooterDock() -> Element {
    rsx! { style { {FOOTER_DOCK_CSS} } }
}

const FOOTER_DOCK_CSS: &str = r#"
/* On desktop, the dock IS the footer — hide the separate AppFooter strip
   so there is no double bottom bar. */
@media (min-width: 761px) {
  .site-footer { display: none; }
}
"#;
