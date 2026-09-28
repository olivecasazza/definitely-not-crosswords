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
