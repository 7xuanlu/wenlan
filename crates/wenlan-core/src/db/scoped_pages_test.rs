// SPDX-License-Identifier: Apache-2.0

use super::tests::test_db;
use crate::pages::Page;
use crate::read_scope::ReadScope;
use crate::sources::RawDocument;
use std::collections::{HashMap, HashSet};

fn memory_doc(source_id: &str, space: &str) -> RawDocument {
    RawDocument {
        source: "memory".to_string(),
        source_id: source_id.to_string(),
        title: source_id.to_string(),
        summary: None,
        content: "deliberately unrelated candidate text".to_string(),
        url: None,
        last_modified: chrono::Utc::now().timestamp(),
        metadata: HashMap::new(),
        memory_type: Some("fact".to_string()),
        space: Some(space.to_string()),
        source_agent: None,
        confidence: Some(0.9),
        confirmed: Some(true),
        supersedes: None,
        pending_revision: false,
        ..Default::default()
    }
}

fn page(id: &str, workspace: Option<&str>) -> Page {
    Page {
        id: id.to_string(),
        title: id.to_string(),
        summary: None,
        content: String::new(),
        entity_id: None,
        space: None,
        source_memory_ids: vec!["work-memory".to_string()],
        version: 1,
        status: "active".to_string(),
        created_at: String::new(),
        last_compiled: String::new(),
        last_modified: String::new(),
        sources_updated_count: 0,
        stale_reason: None,
        pending_rebuild: None,
        user_edited: false,
        relevance_score: 1.0,
        last_edited_by: None,
        last_edited_at: None,
        last_delta_summary: None,
        changelog: None,
        creation_kind: "distilled".to_string(),
        review_status: "confirmed".to_string(),
        workspace: workspace.map(str::to_string),
        citations: Vec::new(),
    }
}

#[tokio::test]
async fn search_memory_scopes_before_vector_limit() {
    let (db, _tmp) = test_db().await;
    db.upsert_documents(vec![memory_doc("work-memory", "work")])
        .await
        .unwrap();
    for index in 0..8 {
        db.upsert_documents(vec![memory_doc(&format!("personal-{index}"), "personal")])
            .await
            .unwrap();
    }

    let query_embedding = db.get_or_compute_embedding("quasar nebula").unwrap();
    let exact = super::MemoryDB::vec_to_sql(&query_embedding);
    let opposite = super::MemoryDB::vec_to_sql(
        &query_embedding
            .iter()
            .map(|value| -*value)
            .collect::<Vec<_>>(),
    );
    let conn = db.conn.lock().await;
    conn.execute(
        "UPDATE memories SET embedding = vector32(?1) WHERE source_id = 'work-memory'",
        libsql::params![opposite],
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE memories SET embedding = vector32(?1) WHERE source_id LIKE 'personal-%'",
        libsql::params![exact],
    )
    .await
    .unwrap();
    drop(conn);

    let results = db
        .search_memory(
            "quasar nebula",
            1,
            None,
            &ReadScope::Space("work".to_string()),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].source_id, "work-memory");
}

#[tokio::test]
async fn legacy_search_boundary_scopes_before_vector_limit() {
    let (db, _tmp) = test_db().await;
    db.upsert_documents(vec![memory_doc("work-search", "work")])
        .await
        .unwrap();
    for index in 0..8 {
        db.upsert_documents(vec![memory_doc(
            &format!("personal-search-{index}"),
            "personal",
        )])
        .await
        .unwrap();
    }

    let query_embedding = db.get_or_compute_embedding("legacy route query").unwrap();
    let exact = super::MemoryDB::vec_to_sql(&query_embedding);
    let opposite = super::MemoryDB::vec_to_sql(
        &query_embedding
            .iter()
            .map(|value| -*value)
            .collect::<Vec<_>>(),
    );
    let conn = db.conn.lock().await;
    conn.execute(
        "UPDATE memories SET embedding = vector32(?1) WHERE source_id = 'work-search'",
        libsql::params![opposite],
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE memories SET embedding = vector32(?1) WHERE source_id LIKE 'personal-search-%'",
        libsql::params![exact],
    )
    .await
    .unwrap();
    drop(conn);

    let results = db
        .search(
            "legacy route query",
            1,
            Some("memory"),
            &ReadScope::Space("work".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].source_id, "work-search");
}

#[tokio::test]
async fn search_pages_scopes_before_vector_limit() {
    let (db, _tmp) = test_db().await;
    let now = chrono::Utc::now().to_rfc3339();
    db.insert_page_with_kind(
        "work-page",
        "Work page",
        None,
        "deliberately unrelated page text",
        None,
        None,
        &[],
        &now,
        "distilled",
        "confirmed",
        Some("work"),
        None,
    )
    .await
    .unwrap();
    for index in 0..8 {
        db.insert_page_with_kind(
            &format!("personal-page-{index}"),
            "Personal page",
            None,
            "deliberately unrelated page text",
            None,
            None,
            &[],
            &now,
            "distilled",
            "confirmed",
            Some("personal"),
            None,
        )
        .await
        .unwrap();
    }

    let query_embedding = db.get_or_compute_embedding("quasar nebula").unwrap();
    let exact = super::MemoryDB::vec_to_sql(&query_embedding);
    let opposite = super::MemoryDB::vec_to_sql(
        &query_embedding
            .iter()
            .map(|value| -*value)
            .collect::<Vec<_>>(),
    );
    let conn = db.conn.lock().await;
    conn.execute(
        "UPDATE pages SET embedding = vector32(?1) WHERE id = 'work-page'",
        libsql::params![opposite],
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE pages SET embedding = vector32(?1) WHERE id LIKE 'personal-page-%'",
        libsql::params![exact],
    )
    .await
    .unwrap();
    drop(conn);

    let results = db
        .search_pages_scoped(
            "quasar nebula",
            1,
            None,
            &ReadScope::Space("work".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "work-page");
}

#[tokio::test]
async fn selected_page_visibility_requires_matching_workspace() {
    let (db, _tmp) = test_db().await;
    let source_ids = HashSet::from(["work-memory".to_string()]);
    let visible = db
        .select_visible_pages_scoped(
            vec![
                page("work", Some("work")),
                page("personal", Some("personal")),
            ],
            &ReadScope::Space("work".to_string()),
            &source_ids,
            "full",
            10,
        )
        .await;

    assert_eq!(
        visible
            .iter()
            .map(|page| page.id.as_str())
            .collect::<Vec<_>>(),
        vec!["work"]
    );
}

#[tokio::test]
async fn selected_summary_requires_nonempty_all_matching_sources() {
    let (db, _tmp) = test_db().await;
    db.upsert_documents(vec![
        memory_doc("work-summary-source", "work"),
        memory_doc("personal-summary-source", "personal"),
    ])
    .await
    .unwrap();
    let embedding = db.get_or_compute_embedding("summary scope query").unwrap();
    db.insert_summary_node(
        "work-summary",
        0,
        Some("work"),
        "Work summary",
        "summary scope query",
        &embedding,
        1,
        1,
        &["work-summary-source".to_string()],
    )
    .await
    .unwrap();
    db.insert_summary_node(
        "mixed-summary",
        0,
        Some("mixed"),
        "Mixed summary",
        "summary scope query",
        &embedding,
        2,
        1,
        &[
            "work-summary-source".to_string(),
            "personal-summary-source".to_string(),
        ],
    )
    .await
    .unwrap();
    db.insert_summary_node(
        "empty-summary",
        0,
        Some("empty"),
        "Empty summary",
        "summary scope query",
        &embedding,
        0,
        1,
        &[],
    )
    .await
    .unwrap();

    let results = db
        .search_summary_nodes_scoped(
            "summary scope query",
            10,
            &ReadScope::Space("work".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(
        results
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        vec!["work-summary"]
    );
}
