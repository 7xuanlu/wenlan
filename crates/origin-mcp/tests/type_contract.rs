//! Type-contract tests: verify origin-mcp deserializes origin-server's wire shapes correctly.
//!
//! Unlike `serve_integration.rs` (which tests origin-mcp's own HTTP endpoints without a
//! backend), these tests stand up a live `wiremock` listening on a real port, point
//! origin-mcp's HTTP client at it, and invoke the `_impl` methods directly.
//!
//! The wiremock responses are constructed from `origin_types::*` types via
//! `serde_json::to_value` - by construction, a passing test proves origin-mcp deserializes
//! the same JSON origin-server would emit for that shape.

use origin_mcp::client::OriginClient;
use origin_mcp::tools::{
    CaptureParams, ContextParams, OriginMcpServer, RecallParams, TransportMode,
};
use origin_types::memory::SearchResult;
use origin_types::responses::{
    ChatContextResponse, DeleteResponse, KnowledgeContext, ProfileContext, SearchMemoryResponse,
    StoreMemoryResponse, TierTokenEstimates,
};
use rmcp::model::{CallToolResult, RawContent};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn setup() -> (MockServer, OriginClient) {
    let mock = MockServer::start().await;
    let client = OriginClient::new(mock.uri());
    (mock, client)
}

fn make_server(client: OriginClient) -> OriginMcpServer {
    OriginMcpServer::new(client, TransportMode::Stdio, "test-agent".into(), None)
}

fn text_of(result: &CallToolResult) -> String {
    for content in &result.content {
        match &content.raw {
            RawContent::Text(text) => return text.text.clone(),
            _ => continue,
        }
    }
    panic!(
        "expected at least one text Content block; got: {:?}",
        result.content
    );
}

async fn captured_body(mock: &MockServer) -> serde_json::Value {
    let received = mock
        .received_requests()
        .await
        .expect("wiremock captured no requests");
    assert_eq!(
        received.len(),
        1,
        "expected exactly 1 captured request, got {}",
        received.len()
    );
    serde_json::from_slice(&received[0].body).expect("captured body is not valid JSON")
}

fn sample_search_result() -> SearchResult {
    SearchResult {
        id: "1".into(),
        content: "some memory content".into(),
        source: "memory".into(),
        source_id: "mem_r1".into(),
        title: "title".into(),
        url: None,
        chunk_index: 0,
        last_modified: 0,
        score: 0.9,
        chunk_type: None,
        language: None,
        semantic_unit: None,
        memory_type: Some("fact".into()),
        domain: None,
        source_agent: None,
        confidence: None,
        confirmed: None,
        stability: None,
        supersedes: None,
        summary: None,
        entity_id: None,
        entity_name: None,
        quality: None,
        is_archived: false,
        is_recap: false,
        structured_fields: None,
        retrieval_cue: None,
        source_text: None,
        raw_score: 0.0,
    }
}

#[tokio::test]
async fn t1_remember_roundtrip() {
    let (mock, client) = setup().await;
    let response = StoreMemoryResponse {
        source_id: "mem_t1".into(),
        chunks_created: 1,
        memory_type: "fact".into(),
        entity_id: None,
        quality: None,
        warnings: vec![],
        extraction_method: "none".into(),

        enrichment: String::new(),

        hint: String::new(),
    };
    Mock::given(method("POST"))
        .and(path("/api/memory/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .capture_impl(CaptureParams {
            content: "anything".into(),
            memory_type: None,
            domain: None,
            entity: None,
            confidence: None,
            supersedes: None,
            structured_fields: None,
            retrieval_cue: None,
        })
        .await
        .expect("capture_impl failed");

    let text = text_of(&result);
    assert_eq!(text, "Stored mem_t1");

    let body = captured_body(&mock).await;
    assert_eq!(body["content"], serde_json::json!("anything"));
    assert_eq!(body["source_agent"], serde_json::json!("test-agent"));
}

#[tokio::test]
async fn t2_remember_surfaces_warnings_when_present() {
    let (mock, client) = setup().await;
    let response = StoreMemoryResponse {
        source_id: "mem_t2".into(),
        chunks_created: 1,
        memory_type: "decision".into(),
        entity_id: None,
        quality: None,
        warnings: vec!["decision memory missing required 'claim' field".into()],
        extraction_method: "agent".into(),

        enrichment: String::new(),

        hint: String::new(),
    };
    Mock::given(method("POST"))
        .and(path("/api/memory/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .capture_impl(CaptureParams {
            content: "anything".into(),
            memory_type: Some("decision".into()),
            domain: None,
            entity: None,
            confidence: None,
            supersedes: None,
            structured_fields: None,
            retrieval_cue: None,
        })
        .await
        .expect("capture_impl failed");

    let text = text_of(&result);
    assert!(
        text.starts_with("Stored mem_t2"),
        "expected source_id line first; got: {text}"
    );
    assert!(
        text.contains("Warnings:"),
        "expected Warnings: section; got: {text}"
    );
    assert!(
        text.contains("decision memory missing required 'claim' field"),
        "expected validation text; got: {text}"
    );
}

#[tokio::test]
async fn t3_structured_fields_schema_is_object() {
    use schemars::schema_for;

    let (mock, client) = setup().await;
    let response = StoreMemoryResponse {
        source_id: "mem_t3".into(),
        chunks_created: 1,
        memory_type: "fact".into(),
        entity_id: None,
        quality: None,
        warnings: vec![],
        extraction_method: "agent".into(),

        enrichment: String::new(),

        hint: String::new(),
    };
    Mock::given(method("POST"))
        .and(path("/api/memory/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let mut structured_fields = serde_json::Map::new();
    structured_fields.insert("theme".into(), serde_json::json!("dark"));

    let server = make_server(client);
    let result = server
        .capture_impl(CaptureParams {
            content: "prefers dark mode".into(),
            memory_type: None,
            domain: None,
            entity: None,
            confidence: None,
            supersedes: None,
            structured_fields: Some(structured_fields),
            retrieval_cue: None,
        })
        .await
        .expect("capture_impl failed");

    let text = text_of(&result);
    assert_eq!(text, "Stored mem_t3");

    let body = captured_body(&mock).await;
    assert_eq!(
        body["structured_fields"],
        serde_json::json!({ "theme": "dark" })
    );

    let schema = schema_for!(CaptureParams);
    let json = serde_json::to_value(&schema).unwrap();
    let sf = json
        .pointer("/properties/structured_fields")
        .expect("structured_fields property present in schema");
    let t = sf.pointer("/type").expect("type constraint present");
    match t {
        serde_json::Value::Array(arr) => {
            assert!(
                arr.iter().any(|v| v.as_str() == Some("object")),
                "expected 'object' among type array; got: {:?}",
                arr
            );
            assert!(
                arr.iter().any(|v| v.as_str() == Some("null")),
                "expected 'null' among type array; got: {:?}",
                arr
            );
        }
        serde_json::Value::String(s) => panic!("expected nullable object schema, got {}", s),
        other => panic!(
            "schema type constraint is not a string or array: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn t4_recall_roundtrip() {
    let (mock, client) = setup().await;
    let response = SearchMemoryResponse {
        results: vec![sample_search_result()],
        took_ms: 10.0,
    };
    Mock::given(method("POST"))
        .and(path("/api/memory/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .recall_impl(RecallParams {
            query: "anything".into(),
            limit: None,
            memory_type: None,
            domain: None,
        })
        .await
        .expect("recall_impl failed");

    let text = text_of(&result);
    assert!(
        text.contains("1 results"),
        "expected result count line; got: {text}"
    );
    assert!(
        text.contains("mem_r1"),
        "expected source_id in rendered JSON; got: {text}"
    );

    let body = captured_body(&mock).await;
    assert_eq!(body["query"], serde_json::json!("anything"));
    assert_eq!(body["limit"], serde_json::json!(10));
    assert!(body["memory_type"].is_null());
    assert!(body["domain"].is_null());
    assert_eq!(
        body["source_agent"],
        serde_json::json!("test-agent"),
        "recall should send resolved agent name, not null"
    );
}

#[tokio::test]
async fn t5_memory_type_hint_preserved_without_forcing_domain() {
    let (mock, client) = setup().await;
    let response = StoreMemoryResponse {
        source_id: "mem_t5".into(),
        chunks_created: 1,
        memory_type: "fact".into(),
        entity_id: None,
        quality: Some("medium".into()),
        warnings: vec![],
        extraction_method: "llm".into(),

        enrichment: String::new(),

        hint: String::new(),
    };
    Mock::given(method("POST"))
        .and(path("/api/memory/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .capture_impl(CaptureParams {
            content: "some content".into(),
            memory_type: Some("fact".into()),
            domain: None,
            entity: None,
            confidence: None,
            supersedes: None,
            structured_fields: None,
            retrieval_cue: None,
        })
        .await
        .expect("capture_impl failed");

    let text = text_of(&result);
    assert_eq!(text, "Stored mem_t5");

    let body = captured_body(&mock).await;
    assert_eq!(body["memory_type"], serde_json::json!("fact"));
    assert!(body["domain"].is_null());
}

#[tokio::test]
async fn t6_context_roundtrip_bug_regression() {
    let (mock, client) = setup().await;
    let response = ChatContextResponse {
        context: "you are Lucian".into(),
        profile: ProfileContext {
            narrative: "n".into(),
            identity: vec!["rust developer".into()],
            preferences: vec![],
            goals: vec![],
        },
        knowledge: KnowledgeContext {
            pages: vec![],
            decisions: vec![],
            relevant_memories: vec![],
            graph_context: vec![],
        },
        took_ms: 12.0,
        token_estimates: TierTokenEstimates {
            tier1_identity: 5,
            tier2_project: 10,
            tier3_relevant: 15,
            total: 30,
        },
    };
    Mock::given(method("POST"))
        .and(path("/api/chat-context"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .context_impl(ContextParams {
            topic: Some("orientation".into()),
            limit: None,
            domain: None,
        })
        .await
        .expect("context_impl failed");

    let text = text_of(&result);
    assert_eq!(text, "you are Lucian");

    let body = captured_body(&mock).await;
    assert_eq!(body["conversation_id"], serde_json::json!("orientation"));
    assert_eq!(body["max_chunks"], serde_json::json!(20));
    assert_eq!(body["include_goals"], serde_json::json!(true));
    assert!(body["domain"].is_null());
}

#[tokio::test]
async fn t7_context_with_domain() {
    let (mock, client) = setup().await;
    let response = ChatContextResponse {
        context: "work context".into(),
        profile: ProfileContext {
            narrative: String::new(),
            identity: vec![],
            preferences: vec![],
            goals: vec![],
        },
        knowledge: KnowledgeContext {
            pages: vec![],
            decisions: vec![],
            relevant_memories: vec![],
            graph_context: vec![],
        },
        took_ms: 5.0,
        token_estimates: TierTokenEstimates {
            tier1_identity: 0,
            tier2_project: 0,
            tier3_relevant: 0,
            total: 0,
        },
    };
    Mock::given(method("POST"))
        .and(path("/api/chat-context"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .context_impl(ContextParams {
            topic: None,
            limit: None,
            domain: Some("work".into()),
        })
        .await
        .expect("context_impl failed");

    let text = text_of(&result);
    assert_eq!(text, "work context");

    let body = captured_body(&mock).await;
    assert_eq!(body["domain"], serde_json::json!("work"));
}

#[tokio::test]
async fn t8_forget_roundtrip() {
    let (deleted_mock, deleted_client) = setup().await;
    let deleted_response = DeleteResponse { deleted: true };
    Mock::given(method("DELETE"))
        .and(path("/api/memory/delete/mem_xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&deleted_response))
        .mount(&deleted_mock)
        .await;

    let deleted_server = make_server(deleted_client);
    let deleted_result = deleted_server
        .forget_impl("mem_xyz")
        .await
        .expect("forget_impl failed for deleted=true");

    let deleted_text = text_of(&deleted_result);
    assert_eq!(deleted_text, "Memory deleted");

    let (missing_mock, missing_client) = setup().await;
    let missing_response = DeleteResponse { deleted: false };
    Mock::given(method("DELETE"))
        .and(path("/api/memory/delete/mem_missing"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&missing_response))
        .mount(&missing_mock)
        .await;

    let missing_server = make_server(missing_client);
    let missing_result = missing_server
        .forget_impl("mem_missing")
        .await
        .expect("forget_impl failed for deleted=false");

    let missing_text = text_of(&missing_result);
    assert_eq!(missing_text, "Memory not found");
}

#[tokio::test]
async fn t9_recall_request_does_not_contain_entity() {
    let (mock, client) = setup().await;
    let response = SearchMemoryResponse {
        results: vec![],
        took_ms: 1.0,
    };
    Mock::given(method("POST"))
        .and(path("/api/memory/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .recall_impl(RecallParams {
            query: "anything".into(),
            limit: None,
            memory_type: None,
            domain: None,
        })
        .await
        .expect("recall_impl failed");

    let text = text_of(&result);
    assert!(
        text.contains("0 results"),
        "expected empty result count; got: {text}"
    );

    let body = captured_body(&mock).await;
    let obj = body.as_object().expect("body is an object");
    assert!(
        !obj.contains_key("entity"),
        "entity field leaked into wire body: {:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn t10_remember_request_does_not_contain_user_id() {
    let (mock, client) = setup().await;
    let response = StoreMemoryResponse {
        source_id: "mem_t10".into(),
        chunks_created: 1,
        memory_type: "fact".into(),
        entity_id: None,
        quality: None,
        warnings: vec![],
        extraction_method: "none".into(),

        enrichment: String::new(),

        hint: String::new(),
    };
    Mock::given(method("POST"))
        .and(path("/api/memory/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .capture_impl(CaptureParams {
            content: "anything".into(),
            memory_type: None,
            domain: None,
            entity: None,
            confidence: None,
            supersedes: None,
            structured_fields: None,
            retrieval_cue: None,
        })
        .await
        .expect("capture_impl failed");

    let text = text_of(&result);
    assert_eq!(text, "Stored mem_t10");

    let body = captured_body(&mock).await;
    let obj = body.as_object().expect("body is an object");
    assert!(
        !obj.contains_key("user_id"),
        "user_id field leaked into wire body: {:?}",
        obj.keys().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn t11_extraction_method_none_not_in_text() {
    let (mock, client) = setup().await;
    let response = StoreMemoryResponse {
        source_id: "mem_t11".into(),
        chunks_created: 1,
        memory_type: "fact".into(),
        entity_id: None,
        quality: None,
        warnings: vec![],
        extraction_method: "none".into(),

        enrichment: String::new(),

        hint: String::new(),
    };
    Mock::given(method("POST"))
        .and(path("/api/memory/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&response))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .capture_impl(CaptureParams {
            content: "anything".into(),
            memory_type: None,
            domain: None,
            entity: None,
            confidence: None,
            supersedes: None,
            structured_fields: None,
            retrieval_cue: None,
        })
        .await
        .expect("capture_impl failed");

    let text = text_of(&result);
    assert!(
        !text.contains("extraction_method"),
        "extraction_method label leaked into text: {text}"
    );
    assert_eq!(text, "Stored mem_t11");
}

/// Regression test: context_impl must succeed even when the daemon returns
/// extra fields in the ChatContextResponse that origin-types v0.1.0 doesn't know
/// about (e.g. future enrichment fields added to SearchResult or top-level).
#[tokio::test]
async fn t13_context_forward_compat_with_extra_fields() {
    // Simulate a future daemon response that adds unknown fields to
    // relevant_memories items and to the top-level response.
    let raw_json = serde_json::json!({
        "context": "you are Lucian",
        "profile": {
            "narrative": "n",
            "identity": ["rust developer"],
            "preferences": [],
            "goals": []
        },
        "knowledge": {
            "relevant_memories": [{
                "id": "1",
                "content": "some memory",
                "source": "memory",
                "source_id": "mem_r1",
                "title": "title",
                "url": null,
                "chunk_index": 0,
                "last_modified": 0,
                "score": 0.9,
                "is_archived": false,
                "is_recap": false,
                "raw_score": 0.0,
                "unknown_future_field": "this should not break deserialization",
                "another_new_field": {"nested": "object"}
            }],
            "graph_context": [],
            "pages": [],
            "decisions": []
        },
        "took_ms": 12.0,
        "token_estimates": {
            "tier1_identity": 5,
            "tier2_project": 10,
            "tier3_relevant": 15,
            "total": 30
        },
        "top_level_future_field": "also ignored"
    });

    let (mock, client) = setup().await;
    Mock::given(method("POST"))
        .and(path("/api/chat-context"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&raw_json))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .context_impl(ContextParams {
            topic: None,
            limit: None,
            domain: None,
        })
        .await
        .expect("context_impl must succeed even with extra unknown fields in response");

    let text = text_of(&result);
    assert_eq!(text, "you are Lucian");
}

#[tokio::test]
async fn t12_forward_compat_response_missing_extraction_method() {
    let raw_json = serde_json::json!({
        "source_id": "mem_t12",
        "chunks_created": 2,
        "memory_type": "fact"
    });

    let (mock, client) = setup().await;
    Mock::given(method("POST"))
        .and(path("/api/memory/store"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&raw_json))
        .mount(&mock)
        .await;

    let server = make_server(client);
    let result = server
        .capture_impl(CaptureParams {
            content: "anything".into(),
            memory_type: None,
            domain: None,
            entity: None,
            confidence: None,
            supersedes: None,
            structured_fields: None,
            retrieval_cue: None,
        })
        .await
        .expect("capture_impl failed against pre-D9 response");

    let text = text_of(&result);
    assert_eq!(text, "Stored mem_t12");

    let parsed: StoreMemoryResponse = serde_json::from_value(raw_json).unwrap();
    assert_eq!(parsed.extraction_method, "unknown");
    assert!(parsed.warnings.is_empty());
}
