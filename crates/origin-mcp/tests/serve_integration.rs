use std::time::Duration;

#[tokio::test]
async fn test_health_endpoint_no_auth() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = test_config(port, None);

    let handle = tokio::spawn(async move {
        origin_mcp::serve::run_serve(config).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/health", port))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["server"], "origin-mcp");

    handle.abort();
}

#[tokio::test]
async fn test_auth_rejects_missing_token() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = test_config(port, Some("secret-token".into()));

    let handle = tokio::spawn(async move {
        origin_mcp::serve::run_serve(config).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/mcp", port))
        .header("Content-Type", "application/json")
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);

    handle.abort();
}

#[tokio::test]
async fn test_auth_rejects_wrong_token() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = test_config(port, Some("correct-token".into()));

    let handle = tokio::spawn(async move {
        origin_mcp::serve::run_serve(config).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/mcp", port))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer wrong-token")
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);

    handle.abort();
}

#[tokio::test]
async fn test_health_bypasses_auth() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = test_config(port, Some("secret-token".into()));

    let handle = tokio::spawn(async move {
        origin_mcp::serve::run_serve(config).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/health", port))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    handle.abort();
}

/// Regression: claude.ai MCP context error (Origin Kanban, 2026-04-23).
///
/// rmcp 1.5 added DNS-rebinding protection via
/// `StreamableHttpServerConfig.allowed_hosts`, defaulting to
/// `[localhost, 127.0.0.1, ::1]`. Production deployments expose this
/// server through a public tunnel (Cloudflare, ngrok) that forwards the
/// tunnel hostname in `Host`, so rmcp rejected every tunneled request
/// with a plain-text 403 "Forbidden: Host header is not allowed" before
/// any MCP handling. The Anthropic MCP proxy cannot parse that body as
/// JSON-RPC and surfaces it to users as "-32600 Invalid Request".
///
/// This test exercises the production shape: bearer auth on, foreign
/// Host header, and the full two-leg handshake (`initialize` to mint a
/// session, then `tools/call` for `context` using that session). Both
/// legs must return 200; the old default fails the first leg with 403.
///
/// The downstream daemon is intentionally unreachable (dead port in
/// `test_config`) — the HTTP/transport layer is what this test
/// validates. A failing daemon surfaces as a tool-level error in the
/// SSE stream, which is orthogonal to the DNS-rebinding bug.
#[tokio::test]
async fn test_tunneled_host_passes_full_mcp_handshake_with_auth() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = test_config(port, Some("test-token".into()));

    let handle = tokio::spawn(async move {
        origin_mcp::serve::run_serve(config).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    // Leg 1: initialize. Foreign Host + valid bearer token.
    let init_resp = client
        .post(format!("http://127.0.0.1:{}/mcp", port))
        .header("Host", "origin-mcp.example.com")
        .header("Authorization", "Bearer test-token")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        )
        .send()
        .await
        .expect("initialize request must complete");

    let init_status = init_resp.status();
    let session_id = init_resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    assert_eq!(
        init_status, 200,
        "initialize with tunneled Host + auth must return 200 (was 403 with default allowed_hosts)",
    );
    let session_id = session_id.expect("Mcp-Session-Id header must be present after initialize");

    // Leg 2: tools/call context, reusing the session from leg 1.
    let call_resp = client
        .post(format!("http://127.0.0.1:{}/mcp", port))
        .header("Host", "origin-mcp.example.com")
        .header("Authorization", "Bearer test-token")
        .header("Mcp-Session-Id", &session_id)
        .header("Mcp-Protocol-Version", "2025-06-18")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"context","arguments":{}}}"#,
        )
        .send()
        .await
        .expect("tools/call request must complete");

    assert_eq!(
        call_resp.status(),
        200,
        "tools/call context with tunneled Host + auth must return 200 (rmcp HTTP layer success)",
    );

    handle.abort();
}

/// Secondary regression: `--no-auth` loopback mode (the Origin.app
/// production shape, fronted by cloudflared) must also accept foreign
/// Host headers. cloudflared forwards the public tunnel hostname to
/// 127.0.0.1:PORT regardless of whether auth is configured — this was
/// the real-world miss in the first fix, which only disabled
/// allowed_hosts when a token was set.
#[tokio::test]
async fn test_no_auth_mode_also_allows_tunneled_host() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = test_config(port, None);

    let handle = tokio::spawn(async move {
        origin_mcp::serve::run_serve(config).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/mcp", port))
        .header("Host", "my-tunnel.trycloudflare.com")
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .body(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}"#,
        )
        .send()
        .await
        .unwrap();

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    assert_ne!(
        status, 403,
        "--no-auth + tunneled Host must not be rejected; got {status} {body}"
    );
    assert!(
        !body.contains("Host header is not allowed"),
        "response must not be the rmcp DNS-rebinding reject body; got: {body}"
    );

    handle.abort();
}

#[tokio::test]
async fn test_rejects_disallowed_origin() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = origin_mcp::serve::ServeConfig {
        port,
        host: "127.0.0.1".into(),
        origin_url: "http://127.0.0.1:19999".into(),
        token: Some("test-token".into()),
        agent_name: "test-agent".into(),
        user_id: None,
        allowed_origins: vec!["https://claude.ai".into()],
    };

    let handle = tokio::spawn(async move {
        origin_mcp::serve::run_serve(config).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let client = reqwest::Client::new();

    // Request with disallowed Origin should get 403
    let resp = client
        .post(format!("http://127.0.0.1:{}/mcp", port))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-token")
        .header("Origin", "https://evil.com")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);

    // Request with allowed Origin should pass auth (may fail downstream, but not 403)
    let resp = client
        .post(format!("http://127.0.0.1:{}/mcp", port))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test-token")
        .header("Origin", "https://claude.ai")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_ne!(resp.status(), 403);

    handle.abort();
}

fn test_config(port: u16, token: Option<String>) -> origin_mcp::serve::ServeConfig {
    origin_mcp::serve::ServeConfig {
        port,
        host: "127.0.0.1".into(),
        origin_url: "http://127.0.0.1:19999".into(), // non-existent, OK for these tests
        token,
        agent_name: "test-agent".into(),
        user_id: None,
        allowed_origins: vec!["*".into()],
    }
}

#[tokio::test]
async fn version_handshake_warns_when_daemon_minor_ahead() {
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

    let mock_daemon = MockServer::start().await;
    Mock::given(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "db_initialized": true,
            "version": "9.9.9"
        })))
        .mount(&mock_daemon)
        .await;

    let client = origin_mcp::client::OriginClient::new(mock_daemon.uri());
    let warning = client.version_handshake().await;
    assert!(warning.is_some(), "expected a warning when daemon ahead");
    let msg = warning.unwrap();
    assert!(msg.contains("origin-mcp"), "msg={msg}");
    assert!(msg.contains("brew upgrade origin-mcp"), "msg={msg}");
}

#[tokio::test]
async fn version_handshake_silent_when_compatible() {
    use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

    let mock_daemon = MockServer::start().await;
    Mock::given(path("/api/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "ok",
            "db_initialized": true,
            "version": env!("CARGO_PKG_VERSION")
        })))
        .mount(&mock_daemon)
        .await;

    let client = origin_mcp::client::OriginClient::new(mock_daemon.uri());
    assert_eq!(client.version_handshake().await, None);
}

#[tokio::test]
async fn version_handshake_silent_when_daemon_unreachable() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let client = origin_mcp::client::OriginClient::new(format!("http://127.0.0.1:{port}"));
    assert_eq!(client.version_handshake().await, None);
}
