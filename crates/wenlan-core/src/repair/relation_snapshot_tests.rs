// SPDX-License-Identifier: Apache-2.0
//! Focused regression tests for lossless relation snapshot capture.
//!
//! Scratch-only coverage of the behavioral boundaries in
//! `relation_snapshot::capture`: original edge payload/provenance fidelity
//! with change detection on source memory chunks, endpoint pages, and the
//! review row; capture scoping for unrelated edges and foreign activity;
//! lossless cell encoding; and the bounded-capture guard.

use super::*;
use crate::{
    db::{tests::test_db, TestEntity},
    error::WenlanError,
};
use wenlan_types::repair_relation::{
    RepairRelationSnapshot, RepairRelationSqlValue, RepairRelationTable,
    RepairRelationTableSnapshot,
};

struct RelationSnapshotFixture {
    db: MemoryDB,
    _dir: tempfile::TempDir,
    page_a: String,
    page_b: String,
}

async fn page_id_for(db: &MemoryDB, entity_id: &str) -> String {
    let session = db.test_primary_session().await;
    let mut rows = session
        .query(
            "SELECT page_id FROM entity_page_map WHERE entity_id=?1",
            libsql::params![entity_id.to_string()],
        )
        .await
        .unwrap();
    rows.next()
        .await
        .unwrap()
        .expect("seeded entity has a shadow page")
        .get::<String>(0)
        .unwrap()
}

async fn relation_snapshot_fixture() -> RelationSnapshotFixture {
    let (db, dir) = test_db().await;
    let session = db.test_primary_session().await;
    session
        .execute_batch(
            "INSERT INTO spaces (id,name,created_at,updated_at)
             VALUES ('space-work','work',1,1);",
        )
        .await
        .unwrap();
    drop(session);
    for (id, name) in [
        ("ent-a", "Alpha"),
        ("ent-b", "Beta"),
        ("ent-other", "Other"),
    ] {
        db.test_seed_entity_shadow_page(TestEntity::new(id, name, "concept").space("work"))
            .await
            .unwrap();
    }
    let session = db.test_primary_session().await;
    session
        .execute(
            "INSERT INTO memories
                 (id,content,source,source_id,title,chunk_index,last_modified,
                  chunk_type,pending_revision,is_recap,supersede_mode,
                  memory_type,space)
             VALUES
                 ('mem-a','scratch chunk alpha','memory','ent-a','scratch',
                  0,10,'text',0,0,'hide','fact','work'),
                 ('mem-other','scratch decoy chunk','memory','ent-other','scratch',
                  0,10,'text',0,0,'hide','fact','work'),
                 ('mem-note','scratch non-memory row','note','ent-a','scratch',
                  0,10,'text',0,0,'hide','fact','work')",
            (),
        )
        .await
        .unwrap();
    session
        .execute(
            "INSERT INTO edges
                 (edge_id,src_id,src_kind,dst_id,dst_kind,edge_type,lineage,
                  grounded,space,weight,payload,provenance,created_at)
             VALUES
                 ('edge-target','ent-a','entity','ent-b','entity','relates',
                  'evidence',1,'work',0.75,
                  '{\"rel\":\"works_on\"}','{\"source\":\"memory:mem-a\"}',
                  1721000000)",
            (),
        )
        .await
        .unwrap();
    session
        .execute(
            "INSERT INTO edges
                 (edge_id,src_id,src_kind,dst_id,dst_kind,edge_type,lineage,
                  grounded,space,created_at)
             VALUES
                 ('edge-decoy','ent-a','entity','ent-other','entity','relates',
                  'evidence',1,'work',1721000000)",
            (),
        )
        .await
        .unwrap();
    session
        .execute(
            "INSERT INTO refinement_queue (id,action,source_ids,payload)
             VALUES ('rv-1','review','[]','{\"choice\":\"works_on\"}')",
            (),
        )
        .await
        .unwrap();
    session
        .execute(
            "INSERT INTO agent_activity
                 (timestamp,agent_name,action,memory_ids,query,detail)
             VALUES
                 (1721000000,'tester','run','[]','manifest-1','this manifest'),
                 (1721000000,'tester','run','[]','manifest-other','decoy')",
            (),
        )
        .await
        .unwrap();
    drop(session);
    let page_a = page_id_for(&db, "ent-a").await;
    let page_b = page_id_for(&db, "ent-b").await;
    RelationSnapshotFixture {
        db,
        _dir: dir,
        page_a,
        page_b,
    }
}

fn context_for(owners: &[String]) -> RelationCaptureContext<'_> {
    RelationCaptureContext {
        manifest_id: "manifest-1",
        review_id: "rv-1",
        from_entity: "ent-a",
        to_entity: "ent-b",
        owner_ids: owners,
        canonical_relation_type: Some("works_on"),
        vocabulary_promotion: None,
    }
}

async fn capture_now(db: &MemoryDB, owners: &[String]) -> RepairRelationSnapshot {
    let snapshot = db.open_lint_snapshot().await.expect("lint snapshot opens");
    let captured = capture(&RelationReader::Snapshot(&snapshot), &context_for(owners))
        .await
        .expect("relation capture succeeds");
    assert!(snapshot.finish().await.unwrap().is_consistent());
    captured
}

fn table(
    snapshot: &RepairRelationSnapshot,
    table: RepairRelationTable,
) -> &RepairRelationTableSnapshot {
    snapshot
        .tables
        .iter()
        .find(|entry| entry.table == table)
        .expect("snapshot carries every table in order")
}

fn column(table: &RepairRelationTableSnapshot, name: &str) -> usize {
    table
        .columns
        .iter()
        .position(|entry| entry == name)
        .expect("captured table carries the column")
}

fn find_row<'a>(
    table: &'a RepairRelationTableSnapshot,
    key_column: &str,
    key: &str,
) -> &'a Vec<RepairRelationSqlValue> {
    let index = column(table, key_column);
    table
        .rows
        .iter()
        .find(|row| {
            row[index]
                == RepairRelationSqlValue::Text {
                    value: key.to_string(),
                }
        })
        .expect("captured table carries the row")
}

#[tokio::test]
async fn capture_records_edge_payload_and_detects_changed_source_chunk() {
    let fixture = relation_snapshot_fixture().await;
    let owners = vec!["ent-a".to_string(), "ent-b".to_string()];
    let before = capture_now(&fixture.db, &owners).await;

    let edges = table(&before, RepairRelationTable::Edges);
    let target = find_row(edges, "edge_id", "edge-target");
    assert_eq!(
        target[column(edges, "payload")],
        RepairRelationSqlValue::Text {
            value: "{\"rel\":\"works_on\"}".to_string(),
        }
    );
    assert_eq!(
        target[column(edges, "provenance")],
        RepairRelationSqlValue::Text {
            value: "{\"source\":\"memory:mem-a\"}".to_string(),
        }
    );
    let before_receipt = receipt(&before).expect("receipt digests the snapshot");

    fixture
        .db
        .test_primary_session()
        .await
        .execute(
            "UPDATE memories SET content='scratch chunk alpha edited' WHERE id='mem-a'",
            (),
        )
        .await
        .unwrap();
    let after = capture_now(&fixture.db, &owners).await;
    let after_receipt = receipt(&after).expect("receipt digests the snapshot");

    assert_ne!(
        before_receipt.as_str(),
        after_receipt.as_str(),
        "a changed source memory chunk must alter the snapshot receipt"
    );
    let memories = table(&after, RepairRelationTable::Memories);
    let chunk = find_row(memories, "id", "mem-a");
    assert_eq!(
        chunk[column(memories, "content")],
        RepairRelationSqlValue::Text {
            value: "scratch chunk alpha edited".to_string(),
        }
    );
}

#[tokio::test]
async fn endpoint_page_and_review_row_changes_alter_receipt() {
    let fixture = relation_snapshot_fixture().await;
    let owners = vec!["ent-a".to_string(), "ent-b".to_string()];
    let before = capture_now(&fixture.db, &owners).await;
    let before_receipt = receipt(&before).expect("receipt digests the snapshot");

    let session = fixture.db.test_primary_session().await;
    session
        .execute(
            &format!(
                "UPDATE pages SET content='scratch page alpha edited' WHERE id='{page}'",
                page = fixture.page_a
            ),
            (),
        )
        .await
        .unwrap();
    session
        .execute(
            "UPDATE refinement_queue SET payload='{\"choice\":\"knows\"}' WHERE id='rv-1'",
            (),
        )
        .await
        .unwrap();
    drop(session);

    let after = capture_now(&fixture.db, &owners).await;
    let after_receipt = receipt(&after).expect("receipt digests the snapshot");
    assert_ne!(
        before_receipt.as_str(),
        after_receipt.as_str(),
        "endpoint page and review row edits must not pass silently"
    );

    let pages = table(&after, RepairRelationTable::Pages);
    let page = find_row(pages, "id", &fixture.page_a);
    assert_eq!(
        page[column(pages, "content")],
        RepairRelationSqlValue::Text {
            value: "scratch page alpha edited".to_string(),
        }
    );
    let queue = table(&after, RepairRelationTable::RefinementQueue);
    let review = find_row(queue, "id", "rv-1");
    assert_eq!(
        review[column(queue, "payload")],
        RepairRelationSqlValue::Text {
            value: "{\"choice\":\"knows\"}".to_string(),
        }
    );
}

#[tokio::test]
async fn applied_receipt_survives_review_completion_but_binds_review_payload() {
    let fixture = relation_snapshot_fixture().await;
    let owners = vec!["ent-a".to_string(), "ent-b".to_string()];
    let before = capture_now(&fixture.db, &owners).await;
    let applied = applied_receipt(&before, &context_for(&owners)).unwrap();
    fixture.db.test_primary_session().await.execute(
        "UPDATE refinement_queue SET status='resolved', resolved_at=datetime(100,'unixepoch') WHERE id='rv-1'", (),
    ).await.unwrap();
    let completed = capture_now(&fixture.db, &owners).await;
    assert_ne!(receipt(&before).unwrap(), receipt(&completed).unwrap());
    assert_eq!(
        applied,
        applied_receipt(&completed, &context_for(&owners)).unwrap()
    );
    fixture
        .db
        .test_primary_session()
        .await
        .execute(
            "UPDATE refinement_queue SET payload='{}' WHERE id='rv-1'",
            (),
        )
        .await
        .unwrap();
    let altered = capture_now(&fixture.db, &owners).await;
    assert_ne!(
        applied,
        applied_receipt(&altered, &context_for(&owners)).unwrap()
    );
}

#[tokio::test]
async fn applied_receipt_ignores_background_dependencies_but_binds_owned_result() {
    let fixture = relation_snapshot_fixture().await;
    let owners = vec!["ent-a".to_string(), "ent-b".to_string()];
    let before = capture_now(&fixture.db, &owners).await;
    let context = context_for(&owners);
    let applied = applied_receipt(&before, &context).unwrap();
    fixture
        .db
        .test_primary_session()
        .await
        .execute_batch(
            "UPDATE memories SET access_count=COALESCE(access_count,0)+1, last_accessed=1721000200;
         UPDATE pages SET content=content || ' background enrichment';
         UPDATE space_graph_state SET graph_generation=graph_generation+1;
         UPDATE relation_type_vocabulary SET count=COALESCE(count,0)+1;
         INSERT INTO relation_type_vocabulary(canonical,aliases,category,count)
         VALUES ('background_predicate','[]','other',1);",
        )
        .await
        .unwrap();
    let background = capture_now(&fixture.db, &owners).await;
    assert_ne!(receipt(&before).unwrap(), receipt(&background).unwrap());
    assert_eq!(applied, applied_receipt(&background, &context).unwrap());

    // The projection must still detect tampering with any durable result,
    // including vocabulary meaning and this manifest's activity witness.
    for (kind, key, changed) in [
        (RepairRelationTable::Edges, "payload", "changed edge"),
        (
            RepairRelationTable::RelationTypeVocabulary,
            "aliases",
            "changed aliases",
        ),
        (
            RepairRelationTable::RefinementQueue,
            "payload",
            "changed review",
        ),
        (
            RepairRelationTable::AgentActivity,
            "detail",
            "changed activity",
        ),
        (
            RepairRelationTable::EntityPageMap,
            "page_id",
            "changed endpoint",
        ),
    ] {
        let mut tampered = background.clone();
        let target = tampered
            .tables
            .iter_mut()
            .find(|table| table.table == kind)
            .unwrap();
        let index = target.columns.iter().position(|name| name == key).unwrap();
        let row = if kind == RepairRelationTable::RelationTypeVocabulary {
            let canonical = target
                .columns
                .iter()
                .position(|name| name == "canonical")
                .unwrap();
            target.rows.iter_mut().find(|row| matches!(&row[canonical], RepairRelationSqlValue::Text { value } if value == "works_on")).unwrap()
        } else {
            &mut target.rows[0]
        };
        row[index] = RepairRelationSqlValue::Text {
            value: changed.to_string(),
        };
        assert_ne!(
            applied,
            applied_receipt(&tampered, &context).unwrap(),
            "{kind:?}"
        );
    }
}

#[tokio::test]
async fn unrelated_edges_and_foreign_activity_stay_out_of_capture() {
    let fixture = relation_snapshot_fixture().await;
    let owners = vec!["ent-a".to_string(), "ent-b".to_string()];
    let snapshot = capture_now(&fixture.db, &owners).await;

    let edges = table(&snapshot, RepairRelationTable::Edges);
    let edge_id = column(edges, "edge_id");
    assert!(
        edges.rows.iter().any(|row| row[edge_id]
            == RepairRelationSqlValue::Text {
                value: "edge-target".to_string()
            }),
        "the target pair edge is captured"
    );
    assert!(
        edges.rows.iter().all(|row| row[edge_id]
            != RepairRelationSqlValue::Text {
                value: "edge-decoy".to_string()
            }),
        "an edge for an unrelated pair must not enter the target capture"
    );

    let memories = table(&snapshot, RepairRelationTable::Memories);
    let memory_id = column(memories, "id");
    let captured: Vec<&str> = memories
        .rows
        .iter()
        .filter_map(|row| match &row[memory_id] {
            RepairRelationSqlValue::Text { value } => Some(value.as_str()),
            _ => None,
        })
        .collect();
    assert!(captured.contains(&"mem-a"));
    assert!(
        !captured.contains(&"mem-other") && !captured.contains(&"mem-note"),
        "only source='memory' chunks owned by this repair are captured, got: {captured:?}"
    );

    let pages = table(&snapshot, RepairRelationTable::Pages);
    let id_column = column(pages, "id");
    let captured_pages: Vec<&str> = pages
        .rows
        .iter()
        .filter_map(|row| match &row[id_column] {
            RepairRelationSqlValue::Text { value } => Some(value.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        captured_pages.contains(&fixture.page_a.as_str())
            && captured_pages.contains(&fixture.page_b.as_str()),
        "both endpoint pages are captured, got: {captured_pages:?}"
    );

    let activity = table(&snapshot, RepairRelationTable::AgentActivity);
    let query = column(activity, "query");
    assert!(
        !activity.rows.is_empty(),
        "this manifest's activity is captured"
    );
    assert!(
        activity.rows.iter().all(|row| row[query]
            == RepairRelationSqlValue::Text {
                value: "manifest-1".to_string()
            }),
        "activity is selected only by this manifest id"
    );
}

#[tokio::test]
async fn cell_encoding_preserves_real_bits_blob_text_and_null() {
    let fixture = relation_snapshot_fixture().await;
    fixture
        .db
        .test_primary_session()
        .await
        .execute(
            "UPDATE edges SET provenance=X'DEADBEEF' WHERE edge_id='edge-target'",
            (),
        )
        .await
        .unwrap();
    let owners = vec!["ent-a".to_string(), "ent-b".to_string()];
    let snapshot = capture_now(&fixture.db, &owners).await;

    let edges = table(&snapshot, RepairRelationTable::Edges);
    let target = find_row(edges, "edge_id", "edge-target");
    let expected_bits = format!("{:016x}", 0.75_f64.to_bits());
    assert_eq!(
        target[column(edges, "weight")],
        RepairRelationSqlValue::Real {
            bits: expected_bits.clone()
        }
    );
    let round_trip = f64::from_bits(u64::from_str_radix(&expected_bits, 16).expect("bits are hex"));
    assert_eq!(round_trip, 0.75_f64);
    assert_eq!(
        target[column(edges, "semantic_type")],
        RepairRelationSqlValue::Null
    );

    assert_eq!(
        target[column(edges, "provenance")],
        RepairRelationSqlValue::Blob {
            hex: "deadbeef".to_string()
        }
    );
}

#[tokio::test]
async fn oversized_source_text_is_rejected_before_materializing() {
    let fixture = relation_snapshot_fixture().await;
    let oversized = "x".repeat(8 * 1024 * 1024);
    fixture
        .db
        .test_primary_session()
        .await
        .execute(
            "UPDATE memories SET content=?1 WHERE id='mem-a'",
            libsql::params![oversized],
        )
        .await
        .unwrap();
    let owners = vec!["ent-a".to_string(), "ent-b".to_string()];

    let snapshot = fixture
        .db
        .open_lint_snapshot()
        .await
        .expect("lint snapshot opens");
    let result = capture(&RelationReader::Snapshot(&snapshot), &context_for(&owners)).await;
    drop(snapshot);

    assert!(
        matches!(
            &result,
            Err(WenlanError::Validation(message))
                if message == "repair_relation_snapshot_too_large"
        ),
        "bounded capture must reject oversized text, got: {result:?}"
    );
}
