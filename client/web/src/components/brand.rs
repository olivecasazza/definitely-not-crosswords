//! Shared brand mark — the 6×6 dot-grid logo with the dimmed cross, used by the
//! header, home, and the auth pages (previously duplicated in each).

use dioxus::prelude::*;

/// The dimmed dots form a plus/cross: column x=14 (rows 2–18) and row y=10
/// (x=6–22), matching the original `AppHeader.vue` logo's `opacity-30` circles.
fn dimmed(x: i32, y: i32) -> bool {
    (x == 14 && (2..=18).contains(&y)) || (y == 10 && (6..=22).contains(&x))
}

/// The dot-grid logo SVG, sized to `size` px, colored `--pastel-yellow`.
#[component]
pub fn BrandLogo(size: u32) -> Element {
    rsx! {
        svg {
            width: "{size}",
            height: "{size}",
            view_box: "0 0 24 24",
            fill: "none",
            xmlns: "http://www.w3.org/2000/svg",
            style: "color: var(--pastel-yellow);",
            for y in [2, 6, 10, 14, 18, 22] {
                for x in [2, 6, 10, 14, 18, 22] {
                    circle {
                        cx: "{x}",
                        cy: "{y}",
                        r: "1.2",
                        fill: "currentColor",
                        opacity: if dimmed(x, y) { "0.3" } else { "1" },
                    }
                }
            }
        }
    }
}

/// A centered brand panel: logo + title + subtitle. Used by the auth pages.
pub fn brand_panel(subtitle: &str) -> Element {
    rsx! {
        div { style: "display:flex; flex-direction:column; align-items:center; justify-content:center; height:100%; text-align:center; gap:1rem; padding:1.5rem;",
            BrandLogo { size: 72 }
            h1 { style: "font-family: var(--mono, monospace); font-size: 1.3rem; font-weight: 800; margin: 0;",
                "definitely-not-crosswords" }
            p { class: "muted", style: "font-size: .8rem; line-height: 1.6; max-width: 16rem;", "{subtitle}" }
        }
    }
}
