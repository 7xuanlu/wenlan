// SPDX-License-Identifier: Apache-2.0
//! Shared helpers for origin-server integration tests.

use origin_core::db::MemoryDB;
use origin_core::events::NoopEmitter;
use origin_core::sources::RawDocument;
use origin_server::router::build_router;
use origin_server::state::ServerState;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Build a test app and return `(router, tmp, db_arc)`.
///
/// The caller binds `_tmp` to keep the `TempDir` alive for the test's
/// duration; it drops (and cleans up) when the test function returns.
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

/// Insert a memory row directly via `upsert_documents`.
///
/// Matches the `insert_memory_for_test` helper used in `origin-core`'s DB
/// unit tests. All NOT NULL columns in `memories` are covered.
#[allow(clippy::too_many_arguments)]
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
