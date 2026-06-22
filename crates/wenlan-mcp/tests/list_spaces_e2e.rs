//! E2E: MCP `list_spaces` tool round-trips through the daemon HTTP layer.
//!
//! Uses wiremock to stand in for the daemon (same pattern as `space_roundtrip_e2e.rs`).
//! Verifies that:
//!   (a) `list_spaces` issues a GET to `/api/spaces`
//!   (b) the daemon's `Vec<Space>` response is rendered with a count prefix and
//!       includes each space's name in the formatted output
//!   (c) an empty array is rendered as "0 spaces"

use wenlan_mcp::client::WenlanClient;
use wenlan_mcp::tools::{ListSpacesParams, WenlanMcpServer, TransportMode};
use wenlan_types::memory::Space;
use rmcp::model::RawContent;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_server(client: WenlanClient) -> WenlanMcpServer {
    WenlanMcpServer::new(client, TransportMode::Stdio, "test-agent".into(), None)
}

fn text_of(result: &rmcp::model::CallToolResult) -> String {
    for content in &result.content {
        if let RawContent::Text(text) = &content.raw {
            return text.text.clone();
        }
    }
    panic!(
        "expected at least one text Content block; got: {:?}",
        result.content
    );
}

fn space_fixture(name: &str, memory_count: u64) -> Space {
    Space {
        id: format!("space_{name}"),
        name: name.to_string(),
        description: None,
        suggested: false,
        starred: false,
        sort_order: 0,
        memory_count,
        entity_count: 0,
        created_at: 0.0,
        updated_at: 0.0,
    }
}

#[tokio::test]
async fn mcp_list_spaces_renders_daemon_response() {
    let mock = MockServer::start().await;

    let body = vec![space_fixture("alpha", 2), space_fixture("beta", 1)];
    Mock::given(method("GET"))
        .and(path("/api/spaces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .expect(1)
        .mount(&mock)
        .await;

    let client = WenlanClient::new(mock.uri());
    let server = make_server(client);
    let result = server
        .list_spaces_impl(ListSpacesParams {})
        .await
        .expect("list_spaces_impl must succeed");

    let rendered = text_of(&result);
    assert!(
        rendered.starts_with("2 spaces"),
        "rendered output must begin with count prefix, got: {rendered}"
    );
    assert!(
        rendered.contains("\"alpha\""),
        "rendered output must contain space name 'alpha', got: {rendered}"
    );
    assert!(
        rendered.contains("\"beta\""),
        "rendered output must contain space name 'beta', got: {rendered}"
    );
}

#[tokio::test]
async fn mcp_list_spaces_empty_array_renders_zero_count() {
    let mock = MockServer::start().await;

    let body: Vec<Space> = vec![];
    Mock::given(method("GET"))
        .and(path("/api/spaces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&body))
        .expect(1)
        .mount(&mock)
        .await;

    let client = WenlanClient::new(mock.uri());
    let server = make_server(client);
    let result = server
        .list_spaces_impl(ListSpacesParams {})
        .await
        .expect("list_spaces_impl must succeed on empty body");

    let rendered = text_of(&result);
    assert!(
        rendered.starts_with("0 spaces"),
        "empty rendered output must begin with '0 spaces', got: {rendered}"
    );
}
