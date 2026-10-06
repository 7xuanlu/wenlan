// SPDX-License-Identifier: Apache-2.0
use crate::error::ServerError;
use crate::route_registry::{post, TrackedRouter};
use crate::security::{bind_scope_from_env, BindScope};
use crate::state::SharedState;
use crate::web_fetch::{fetch_with, FetchError, PublicTransport, TOTAL_TIMEOUT};
use axum::{http::StatusCode, Json};
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;
use wenlan_types::{requests::FetchWebpageRequest, responses::FetchWebpageResponse};

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router.route("/api/webpage/fetch", post(handle_fetch_webpage))
}

fn failure(code: &str, message: &str, status: StatusCode) -> ServerError {
    ServerError::Structured {
        status,
        body: serde_json::json!({
            "code": code, "error": message, "manual_excerpt_available": true
        }),
    }
}

impl From<FetchError> for ServerError {
    fn from(error: FetchError) -> Self {
        let (code, message, status) = match error {
            FetchError::InvalidUrl => ("invalid_url", "Use a public HTTP(S) URL without credentials or custom ports.", StatusCode::BAD_REQUEST),
            FetchError::BlockedDestination => ("blocked_destination", "This address is not an allowed public destination.", StatusCode::FORBIDDEN),
            FetchError::DnsFailed => ("dns_failed", "The public address could not be resolved.", StatusCode::BAD_GATEWAY),
            FetchError::RedirectLimit => ("too_many_redirects", "The page redirected too many times.", StatusCode::BAD_GATEWAY),
            FetchError::UnsupportedContentType => ("unsupported_content_type", "Only UTF-8 HTML and plain text can be fetched. Paste an excerpt instead.", StatusCode::UNSUPPORTED_MEDIA_TYPE),
            FetchError::UnsupportedEncoding => ("unsupported_encoding", "The site returned compressed content despite an identity request. Paste an excerpt instead.", StatusCode::UNSUPPORTED_MEDIA_TYPE),
            FetchError::TooLarge => ("response_too_large", "The page exceeds the fetch size limit. Paste an excerpt instead.", StatusCode::PAYLOAD_TOO_LARGE),
            FetchError::Timeout => ("timeout", "The page took too long to respond. Paste an excerpt instead.", StatusCode::GATEWAY_TIMEOUT),
            FetchError::Transport => ("fetch_failed", "The page could not be fetched. Paste an excerpt instead.", StatusCode::BAD_GATEWAY),
            FetchError::HttpStatus(_) => ("http_error", "The site refused the request or did not return a complete page. Paste an excerpt instead.", StatusCode::BAD_GATEWAY),
            FetchError::NoReadableText => ("no_readable_text", "No useful public text was found. Paste an excerpt instead.", StatusCode::UNPROCESSABLE_ENTITY),
        };
        failure(code, message, status)
    }
}

/// Public text preview, deliberately separate from the existing ingestion write.
/// The production router's browser guard still applies. Unlike ingestion, the
/// external-bind opt-out is never honored for this outbound-network capability.
pub async fn handle_fetch_webpage(
    request: Result<Json<FetchWebpageRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<FetchWebpageResponse>, ServerError> {
    if !matches!(
        bind_scope_from_env(),
        BindScope::Unset | BindScope::Loopback
    ) {
        return Err(failure(
            "local_only",
            "Public text fetching is available only on a loopback daemon.",
            StatusCode::FORBIDDEN,
        ));
    }
    let Json(request) = request.map_err(|error| failure("invalid_request", "Send a JSON object containing only the URL. Paste an excerpt if fetching is unavailable.", error.status()))?;
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let permit = SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(4)))
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            failure(
                "busy",
                "Too many pages are being fetched. Try again or paste an excerpt.",
                StatusCode::TOO_MANY_REQUESTS,
            )
        })?;
    let result = tokio::time::timeout(
        TOTAL_TIMEOUT,
        fetch_with(&PublicTransport, request.url.trim(), Some(permit)),
    )
    .await
    .map_err(|_| ServerError::from(FetchError::Timeout))??;
    Ok(Json(result))
}

#[cfg(test)]
#[path = "web_fetch_route_tests.rs"]
mod tests;
