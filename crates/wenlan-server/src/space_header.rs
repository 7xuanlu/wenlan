// SPDX-License-Identifier: Apache-2.0
//! `X-Wenlan-Space` HTTP header extractor.
//!
//! When the request includes `X-Wenlan-Space: <name>`, this extractor
//! returns `Some(name)`. Handlers use it as a fallback applied only when
//! the request body omits the `space` field. Explicit body `space` always
//! wins to preserve the user's per-call override path.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use std::convert::Infallible;

pub const HEADER_NAME: &str = "x-origin-space";

#[derive(Debug, Clone, Default)]
pub struct SpaceHeader(pub Option<String>);

impl<S> FromRequestParts<S> for SpaceHeader
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let val = parts
            .headers
            .get(HEADER_NAME)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Ok(SpaceHeader(val))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    fn build_parts(headers: &[(&str, &str)]) -> Parts {
        let mut req = Request::builder();
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        req.body(()).unwrap().into_parts().0
    }

    #[tokio::test]
    async fn missing_header_yields_none() {
        let mut parts = build_parts(&[]);
        let SpaceHeader(val) = SpaceHeader::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn present_header_yields_some() {
        let mut parts = build_parts(&[("X-Wenlan-Space", "career")]);
        let SpaceHeader(val) = SpaceHeader::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert_eq!(val.as_deref(), Some("career"));
    }

    #[tokio::test]
    async fn empty_header_yields_none() {
        let mut parts = build_parts(&[("X-Wenlan-Space", "   ")]);
        let SpaceHeader(val) = SpaceHeader::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        assert_eq!(val, None);
    }
}
