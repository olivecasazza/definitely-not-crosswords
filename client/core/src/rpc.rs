//! tRPC v10 wire format (HTTP), renderer-neutral.
//!
//! The backend uses `httpBatchLink` with **no** data transformer (verified in
//! `plugins/client.ts` — no `superjson`), so inputs/outputs are raw JSON. This
//! module builds the request URL/body and unwraps the response envelope; the
//! actual fetch (gloo-net on web) lives in the renderer.
//!
//! Wire shape (batch of one):
//! - query:    `GET  /api/trpc/<proc>?batch=1&input={"0":<input>}`
//! - mutation: `POST /api/trpc/<proc>?batch=1`  body `{"0":<input>}`
//! - response: `[{"result":{"data":<data>}}]`  (or `[{"error":{...}}]`)

use serde_json::Value;

/// Percent-encode a query-param value (RFC 3986 unreserved stay literal).
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Batched input object `{"0": <input>}` (tRPC indexes calls by position).
fn batch_body(input: Option<&Value>) -> Value {
    let inner = input.cloned().unwrap_or(Value::Null);
    serde_json::json!({ "0": inner })
}

/// URL for a single batched query. `input` is the raw procedure input (omit for
/// no-arg procedures — tRPC then needs `input={"0":null}` to still be a batch).
pub fn query_url(base: &str, proc: &str, input: Option<&Value>) -> String {
    let body = batch_body(input).to_string();
    format!("{base}/{proc}?batch=1&input={}", encode(&body))
}

/// `(url, body)` for a single batched mutation (POST).
pub fn mutation_request(base: &str, proc: &str, input: Option<&Value>) -> (String, String) {
    (
        format!("{base}/{proc}?batch=1"),
        batch_body(input).to_string(),
    )
}

/// Unwrap `[{"result":{"data":D}}]` → `D`, surfacing a tRPC error as `Err`.
pub fn parse_batch_single(resp: &str) -> Result<Value, String> {
    let v: Value = serde_json::from_str(resp).map_err(|e| e.to_string())?;
    let first = v.get(0).ok_or("empty batch response")?;
    if let Some(err) = first.get("error") {
        return Err(err.to_string());
    }
    first
        .get("result")
        .and_then(|r| r.get("data"))
        .cloned()
        .ok_or_else(|| "missing result.data".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_url_encodes_batched_input() {
        // activeGame.get takes a bare id string.
        let url = query_url("/api/trpc", "activeGame.get", Some(&json!("game123")));
        assert_eq!(
            url,
            "/api/trpc/activeGame.get?batch=1&input=%7B%220%22%3A%22game123%22%7D"
        );
    }

    #[test]
    fn no_arg_query_still_batches() {
        let url = query_url("/api/trpc", "subscription.getStatus", None);
        assert_eq!(
            url,
            "/api/trpc/subscription.getStatus?batch=1&input=%7B%220%22%3Anull%7D"
        );
    }

    #[test]
    fn mutation_request_shape() {
        let (url, body) = mutation_request(
            "/api/trpc",
            "activeGame.addActions",
            Some(&json!({"actions": []})),
        );
        assert_eq!(url, "/api/trpc/activeGame.addActions?batch=1");
        assert_eq!(body, r#"{"0":{"actions":[]}}"#);
    }

    #[test]
    fn parse_unwraps_data_envelope() {
        let data =
            parse_batch_single(r#"[{"result":{"data":{"id":"g1","title":"Daily"}}}]"#).unwrap();
        assert_eq!(data["title"], "Daily");
    }

    #[test]
    fn parse_surfaces_error() {
        let err = parse_batch_single(r#"[{"error":{"message":"UNAUTHORIZED"}}]"#).unwrap_err();
        assert!(err.contains("UNAUTHORIZED"));
    }
}
