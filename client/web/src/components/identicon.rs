//! Per-user identicon: a dot-matrix avatar in the brand style (see `brand.rs`).
//! Deterministic from a seed (the user id): a stable FNV-1a hash drives which
//! dots are "on", and the pattern is vertically mirror-symmetric (GitHub-style)
//! so the mark always reads as balanced. "On" dots are `--pastel-yellow`, "off"
//! dots a subtle grey — a yellow/grey grid in the app's circle style.

use dioxus::prelude::*;

/// FNV-1a over the seed bytes — a tiny, stable, dependency-free hash.
fn fnv1a(seed: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 5 dot centers across the 24×24 viewBox, inset so the corner dots stay inside
/// the circular container (symmetric: 5+19 = 8.5+15.5 = 24, 12 centered).
const COORDS: [f32; 5] = [5.0, 8.5, 12.0, 15.5, 19.0];

/// A deterministic dot-matrix avatar. `size` is the pixel width/height; the
/// container is a perfect circle at any size.
#[component]
pub fn Identicon(seed: String, size: u32) -> Element {
    let hash = fnv1a(&seed);
    rsx! {
        div {
            // Opaque dark fill gives the circle its own chrome standalone (header,
            // stats) and keeps the yellow "on" dots readable — a transparent bg
            // would let them vanish over the profile circle's yellow gradient.
            style: "width:{size}px; height:{size}px; border-radius:50%; overflow:hidden; \
                    display:inline-flex; align-items:center; justify-content:center; background:var(--bg-cell-empty);",
            svg {
                width: "{size}",
                height: "{size}",
                view_box: "0 0 24 24",
                fill: "none",
                xmlns: "http://www.w3.org/2000/svg",
                for (ci , cx) in COORDS.iter().enumerate() {
                    for (ri , cy) in COORDS.iter().enumerate() {
                        {
                            // Vertical mirror symmetry: cols 3,4 mirror cols 1,0.
                            let half = if ci < 3 { ci } else { 4 - ci };
                            let bit = half * 5 + ri;
                            let on = (hash >> bit) & 1 == 1;
                            rsx! {
                                circle {
                                    cx: "{cx}",
                                    cy: "{cy}",
                                    r: "1.6",
                                    fill: if on { "var(--pastel-yellow)" } else { "var(--text-secondary)" },
                                    opacity: if on { "1" } else { "0.25" },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
