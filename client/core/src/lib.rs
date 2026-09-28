//! Renderer-agnostic crossword logic and backend types, shared by every
//! frontend (Dioxus web today, a desktop shell later). Pure data + math, no I/O.

pub mod auth;
pub mod fmt;
pub mod game;
pub mod rpc;

#[cfg(test)]
mod def_220_failure_path_canary {
    //! DEF-220 AC#3: prove the CI `rust-test` job has a real failure path.
    //!
    //! A green `Rust Test Suite` check and a check that compiled but ran zero
    //! tests look identical — crane suppresses test stdout on success, so there
    //! is no `test result:` line to distinguish them. This test must therefore
    //! be made to fail once, on purpose, and the job watched to go red.
    //!
    //! It is expected to break the build. Delete this module in the revert.

    #[test]
    fn deliberately_failing_canary() {
        assert!(
            false,
            "DEF-220 AC#3 canary: this assertion is meant to fail. If you are reading \
             this in a PR that is not the DEF-220 canary, delete client/core/src/lib.rs's \
             def_220_failure_path_canary module."
        );
    }
}
