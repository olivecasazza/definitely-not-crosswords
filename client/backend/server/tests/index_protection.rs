//! Index protection for the SPA origin (DEF-126, DEF-130).
//!
//! The origin carries whatever copy its build was compiled with, and the thing
//! on it that is not announced is the Pro price. So the origin has to refuse
//! indexing itself until the price is announced. Two layers, both on a closed
//! surface:
//!
//!   * `/robots.txt` must be a real `text/plain` 200 carrying `Disallow: /`. It
//!     used to fall through to the SPA fallback and answer 200 with the HTML
//!     shell, which no robots parser can read.
//!   * every response carries `X-Robots-Tag: noindex, nofollow`, including `/`
//!     — the shell is WASM, so a body-level meta tag only reaches crawlers that
//!     execute the module.
//!
//! "Closed" is not "not production": production is closed too, until
//! [`seo::PRO_PRICE_ANNOUNCED`] says the price is public. Being named
//! production is not a reason to be crawled. Nothing that isn't production is
//! indexable at all, whatever the flag says.
//!
//! No database: the app here is a stand-in with the same shape as `main.rs`'s —
//! one HTML route plus the SPA fallback that used to answer `/robots.txt`.

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    routing::get,
    Router,
};
use crossword_server::seo;
use tower::Service;

/// The shell `main.rs` serves as its fallback, trimmed to what matters here.
const SHELL: &str = "<!doctype html><html><head><title>definitely-not-crosswords</title></head>\
                    <body><div id=\"main\"></div></body></html>";

/// A router shaped like the real one: an API route, plus the SPA fallback that
/// answers `/` and every client-side path, plus index protection.
///
/// `announced` is the parsed `PRO_PRICE_ANNOUNCED`, passed in rather than read
/// from the environment so each test states the deploy it means and no two tests
/// race over a process-wide variable.
fn app(env: &str, announced: bool) -> Router {
    seo::protect_index(
        Router::new()
            .route("/api/healthz", get(|| async { "ok" }))
            .fallback(move || {
                let body = SHELL.to_string();
                async move { ([(header::CACHE_CONTROL, "no-store")], body) }
            }),
        env,
        announced,
    )
}

/// `GET path` against the app for this deploy, returning status,
/// `X-Robots-Tag` and body.
///
/// The body string is prefixed with the `Content-Type` so one value carries
/// both assertions a test needs to make about the same response.
async fn fetch(env: &str, announced: bool, path: &str) -> (StatusCode, Option<String>, String) {
    let mut app = app(env, announced);
    let req = Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("static request builds");
    let res = app.call(req).await.expect("router is infallible");
    let status = res.status();
    let noindex = res
        .headers()
        .get("x-robots-tag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let content_type = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("body reads");
    let body = String::from_utf8(bytes.to_vec()).expect("utf-8 shell");
    (status, noindex, format!("{content_type:?}|{body}"))
}

#[tokio::test]
async fn staging_robots_txt_is_a_real_robots_file_not_the_spa_fallback() {
    let (status, _, body) = fetch("staging", false, "/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.starts_with("Some(\"text/plain"),
        "robots.txt must be text/plain, got {body}"
    );
    assert!(body.contains("User-agent: *"), "{body}");
    assert!(body.contains("Disallow: /"), "{body}");
    assert!(
        !body.contains("<html"),
        "robots.txt answered with the HTML shell: {body}"
    );
}

#[tokio::test]
async fn staging_html_carries_noindex() {
    // `/` is the SPA fallback, and so is every client-side route — if the
    // header only reached real routes, the whole surface would still be
    // indexable at the one URL a crawler lands on.
    let (status, noindex, body) = fetch("staging", false, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<html"), "{body}");
    assert_eq!(noindex.as_deref(), Some("noindex, nofollow"), "/ header");
}

#[tokio::test]
async fn staging_api_responses_carry_noindex_too() {
    let (_, noindex, _) = fetch("staging", false, "/api/healthz").await;
    assert_eq!(noindex.as_deref(), Some("noindex, nofollow"));
}

#[tokio::test]
async fn local_is_closed_too() {
    // Local is not indexed by anything, but the same build artefact ships there
    // and an unknown APP_ENV must not be assumed open.
    let (_, noindex, _) = fetch("local", false, "/").await;
    assert_eq!(noindex.as_deref(), Some("noindex, nofollow"));
    let (_, _, body) = fetch("local", false, "/robots.txt").await;
    assert!(body.contains("Disallow: /"), "{body}");
}

#[tokio::test]
async fn unknown_environment_is_closed() {
    // Only `production` is a candidate for indexing, and only with the price
    // announced. A typo'd APP_ENV, or an environment added to the chart before
    // this predicate learns about it, must not expose itself.
    let (_, noindex, _) = fetch("stagng", true, "/").await;
    assert_eq!(noindex.as_deref(), Some("noindex, nofollow"));
    let (_, _, body) = fetch("stagng", true, "/robots.txt").await;
    assert!(body.contains("Disallow: /"), "{body}");
}

/// DEF-130: the exposure this issue closes. Production runs the same wasm as
/// staging, so while the Pro price is unannounced it must answer exactly as
/// staging does — both layers, on every response, and never the HTML shell.
#[tokio::test]
async fn production_is_closed_until_the_price_is_announced() {
    let (status, noindex, body) = fetch("production", false, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        noindex.as_deref(),
        Some("noindex, nofollow"),
        "production with an unannounced price must not be indexable"
    );
    assert!(body.contains("<html"), "{body}");

    let (status, noindex, body) = fetch("production", false, "/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(noindex.as_deref(), Some("noindex, nofollow"));
    assert!(
        body.starts_with("Some(\"text/plain"),
        "robots.txt must be text/plain, not the shell: {body}"
    );
    assert!(body.contains("User-agent: *"), "{body}");
    assert!(body.contains("Disallow: /"), "{body}");
    assert!(!body.contains("Allow: /"), "{body}");
    assert!(!body.contains("<html"), "{body}");

    // API responses too: a crawler can reach them without the shell.
    let (_, noindex, _) = fetch("production", false, "/api/healthz").await;
    assert_eq!(noindex.as_deref(), Some("noindex, nofollow"));
}

/// The other half of the same decision: once the price IS announced, production
/// is a real indexed surface again — and only then.
#[tokio::test]
async fn production_is_indexable_once_the_price_is_announced() {
    let (status, noindex, body) = fetch("production", true, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(noindex, None, "production must not send X-Robots-Tag");
    assert!(body.contains("<html"), "{body}");

    // And its robots.txt says so explicitly, instead of the malformed HTML the
    // fallback used to answer with.
    let (status, noindex, body) = fetch("production", true, "/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(noindex, None, "production must not send X-Robots-Tag");
    assert!(body.starts_with("Some(\"text/plain"), "{body}");
    assert!(body.contains("Allow: /"), "{body}");
    assert!(!body.contains("Disallow: /"), "{body}");
}

/// The flag opens production. It does not open staging, which is a
/// pre-announcement surface by construction — a value set in the wrong
/// deployment must not expose the wrong host.
#[tokio::test]
async fn the_announcement_flag_does_not_open_staging() {
    let (_, noindex, _) = fetch("staging", true, "/").await;
    assert_eq!(noindex.as_deref(), Some("noindex, nofollow"));
    let (_, _, body) = fetch("staging", true, "/robots.txt").await;
    assert!(body.contains("Disallow: /"), "{body}");
    assert!(!body.contains("Allow: /"), "{body}");
}
