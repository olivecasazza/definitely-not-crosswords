//! tRPC wire-format helpers shared between `main.rs` and the integration tests.
//!
//! Lives in its own module so the `envelope` function can be exercised by
//! `#[test]` blocks without spinning up the full Axum router. Kept deliberately
//! tiny: anything that needs DB access, async, or auth lives in the routers.

use axum::Json;
use serde_json::{json, Value};

/// Pick a structured tRPC error envelope from a router error string.
///
/// Routers historically returned plain `Err("FORBIDDEN")` / `Err("UNAUTHORIZED")`
/// strings, which the previous envelope blindly turned into a 400 BAD_REQUEST.
/// With RBAC-gated mutations (DEF-70 job:create) the client needs the actual
/// status (403 FORBIDDEN / 401 UNAUTHORIZED / 400 BAD_REQUEST) so it can show
/// "you can't do that" instead of retrying forever. Routers can also return a
/// string starting with `INTERNAL: ...` for genuine 500s; anything else falls
/// through as BAD_REQUEST to preserve the old behavior.
pub fn envelope(res: Result<Value, String>) -> Json<Value> {
    match res {
        Ok(data) => Json(json!([{ "result": { "data": data } }])),
        Err(e) => {
            let (code, http_status) = if e == "FORBIDDEN" {
                ("FORBIDDEN", 403)
            } else if e == "UNAUTHORIZED" {
                ("UNAUTHORIZED", 401)
            } else if let Some(rest) = e.strip_prefix("INTERNAL: ") {
                // Genuine server error: surface the underlying message but with
                // a 500 envelope so the client doesn't loop on a known-bad call.
                return Json(json!([{
                    "error": { "message": rest.to_string(), "code": -32603,
                               "data": { "code": "INTERNAL_SERVER_ERROR", "httpStatus": 500 } }
                }]));
            } else {
                ("BAD_REQUEST", 400)
            };
            Json(json!([{
                "error": { "message": e, "code": -32600,
                           "data": { "code": code, "httpStatus": http_status } }
            }]))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Wire-format regression tests for the structured error envelopes that
    //! `job:create` and the other RBAC-gated procs return. Catches drift in
    //! the JSON shape (`error.data.code`, `error.data.httpStatus`) — the
    //! client depends on these to decide between "retry", "show error", and
    //! "redirect to login".

    use super::*;

    #[test]
    fn success_envelope_keeps_data_payload() {
        let v = envelope(Ok(json!({ "jobId": "abc" }))).0();
        assert_eq!(v[0]["result"]["data"]["jobId"], "abc");
        assert!(v[0].get("error").is_none());
    }

    #[test]
    fn forbidden_envelope_is_403_not_500() {
        // Regression for DEF-70: the router's `Err("FORBIDDEN")` MUST surface
        // as a structured 403, not a 500 / BAD_REQUEST.
        let v = envelope(Err("FORBIDDEN".into())).0();
        assert_eq!(v[0]["error"]["data"]["code"], "FORBIDDEN");
        assert_eq!(v[0]["error"]["data"]["httpStatus"], 403);
        assert_eq!(v[0]["error"]["code"], -32600);
    }

    #[test]
    fn unauthorized_envelope_is_401() {
        let v = envelope(Err("UNAUTHORIZED".into())).0();
        assert_eq!(v[0]["error"]["data"]["code"], "UNAUTHORIZED");
        assert_eq!(v[0]["error"]["data"]["httpStatus"], 401);
    }

    #[test]
    fn internal_prefix_is_500() {
        let v = envelope(Err("INTERNAL: db is on fire".into())).0();
        assert_eq!(v[0]["error"]["data"]["code"], "INTERNAL_SERVER_ERROR");
        assert_eq!(v[0]["error"]["data"]["httpStatus"], 500);
        assert_eq!(v[0]["error"]["code"], -32603);
        assert_eq!(v[0]["error"]["message"], "db is on fire");
    }

    #[test]
    fn plain_string_is_400_bad_request() {
        // Anything that doesn't match the structured prefixes falls through
        // as BAD_REQUEST / 400 — preserves the old behavior for all the
        // existing validation-error paths.
        let v = envelope(Err("topic is required".into())).0();
        assert_eq!(v[0]["error"]["data"]["code"], "BAD_REQUEST");
        assert_eq!(v[0]["error"]["data"]["httpStatus"], 400);
        assert_eq!(v[0]["error"]["message"], "topic is required");
    }
}
