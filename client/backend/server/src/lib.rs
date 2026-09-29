//! Library target so `tests/` integration tests can reach the tRPC dispatch
//! (`routers::generator::try_handle`) and `Ctx` without going through the
//! Axum router. The binary keeps its `main.rs`; both share these modules.

pub mod assets;
pub mod auth_routes;
pub mod checkout;
pub mod ctx;
pub mod mailer;
pub mod origin;
pub mod routers;
pub mod scoring;
pub mod seo;
pub mod spa;
pub mod state;
pub mod webhook;
pub mod wire;
