// SPDX-License-Identifier: Apache-2.0
//! Real JSON-RPC framing and tool dispatch after a modern client's legacy probe.

use std::time::Duration;

use rmcp::{transport::async_rw::AsyncRwTransport, ServiceExt};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};
use wenlan_mcp::{
    client::WenlanClient,
    stdio::DiscoveryFallback,
    tools::{TransportMode, WenlanMcpServer},
};
use wiremock::{
    matchers::{body_partial_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

struct Client {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
}

impl Client {
    async fn send(&mut self, value: Value) {
        let line = format!("{value}\n");
        self.writer.write_all(line.as_bytes()).await.unwrap();
        self.writer.flush().await.unwrap();
    }

    async fn response(&mut self, id: Value) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let mut line = String::new();
                assert_ne!(
                    self.reader.read_line(&mut line).await.unwrap(),
                    0,
                    "server closed before replying to {id}"
                );
                let message: Value = serde_json::from_str(&line).unwrap();
                if message.get("id") == Some(&id) {
                    return message;
                }
            }
        })
        .await
        .expect("bounded response wait")
    }

    async fn initialize(&mut self, roots_during: bool) {
        self.send(json!({
            "jsonrpc": "2.0", "id": "init", "method": "initialize",
            "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "discovery-regression", "version": "1"}}
        }))
        .await;
        let reply = self.response(json!("init")).await;
        assert_eq!(reply["result"]["protocolVersion"], "2025-11-25");
        if roots_during {
            self.send(json!({"jsonrpc": "2.0", "method": "notifications/roots/list_changed"}))
                .await;
        }
        self.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
    }
}

async fn exercise_connection(probe_id: Option<Value>, roots_before: bool, roots_during: bool) {
    let mock = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/memory/search"))
        .and(body_partial_json(
            json!({"query": "discovery-canary", "limit": 1}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"results": [], "took_ms": 1.0})),
        )
        .expect(1)
        .mount(&mock)
        .await;
    let (client_io, server_io) = tokio::io::duplex(1024 * 1024);
    let (reader, writer) = tokio::io::split(client_io);
    let mut client = Client {
        reader: BufReader::new(reader),
        writer,
    };
    let (reader, writer) = tokio::io::split(server_io);
    let server = WenlanMcpServer::new(
        WenlanClient::new(mock.uri()),
        TransportMode::Stdio,
        "antigravity".into(),
        None,
    );
    let task = tokio::spawn(async move {
        let service = server
            .serve(DiscoveryFallback::new(AsyncRwTransport::new_server(
                reader, writer,
            )))
            .await
            .unwrap();
        service.waiting().await.unwrap();
    });
    if let Some(id) = probe_id {
        // A pre-initialize ping must retain the SDK's existing behavior.
        client
            .send(json!({"jsonrpc": "2.0", "id": "ping", "method": "ping"}))
            .await;
        assert_eq!(client.response(json!("ping")).await["result"], json!({}));
        // This empty params object is the actual Antigravity 1.1.27 probe.
        client
            .send(json!({"jsonrpc": "2.0", "id": id, "method": "server/discover", "params": {}}))
            .await;
        let reply = client.response(id.clone()).await;
        assert_eq!(reply["id"], id);
        assert_eq!(reply["error"]["code"], -32601);
        assert!(
            reply.get("result").is_none(),
            "legacy server must not advertise modern support"
        );
    }
    if roots_before {
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/roots/list_changed"}))
            .await;
    }
    client.initialize(roots_during).await;
    client
        .send(json!({"jsonrpc": "2.0", "id": "list", "method": "tools/list", "params": {}}))
        .await;
    let listing = client.response(json!("list")).await;
    assert!(listing["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "recall"));
    client
        .send(
            json!({"jsonrpc": "2.0", "id": "call", "method": "tools/call", "params": {
                "name": "recall", "arguments": {"query": "discovery-canary", "limit": 1}
            }}),
        )
        .await;
    let reply = client.response(json!("call")).await;
    assert_ne!(reply["result"]["isError"], true);
    assert!(reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("0 results"));
    drop(client);
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    mock.verify().await;
}

#[tokio::test]
async fn discovery_probe_preserves_ids_and_allows_legacy_tool_calls() {
    for id in [json!(1), json!("discover"), json!("quoted-\"-雪-\n")] {
        exercise_connection(Some(id), false, false).await;
    }
}

#[tokio::test]
async fn legacy_client_connects_without_discovery() {
    exercise_connection(None, false, false).await;
}

#[tokio::test]
async fn roots_change_before_initialize_does_not_interrupt_fallback() {
    exercise_connection(Some(json!("roots-before")), true, false).await;
}

#[tokio::test]
async fn roots_change_before_initialized_notification_does_not_interrupt_handshake() {
    exercise_connection(Some(json!("roots-during")), false, true).await;
}

#[tokio::test]
async fn unrelated_preinitialize_request_is_not_consumed() {
    let (client_io, server_io) = tokio::io::duplex(1024);
    let (reader, writer) = tokio::io::split(server_io);
    let server = WenlanMcpServer::new(
        WenlanClient::new("http://127.0.0.1:9".into()),
        TransportMode::Stdio,
        "test".into(),
        None,
    );
    let task = tokio::spawn(async move {
        server
            .serve(DiscoveryFallback::new(AsyncRwTransport::new_server(
                reader, writer,
            )))
            .await
            .is_err()
    });
    let (reader, writer) = tokio::io::split(client_io);
    let mut client = Client {
        reader: BufReader::new(reader),
        writer,
    };
    client
        .send(json!({"jsonrpc": "2.0", "id": 1, "method": "unrelated/method", "params": {}}))
        .await;
    assert!(tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap());
}
