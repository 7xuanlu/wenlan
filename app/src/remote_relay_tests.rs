// SPDX-License-Identifier: AGPL-3.0-only
use super::reverse_client::{PreparedReverse, ReverseCandidate};
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn credential() -> DeviceCredential {
    DeviceCredential {
        id: "d".repeat(64),
        management_token: "m".repeat(64),
        expires_at: now_ms() + 60_000,
    }
}
fn candidate() -> ConnectorCandidate {
    ConnectorCandidate {
        tunnel_origin: "https://synthetic.trycloudflare.com".into(),
        backend_token: "b".repeat(64),
        space: "review".into(),
    }
}

async fn server(
    status: u16,
    body: String,
    headers: &str,
) -> (RelayClient, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let response = format!("HTTP/1.1 {status} Synthetic\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}", body.len());
    let handle = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buf = [0; 2048];
            let length = socket.read(&mut buf).await.unwrap();
            if length == 0 {
                break;
            }
            bytes.extend_from_slice(&buf[..length]);
            assert!(bytes.len() <= 16 * 1024);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&bytes[..end]).to_ascii_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .map(|value| value.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        socket.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (RelayClient::build(origin, false).unwrap(), handle)
}

const JSON: &str = "Content-Type: application/json\r\n";

#[test]
fn production_target_is_the_standalone_service_and_debug_redacts_credentials() {
    assert_eq!(
        RelayClient::new().unwrap().mcp_url(),
        "https://relay.wenlan.app/mcp"
    );
    let device = credential();
    let tunnel = candidate();
    let output = format!("{device:?} {tunnel:?}");
    for private in [
        &device.id,
        &device.management_token,
        &tunnel.backend_token,
        &tunnel.tunnel_origin,
    ] {
        assert!(!output.contains(private));
    }
}

#[tokio::test]
async fn enrollment_uses_new_endpoint_and_only_separate_backend_credentials() {
    let device = credential();
    let (client, capture) = server(201, serde_json::to_string(&device).unwrap(), JSON).await;
    assert_eq!(client.enroll(&candidate()).await.unwrap().id, device.id);
    let request = capture.await.unwrap();
    assert!(request.starts_with("POST /devices HTTP/1.1"));
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    let headers = headers.to_ascii_lowercase();
    for header in [
        "authorization:",
        "cookie:",
        "origin:",
        "x-wenlan-device-id:",
    ] {
        assert!(!headers.contains(header));
    }
    let body: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["space"], "review");
    assert_eq!(body["backendToken"], candidate().backend_token);
    assert!(body.get("secret").is_none());
    assert!(body.get("user_id").is_none());
}

#[tokio::test]
async fn native_management_headers_never_put_secrets_in_query_or_body() {
    let device = credential();
    let (client, capture) = server(200, "{\"success\":true}".into(), JSON).await;
    client.refresh(&device, &candidate()).await.unwrap();
    let request = capture.await.unwrap();
    assert!(request.starts_with("POST /devices/refresh HTTP/1.1"));
    assert!(request.contains(&format!(
        "authorization: Bearer {}",
        device.management_token
    )));
    assert!(request.contains(&format!("x-wenlan-device-id: {}", device.id)));
    assert!(!request.contains("origin:"));
    assert!(!request.contains("cookie:"));
    let (_, body) = request.split_once("\r\n\r\n").unwrap();
    assert!(!body.contains(&device.management_token));
}

#[tokio::test]
async fn invalid_request_fields_fail_before_network_io() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = RelayClient::build(
        Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap(),
        false,
    )
    .unwrap();
    for value in [
        "https://trycloudflare.com.attacker.example",
        "http://a.trycloudflare.com",
        "https://a.b.trycloudflare.com",
        "https://user:pass@a.trycloudflare.com",
        "https://a.trycloudflare.com/mcp",
        "https://a.trycloudflare.com?secret=bad",
    ] {
        let mut invalid = candidate();
        invalid.tunnel_origin = value.into();
        assert_eq!(
            client.enroll(&invalid).await.unwrap_err(),
            RelayError::InvalidInput
        );
    }
    assert_eq!(
        client
            .inspect_pairing(&credential(), "../devices")
            .await
            .unwrap_err(),
        RelayError::InvalidInput
    );
    assert_eq!(
        client
            .grants(&credential(), Some("x:other"))
            .await
            .unwrap_err(),
        RelayError::InvalidInput
    );
    assert_eq!(
        client
            .revoke_grant(&credential(), "bad/path")
            .await
            .unwrap_err(),
        RelayError::InvalidInput
    );
    let mut expired = credential();
    expired.expires_at = 0;
    assert_eq!(
        client.refresh(&expired, &candidate()).await.unwrap_err(),
        RelayError::Unauthorized
    );
    expired.management_token = "malformed".into();
    assert_eq!(
        client.revoke_device(&expired).await.unwrap_err(),
        RelayError::Unauthorized
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn expired_device_can_request_revocation_but_server_must_confirm_it() {
    let mut expired = credential();
    expired.expires_at = 1;
    for (status, body, expected) in [
        (200, "{\"success\":true}", Ok(())),
        (401, "{\"error\":\"denied\"}", Err(RelayError::Unauthorized)),
        (
            503,
            "{\"error\":\"unavailable\"}",
            Err(RelayError::Unavailable),
        ),
        (200, "{\"success\":false}", Err(RelayError::InvalidResponse)),
    ] {
        let (client, capture) = server(status, body.into(), JSON).await;
        assert_eq!(client.revoke_device(&expired).await, expected);
        let request = capture.await.unwrap();
        assert!(request.starts_with("POST /devices/revoke HTTP/1.1"));
        assert!(request.contains(&format!(
            "authorization: Bearer {}",
            expired.management_token
        )));
        assert!(!request
            .split_once("\r\n\r\n")
            .unwrap()
            .1
            .contains(&expired.management_token));
    }
}

#[tokio::test]
async fn redirects_are_not_followed_and_raw_errors_are_not_exposed() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let headers = format!(
        "{JSON}Location: http://{}/credential-sink\r\n",
        listener.local_addr().unwrap()
    );
    let (client, capture) = server(307, "PRIVATE_RESPONSE".into(), &headers).await;
    let error = client
        .refresh(&credential(), &candidate())
        .await
        .unwrap_err();
    assert_eq!(error, RelayError::Rejected(307));
    assert!(!error.to_string().contains("PRIVATE_RESPONSE"));
    capture.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn revocation_distinguishes_authoritative_denial_from_pending_token_cleanup() {
    for (status, pending) in [(200, false), (503, true)] {
        let body = serde_json::json!({ "revoked": true, "cleanupPending": pending }).to_string();
        let (client, capture) = server(status, body, JSON).await;
        let result = client
            .revoke_grant(&credential(), &"g".repeat(16))
            .await
            .unwrap();
        assert_eq!(
            result,
            GrantRevocation {
                revoked: true,
                cleanup_pending: pending
            }
        );
        capture.await.unwrap();
    }
    let (client, capture) =
        server(503, "{\"error\":\"private outage details\"}".into(), JSON).await;
    assert!(client
        .revoke_grant(&credential(), &"g".repeat(16))
        .await
        .is_err());
    capture.await.unwrap();
}

#[tokio::test]
async fn only_explicit_approved_response_completes_pairing() {
    let (client, capture) = server(200, "{\"approved\":true}".into(), JSON).await;
    let view = PairingView {
        pairing_id: "p".repeat(64),
        client_id: "synthetic-client".into(),
        resource: client.mcp_url(),
        scopes: vec![QUERY_SCOPE.into()],
        expires_at: now_ms() + 60_000,
    };
    client
        .approve_pairing(&credential(), &view, "review")
        .await
        .unwrap();
    let request = capture.await.unwrap();
    let (_, body) = request.split_once("\r\n\r\n").unwrap();
    let body: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(body["approved"], true);
    assert_eq!(body["clientId"], view.client_id);
    assert_eq!(body["space"], "review");
}

#[tokio::test]
async fn credential_rotation_cannot_rebind_device_or_reuse_old_token() {
    for changed_id in [true, false] {
        let original = credential();
        let mut next = original.clone();
        if changed_id {
            next.id = "x".repeat(64);
            next.management_token = "y".repeat(64);
        }
        let (client, capture) = server(200, serde_json::to_string(&next).unwrap(), JSON).await;
        assert_eq!(
            client.rotate(&original).await.unwrap_err(),
            RelayError::InvalidResponse
        );
        capture.await.unwrap();
    }
}

#[tokio::test]
async fn malformed_or_oversized_responses_fail_closed_and_rate_limits_remain_typed() {
    for (body, headers) in [
        ("{".into(), JSON),
        ("x".repeat(RESPONSE_LIMIT + 1), JSON),
        ("{\"success\":true}".into(), "Content-Type: text/html\r\n"),
    ] {
        let (client, capture) = server(200, body, headers).await;
        assert_eq!(
            client
                .refresh(&credential(), &candidate())
                .await
                .unwrap_err(),
            RelayError::InvalidResponse
        );
        capture.await.unwrap();
    }
    let (client, capture) = server(429, "PRIVATE".into(), "Retry-After: 30\r\n").await;
    assert_eq!(
        client.revoke_device(&credential()).await.unwrap_err(),
        RelayError::RateLimited {
            retry_after_seconds: Some(30)
        }
    );
    capture.await.unwrap();
}

#[tokio::test]
async fn pairing_view_must_match_the_fixed_resource_and_requested_transaction() {
    let id = "p".repeat(64);
    for (returned_id, resource, scopes) in [
        (
            "q".repeat(64),
            format!("{RELAY_ORIGIN}/mcp"),
            vec![QUERY_SCOPE],
        ),
        (
            id.clone(),
            "https://attacker.example/mcp".into(),
            vec![QUERY_SCOPE],
        ),
        (
            id.clone(),
            format!("{RELAY_ORIGIN}/mcp"),
            vec!["wenlan:write"],
        ),
    ] {
        let body = serde_json::json!({ "pairingId": returned_id, "clientId": "synthetic",
            "resource": resource, "scopes": scopes, "expiresAt": now_ms() + 60_000 })
        .to_string();
        let (client, capture) = server(200, body, JSON).await;
        assert_eq!(
            client
                .inspect_pairing(&credential(), &id)
                .await
                .unwrap_err(),
            RelayError::InvalidResponse
        );
        capture.await.unwrap();
    }
}

#[tokio::test]
async fn grant_listing_rejects_unbounded_or_repeating_pages() {
    let item = serde_json::json!({ "id": "g".repeat(16), "clientId": "synthetic", "space": "review",
        "createdAt": 1, "expiresAt": 2, "status": "inactive", "cleanupPending": false });
    for body in [
        serde_json::json!({ "items": vec![item.clone(); 26] }),
        serde_json::json!({ "items": [item.clone(), item] }),
        serde_json::json!({ "items": [], "cursor": "x:other-owner" }),
    ] {
        let (client, capture) = server(200, body.to_string(), JSON).await;
        assert_eq!(
            client.grants(&credential(), None).await.unwrap_err(),
            RelayError::InvalidResponse
        );
        capture.await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires the isolated desktop-client-contract.mjs Worker fixture"]
async fn actual_worker_contract() {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let origin = Url::parse(
        &std::env::var("WENLAN_RELAY_CONTRACT_ORIGIN").expect("synthetic fixture origin required"),
    )
    .unwrap();
    assert_eq!(origin.scheme(), "http");
    assert_eq!(origin.host_str(), Some("127.0.0.1"));
    let client = RelayClient::build(origin, false).unwrap();
    let transport = std::env::var("WENLAN_RELAY_CONTRACT_TRANSPORT").unwrap_or_default();
    let reverse = transport == "reverse" || transport == "reverse-runtime";
    let scratch = tempfile::tempdir().unwrap();
    let (device, connection) = if transport == "reverse-runtime" {
        let store = store::Store::in_directory(scratch.path().join("relay"));
        let configured = store.configure(None, "review").unwrap();
        let enabled = store.enable(configured.revision()).unwrap();
        let port = std::env::var("WENLAN_RELAY_CONTRACT_MCP_PORT")
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let active = reverse_runtime::connect_at(store.clone(), client.clone(), enabled, port)
            .await
            .unwrap();
        let saved = store.load().unwrap().unwrap();
        let device = saved.device().unwrap().clone();
        assert_eq!(saved.revision(), active.profile.revision());
        assert_eq!(device.id, active.profile.device().unwrap().id);
        let replacement = reverse_runtime::connect_at(store, client.clone(), saved, port)
            .await
            .unwrap();
        assert_eq!(replacement.profile.device().unwrap().id, device.id);
        tokio::time::timeout(Duration::from_secs(4), async {
            while !active.connection.is_finished() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(active.connection.shutdown().await.is_err());
        (device, Some(replacement.connection))
    } else if reverse {
        let port = std::env::var("WENLAN_RELAY_CONTRACT_MCP_PORT")
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let prepared = client.prepare_reverse(&reverse_candidate()).await.unwrap();
        let device = prepared.credential;
        let connection = client
            .connect_reverse(&device, port, reverse_candidate().backend_token)
            .await
            .unwrap();
        let mut ready = false;
        for wait in [0, 10, 20, 40, 80, 160, 320, 640, 1280] {
            tokio::time::sleep(Duration::from_millis(wait)).await;
            if client
                .reverse_status(&device, connection.connection_id())
                .await
                .unwrap()
                == Some(0)
            {
                ready = true;
                break;
            }
        }
        assert!(ready, "Reverse connector activation did not complete");
        (device, Some(connection))
    } else {
        let device = client.enroll(&candidate()).await.unwrap();
        client.refresh(&device, &candidate()).await.unwrap();
        (device, None)
    };
    assert!(client.grants(&device, None).await.unwrap().items.is_empty());
    let registration: serde_json::Value = client.http.post(client.endpoint("/oauth/register")).json(&serde_json::json!({
        "client_name": "Synthetic native contract", "redirect_uris": ["https://client.example/callback"],
        "grant_types": ["authorization_code", "refresh_token"], "response_types": ["code"], "token_endpoint_auth_method": "none"
    })).send().await.unwrap().json().await.unwrap();
    let client_id = registration["client_id"].as_str().unwrap();
    let verifier = "v".repeat(43);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    let resource = client.mcp_url();
    let authorization = client
        .http
        .get(client.endpoint("/authorize"))
        .query(&[
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", "https://client.example/callback"),
            ("resource", resource.as_str()),
            ("scope", QUERY_SCOPE),
            ("code_challenge_method", "S256"),
            ("code_challenge", challenge.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(authorization.status(), StatusCode::SEE_OTHER);
    let cookie = authorization
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let pairing_id = cookie.split_once('=').unwrap().1.split('.').next().unwrap();
    let view = client.inspect_pairing(&device, pairing_id).await.unwrap();
    assert_eq!(view.client_id, client_id);
    client
        .approve_pairing(&device, &view, "review")
        .await
        .unwrap();
    let completion = client
        .http
        .post(client.endpoint("/pairing/complete"))
        .header("origin", RELAY_ORIGIN)
        .header("cookie", cookie)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(completion.status(), StatusCode::OK);
    let completed: serde_json::Value = completion.json().await.unwrap();
    let callback = Url::parse(completed["redirectTo"].as_str().unwrap()).unwrap();
    let code = callback
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .to_string();
    let response = client
        .http
        .post(client.endpoint("/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", "https://client.example/callback"),
            ("resource", resource.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let tokens: serde_json::Value = response.json().await.unwrap();
    let initialized = client
        .http
        .post(client.endpoint("/mcp"))
        .bearer_auth(tokens["access_token"].as_str().unwrap())
        .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }))
        .send()
        .await
        .unwrap();
    assert_eq!(initialized.status(), StatusCode::OK);
    assert!(initialized.headers().contains_key("mcp-session-id"));
    let _ = initialized.bytes().await.unwrap();
    let page = client.grants(&device, None).await.unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].status, GrantStatus::Active);
    let result = client
        .revoke_grant(&device, &page.items[0].id)
        .await
        .unwrap();
    assert!(result.revoked && !result.cleanup_pending);
    assert_eq!(
        client.grants(&device, None).await.unwrap().items[0].status,
        GrantStatus::Inactive
    );
    let rotated = client.rotate(&device).await.unwrap();
    assert_eq!(
        client.grants(&device, None).await.unwrap_err(),
        RelayError::Unauthorized
    );
    assert_eq!(
        client.revoke_device(&device).await.unwrap_err(),
        RelayError::Unauthorized
    );
    // The fixture drops the first successful revocation reply after commit.
    assert_eq!(
        client.revoke_device(&rotated).await.unwrap_err(),
        RelayError::Unavailable
    );
    client.revoke_device(&rotated).await.unwrap();
    let mut locally_expired = rotated.clone();
    locally_expired.expires_at = 1;
    client.revoke_device(&locally_expired).await.unwrap();
    assert_eq!(
        client.grants(&rotated, None).await.unwrap_err(),
        RelayError::Unauthorized
    );
    if let Some(connection) = connection {
        tokio::time::timeout(Duration::from_secs(4), async {
            while !connection.is_finished() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(connection.shutdown().await.is_err());
    }
}

fn reverse_candidate() -> ReverseCandidate {
    ReverseCandidate {
        backend_token: "b".repeat(64),
        space: "review".into(),
    }
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "requires the deployed public relay and a real local MCP listener"]
async fn public_reverse_socket_helper() {
    use serde::Deserialize;
    use std::io::{self, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    const CREDENTIALS_ENV: &str = "WENLAN_REVERSE_HELPER_CREDENTIALS_PATH";
    const MCP_PORT_ENV: &str = "WENLAN_REVERSE_HELPER_MCP_PORT";
    const MAX_CREDENTIAL_BYTES: u64 = 16 * 1024;
    const READY_MARKER: &str = "WENLAN_REVERSE_HELPER_READY";
    const HELPER_LIFETIME: Duration = Duration::from_secs(240);

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct HelperCredentials {
        origin: String,
        device: DeviceCredential,
        #[serde(rename = "backendToken")]
        backend_token: String,
    }

    assert_eq!(
        std::env::var("WENLAN_TEST_PUBLIC_RELAY").ok().as_deref(),
        Some("1"),
        "explicit public test approval gate required"
    );
    let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .unwrap_or_else(|_| panic!("SIGTERM handler could not be installed"));
    let path_value =
        std::env::var(CREDENTIALS_ENV).unwrap_or_else(|_| panic!("{CREDENTIALS_ENV} is required"));
    let path = PathBuf::from(path_value);
    assert!(
        path.is_absolute(),
        "helper credential path must be absolute"
    );
    let metadata = std::fs::symlink_metadata(&path)
        .unwrap_or_else(|_| panic!("helper credential file is unavailable"));
    assert!(
        !metadata.file_type().is_symlink(),
        "helper credential file must not be a symlink"
    );
    assert!(
        metadata.file_type().is_file(),
        "helper credential path must be a regular file"
    );
    assert_eq!(
        metadata.permissions().mode() & 0o077,
        0,
        "helper credential file must not be group or world readable"
    );
    assert!(
        metadata.len() <= MAX_CREDENTIAL_BYTES,
        "helper credential file exceeds the size limit"
    );
    let bytes = tokio::fs::read(&path)
        .await
        .unwrap_or_else(|_| panic!("helper credential file could not be read"));
    assert!(
        (bytes.len() as u64) <= MAX_CREDENTIAL_BYTES,
        "helper credential file exceeds the size limit"
    );
    let input: HelperCredentials = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| panic!("helper credential file has an invalid shape"));
    assert!(
        input.origin == RELAY_ORIGIN,
        "production relay origin required"
    );

    let port = std::env::var(MCP_PORT_ENV)
        .unwrap_or_else(|_| panic!("{MCP_PORT_ENV} is required"))
        .parse::<u16>()
        .unwrap_or_else(|_| panic!("helper MCP port is invalid"));
    assert_ne!(port, 0, "helper MCP port must be nonzero");

    let client =
        RelayClient::new().unwrap_or_else(|_| panic!("production relay client unavailable"));
    let connection = client
        .connect_reverse(&input.device, port, input.backend_token)
        .await
        .unwrap_or_else(|_| panic!("reverse connection could not be established"));

    let ready_deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    let mut ready = false;
    while tokio::time::Instant::now() < ready_deadline {
        if connection.is_finished() {
            break;
        }
        let remaining = ready_deadline.saturating_duration_since(tokio::time::Instant::now());
        let status = tokio::time::timeout(
            remaining,
            client.reverse_status(&input.device, connection.connection_id()),
        )
        .await;
        if matches!(status, Ok(Ok(Some(_)))) {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !ready {
        let _ = connection.shutdown().await;
        panic!("reverse connection readiness was not confirmed");
    }

    let mut output = io::stdout();
    let _ = writeln!(output, "{READY_MARKER}");
    let _ = output.flush();
    tokio::select! {
        _ = signal.recv() => {}
        _ = tokio::time::sleep(HELPER_LIFETIME) => {}
    }
    let _ = connection.shutdown().await;
}

fn reverse_body(credential: &DeviceCredential, pending_until: u64) -> String {
    serde_json::json!({
        "id": credential.id,
        "managementToken": credential.management_token,
        "expiresAt": credential.expires_at,
        "pendingUntil": pending_until,
    })
    .to_string()
}

#[tokio::test]
async fn reverse_prepare_sends_exact_contract_and_redacts_debug() {
    let candidate = reverse_candidate();
    let device = credential();
    let pending_until = now_ms() + 30_000;
    let (client, capture) = server(201, reverse_body(&device, pending_until), JSON).await;
    let prepared = client.prepare_reverse(&candidate).await.unwrap();
    assert_eq!(prepared.credential.id, device.id);
    assert_eq!(prepared.pending_until, pending_until);

    let request = capture.await.unwrap();
    assert!(request.starts_with("POST /devices/reverse HTTP/1.1"));
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    let headers = headers.to_ascii_lowercase();
    for header in [
        "authorization:",
        "cookie:",
        "origin:",
        "x-wenlan-device-id:",
    ] {
        assert!(!headers.contains(header));
    }
    let body: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "backendToken": candidate.backend_token,
            "space": candidate.space,
        })
    );

    let output = format!("{candidate:?} {prepared:?}");
    for secret in [
        &candidate.backend_token,
        &device.id,
        &device.management_token,
    ] {
        assert!(!output.contains(secret));
    }
}

#[tokio::test]
async fn reverse_status_sends_native_headers_without_query_body_or_browser_headers() {
    let device = credential();
    let connection_id = "a".repeat(64);
    let (client, capture) = server(200, "{\"connected\":true,\"generation\":7}".into(), JSON).await;
    assert_eq!(
        client
            .reverse_status(&device, &connection_id)
            .await
            .unwrap(),
        Some(7)
    );
    let request = capture.await.unwrap();
    assert!(request.starts_with("GET /devices/reverse/status HTTP/1.1"));
    let (headers, body) = request.split_once("\r\n\r\n").unwrap();
    let headers = headers.to_ascii_lowercase();
    assert!(headers.contains(&format!(
        "authorization: bearer {}",
        device.management_token
    )));
    assert!(headers.contains(&format!("x-wenlan-device-id: {}", device.id)));
    assert!(headers.contains(&format!("x-wenlan-connection-id: {connection_id}")));
    for header in ["cookie:", "origin:"] {
        assert!(!headers.contains(header));
    }
    assert!(body.is_empty());
}

#[tokio::test]
async fn reverse_invalid_inputs_fail_before_network_io() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = RelayClient::build(
        Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap(),
        false,
    )
    .unwrap();

    let mut invalid_candidate = reverse_candidate();
    invalid_candidate.backend_token = "short".into();
    assert_eq!(
        client
            .prepare_reverse(&invalid_candidate)
            .await
            .unwrap_err(),
        RelayError::InvalidInput
    );
    let mut invalid_candidate = reverse_candidate();
    invalid_candidate.space = " review".into();
    assert_eq!(
        client
            .prepare_reverse(&invalid_candidate)
            .await
            .unwrap_err(),
        RelayError::InvalidInput
    );
    for connection_id in [
        "a".repeat(63),
        "a".repeat(65),
        "A".repeat(64),
        "g".repeat(64),
        format!("{}-", "a".repeat(63)),
    ] {
        assert_eq!(
            client
                .reverse_status(&credential(), &connection_id)
                .await
                .unwrap_err(),
            RelayError::InvalidInput
        );
    }
    let mut expired = credential();
    expired.expires_at = 1;
    assert_eq!(
        client
            .reverse_status(&expired, &"a".repeat(64))
            .await
            .unwrap_err(),
        RelayError::Unauthorized
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn reverse_status_rejects_malformed_state_and_preserves_disconnected_shape() {
    for body in [
        r#"{"connected":false,"generation":0}"#,
        r#"{"connected":false,"generation":null}"#,
        r#"{"connected":true}"#,
        r#"{"connected":true,"generation":null}"#,
        r#"{"connected":true,"generation":-1}"#,
        r#"{"connected":true,"generation":9007199254740992}"#,
        r#"{"connected":"true"}"#,
    ] {
        let (client, capture) = server(200, body.into(), JSON).await;
        assert_eq!(
            client
                .reverse_status(&credential(), &"a".repeat(64))
                .await
                .unwrap_err(),
            RelayError::InvalidResponse,
            "body: {body}"
        );
        capture.await.unwrap();
    }

    let (client, capture) = server(200, r#"{"connected":false}"#.into(), JSON).await;
    assert_eq!(
        client
            .reverse_status(&credential(), &"a".repeat(64))
            .await
            .unwrap(),
        None
    );
    capture.await.unwrap();
}

#[tokio::test]
async fn reverse_prepare_rejects_wrong_status_and_invalid_pending_window() {
    let device = credential();
    let valid_pending = now_ms() + 30_000;
    let (client, capture) = server(200, reverse_body(&device, valid_pending), JSON).await;
    assert_eq!(
        client
            .prepare_reverse(&reverse_candidate())
            .await
            .unwrap_err(),
        RelayError::InvalidResponse
    );
    capture.await.unwrap();

    for pending_until in [1, device.expires_at + 1] {
        let (client, capture) = server(201, reverse_body(&device, pending_until), JSON).await;
        assert_eq!(
            client
                .prepare_reverse(&reverse_candidate())
                .await
                .unwrap_err(),
            RelayError::InvalidResponse
        );
        capture.await.unwrap();
    }

    let mut expired = device.clone();
    expired.expires_at = 1;
    let (client, capture) = server(201, reverse_body(&expired, valid_pending), JSON).await;
    assert_eq!(
        client
            .prepare_reverse(&reverse_candidate())
            .await
            .unwrap_err(),
        RelayError::InvalidResponse
    );
    capture.await.unwrap();
}

#[tokio::test]
async fn reverse_generic_errors_are_mapped_without_exposing_response_or_secret_data() {
    for (status, expected) in [
        (401, RelayError::Unauthorized),
        (503, RelayError::Unavailable),
        (
            429,
            RelayError::RateLimited {
                retry_after_seconds: None,
            },
        ),
    ] {
        let (client, capture) = server(status, "private response details".into(), JSON).await;
        let error = client
            .prepare_reverse(&reverse_candidate())
            .await
            .unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.to_string().contains("private response details"));
        capture.await.unwrap();
    }

    let candidate = reverse_candidate();
    let device = credential();
    let prepared = PreparedReverse {
        credential: device.clone(),
        pending_until: now_ms() + 30_000,
    };
    let output = format!("{candidate:?} {prepared:?}");
    for secret in [
        &candidate.backend_token,
        &device.id,
        &device.management_token,
    ] {
        assert!(!output.contains(secret));
    }
}
