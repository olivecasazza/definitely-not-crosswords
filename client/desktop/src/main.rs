//! definitely-not-crosswords desktop shell.
//!
//! A minimal Tauri v2 host: it loads the bundled Dioxus wasm frontend
//! (`frontendDist`, populated from the `crossword-web` build) into a native
//! webview. All app logic lives in the wasm bundle, which reaches the backend
//! over the network exactly like the web build (see `web/src/net.rs`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running definitely-not-crosswords desktop");
}
