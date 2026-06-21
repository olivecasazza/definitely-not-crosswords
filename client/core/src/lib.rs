//! Renderer-agnostic crossword logic and backend types, shared by every
//! frontend (Dioxus web today, a desktop shell later). Pure data + math, no I/O.

pub mod auth;
pub mod game;
pub mod rpc;
