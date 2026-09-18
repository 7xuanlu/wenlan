use std::time::Duration;

#[tokio::test]
async fn test_health_endpoint_no_auth() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = test_config(port, None);

    let handle = tokio::spawn(async move {
        wenlan_mcp::serve::run_serve(config).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/health", port))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["server"], "wenlan-mcp");

    handle.abort();
}

#[tokio::test]
async fn test_auth_rejects_missing_token() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let config = test_config(port, Some("secret-token".into()));

    let handle = tokio::spawn(async move {
        wenlan_mcp::serve::run_serve(config).await.unwrap();
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
        wenlan_mcp::serve::run_serve(config).await.unwrap();
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
        wenlan_mcp::serve::run_serve(config).await.unwrap();
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
        wenlan_mcp::serve::run_serve(config).await.unwrap();
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
        wenlan_mcp::serve::run_serve(config).await.unwrap();
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
    let config = wenlan_mcp::serve::ServeConfig {
        port,
        host: "127.0.0.1".into(),
        origin_url: "http://127.0.0.1:19999".into(),
        token: Some("test-token".into()),
        agent_name: "test-agent".into(),
        user_id: None,
        allowed_origins: vec!["https://claude.ai".into()],
    };

    let handle = tokio::spawn(async move {
        wenlan_mcp::serve::run_serve(config).await.unwrap();
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

#[tokio::test]
async fn query_only_requires_a_non_empty_bearer_token() {
    for token in [None, Some(String::new()), Some(" \t\n".into())] {
        let config = test_config(0, token);

        let error = wenlan_mcp::serve::run_serve_with_profile(
            config,
            wenlan_mcp::tools::ToolProfile::QueryOnly,
        )
        .await
        .expect_err("query-only must reject missing or blank auth tokens");
        assert_eq!(error.to_string(), wenlan_mcp::serve::QUERY_ONLY_AUTH_ERROR);
    }
}

#[tokio::test]
async fn query_only_requires_a_strict_space_pin() {
    // No test in this binary calls `lock_state::init_from_env()`, so
    // `locked_space()` stays at its process-default `None` here — this
    // exercises the same "no pin" state without touching env vars.
    let config = test_config(0, Some("secret-token".into()));

    let error = wenlan_mcp::serve::run_serve_with_profile(
        config,
        wenlan_mcp::tools::ToolProfile::QueryOnly,
    )
    .await
    .expect_err("query-only must reject when no strict WENLAN_SPACE pin is active");
    assert_eq!(error.to_string(), wenlan_mcp::serve::QUERY_ONLY_SPACE_ERROR);
}

/// Isolated child-process regression: each scenario spawns the real
/// `wenlan-mcp` binary with its own environment, so `WENLAN_SPACE` /
/// `WENLAN_DEFAULT_SPACE` mutation here can never race the env-reading
/// statics used by other tests in this process.
#[test]
fn query_only_without_strict_space_pin_rejects_before_bind() {
    let scenarios: [(&str, Option<&str>, Option<&str>); 3] = [
        ("space unset, no default", None, None),
        ("space unset, default only", None, Some("fallback")),
        ("space whitespace-only", Some("   "), None),
    ];

    for (label, space, default_space) in scenarios {
        let port = portpicker::pick_unused_port().expect("no free port");
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_wenlan-mcp"));
        command.env("WENLAN_NO_AUTOSTART", "1");
        command.args([
            "serve",
            "--tool-profile",
            "query-only",
            "--token",
            "test-token",
            "--port",
            &port.to_string(),
        ]);
        match space {
            Some(value) => {
                command.env("WENLAN_SPACE", value);
            }
            None => {
                command.env_remove("WENLAN_SPACE");
            }
        }
        match default_space {
            Some(value) => {
                command.env("WENLAN_DEFAULT_SPACE", value);
            }
            None => {
                command.env_remove("WENLAN_DEFAULT_SPACE");
            }
        }

        let output = command.output().expect("wenlan-mcp binary must run");
        assert!(
            !output.status.success(),
            "{label}: must reject without a strict Space pin"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(wenlan_mcp::serve::QUERY_ONLY_SPACE_ERROR),
            "{label}: stderr={stderr}"
        );
        assert!(
            std::net::TcpListener::bind(("127.0.0.1", port)).is_ok(),
            "{label}: port must remain free — process must exit before binding"
        );
    }
}

fn test_config(port: u16, token: Option<String>) -> wenlan_mcp::serve::ServeConfig {
    wenlan_mcp::serve::ServeConfig {
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
async fn connector_info_is_authenticated_and_query_only() {
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    for profile in ["query-only", "standard"] {
        let port = portpicker::pick_unused_port().expect("no free port");
        let child = std::process::Command::new(env!("CARGO_BIN_EXE_wenlan-mcp"))
            .env("WENLAN_NO_AUTOSTART", "1")
            .env("WENLAN_SPACE", "synthetic-review")
            .env_remove("WENLAN_DEFAULT_SPACE")
            .args([
                "--origin-url",
                "http://127.0.0.1:19999",
                "serve",
                "--tool-profile",
                profile,
                "--token",
                "synthetic-connector-token",
                "--port",
                &port.to_string(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("binary must start");
        let _guard = ChildGuard(child);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let base = format!("http://127.0.0.1:{port}");
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if client
                    .get(format!("{base}/health"))
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("server must become ready");

        let invalid_init = serde_json::json!({
            "jsonrpc": "2.0", "method": "initialize",
            "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                "clientInfo": {"name": "synthetic", "version": "1"}}
        });
        let anonymous_init = client
            .post(format!("{base}/mcp"))
            .header("Accept", "application/json, text/event-stream")
            .json(&invalid_init)
            .send()
            .await
            .unwrap();
        assert_eq!(
            anonymous_init.status(),
            401,
            "auth must run before initialization parsing"
        );
        let rejected_init = client
            .post(format!("{base}/mcp"))
            .header("Accept", "application/json, text/event-stream")
            .bearer_auth("synthetic-connector-token")
            .json(&invalid_init)
            .send()
            .await
            .unwrap();
        assert_eq!(
            rejected_init.status(),
            if profile == "query-only" { 400 } else { 422 },
            "only the query-only binary path applies the pre-allocation guard"
        );
        assert!(rejected_init.headers().get("mcp-session-id").is_none());
        let mut valid_init = invalid_init.clone();
        valid_init["id"] = serde_json::json!(1);
        let initialized = client
            .post(format!("{base}/mcp"))
            .header("Accept", "application/json, text/event-stream")
            .bearer_auth("synthetic-connector-token")
            .json(&valid_init)
            .send()
            .await
            .unwrap();
        assert_eq!(initialized.status(), 200);
        let session_id = initialized.headers()["mcp-session-id"]
            .to_str()
            .unwrap()
            .to_owned();
        initialized.text().await.unwrap();
        if profile == "query-only" {
            let notified = client
                .post(format!("{base}/mcp"))
                .header("Accept", "application/json, text/event-stream")
                .header("Mcp-Session-Id", &session_id)
                .bearer_auth("synthetic-connector-token")
                .json(&serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
                .send()
                .await
                .unwrap();
            assert_eq!(notified.status(), 202);
            let mut events = client
                .get(format!("{base}/mcp"))
                .header("Accept", "text/event-stream")
                .header("Mcp-Session-Id", &session_id)
                .bearer_auth("synthetic-connector-token")
                .send()
                .await
                .unwrap();
            assert_eq!(events.status(), 200);
            let heartbeat = tokio::time::timeout(Duration::from_millis(1800), async {
                let mut text = String::new();
                while let Some(bytes) = events.chunk().await.unwrap() {
                    text.push_str(&String::from_utf8_lossy(&bytes));
                    assert!(text.len() < 4096);
                    if text.lines().any(|line| line.starts_with(':')) {
                        return;
                    }
                }
                panic!("SDK SSE stream ended before its heartbeat");
            })
            .await;
            drop(events);
            assert!(
                heartbeat.is_ok(),
                "query-only SDK heartbeat must arrive within 1.8s"
            );
        }
        let deleted = client
            .delete(format!("{base}/mcp"))
            .header("Mcp-Session-Id", session_id)
            .bearer_auth("synthetic-connector-token")
            .send()
            .await
            .unwrap();
        assert_eq!(deleted.status(), 202);

        for token in [None, Some("wrong-token")] {
            let mut request = client.get(format!("{base}/connector-info"));
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), 401);
            assert!(!response.text().await.unwrap().contains("synthetic-review"));
        }
        let response = client
            .get(format!("{base}/connector-info"))
            .bearer_auth("synthetic-connector-token")
            .send()
            .await
            .unwrap();
        if profile == "standard" {
            assert_eq!(response.status(), 404);
            continue;
        }
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap(),
            serde_json::json!({
                "contract_version": 1,
                "server": "wenlan-mcp",
                "tool_profile": "query-only",
                "space": "synthetic-review",
                "authentication": "bearer",
            })
        );
        let health = client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(!health.contains("synthetic-review"));
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

    let client = wenlan_mcp::client::WenlanClient::new(mock_daemon.uri());
    let warning = client.version_handshake().await;
    assert!(warning.is_some(), "expected a warning when daemon ahead");
    let msg = warning.unwrap();
    assert!(msg.contains("wenlan-mcp"), "msg={msg}");
    assert!(msg.contains("brew upgrade wenlan-mcp"), "msg={msg}");
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

    let client = wenlan_mcp::client::WenlanClient::new(mock_daemon.uri());
    assert_eq!(client.version_handshake().await, None);
}

#[tokio::test]
async fn version_handshake_silent_when_daemon_unreachable() {
    let port = portpicker::pick_unused_port().expect("no free port");
    let client = wenlan_mcp::client::WenlanClient::new(format!("http://127.0.0.1:{port}"));
    assert_eq!(client.version_handshake().await, None);
}
