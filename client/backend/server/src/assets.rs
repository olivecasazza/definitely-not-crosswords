//! `/_assets` static serving and its cache-policy layer.

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::Response,
};
use std::task::{Context as TaskContext, Poll};

/// Stamps `Cache-Control: public, max-age=31536000, immutable` on `/_assets/*`
/// responses ONLY when the inner service answered 200. Defends the edge cache
/// against poisoning: a transient 404 (asset mid-deploy, WEB_DIST mismatch) must
/// never be advertised as immutable, or the edge turns a seconds-long blip into a
/// year-long cached 404 and the front door blanks for every visitor whose PoP
/// holds the bad object (DEF-140/145/152). Non-200 responses pass through with
/// whatever ServeDir set (typically none), so the edge revalidates them.
#[derive(Clone, Copy)]
pub struct CacheControlOnOkLayer;

impl<S> tower::Layer<S> for CacheControlOnOkLayer {
    type Service = CacheControlOnOk<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CacheControlOnOk { inner }
    }
}

#[derive(Clone)]
pub struct CacheControlOnOk<S> {
    inner: S,
}

impl<S, B> tower::Service<axum::http::Request<axum::body::Body>> for CacheControlOnOk<S>
where
    S: tower::Service<axum::http::Request<axum::body::Body>, Response = Response<B>>,
    S::Future: Send + 'static,
    B: http_body::Body<Data = axum::body::Bytes> + Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = futures_util::future::Map<
        S::Future,
        fn(Result<S::Response, S::Error>) -> Result<S::Response, S::Error>,
    >;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: axum::http::Request<axum::body::Body>) -> Self::Future {
        let fut = self.inner.call(req);
        futures_util::FutureExt::map(fut, |res| {
            res.map(|mut resp| {
                if resp.status() == StatusCode::OK {
                    resp.headers_mut().insert(
                        header::CACHE_CONTROL,
                        HeaderValue::from_static("public, max-age=31536000, immutable"),
                    );
                }
                resp
            })
        })
    }
}

#[cfg(test)]
mod tests {
    //! DEF-152 acceptance criterion 3: errors are not immutable. These prove
    //! `CacheControlOnOkLayer` stamps `immutable` on a real 200 but not on a
    //! 404, against a real ServeDir over a WEB_DIST-shaped tree.

    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tower::{Layer, Service, ServiceExt};

    fn web_dist() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dnc-assets-test-{}", std::process::id()));
        let assets = dir.join("_assets").join("deadbeefdeadbeef");
        fs::create_dir_all(&assets).unwrap();
        fs::write(assets.join("crossword-web.js"), b"/* glue */").unwrap();
        dir
    }

    type Svc = super::CacheControlOnOk<tower_http::services::ServeDir>;

    fn svc(web_dist: &PathBuf) -> Svc {
        let inner = tower_http::services::ServeDir::new(format!("{}/_assets", web_dist.display()));
        CacheControlOnOkLayer.layer(inner)
    }

    async fn get(svc: Svc, uri: &str) -> (StatusCode, axum::http::HeaderMap) {
        let resp = svc
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        (resp.status(), resp.headers().clone())
    }

    #[tokio::test]
    async fn ok_asset_is_immutable() {
        let dist = web_dist();
        let (status, headers) = get(svc(&dist), "/deadbeefdeadbeef/crossword-web.js").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers
                .get(header::CACHE_CONTROL)
                .and_then(|v| v.to_str().ok()),
            Some("public, max-age=31536000, immutable"),
            "a real 200 asset must be cached forever",
        );
    }

    #[tokio::test]
    async fn missing_asset_is_not_immutable() {
        let dist = web_dist();
        let (status, headers) = get(svc(&dist), "/deadbeefdeadbeef/nope.js").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let cc = headers
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            !cc.contains("immutable"),
            "a 404 must never be advertised as immutable (got Cache-Control: {cc:?})",
        );
        assert!(
            !cc.contains("31536000"),
            "a 404 must not carry the year-long max-age (got Cache-Control: {cc:?})",
        );
    }

    #[tokio::test]
    async fn missing_hash_dir_is_not_immutable() {
        let dist = web_dist();
        let (status, headers) = get(svc(&dist), "/ffffffffffffffff/crossword-web.js").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let cc = headers
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            !cc.contains("immutable") && !cc.contains("31536000"),
            "a 404 under an unknown content-hash must not be cached long-term (got {cc:?})",
        );
    }
}
