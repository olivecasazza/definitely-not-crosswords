//! Which documents the SPA origin serves, and which it refuses to (DEF-191).
//!
//! The origin that serves the wasm bundle also has to answer for the paths
//! *around* it. `/_assets` holds the bundle; every other path is the fallback
//! that boots the shell for client-side routing. The trap is that "every other
//! path" is not the same as "every other route": the fallback used to answer
//! **200 `text/html` for literally anything**, which produced two kinds of lie.
//!
//! * A crawler asking for `/sitemap.xml` or `/favicon.ico` got HTML at 200 —
//!   a parser concludes the site has neither, having been handed a document it
//!   cannot use.
//! * Any genuinely missing path answered 200 with the shell, which is
//!   *soft-404 behaviour*: a status that says "this page exists" for a page
//!   that does not. Search engines treat it as an indexing signal in its own
//!   right, so a domain whose every sibling "exists" cannot be crawled into a
//!   useful site map — worse than staying closed.
//!
//! The fix is not "404 everything unknown". The fallback is load-bearing:
//! deep-linking into `/game/<id>` has to boot the app, and the app's own route
//! table is the only thing that knows `/game/<id>` from `/this-page-does-not-
//! exist`. So the split is drawn where it can be drawn honestly, on the shape
//! of the request:
//!
//! * **A path whose last segment carries a file extension is a request for a
//!   file, not a route.** There are no files at the origin outside `/_assets`
//!   — no `robots.txt` (that is a route, see [`crate::seo`]), no favicon, no
//!   sitemap — so every one of them is missing, and a missing file gets a real
//!   [`StatusCode::NOT_FOUND`] rather than a page that claims otherwise.
//! * **Everything else is the app's**, extension-free and extensioned-alike
//!   alike, and gets the shell at 200. That is what keeps client-side routing
//!   working, and it is why this is a routing change and not a policy change.
//!
//! What remains deliberately is that an *unknown extension-free* path
//! (`/this-page-does-not-exist`) still boots the shell at 200: the Dioxus route
//! table ends in `/:..segments`, so the client renders its own not-found page
//! for it. Telling those apart from real routes server-side would mean a second
//! copy of the client's route table in this crate, which rots the moment a
//! route is added. The status code the shell is served with is the honest part,
//! and it now is.
//!
//! Layering: call [`mount`] **before** [`crate::seo::protect_index`], so
//! `X-Robots-Tag` still lands on the 404s as well as on the shell — a closed
//! surface must say so on the document that says the page is missing, not just
//! on the one a crawler lands on.

use axum::{
    extract::Request,
    http::{header::CACHE_CONTROL, StatusCode},
    response::{Html, IntoResponse, Response},
    Router,
};
use std::path::Path;
use tower::ServiceBuilder;

use crate::assets::CacheControlOnOkLayer;

/// The 404 body. Plain text on purpose: the HTML shell *is* the claim that the
/// requested page exists, so it cannot also be the evidence that it doesn't.
const NOT_FOUND_BODY: &str = "Not found\n";

/// `Cache-Control` for the fallback, both the 200 shell and the 404.
///
/// `no-store` on the shell is load-bearing: index.html is the POINTER to the
/// current bundle and must never be reused. `no-cache` is NOT enough here —
/// every file in a nix store path has mtime 1970-01-01T00:00:01 and ServeDir
/// sends Last-Modified with no ETag, so every release advertises an identical
/// validator; a revalidation of changed content answers 304 and refreshes the
/// stale entry's TTL indefinitely (DEF-140/145/152). `no-store` means it is
/// never stored, so it is never conditionally revalidated. (A bogus 304 under
/// /_assets is harmless by contrast: that content genuinely never changes.)
///
/// It is the right answer for the 404 for the same reason the assets layer
/// refuses to mark a 404 `immutable`: a 404 stored at the edge outlives the
/// reason for it. Neither body changes meaningfully; neither may be pinned.
const FALLBACK_CACHE_CONTROL: &str = "no-store";

/// Serve the built wasm frontend on `app`: `/_assets` from `{dist}/_assets`,
/// and the routing fallback for everything else.
///
/// This is the whole `WEB_DIST` block of `main.rs`, factored out so the
/// request-routing decisions in it are testable without a database or a
/// running server — the routing is the part of it that broke, and the part
/// that has to stay working.
///
/// # Panics
///
/// If `dist` has no `index.html`. A deploy pointed at a dist directory that
/// cannot boot the app has nothing to serve, and failing at startup is the
/// honest report — the alternative is a pod that answers every request with an
/// empty body.
pub fn mount<S>(app: Router<S>, dist: &str) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    // Assets live under /_assets/<content-hash>/ (see client/flake.nix), so
    // bytes at a given URL never change — cache them forever. BUT only stamp
    // `immutable` on a real 200: `SetResponseHeaderLayer:: overriding` fires on
    // every response including ServeDir's 404s, and a
    // `immutable, max-age=31536000` on a transiently-missing asset is what
    // pinned a year-long cached 404 at the edge (DEF-140/145/152). Errors get
    // no explicit max-age, so the edge revalidates them.
    let assets = ServiceBuilder::new().layer(CacheControlOnOkLayer).service(
        tower_http::services::ServeDir::new(format!("{dist}/_assets")),
    );

    let index_html = std::fs::read_to_string(format!("{dist}/index.html"))
        .expect("WEB_DIST is set but index.html is missing");
    tracing::info!("serving frontend bundle from {dist}");

    app.nest_service("/_assets", assets)
        .fallback(move |req: Request| {
            // Decided before the async block, so the borrow of the request ends
            // there. The shell is only cloned on the paths that get it: a 404
            // must not pay for a copy of the document it refuses to serve.
            let is_route = is_client_route(req.uri().path());
            async move {
                if is_route {
                    shell_response(index_html.clone())
                } else {
                    not_found_response()
                }
            }
        })
}

/// Whether `path` belongs to the client-side router, i.e. whether the shell has
/// to boot for it. The complement of "names a file" — see the module docs for
/// why that is the line.
pub fn is_client_route(path: &str) -> bool {
    !names_a_file(path)
}

/// Whether the request is for a file rather than for a route, decided by the
/// one thing that tells them apart without a copy of the client's route table:
/// the final segment's file extension.
///
/// `/favicon.ico`, `/sitemap.xml`, `/site.webmanifest` and
/// `/_assets/<hash>/missing.js` all name files. `/`, `/games` and
/// `/game/<uuid>` do not, and neither does `/.well-known` — a leading dot is
/// not an extension, which is why this asks `Path::extension` rather than
/// looking for a dot.
///
/// The honest reading of `/game/1.5` is that it names a file too: no route
/// segment in the app carries a dot (`/game/:id` is a UUID), so the 404 it
/// earns is the correct one.
fn names_a_file(path: &str) -> bool {
    Path::new(path).extension().is_some()
}

/// The shell at 200: what every client-side route, and `/`, is answered with.
fn shell_response(index_html: String) -> Response {
    (
        StatusCode::OK,
        [(CACHE_CONTROL, FALLBACK_CACHE_CONTROL)],
        Html(index_html),
    )
        .into_response()
}

/// A real 404: a status, a `text/plain` body, and the same no-store policy as
/// the shell so the edge cannot pin it.
fn not_found_response() -> Response {
    (
        StatusCode::NOT_FOUND,
        [
            (CACHE_CONTROL, FALLBACK_CACHE_CONTROL),
            (
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            ),
        ],
        NOT_FOUND_BODY,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    //! The decision, stated as data. These are the paths the origin is asked
    //! about; the first group boots the app, the second is a missing file.

    use super::*;

    #[test]
    fn client_side_routes_boot_the_shell() {
        for route in [
            "/",
            "/games",
            "/game/1f9a3f6e-0c1a-4a1e-9c1a-1f9a3f6e0c1a",
            "/game/1f9a3f6e-0c1a-4a1e-9c1a-1f9a3f6e0c1a/completed",
            "/profile",
            "/stats",
            "/admin",
            "/auth/verify-email",
        ] {
            assert!(is_client_route(route), "{route} is a client route");
        }
    }

    #[test]
    fn file_requests_are_not_client_routes() {
        for file in [
            "/favicon.ico",
            "/sitemap.xml",
            "/apple-touch-icon.png",
            "/site.webmanifest",
            "/old-index.html",
            "/_assets/deadbeefdeadbeef/missing.js",
        ] {
            assert!(!is_client_route(file), "{file} names a file, not a route");
        }
    }

    /// A query string is not part of the path, so `?sort=new` cannot turn a
    /// route into a file request. The fallback reads `req.uri().path()`, which
    /// is what this pins.
    #[test]
    fn a_query_string_does_not_make_a_route_a_file() {
        let uri: axum::http::Uri = "/games?sort=new".parse().expect("static uri parses");
        assert_eq!(uri.path(), "/games");
        assert!(is_client_route(uri.path()));
    }

    /// A leading dot is not an extension. Getting this wrong would 404 every
    /// `/.well-known/…` path the app or an app-store validator asks for.
    #[test]
    fn dot_prefixed_segments_are_not_file_extensions() {
        assert!(is_client_route("/.well-known"));
        assert!(is_client_route("/.well-known/apple-app-site-association"));
    }
}
