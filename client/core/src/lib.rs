//! Renderer-agnostic crossword logic and backend types, shared by every
//! frontend (Dioxus web today, a desktop shell later). Pure data + math, no I/O.

pub mod auth;
pub mod fmt;
pub mod game;
pub mod rpc;

#[cfg(test)]
mod def_220_required_check_canary {
    #[test]
    fn deliberately_failing_canary() {
        assert!(false, "DEF-220: a red suite must block merge on main");
    }
}
