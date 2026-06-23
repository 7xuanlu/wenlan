// SPDX-License-Identifier: Apache-2.0
//! Cross-crate integration test for Spec C-2 mutate wrappers.
//!
//! Adversarial-finding mitigation from PR #101: pure wiremock cannot prove
//! URL alignment between wrapper and daemon route. This test boots the real
//! `origin-server::router::build_router` in-process and invokes each wrapper
//! through a real HTTP server. If a wrapper URL drifts from a daemon route,
//! the real router returns 404 and the assertion fails.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;

use rmcp::model::{CallToolResult, RawContent};
use wenlan_core::{db::MemoryDB, NoopEmitter};
use wenlan_mcp::{
    client::WenlanClient,
    tools::{
        AcceptRevisionRequest, DismissContradictionRequest, DismissRevisionRequest, TransportMode,
        WenlanMcpServer,
    },
};
use wenlan_types::{
    ContradictionDismissResponse, RawDocument, RevisionAcceptResponse, RevisionDismissResponse,
};

// ── helpers ────────────────────────────────────────────────────────────────

async fn boot_test_server() -> (String, Arc<MemoryDB>) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = Arc::new(
        MemoryDB::new(&db_path, Arc::new(NoopEmitter))
            .await
            .unwrap(),
    );

    let state = Arc::new(RwLock::new(wenlan_server::state::ServerState {
        db: Some(db.clone()),
        ..wenlan_server::state::ServerState::default()
    }));
    let router = wenlan_server::router::build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    // Keep the tempdir alive for the process lifetime via intentional forget.
    std::mem::forget(tmp);
    (format!("http://{}", addr), db)
}

fn make_server(base_url: &str) -> WenlanMcpServer {
    let client = WenlanClient::new(base_url.to_string()).with_agent_name("test-agent".to_string());
    WenlanMcpServer::new(client, TransportMode::Stdio, "test-agent".into(), None)
}

/// Extract the text body from a successful CallToolResult.
fn text_of(result: &CallToolResult) -> String {
    for content in &result.content {
        if let RawContent::Text(t) = &content.raw {
            return t.text.clone();
        }
    }
    panic!(
        "expected at least one text Content block; got: {:?}",
        result.content
    );
}

/// Seed a pending revision via public API (upsert_documents).
///
/// The target memory is a normal confirmed memory; the revision memory has
/// `pending_revision: true` and `supersedes: Some(target_source_id)`.
async fn seed_pending_revision(db: &MemoryDB, target_source_id: &str, revision_source_id: &str) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Insert target (original, confirmed)
    db.upsert_documents(vec![RawDocument {
        source: "memory".to_string(),
        source_id: target_source_id.to_string(),
        title: "original".to_string(),
        content: "original content".to_string(),
        last_modified: now,
        confirmed: Some(true),
        stability: Some("confirmed".to_string()),
        memory_type: Some("fact".to_string()),
        ..RawDocument::default()
    }])
    .await
    .unwrap();

    // Insert revision (pending_revision=true, supersedes=target)
    db.upsert_documents(vec![RawDocument {
        source: "memory".to_string(),
        source_id: revision_source_id.to_string(),
        title: "revision".to_string(),
        content: "revised content".to_string(),
        last_modified: now,
        confirmed: Some(false),
        stability: Some("new".to_string()),
        memory_type: Some("fact".to_string()),
        supersedes: Some(target_source_id.to_string()),
        pending_revision: true,
        ..RawDocument::default()
    }])
    .await
    .unwrap();
}

/// Seed a contradiction refinement row (detect_contradiction, awaiting_review).
async fn seed_contradiction(db: &MemoryDB, proposal_id: &str, source_id: &str) {
    db.insert_refinement_proposal(
        proposal_id,
        "detect_contradiction",
        &[source_id.to_string()],
        None,
        0.9,
    )
    .await
    .unwrap();
    // Upgrade from default 'pending' to 'awaiting_review' so dismiss finds it.
    db.resolve_refinement_if_open(proposal_id, "awaiting_review")
        .await
        .unwrap();
}

/// Count agent_activity rows matching action + agent.
async fn activity_count(db: &MemoryDB, action: &str, agent: &str) -> usize {
    db.list_agent_activity(100, Some(agent), None)
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.action == action)
        .count()
}

// ── tests ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn accept_revision_aligns_with_real_router() {
    let (base_url, db) = boot_test_server().await;
    seed_pending_revision(&db, "mem_real_acc", "mem_real_acc_rev").await;

    let server = make_server(&base_url);
    let result = server
        .accept_revision_impl(AcceptRevisionRequest {
            target_source_id: "mem_real_acc".into(),
        })
        .await
        .unwrap();

    let body = text_of(&result);
    let parsed: RevisionAcceptResponse = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed.target_source_id, "mem_real_acc");
    assert_eq!(parsed.revision_source_id, "mem_real_acc_rev");

    assert_eq!(
        activity_count(&db, "revision_accept", "test-agent").await,
        1
    );
}

#[tokio::test]
async fn dismiss_revision_aligns_with_real_router() {
    let (base_url, db) = boot_test_server().await;
    seed_pending_revision(&db, "mem_real_dis", "mem_real_dis_rev").await;

    let server = make_server(&base_url);
    let result = server
        .dismiss_revision_impl(DismissRevisionRequest {
            target_source_id: "mem_real_dis".into(),
        })
        .await
        .unwrap();

    let body = text_of(&result);
    let parsed: RevisionDismissResponse = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed.target_source_id, "mem_real_dis");
    assert!(parsed.wrote);

    assert_eq!(
        activity_count(&db, "revision_dismiss", "test-agent").await,
        1
    );
}

#[tokio::test]
async fn dismiss_contradiction_aligns_with_real_router() {
    let (base_url, db) = boot_test_server().await;
    seed_contradiction(&db, "ref_real_contra", "mem_real_contra").await;

    let server = make_server(&base_url);
    let result = server
        .dismiss_contradiction_impl(DismissContradictionRequest {
            source_id: "mem_real_contra".into(),
        })
        .await
        .unwrap();

    let body = text_of(&result);
    let parsed: ContradictionDismissResponse = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed.source_id, "mem_real_contra");
    assert!(parsed.wrote);

    // Activity row proves request reached the right route.
    assert_eq!(
        activity_count(&db, "contradiction_dismiss", "test-agent").await,
        1,
        "activity row should prove request reached the right route"
    );

    // State-flip is the adversarial guard for this test: silent-idempotent dismiss
    // cannot be detected by 404, so we verify the row flipped to 'dismissed'.
    let proposal = db
        .get_refinement_proposal("ref_real_contra")
        .await
        .unwrap()
        .expect("proposal should still exist");
    assert_eq!(
        proposal.status, "dismissed",
        "refinement_queue row should be flipped to dismissed"
    );
}
