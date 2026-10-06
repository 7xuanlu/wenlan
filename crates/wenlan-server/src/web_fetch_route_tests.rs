// SPDX-License-Identifier: Apache-2.0
use super::*;
use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn call(headers: &[(&str, &str)], payload: &str) -> (StatusCode, Vec<u8>) {
    let state = crate::state::ServerState::default();
    // No DB, daemon, vault or personal files exist in this in-process router.
    let router = crate::router::build_router(Arc::new(RwLock::new(state)));
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/webpage/fetch")
        .header("content-type", "application/json");
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = router
        .oneshot(request.body(Body::from(payload.to_owned())).unwrap())
        .await
        .unwrap();
    (
        response.status(),
        to_bytes(response.into_body(), 8192).await.unwrap().to_vec(),
    )
}

#[test]
fn production_web_fetch_guard_rejects_untrusted_browser_and_rebinding_headers() {
    temp_env::with_var("WENLAN_BIND_ADDR", None::<&str>, || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        for headers in [
            vec![("origin", "https://attacker.example")],
            vec![("origin", "null")],
            vec![("host", "attacker.example:7878")],
            vec![("sec-fetch-site", "cross-site")],
            vec![("sec-fetch-site", "same-site")],
        ] {
            let (status, _) =
                runtime.block_on(call(&headers, r#"{"url":"https://public.example"}"#));
            assert_eq!(status, StatusCode::FORBIDDEN, "{headers:?}");
        }
    });
}

#[test]
fn production_web_fetch_route_preserves_local_native_contract_without_network() {
    temp_env::with_var("WENLAN_BIND_ADDR", None::<&str>, || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        // Missing browser headers is the existing native loopback contract,
        // not token authentication. Invalid destination proves route dispatch.
        for headers in [
            vec![],
            vec![
                ("host", "127.0.0.1:7878"),
                ("origin", "tauri://localhost"),
                ("sec-fetch-site", "cross-site"),
            ],
        ] {
            let (status, body) = runtime.block_on(call(&headers, r#"{"url":"http://127.0.0.1/"}"#));
            assert_eq!(status, StatusCode::FORBIDDEN);
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["code"], "blocked_destination");
            assert_eq!(body["manual_excerpt_available"], true);
        }
        let (status, body) = runtime.block_on(call(
            &[],
            r#"{"url":"https://public.example","headers":{"Cookie":"secret"}}"#,
        ));
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"],
            "invalid_request"
        );
    });
}

#[test]
fn production_web_fetch_route_never_honors_external_bind_opt_out() {
    for bind in [
        "0.0.0.0:7878",
        "[::]:7878",
        "public.example:7878",
        "not-a-bind",
    ] {
        temp_env::with_var("WENLAN_BIND_ADDR", Some(bind), || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let (status, body) = runtime.block_on(call(
                &[("host", "localhost:7878"), ("origin", "tauri://localhost")],
                r#"{"url":"https://public.example"}"#,
            ));
            assert_eq!(status, StatusCode::FORBIDDEN);
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"],
                "local_only"
            );
        });
    }
}

#[test]
fn web_fetch_errors_are_explicit_and_offer_manual_excerpt() {
    use axum::response::IntoResponse;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for error in [
        FetchError::UnsupportedEncoding,
        FetchError::UnsupportedContentType,
        FetchError::TooLarge,
        FetchError::Timeout,
        FetchError::NoReadableText,
        FetchError::HttpStatus(403),
    ] {
        let response = ServerError::from(error).into_response();
        assert!(!response.status().is_success());
        let body = runtime
            .block_on(to_bytes(response.into_body(), 8192))
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["manual_excerpt_available"], true);
        assert!(!body["error"].as_str().unwrap().is_empty());
    }
}
