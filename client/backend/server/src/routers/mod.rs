//! Router fan-out. Each submodule owns one tRPC router (e.g. `stats.*`) and
//! exposes `try_handle(proc, input, ctx) -> Option<Result<Value, String>>`,
//! returning `None` for procedures it doesn't own. `dispatch` tries each in
//! turn. Add real ports inside the matching submodule; don't edit this file.

use crate::ctx::Ctx;
use serde_json::Value;

pub mod active_game;
pub mod discount;
pub mod game_list;
pub mod generator;
pub mod message;
pub mod stats;
pub mod subscription;
pub mod team;
pub mod user;

pub async fn dispatch(proc: &str, input: &Value, ctx: &Ctx) -> Result<Value, String> {
    if let Some(r) = stats::try_handle(proc, input, ctx).await {
        return r;
    }
    if let Some(r) = user::try_handle(proc, input, ctx).await {
        return r;
    }
    if let Some(r) = team::try_handle(proc, input, ctx).await {
        return r;
    }
    if let Some(r) = game_list::try_handle(proc, input, ctx).await {
        return r;
    }
    if let Some(r) = active_game::try_handle(proc, input, ctx).await {
        return r;
    }
    if let Some(r) = subscription::try_handle(proc, input, ctx).await {
        return r;
    }
    if let Some(r) = discount::try_handle(proc, input, ctx).await {
        return r;
    }
    if let Some(r) = generator::try_handle(proc, input, ctx).await {
        return r;
    }
    if let Some(r) = message::try_handle(proc, input, ctx).await {
        return r;
    }
    Err(format!(
        "procedure not implemented in Rust backend yet: {proc}"
    ))
}
