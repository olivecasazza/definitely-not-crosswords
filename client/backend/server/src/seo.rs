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
//!    `Disallow: /` on a closed surface.
//! 2. `X-Robots-Tag: noindex, nofollow` on every response from a closed
//!    surface. This is the reliable layer here: the body is a WASM shell, so a
//!    `<meta name="robots">` only reaches a crawler that executes the module.
//!    A response header is read before the body is parsed at all.
//!
//! ## What decides "closed": the Pro price, not the environment name
//!
//! Being *called* production is not a reason to be indexed. The thing that
//! makes a deploy safe to crawl is that everything it serves has been
//! announced — and the one thing on it that is not is the Pro price. So the
//! policy is keyed on [`PRO_PRICE_ANNOUNCED`] and the environment name only
//! narrows it further:
//!
//! * production, price unannounced (**every deploy today**) — closed, with both
//!   layers. This is the state the flag was added for: production used to be
//!   indexed purely for being named production, which is only true once the
//!   price it displays has been announced.
//! * production, price announced — open: no `X-Robots-Tag`, and a `robots.txt`
//!   that explicitly allows crawling rather than leaving the question to
//!   whatever the fallback answers.
//! * anything that isn't production (staging, local, a typo, a new environment
//!   the chart grows before this module learns about it) — closed, whatever
//!   the flag says. Staging is a pre-announcement surface by construction: it
//!   carries the beta banner and exists to be iterated on in public. A flag
//!   meant for production must not be able to expose it.
//!
//! The flag defaults to off and only an explicit affirmative opens it, so a
//! missing, misspelled, or newly-invented value resolves to closed. That is the
//! same fail-closed shape the environment check had: the failure mode of a
//! missing variable must be a protected deploy, never an exposed one.
//!
//! Nothing here gates access. Staging and production both stay fully reachable
//! for closed-alpha testers and for the Playwright e2e canary, which drives the
//! same public hostname (`e2e-canary.yml` sets `E2E_BASE_URL` to it) — `noindex`
//! and `nofollow` are indexing directives, not access control.

use axum::{
    http::{header::CONTENT_TYPE, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower_http::set_header::SetResponseHeaderLayer;

/// Env var saying the Pro price has been announced, so production may be
/// indexed.
///
/// Off until someone deliberately sets it, in the deploy that ships the launch
/// announcement — that is the whole point of the flag: opening indexing is a
/// decision with an owner, not a property of the environment name.
pub const PRO_PRICE_ANNOUNCED: &str = "PRO_PRICE_ANNOUNCED";

/// The `X-Robots-Tag` value sent on a closed surface. `nofollow` as well as
/// `noindex`: a closed surface links to nothing, but a crawler that indexed it
/// should not be pushed anywhere either.
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

/// Whether the Pro price counts as announced.
///
/// Only an explicit affirmative counts. `Some("ture")`, `Some("yes")`,
/// `Some("")` and `None` are all *not* announced: a flag whose whole job is to
/// hold production closed must fail closed on a typo, on a value from a future
/// revision of this parser, and on a variable nobody set.
pub fn price_announced(raw: Option<&str>) -> bool {
    let Some(raw) = raw else {
        return false;
    };
    let value = raw.trim();
    value.eq_ignore_ascii_case("true") || value == "1"
}

/// [`price_announced`] for the running process: reads [`PRO_PRICE_ANNOUNCED`].
pub fn price_announced_from_env() -> bool {
    price_announced(std::env::var(PRO_PRICE_ANNOUNCED).ok().as_deref())
}

/// Whether a deploy is a public surface meant to be indexed.
///
/// Both conditions have to hold, and each fails closed. `production` alone is
/// not enough — that is the change this predicate exists for: a production
/// deploy is crawling-safe only once the price it displays is announced. And
/// the flag alone is not enough either, so a value meant for production cannot
/// expose staging, local, or a misspelled environment.
pub fn is_indexed(env: &str, price_announced: bool) -> bool {
    env == "production" && price_announced
}

/// The `robots.txt` body for a deploy.
pub fn robots_txt(env: &str, price_announced: bool) -> &'static str {
    if is_indexed(env, price_announced) {
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

/// Serve `/robots.txt` and, on a closed surface, mark every response
/// `noindex`.
///
/// Call this LAST, after the `WEB_DIST` block in `main.rs`. `Router::layer`
/// wraps the routes and the SPA fallback together, so applying it earlier would
/// leave the fallback — which answers `/` and every client-side route — without
/// the header. It is `if_not_present`, so a handler that sets the header
/// itself still wins.
pub fn protect_index<S>(app: Router<S>, env: &str, price_announced: bool) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    // Resolved once at startup, not per request: APP_ENV and
    // PRO_PRICE_ANNOUNCED are read once in `main.rs` and never change under a
    // running pod.
    let indexed = is_indexed(env, price_announced);
    let body = robots_txt(env, price_announced);

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The parsing rule, stated as data: only these open indexing. Everything
    /// else — including a misspelling of `true` and a value this parser has
    /// never seen — is "not announced", and not announced means closed.
    #[test]
    fn only_an_explicit_affirmative_counts_as_announced() {
        for open in [Some("true"), Some("TRUE"), Some(" True "), Some("1")] {
            assert!(
                price_announced(open),
                "{open:?} is an explicit affirmative and must open indexing"
            );
        }
        for closed in [None, Some(""), Some("  "), Some("ture"), Some("yes")] {
            assert!(
                !price_announced(closed),
                "{closed:?} must resolve to closed, never to indexed"
            );
        }
    }

    /// Production is indexed only once the price is announced, and never
    /// because it happens to be named `production`.
    #[test]
    fn production_needs_the_announcement_and_staging_never_has_it() {
        assert!(
            !is_indexed("production", false),
            "an unannounced price must keep production closed"
        );
        assert!(is_indexed("production", true), "announced: open");
        // The flag is for production. A deploy that sets it anywhere else must
        // not be able to expose itself.
        for env in ["staging", "local", "stagng", ""] {
            assert!(
                !is_indexed(env, true),
                "{env:?} must stay closed even with the flag on"
            );
        }
    }

    #[test]
    fn robots_txt_follows_the_predicate() {
        assert_eq!(robots_txt("production", false), CLOSED_ROBOTS);
        assert_eq!(robots_txt("production", true), OPEN_ROBOTS);
        assert_eq!(robots_txt("staging", true), CLOSED_ROBOTS);
    }
}
