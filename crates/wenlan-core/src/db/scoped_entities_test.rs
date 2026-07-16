// SPDX-License-Identifier: Apache-2.0

use super::tests::test_db;
use crate::read_scope::ReadScope;
use crate::sources::RawDocument;
use std::collections::HashMap;

fn memory_doc(source_id: &str, space: &str) -> RawDocument {
    RawDocument {
        source: "memory".to_string(),
        source_id: source_id.to_string(),
        title: source_id.to_string(),
        summary: None,
        content: "linked graph memory".to_string(),
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

#[tokio::test]
async fn search_entities_scopes_before_vector_limit() {
    let (db, _tmp) = test_db().await;
    let work_id = db
        .store_entity("Work entity", "project", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let mut personal_ids = Vec::new();
    for index in 0..8 {
        personal_ids.push(
            db.store_entity(
                &format!("Personal entity {index}"),
                "project",
                Some("personal"),
                None,
                Some(0.9),
            )
            .await
            .unwrap(),
        );
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
        "UPDATE entities SET embedding = vector32(?1) WHERE id = ?2",
        libsql::params![opposite, work_id.clone()],
    )
    .await
    .unwrap();
    for id in personal_ids {
        conn.execute(
            "UPDATE entities SET embedding = vector32(?1) WHERE id = ?2",
            libsql::params![exact.clone(), id],
        )
        .await
        .unwrap();
    }
    drop(conn);

    let results = db
        .search_entities_by_vector_scoped("quasar nebula", 1, &ReadScope::Space("work".to_string()))
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].entity.id, work_id);
}

#[tokio::test]
async fn graph_memory_fetch_excludes_cross_scope_rows() {
    let (db, _tmp) = test_db().await;
    let entity_id = db
        .create_entity("Shared topic", "topic", Some("work"))
        .await
        .unwrap();
    db.upsert_documents(vec![
        memory_doc("work-memory", "work"),
        memory_doc("personal-memory", "personal"),
    ])
    .await
    .unwrap();
    for source_id in ["work-memory", "personal-memory"] {
        db.link_memory_entities(source_id, &[entity_id.as_str()])
            .await
            .unwrap();
    }

    let results = db
        .get_memories_for_entities_scoped(
            std::slice::from_ref(&entity_id),
            10,
            &ReadScope::Space("work".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].source_id, "work-memory");
}

#[tokio::test]
async fn selected_khop_keeps_in_scope_neighbor_and_drops_cross_scope_endpoint() {
    let (db, _tmp) = test_db().await;
    let seed = db
        .store_entity("Work seed", "topic", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let work_neighbor = db
        .store_entity("Work neighbor", "topic", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let personal_neighbor = db
        .store_entity(
            "Personal neighbor",
            "topic",
            Some("personal"),
            None,
            Some(0.9),
        )
        .await
        .unwrap();
    db.create_relation(&seed, &work_neighbor, "related_to", None, None, None, None)
        .await
        .unwrap();
    db.create_relation(
        &seed,
        &personal_neighbor,
        "related_to",
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let expanded = db
        .expand_entities_khop_scoped(
            std::slice::from_ref(&seed),
            1,
            64,
            &ReadScope::Space("work".to_string()),
        )
        .await
        .unwrap();

    assert!(expanded.contains(&seed));
    assert!(expanded.contains(&work_neighbor));
    assert!(!expanded.contains(&personal_neighbor));
}

#[tokio::test]
async fn selected_graph_stream_keeps_positive_and_drops_cross_scope_memory() {
    let (db, _tmp) = test_db().await;
    let work_entity = db
        .store_entity("Scoped graph topic", "topic", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let personal_entity = db
        .store_entity(
            "Scoped graph topic",
            "topic",
            Some("personal"),
            None,
            Some(0.9),
        )
        .await
        .unwrap();
    db.upsert_documents(vec![
        memory_doc("work-graph-memory", "work"),
        memory_doc("personal-graph-memory", "personal"),
    ])
    .await
    .unwrap();
    db.link_memory_entities("work-graph-memory", &[work_entity.as_str()])
        .await
        .unwrap();
    db.link_memory_entities("personal-graph-memory", &[personal_entity.as_str()])
        .await
        .unwrap();

    let results = temp_env::async_with_vars(
        [
            ("WENLAN_GRAPH_MEMORY_STREAM", Some("1")),
            ("WENLAN_GRAPH_SURFACE_NEW", Some("1")),
        ],
        async {
            db.augment_with_graph_gated(
                "Scoped graph topic",
                Vec::new(),
                10,
                true,
                &ReadScope::Space("work".to_string()),
            )
            .await
            .unwrap()
        },
    )
    .await;

    assert!(results
        .iter()
        .any(|row| row.source_id == "work-graph-memory"));
    assert!(results
        .iter()
        .all(|row| row.source_id != "personal-graph-memory"));
}

#[tokio::test]
async fn selected_episode_channel_keeps_only_matching_scope() {
    let (db, _tmp) = test_db().await;
    let mut work = memory_doc("work-episode", "work");
    work.source = "episode".to_string();
    let mut personal = memory_doc("personal-episode", "personal");
    personal.source = "episode".to_string();
    db.upsert_documents(vec![work, personal]).await.unwrap();

    let results = db
        .search_episodes_scoped(
            "linked graph memory",
            10,
            &ReadScope::Space("work".to_string()),
        )
        .await
        .unwrap();

    assert!(results.iter().any(|row| row.source_id == "work-episode"));
    assert!(results
        .iter()
        .all(|row| row.source_id != "personal-episode"));
}

#[tokio::test]
async fn selected_fact_channel_rehydrates_only_matching_scope() {
    let (db, _tmp) = test_db().await;
    db.upsert_documents(vec![
        memory_doc("work-fact-parent", "work"),
        memory_doc("personal-fact-parent", "personal"),
    ])
    .await
    .unwrap();
    let embedding = db.get_or_compute_embedding("fact child query").unwrap();
    let vector = super::MemoryDB::vec_to_sql(&embedding);
    let conn = db.conn.lock().await;
    for (id, parent_id) in [
        ("work-child", "work-fact-parent"),
        ("personal-child", "personal-fact-parent"),
    ] {
        conn.execute(
            "INSERT INTO child_vectors (id, parent_kind, parent_id, field, content, embedding) \
             VALUES (?1, 'memory', ?2, 'fact', 'child fact', vector32(?3))",
            libsql::params![id, parent_id, vector.clone()],
        )
        .await
        .unwrap();
    }
    drop(conn);

    let results = db
        .search_facts_channel(
            "fact child query",
            10,
            &ReadScope::Space("work".to_string()),
        )
        .await
        .unwrap();

    assert!(results
        .iter()
        .any(|row| row.source_id == "work-fact-parent"));
    assert!(results
        .iter()
        .all(|row| row.source_id != "personal-fact-parent"));
}

#[tokio::test]
async fn list_entities_scoped_distinguishes_null_from_literal_uncategorized() {
    let (db, _tmp) = test_db().await;
    let work = db
        .store_entity("Scoped list work", "topic", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let personal = db
        .store_entity(
            "Scoped list personal",
            "topic",
            Some("personal"),
            None,
            Some(0.9),
        )
        .await
        .unwrap();
    let null = db
        .store_entity("Scoped list null", "topic", None, None, Some(0.9))
        .await
        .unwrap();
    let literal = db
        .store_entity(
            "Scoped list literal",
            "topic",
            Some("uncategorized"),
            None,
            Some(0.9),
        )
        .await
        .unwrap();

    let work_rows = db
        .list_entities_scoped(None, &ReadScope::Space("work".to_string()))
        .await
        .unwrap();
    assert_eq!(
        work_rows
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        vec![work.as_str()]
    );
    let null_rows = db
        .list_entities_scoped(None, &ReadScope::Uncategorized)
        .await
        .unwrap();
    assert_eq!(
        null_rows
            .iter()
            .map(|entity| entity.id.as_str())
            .collect::<Vec<_>>(),
        vec![null.as_str()]
    );
    let global = db
        .list_entities_scoped(None, &ReadScope::Global)
        .await
        .unwrap();
    let global_ids = global
        .iter()
        .map(|entity| entity.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    for id in [&work, &personal, &null, &literal] {
        assert!(global_ids.contains(id.as_str()));
    }
}

#[tokio::test]
async fn get_entity_detail_scoped_requires_matching_relation_endpoints() {
    let (db, _tmp) = test_db().await;
    let work = db
        .store_entity("Detail work", "topic", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let work_peer = db
        .store_entity("Detail work peer", "topic", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let personal = db
        .store_entity(
            "Detail personal",
            "topic",
            Some("personal"),
            None,
            Some(0.9),
        )
        .await
        .unwrap();
    let visible = db
        .create_relation(&work, &work_peer, "related_to", None, None, None, None)
        .await
        .unwrap();
    let hidden = db
        .create_relation(&work, &personal, "related_to", None, None, None, None)
        .await
        .unwrap();

    let detail = db
        .get_entity_detail_scoped(&work, &ReadScope::Space("work".to_string()))
        .await
        .unwrap();
    assert!(detail
        .relations
        .iter()
        .any(|relation| relation.id == visible));
    assert!(detail
        .relations
        .iter()
        .all(|relation| relation.id != hidden));
    assert!(matches!(
        db.get_entity_detail_scoped(&personal, &ReadScope::Space("work".to_string()))
            .await,
        Err(crate::WenlanError::NotFound(message)) if message == "entity not found"
    ));
}

#[tokio::test]
async fn list_recent_relations_scoped_requires_both_endpoints() {
    let (db, _tmp) = test_db().await;
    let work = db
        .store_entity("Relation work", "topic", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let work_peer = db
        .store_entity("Relation work peer", "topic", Some("work"), None, Some(0.9))
        .await
        .unwrap();
    let personal = db
        .store_entity(
            "Relation personal",
            "topic",
            Some("personal"),
            None,
            Some(0.9),
        )
        .await
        .unwrap();
    let visible = db
        .create_relation(&work, &work_peer, "related_to", None, None, None, None)
        .await
        .unwrap();
    let hidden = db
        .create_relation(&work, &personal, "related_to", None, None, None, None)
        .await
        .unwrap();

    let selected = db
        .list_recent_relations_scoped(20, None, &ReadScope::Space("work".to_string()))
        .await
        .unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|relation| relation.id.as_str())
            .collect::<Vec<_>>(),
        vec![visible.as_str()]
    );
    let global = db
        .list_recent_relations_scoped(20, None, &ReadScope::Global)
        .await
        .unwrap();
    assert!(global.iter().any(|relation| relation.id == hidden));
}

#[tokio::test]
async fn list_entity_suggestions_scoped_excludes_invalid_and_mixed_owner_sets() {
    let (db, _tmp) = test_db().await;
    db.upsert_documents(vec![
        memory_doc("suggest-work", "work"),
        memory_doc("suggest-personal", "personal"),
    ])
    .await
    .unwrap();
    for (id, sources) in [
        ("suggest-work-only", vec!["suggest-work".to_string()]),
        (
            "suggest-mixed",
            vec!["suggest-work".to_string(), "suggest-personal".to_string()],
        ),
        ("suggest-missing", vec!["suggest-absent".to_string()]),
        ("suggest-empty", Vec::new()),
    ] {
        db.insert_refinement_proposal(id, "suggest_entity", &sources, Some(id), 0.9)
            .await
            .unwrap();
    }
    let conn = db.conn.lock().await;
    conn.execute(
        "INSERT INTO refinement_queue (id, action, source_ids, payload, confidence) \
         VALUES ('suggest-malformed', 'suggest_entity', 'not-json', 'malformed', 0.9)",
        (),
    )
    .await
    .unwrap();
    drop(conn);

    let selected = db
        .list_entity_suggestions_scoped(&ReadScope::Space("work".to_string()))
        .await
        .unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|proposal| proposal.id.as_str())
            .collect::<Vec<_>>(),
        vec!["suggest-work-only"]
    );

    let global = db
        .list_entity_suggestions_scoped(&ReadScope::Global)
        .await
        .unwrap();
    let global_ids = global
        .iter()
        .map(|proposal| proposal.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    for id in [
        "suggest-work-only",
        "suggest-mixed",
        "suggest-missing",
        "suggest-empty",
        "suggest-malformed",
    ] {
        assert!(global_ids.contains(id));
    }
}
