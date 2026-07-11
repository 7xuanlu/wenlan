// SPDX-License-Identifier: Apache-2.0
use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use axum::routing::IntoMakeService;
use axum::Router;
use std::convert::Infallible;
use std::task::{Context, Poll};
use tower::Service;

#[derive(Clone)]
pub struct AppRouter {
    inner: Router,
}

impl AppRouter {
    pub(super) const fn new(inner: Router) -> Self {
        Self { inner }
    }

    pub fn into_make_service(self) -> IntoMakeService<Router> {
        self.inner.into_make_service()
    }
}

impl Service<Request<Body>> for AppRouter {
    type Response = Response;
    type Error = Infallible;
    type Future = <Router as Service<Request<Body>>>::Future;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        <Router as Service<Request<Body>>>::poll_ready(&mut self.inner, context)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        self.inner.call(request)
    }
}
