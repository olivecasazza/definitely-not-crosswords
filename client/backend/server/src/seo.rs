//! Crawler/indexing control for the origin that serves the wasm SPA.
//!
//! The frontend and the API share one origin (see the `WEB_DIST` block in
//! `main.rs`), so this origin also decides what a crawler can index about a
//! deploy. That matters because a deploy is not just a running app: it carries
//! whatever copy that build was compiled with. Staging is a public DNS name
//! with a public TLS cert, so anything it serves is reachable by every crawler
//! and every curious visitor — including positioning and Pro pricing that has
//! not been announced or signed off. Nothing about the surface says "not
//! public": no announcement, no auth wall, and a bare `GET /robots.txt` fell
//! through to the SPA fallback and answered 200 with the HTML shell, which no
//! robots parser can read as a directive.
//!
//! So the origin states its own indexing policy, in the two layers crawlers
//! actually honour:
//!
//! 1. `/robots.txt` — a real `text/plain` 200 (a route, not the fallback),
//!    `Disallow: /` off production.
//! 2. `X-Robots-Tag: noindex, nofollow` on every response off production.
//!    This is the reliable layer here: the body is a WASM shell, so a
//!    `<meta name="robots">` only reaches a crawler that executes the module.
//!    A response header is read before the body is parsed at all.
//!
//! Both are per-environment. Production is a real indexed surface and gets
//! neither: it sends no `X-Robots-Tag`, and its `robots.txt` explicitly allows
//! crawling rather than leaving the question to whatever the fallback answers.
//!
//! Nothing here gates access. Staging stays fully reachable for closed-alpha
//! testers and for the Playwright e2e canary, which drives the same public
//! hostname (`e2e-canary.yml` sets `E2E_BASE_URL` to it) — `noindex` and
//! `nofollow` are indexing directives, not access control.

use axum::{
    http::{header::CONTENT_TYPE, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower_http::set_header::SetResponseHeaderLayer;

/// The `X-Robots-Tag` value sent on non-production responses. `nofollow` as
/// well as `noindex`: staging's shell links to nothing, but a crawler that
/// indexed it should not be pushed anywhere either.
pub const NOINDEX: &str = "noindex, nofollow";

/// Body served for a surface that must stay unindexed.
const CLOSED_ROBOTS: &str = "User-agent: *\nDisallow: /\n";

/// Body served for a surface that is meant to be indexed.
///
/// Explicit rather than absent. Before this module existed, `/robots.txt` was
/// answered by the SPA fallback with the HTML shell: a parser reads that as
/// malformed, which is a worse answer than a deliberate "crawl freely" — and a
/// malformed file is exactly the kind of thing that gets reported as a broken
/// site by tooling that expects the file to exist.
const OPEN_ROBOTS: &str = "User-agent: *\nAllow: /\n";

/// Whether a deploy environment is a public surface meant to be indexed.
///
/// Only `production` is. `APP_ENV` defaults to `production` in `main.rs`, so an
/// unset variable is treated as the production surface it claims to be — that
/// default is correct for a real deploy. Everything else, including an
/// unrecognised name (a typo, a new environment added to the chart before this
/// function learns about it), is closed: a surface nobody has announced is
/// better protected than one nobody meant to expose.
pub fn is_indexed(env: &str) -> bool {
    env == "production"
}

/// The `robots.txt` body for an environment.
pub fn robots_txt(env: &str) -> &'static str {
    if is_indexed(env) {
        OPEN_ROBOTS
    } else {
        CLOSED_ROBOTS
    }
}

/// The robots handler: a real `text/plain` 200, never the SPA shell.
async fn robots(body: &'static str) -> Response {
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

/// Serve `/robots.txt` and, off production, mark every response `noindex`.
///
/// Call this LAST, after the `WEB_DIST` block in `main.rs`. `Router::layer`
/// wraps the routes and the SPA fallback together, so applying it earlier would
/// leave the fallback — which answers `/` and every client-side route — without
/// the header. It is `if_not_present`, so a handler that sets the header
/// itself still wins.
pub fn protect_index<S>(app: Router<S>, env: &str) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    // Resolved once at startup, not per request: APP_ENV is read once in
    // `main.rs` and never changes under a running pod.
    let indexed = is_indexed(env);
    let body = robots_txt(env);

    // A route, not a fallback match: this is what makes `/robots.txt` a robots
    // file instead of a 200 with the HTML shell in it.
    let app = app.route("/robots.txt", get(move || robots(body)));

    if indexed {
        return app;
    }

    app.layer(SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static(NOINDEX),
    ))
}
