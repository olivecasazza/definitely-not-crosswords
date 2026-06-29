//! `message` router — port of server/trpc/router/message.ts (TODO).
use crate::ctx::Ctx;
use serde_json::Value;

pub async fn try_handle(_proc: &str, _input: &Value, _ctx: &Ctx) -> Option<Result<Value, String>> {
    match _proc {
        // port procedures here; return Some(Ok(...)) / Some(Err(...)).
        _ => None,
    }
}
