// SPDX-License-Identifier: Apache-2.0
//! Shared helpers for origin-server integration tests.

use origin_core::db::MemoryDB;
use origin_core::events::NoopEmitter;
use origin_core::quality_gate::QualityGate;
use origin_core::sources::RawDocument;
use origin_server::router::build_router;
use origin_server::state::ServerState;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Insert a page with a single wikilink reference whose target does not exist,
/// producing an orphan `page_links` row (`target_page_id IS NULL`).
///
/// The `[[orphan_label]]` syntax in content causes `insert_page` to call
/// `refresh_page_wikilinks`, which writes a `page_links` row with
/// `target_page_id = NULL` (orphan) because no page with that title exists yet.
#[allow(dead_code)]
pub async fn insert_page_with_orphan_link(
    db: &Arc<MemoryDB>,
    page_id: &str,
    page_title: &str,
    orphan_label: &str,
) {
    let now = chrono::Utc::now().to_rfc3339();
    let content = format!("References [[{orphan_label}]] in this page.");
    db.insert_page(page_id, page_title, None, &content, None, None, &[], &now)
        .await
        .expect("insert_page must succeed in test fixture");
}

/// Build a test app and return `(router, tmp, db_arc)`.
///
/// The caller binds `_tmp` to keep the `TempDir` alive for the test's
/// duration; it drops (and cleans up) when the test function returns.
#[allow(dead_code)]
pub async fn test_app() -> (axum::Router, tempfile::TempDir, Arc<MemoryDB>) {
    let dir = tempfile::tempdir().unwrap();
    let db = MemoryDB::new(dir.path(), Arc::new(NoopEmitter))
        .await
        .unwrap();
    let db_arc = Arc::new(db);
    let state = ServerState {
        db: Some(db_arc.clone()),
        ..ServerState::default()
    };
    let router = build_router(Arc::new(RwLock::new(state)));
    (router, dir, db_arc)
}

/// Build a test app with the quality gate disabled.
///
/// Use this when the test needs to store memories through the HTTP API
/// (exercising the full store handler) but the novelty filter would
/// otherwise reject content that is intentionally similar to an existing
/// memory — e.g., when testing the topic-match-protected path.
#[allow(dead_code)]
pub async fn test_app_no_gate() -> (axum::Router, tempfile::TempDir, Arc<MemoryDB>) {
    let dir = tempfile::tempdir().unwrap();
    let db = MemoryDB::new(dir.path(), Arc::new(NoopEmitter))
        .await
        .unwrap();
    let db_arc = Arc::new(db);
    let gate_cfg = origin_core::tuning::GateConfig {
        enabled: false,
        ..Default::default()
    };
    let state = ServerState {
        db: Some(db_arc.clone()),
        quality_gate: QualityGate::new(gate_cfg),
        ..ServerState::default()
    };
    let router = build_router(Arc::new(RwLock::new(state)));
    (router, dir, db_arc)
}

/// Count `agent_activity` rows matching both `action` and `agent_name`.
/// Used by curation mutate HTTP tests to verify activity logging without
/// requiring direct access to the private `conn` field.
#[allow(dead_code)]
pub async fn count_activity_for_action_and_agent(
    db: &Arc<MemoryDB>,
    action: &str,
    agent_name: &str,
) -> i64 {
    let rows = db
        .list_agent_activity(1000, Some(agent_name), None)
        .await
        .unwrap();
    rows.iter().filter(|r| r.action == action).count() as i64
}

/// Insert a memory row directly via `upsert_documents`.
///
/// Matches the `insert_memory_for_test` helper used in `origin-core`'s DB
/// unit tests. All NOT NULL columns in `memories` are covered.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub async fn insert_memory(
    db: &Arc<MemoryDB>,
    source_id: &str,
    content: &str,
    source: &str,
    source_agent: Option<&str>,
    supersedes: Option<&str>,
    pending_revision: bool,
    last_modified: i64,
) {
    let doc = RawDocument {
        source: source.to_string(),
        source_id: source_id.to_string(),
        title: format!("title-{}", source_id),
        content: content.to_string(),
        source_agent: source_agent.map(|s| s.to_string()),
        supersedes: supersedes.map(|s| s.to_string()),
        pending_revision,
        last_modified,
        ..Default::default()
    };
    db.upsert_documents(vec![doc]).await.unwrap();
}
