//! Which documents the SPA origin serves (DEF-191).
//!
//! The fallback that boots the wasm shell used to answer 200 `text/html` for
//! everything the bundle didn't own, so `/sitemap.xml`, `/favicon.ico` and any
//! genuinely missing file were all reported as existing pages. That is
//! soft-404 behaviour, and it is the wrong thing to fix before a surface is
//! deliberately opened to indexing.
//!
//! The split under test, and why it is not "404 everything unknown": the
//! fallback is load-bearing for client-side routing — deep-linking into
//! `/game/<id>` has to boot the app — and the only thing that knows which
//! paths are routes is the client's own route table. So a path that names a
//! *file* (its last segment has an extension) is missing, because there are no
//! files at the origin outside `/_assets`; everything else is the app's and
//! gets the shell at 200.
//!
//! These run the real `spa::mount` over a real (temp) `WEB_DIST` tree, composed
//! in the same order `main.rs` composes it — `spa::mount` first,
//! `seo::protect_index` last — so the `X-Robots-Tag` behaviour on a 404 is the
//! one production would send, not a property of a stand-in router. No database:
//! nothing here touches `AppState`.

use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
    routing::get,
    Router,
};
use crossword_server::{seo, spa};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tower::Service;

/// The shell `WEB_DIST` serves: stands in for the built `index.html`.
const SHELL: &str = "<!doctype html><html><head><title>definitely-not-crosswords</title></head>\
                    <body><div id=\"main\"></div></body></html>";

const GLUE: &[u8] = b"/* glue */";

/// A `WEB_DIST` tree shaped like the real one: a shell at the root, bundle files
/// under `/_assets/<content-hash>/`.
///
/// Built once per test binary: the tests share it read-only, and rebuilding it
/// per test would have them delete each other's tree mid-request.
fn web_dist() -> &'static Path {
    static DIST: OnceLock<PathBuf> = OnceLock::new();
    DIST.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("dnc-spa-fallback-{}", std::process::id()));
        // Rebuilt from scratch: a tree left by an earlier run of a different
        // shape would answer for this one.
        let _ = fs::remove_dir_all(&dir);
        let assets = dir.join("_assets").join("deadbeefdeadbeef");
        fs::create_dir_all(&assets).expect("temp dist is writable");
        fs::write(assets.join("crossword-web.js"), GLUE).expect("write glue");
        fs::write(dir.join("index.html"), SHELL).expect("write shell");
        dir
    })
    .as_path()
}

/// The origin as `main.rs` builds it: the API routes, then the bundle and its
/// fallback, then index protection layered over both.
fn app(env: &str, announced: bool, dist: &Path) -> Router {
    let dist = dist.to_str().expect("temp path is utf-8");
    // `origin::for_env` reads `APP_ORIGIN`, which these tests must not be
    // perturbed by whatever the developer's shell has exported, so the
    // environment's own default is what gets asserted on here. The
    // placeholder-resolution tests below take an explicit origin instead.
    seo::protect_index(
        spa::mount(
            Router::new()
                .route("/api/healthz", get(|| async { "ok" }))
                .route("/api/trpc/:proc", get(|| async { "[]" })),
            dist,
            "https://crosswords.test",
        ),
        env,
        announced,
    )
}

/// `GET path` against the origin, returning status, `X-Robots-Tag`,
/// `Cache-Control` and a body string prefixed with the `Content-Type`.
///
/// The prefix lets one value carry both assertions a test makes about the same
/// response — that it is the expected type *and* that it is (or is not) the
/// shell.
async fn fetch(env: &str, announced: bool, dist: &Path, path: &str) -> Res {
    let mut app = app(env, announced, dist);
    let res = app
        .call(
            Request::builder()
                .uri(path)
                .body(Body::empty())
                .expect("static request builds"),
        )
        .await
        .expect("router is infallible");
    let status = res.status();
    let noindex = header_value(&res, "x-robots-tag");
    let cache_control = header_value(&res, header::CACHE_CONTROL.as_str());
    let content_type = header_value(&res, header::CONTENT_TYPE.as_str());
    let bytes = to_bytes(res.into_body(), usize::MAX)
        .await
        .expect("body reads");
    let body = String::from_utf8_lossy(&bytes).into_owned();
    Res {
        status,
        noindex,
        cache_control,
        body: format!("{content_type:?}|{body}"),
    }
}

struct Res {
    status: StatusCode,
    noindex: Option<String>,
    cache_control: Option<String>,
    body: String,
}

fn header_value(res: &axum::http::Response<Body>, name: &str) -> Option<String> {
    res.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// DEF-191 acceptance 4: deep-linking into an app route still returns the shell
/// at 200. This is the half that must not regress — a 404-everything fix would
/// pass every other test here and break the product.
#[tokio::test]
async fn client_side_routes_still_get_the_shell_at_200() {
    let dist = web_dist();
    for route in [
        "/",
        "/games",
        "/game/1f9a3f6e-0c1a-4a1e-9c1a-1f9a3f6e0c1a",
        "/game/1f9a3f6e-0c1a-4a1e-9c1a-1f9a3f6e0c1a/new",
        "/game/1f9a3f6e-0c1a-4a1e-9c1a-1f9a3f6e0c1a/completed",
        "/profile",
        "/stats",
        "/admin",
        "/auth/login",
        "/auth/verify-email",
        // The client's own not-found page still boots: an unknown path with no
        // extension is the router's to answer, not this origin's.
        "/this-page-does-not-exist",
        // Query strings are not part of the path.
        "/games?sort=new",
    ] {
        let res = fetch("production", true, dist, route).await;
        assert_eq!(res.status, StatusCode::OK, "{route} must boot the shell");
        assert!(
            res.body.contains("<html"),
            "{route} must be answered with the shell, got {}",
            res.body
        );
        assert!(
            res.body.contains("text/html"),
            "{route} must stay HTML, got {}",
            res.body
        );
        // DEF-140/145/152: a stored shell is a stale bundle, and a stored 404
        // is a pinned one. Neither is ever stored.
        assert_eq!(
            res.cache_control.as_deref(),
            Some("no-store"),
            "{route} must not be stored"
        );
    }
}

/// DEF-191 acceptance 1, 2 and 3: the two measured production paths, and the
/// general rule behind them. Before the fix all of these were 200 with the HTML
/// shell.
#[tokio::test]
async fn a_path_that_names_a_file_is_a_real_404() {
    let dist = web_dist();
    for file in [
        "/favicon.ico",
        "/sitemap.xml",
        "/apple-touch-icon.png",
        "/site.webmanifest",
        "/old-index.html",
        "/crossword.css",
        // Under /_assets there is a real static service in front of the
        // fallback, and a bundle file that isn't there is missing there too.
        "/_assets/deadbeefdeadbeef/missing.js",
    ] {
        let res = fetch("production", true, dist, file).await;
        assert_eq!(
            res.status,
            StatusCode::NOT_FOUND,
            "{file} must be a real 404, not the shell at 200"
        );
        assert!(
            !res.body.contains("<html"),
            "{file} must not be answered with the HTML shell, got {}",
            res.body
        );
        assert!(
            !res.body.contains("definitely-not-crosswords"),
            "{file} must not carry the shell's title, got {}",
            res.body
        );
        // A 404 that the edge stores outlives the reason for it.
        assert!(
            !res.cache_control
                .as_deref()
                .unwrap_or_default()
                .contains("immutable"),
            "{file} must not be cached immutably (got {:?})",
            res.cache_control
        );
    }
}

/// A missing bundle file is a missing bundle file: 404, and never advertised as
/// immutable — the poisoning guard from DEF-140/145/152, now reachable through
/// the fallback as well as through ServeDir.
#[tokio::test]
async fn a_missing_bundle_file_is_404_and_never_immutable() {
    let dist = web_dist();
    let res = fetch(
        "production",
        true,
        dist,
        "/_assets/ffffffffffffffff/crossword-web.js",
    )
    .await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let cc = res.cache_control.as_deref().unwrap_or_default();
    assert!(!cc.contains("immutable"), "{cc:?}");
    assert!(!cc.contains("31536000"), "{cc:?}");
}

/// The bundle itself is untouched: the assets service still answers 200 for the
/// bytes it has, still immutable, and still not the shell.
#[tokio::test]
async fn a_real_bundle_asset_is_still_served_from_assets() {
    let dist = web_dist();
    let res = fetch(
        "production",
        true,
        dist,
        "/_assets/deadbeefdeadbeef/crossword-web.js",
    )
    .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(
        res.cache_control.as_deref(),
        Some("public, max-age=31536000, immutable")
    );
    assert!(
        res.body.contains("/* glue */"),
        "the asset bytes must be served, got {}",
        res.body
    );
    assert!(!res.body.contains("<html"), "got {}", res.body);
}

/// DEF-191 acceptance 5: a 404 says so out loud on a closed surface. The header
/// is layered over the whole router, fallback included, so the document that
/// says "this page is missing" also says "do not index me" — a crawler that only
/// ever sees the 404 still learns the surface is closed.
#[tokio::test]
async fn the_404_carries_noindex_on_a_closed_surface() {
    let dist = web_dist();
    for env in ["production", "staging", "local"] {
        for file in ["/favicon.ico", "/sitemap.xml", "/_assets/nope.js"] {
            let res = fetch(env, false, dist, file).await;
            assert_eq!(res.status, StatusCode::NOT_FOUND, "{env} {file}");
            assert_eq!(
                res.noindex.as_deref(),
                Some("noindex, nofollow"),
                "{env} {file} must carry the header a closed surface sends"
            );
        }
    }
}

/// And an open surface sends neither: the 404 is a real 404 and nothing more.
#[tokio::test]
async fn the_404_carries_no_robots_header_on_an_open_surface() {
    let dist = web_dist();
    for file in ["/favicon.ico", "/sitemap.xml"] {
        let res = fetch("production", true, dist, file).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{file}");
        assert_eq!(res.noindex, None, "{file} must not send X-Robots-Tag");
    }
}

/// The routes that existed before the fallback still win over it, including
/// `/robots.txt`, which is added by `protect_index` *after* `spa::mount` — a
/// fallback installed on top of it would turn a robots file back into HTML.
#[tokio::test]
async fn real_routes_still_win_over_the_fallback() {
    let dist = web_dist();
    let res = fetch("staging", false, dist, "/api/healthz").await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.ends_with("|ok"), "got {}", res.body);

    let res = fetch("staging", false, dist, "/robots.txt").await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(
        res.body.contains("User-agent: *"),
        "robots.txt must be a real robots file, got {}",
        res.body
    );
    assert!(res.body.contains("Disallow: /"), "got {}", res.body);
}

// --- DEF-196: the canonical is per-URL, not a consolidation hint ------------
//
// One image serves staging and production, and the build cannot know which of
// the two it is producing a canonical for, so the served shell carries a
// placeholder and `spa::mount` resolves it against the origin it is given.
// These run the real `spa::mount` over a shell shaped like the real one.

/// A shell with the absolute-URL tags `client/flake.nix` actually writes, and
/// the placeholder in each. The `og:image` path is the bundle's own hashed
/// directory, so it exercises the interaction of both placeholders.
const SHELL_WITH_ORIGIN: &str = concat!(
    "<!doctype html><html><head>",
    "<link rel=\"canonical\" href=\"__ORIGIN__/\" />",
    "<meta property=\"og:url\" content=\"__ORIGIN__/\" />",
    "<meta property=\"og:image\" content=\"__ORIGIN__/_assets/deadbeefdeadbeef/og.png\" />",
    "</head><body><div id=\"main\"></div></body></html>"
);

/// A dist whose shell is `SHELL_WITH_ORIGIN`, distinct per origin so the two
/// tests below cannot share a tree.
fn dist_for(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("dnc-origin-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let assets = dir.join("_assets").join("deadbeefdeadbeef");
    fs::create_dir_all(&assets).expect("temp dist is writable");
    fs::write(dir.join("index.html"), SHELL_WITH_ORIGIN).expect("write shell");
    dir
}

/// `GET /` against a server mounted with `origin`, returning the body.
async fn shell_served_with(origin: &str, dist: &Path) -> String {
    let dist = dist.to_str().expect("temp path is utf-8");
    let mut app = spa::mount(Router::new(), dist, origin);
    let res = app
        .call(
            Request::builder()
                .uri("/")
                .body(Body::empty())
                .expect("static request builds"),
        )
        .await
        .expect("the fallback answers /");
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
        .await
        .expect("shell body reads");
    String::from_utf8(bytes.to_vec()).expect("the shell is utf-8")
}

/// The regression, stated as the served bytes: a staging deploy must not claim
/// production is where it lives. It used to, from one shared artifact.
#[tokio::test]
async fn staging_does_not_serve_production_as_its_own_canonical() {
    let dist = dist_for("staging");
    let body = shell_served_with("https://crosswords-staging.casazza.io", &dist).await;

    assert!(
        body.contains("rel=\"canonical\" href=\"https://crosswords-staging.casazza.io/\""),
        "staging must name its own origin, got {body}"
    );
    assert!(
        !body.contains("crosswords.casazza.io"),
        "staging's served HTML still points somewhere on the production host, got {body}"
    );
    assert!(
        !body.contains("__ORIGIN__"),
        "an unresolved placeholder would ship to a scraper, got {body}"
    );
}

/// And the fix must not cost production its own canonical: that one WAS right
/// before, and a change that made both hosts name staging would be a new bug of
/// the same shape.
#[tokio::test]
async fn production_still_names_production() {
    let dist = dist_for("production");
    let body = shell_served_with("https://crosswords.casazza.io", &dist).await;

    assert!(
        body.contains("rel=\"canonical\" href=\"https://crosswords.casazza.io/\""),
        "production must still name itself, got {body}"
    );
    assert!(
        !body.contains("crosswords-staging"),
        "production must not name staging, got {body}"
    );
}

/// `og:image` is an absolute URL resolved against the same origin, and its path
/// has to survive substitution intact — the card lives inside the bundle's own
/// hashed directory, and a mangled path is an empty preview box.
#[tokio::test]
async fn og_image_keeps_its_hashed_path_under_the_serving_origin() {
    let dist = dist_for("ogimage");
    let body = shell_served_with("https://crosswords-staging.casazza.io", &dist).await;

    assert!(
        body.contains(
            "property=\"og:image\" content=\"https://crosswords-staging.casazza.io/_assets/deadbeefdeadbeef/og.png\""
        ),
        "og:image must be the serving origin plus the bundle's own hashed path, got {body}"
    );
    assert!(!body.contains("//_assets"), "doubled separator, got {body}");
}

/// A dist built before the placeholder existed keeps its own bytes. This is the
/// state a rolling update is briefly in, and it is a valid document — so it has
/// to be served, not refused.
#[tokio::test]
async fn a_shell_without_the_placeholder_is_still_served() {
    let body = shell_served_with("https://crosswords-staging.casazza.io", web_dist()).await;
    assert!(
        body.contains("<title>definitely-not-crosswords</title>"),
        "got {body}"
    );
}
