// SPDX-License-Identifier: Apache-2.0
//! Transaction-ownership regression tests for the canonical-write
//! extraction (`create_relation_on_connection`,
//! `retire_relation_on_connection`).
//!
//! Each test drives the helpers on the caller's own locked connection
//! inside an explicit caller-owned transaction, proving the helpers
//! neither commit on their own nor reach another connection: everything
//! they write must vanish on caller ROLLBACK and persist on caller
//! COMMIT. The lock guard is always released before calling any
//! lock-taking `MemoryDB` method, so no test can deadlock itself.
//!
//! Fixtures use the test-only `create_entity` (explicit same-space
//! shadow pages, so live edges classify as `assertion`) and canonical
//! types read back from `relation_canonicals` -- no invented vocabulary.
//! Full-row comparisons below index the `edges` row as:
//! 0 edge_id, 1 src_id, 2 src_kind, 3 dst_id, 4 dst_kind, 5 edge_type,
//! 6 lineage, 7 grounded, 8 root_id, 9 space, 10 weight, 11 payload,
//! 12 provenance, 13 operation_id, 14 created_at, 15 superseded_by,
//! 16 valid_until, 17 semantic_type.

use super::RelationWriteInput;
use crate::db::MemoryDB;

const SPACE_TX: &str = "space-tx";

/// First `n` canonical relation types from the seeded vocabulary.
async fn test_canonicals(db: &MemoryDB, n: usize) -> Vec<String> {
    let cans = db.relation_canonicals().await.unwrap();
    assert!(
        cans.len() >= n,
        "seeded vocabulary must hold at least {n} canonicals, found {}",
        cans.len()
    );
    cans.into_iter().take(n).collect()
}

async fn make_entities(db: &MemoryDB, names: &[&str]) -> Vec<String> {
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        ids.push(
            db.create_entity(name, "concept", Some(SPACE_TX))
                .await
                .unwrap(),
        );
    }
    ids
}

/// Directly mark an edge grounded, mirroring post-sweep state so retire
/// and generation assertions exercise the grounded-assertion path.
async fn ground_edge(db: &MemoryDB, edge_id: &str) {
    let guard = db.conn.lock().await;
    let conn: &libsql::Connection = &guard;
    conn.execute(
        "UPDATE edges SET grounded = 1 WHERE edge_id = ?1",
        libsql::params![edge_id],
    )
    .await
    .unwrap();
}

/// Full `edges` row with every column cast to text (`None` for NULL).
async fn read_edge_row(db: &MemoryDB, edge_id: &str) -> Option<Vec<Option<String>>> {
    let guard = db.conn.lock().await;
    let conn: &libsql::Connection = &guard;
    let mut rows = conn
        .query(
            "SELECT CAST(edge_id AS TEXT), CAST(src_id AS TEXT), CAST(src_kind AS TEXT), \
                    CAST(dst_id AS TEXT), CAST(dst_kind AS TEXT), CAST(edge_type AS TEXT), \
                    CAST(lineage AS TEXT), CAST(grounded AS TEXT), CAST(root_id AS TEXT), \
                    CAST(space AS TEXT), CAST(weight AS TEXT), CAST(payload AS TEXT), \
                    CAST(provenance AS TEXT), CAST(operation_id AS TEXT), \
                    CAST(created_at AS TEXT), CAST(superseded_by AS TEXT), \
                    CAST(valid_until AS TEXT), CAST(semantic_type AS TEXT) \
             FROM edges WHERE edge_id = ?1",
            libsql::params![edge_id],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap()?;
    let mut out = Vec::with_capacity(18);
    for i in 0..18 {
        out.push(
            row.get::<Option<String>>(i)
                .expect("edges column readable as text"),
        );
    }
    Some(out)
}

/// Whole `space_graph_state` table: the durable graph-generation record.
async fn read_graph_state(db: &MemoryDB) -> Vec<Vec<Option<String>>> {
    let guard = db.conn.lock().await;
    let conn: &libsql::Connection = &guard;
    let mut rows = conn
        .query(
            "SELECT CAST(space AS TEXT), CAST(graph_generation AS TEXT), \
                    CAST(grouping_generation AS TEXT), CAST(published_generation AS TEXT), \
                    CAST(dirty AS TEXT) \
             FROM space_graph_state ORDER BY space",
            libsql::params![],
        )
        .await
        .unwrap();
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        let mut entry = Vec::with_capacity(5);
        for i in 0..5 {
            entry.push(
                row.get::<Option<String>>(i)
                    .expect("graph state column readable as text"),
            );
        }
        out.push(entry);
    }
    out
}

async fn edge_is_active(db: &MemoryDB, edge_id: &str) -> bool {
    let guard = db.conn.lock().await;
    let conn: &libsql::Connection = &guard;
    edge_is_active_on(conn, edge_id).await
}

async fn edge_is_active_on(conn: &libsql::Connection, edge_id: &str) -> bool {
    let mut rows = conn
        .query(
            "SELECT valid_until IS NULL FROM edges WHERE edge_id = ?1",
            libsql::params![edge_id],
        )
        .await
        .unwrap();
    match rows.next().await.unwrap() {
        Some(row) => row.get::<i64>(0).unwrap_or(0) == 1,
        None => false,
    }
}

async fn insert_source_memory(db: &MemoryDB, source_id: &str, content: &str) {
    let guard = db.conn.lock().await;
    let conn: &libsql::Connection = &guard;
    conn.execute(
        "INSERT INTO memories (id, content, source, source_id, title, chunk_index, \
                                last_modified, chunk_type, space) \
         VALUES (?1, ?2, 'memory', ?3, 'tx title', 0, 0, 'text', ?4)",
        libsql::params![
            format!("m-{source_id}"),
            content.to_string(),
            source_id.to_string(),
            SPACE_TX.to_string()
        ],
    )
    .await
    .unwrap();
}

async fn memory_exists(db: &MemoryDB, source_id: &str) -> bool {
    let guard = db.conn.lock().await;
    let conn: &libsql::Connection = &guard;
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM memories WHERE source_id = ?1",
            libsql::params![source_id],
        )
        .await
        .unwrap();
    rows.next()
        .await
        .unwrap()
        .map(|row| row.get::<i64>(0).unwrap_or(0) > 0)
        .unwrap_or(false)
}

/// Case 1: a helper write inside the caller's transaction is visible
/// there, yet leaves no durable trace once the caller rolls back --
/// catching a helper that commits on its own or writes elsewhere.
#[tokio::test]
async fn create_helper_holds_no_commit_across_caller_rollback() {
    let (db, _tmp) = crate::db::tests::test_db().await;
    let canon = test_canonicals(&db, 1).await.into_iter().next().unwrap();
    let ids = make_entities(&db, &["Tx Alpha", "Tx Beta"]).await;
    let baseline_graph = read_graph_state(&db).await;
    let now = chrono::Utc::now().timestamp();

    let edge_id = {
        let guard = db.conn.lock().await;
        let conn: &libsql::Connection = &guard;
        conn.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        let (edge_id, existed, _) = MemoryDB::create_relation_on_connection(
            conn,
            RelationWriteInput {
                from_entity: &ids[0],
                to_entity: &ids[1],
                canonical: &canon,
                source_agent: Some("tx-test"),
                confidence: Some(0.7),
                explanation: None,
                source_memory_id: None,
                span_quote: None,
                source_content: None,
                model_version: None,
                prompt_version: None,
                now,
            },
        )
        .await
        .unwrap();
        assert!(!existed, "fresh fixture must mint, not re-assert");
        assert!(
            edge_is_active_on(conn, &edge_id).await,
            "minted edge must read back inside the caller's transaction"
        );
        conn.execute("ROLLBACK", ()).await.unwrap();
        edge_id
    };

    assert!(
        read_edge_row(&db, &edge_id).await.is_none(),
        "helper must not commit: rolled-back edge leaked durably"
    );
    assert_eq!(
        read_graph_state(&db).await,
        baseline_graph,
        "durable graph generation must be unchanged after caller rollback"
    );
}

/// Case 2: retire snapshots provenance and invalidates in-transaction;
/// caller ROLLBACK restores the full row and generation state, while a
/// separate caller-owned COMMIT persists retirement without touching
/// endpoint entities or the source memory record.
#[tokio::test]
async fn retire_helper_rollback_restores_row_and_commit_persists() {
    let (db, _tmp) = crate::db::tests::test_db().await;
    let canon = test_canonicals(&db, 1).await.into_iter().next().unwrap();
    let ids = make_entities(&db, &["Tx Gamma", "Tx Delta"]).await;
    let content = "gamma collaborates with delta in the ledger";
    insert_source_memory(&db, "mem-tx-2", content).await;
    let edge_id = db
        .create_relation_with_span(
            &ids[0],
            &ids[1],
            &canon,
            Some("agent-tx"),
            Some(0.9),
            Some("seeded explanation"),
            Some("mem-tx-2"),
            Some("collaborates"),
            Some(content),
            Some("model-tx-1"),
            Some("prompt-tx-1"),
        )
        .await
        .unwrap();
    ground_edge(&db, &edge_id).await;

    let baseline_row = read_edge_row(&db, &edge_id)
        .await
        .expect("seeded edge present");
    assert_eq!(
        baseline_row[6].as_deref(),
        Some("assertion"),
        "same-space endpoints must classify as assertion for a meaningful generation check"
    );
    assert_eq!(
        baseline_row[7].as_deref(),
        Some("1"),
        "grounded precondition"
    );
    assert_eq!(
        baseline_row[16], None,
        "active precondition (valid_until NULL)"
    );
    let baseline_graph = read_graph_state(&db).await;

    {
        let guard = db.conn.lock().await;
        let conn: &libsql::Connection = &guard;
        conn.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        let (snapshot, updates) = MemoryDB::retire_relation_on_connection(conn, &edge_id)
            .await
            .unwrap();
        let snap = snapshot.expect("active edge must produce a snapshot");
        assert_eq!(
            snap.get("from_entity").and_then(|v| v.as_str()),
            Some(ids[0].as_str()),
            "snapshot carries the losing edge endpoints"
        );
        assert_eq!(
            snap.get("to_entity").and_then(|v| v.as_str()),
            Some(ids[1].as_str())
        );
        assert_eq!(
            snap.get("relation_type").and_then(|v| v.as_str()),
            Some(canon.as_str())
        );
        assert_eq!(
            snap.get("source_memory_id").and_then(|v| v.as_str()),
            Some("mem-tx-2"),
            "archived snapshot includes source provenance"
        );
        assert_eq!(
            snap.get("source_agent").and_then(|v| v.as_str()),
            Some("agent-tx")
        );
        assert!(
            (snap
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                - 0.9)
                .abs()
                < f64::EPSILON,
            "archived snapshot includes confidence"
        );
        assert!(
            !updates.is_empty(),
            "retiring a grounded assertion edge must report generation updates"
        );
        assert!(
            !edge_is_active_on(conn, &edge_id).await,
            "edge must read invalidated inside the caller's transaction"
        );
        conn.execute("ROLLBACK", ()).await.unwrap();
    }

    assert_eq!(
        read_edge_row(&db, &edge_id).await,
        Some(baseline_row.clone()),
        "caller rollback must restore the original full edge row"
    );
    assert_eq!(
        read_graph_state(&db).await,
        baseline_graph,
        "caller rollback must restore graph generation state"
    );
    assert!(
        edge_is_active(&db, &edge_id).await,
        "edge active again after rollback"
    );

    {
        let guard = db.conn.lock().await;
        let conn: &libsql::Connection = &guard;
        conn.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        let (snapshot, _) = MemoryDB::retire_relation_on_connection(conn, &edge_id)
            .await
            .unwrap();
        assert!(snapshot.is_some());
        conn.execute("COMMIT", ()).await.unwrap();
    }

    assert!(
        !edge_is_active(&db, &edge_id).await,
        "retirement persists after caller-owned commit"
    );
    let retired = read_edge_row(&db, &edge_id)
        .await
        .expect("retired edge row is retained, never deleted");
    assert_eq!(
        retired[15], None,
        "soft invalidation keeps superseded_by NULL"
    );
    assert!(
        retired[16].is_some(),
        "soft invalidation stamps valid_until"
    );
    assert_eq!(
        db.count_entities().await.unwrap(),
        2,
        "endpoint entities unaffected by relation retirement"
    );
    assert!(
        memory_exists(&db, "mem-tx-2").await,
        "source memory record unaffected by relation retirement"
    );
}

/// Case 3: the repair seam -- minting a replacement predicate and
/// retiring the old one inside ONE caller transaction is atomic: a
/// forced caller rollback leaves the original active and the
/// replacement absent.
#[tokio::test]
async fn composed_create_plus_retire_rolls_back_atomically() {
    let (db, _tmp) = crate::db::tests::test_db().await;
    let cans = test_canonicals(&db, 2).await;
    let (old_canon, new_canon) = (cans[0].clone(), cans[1].clone());
    let ids = make_entities(&db, &["Tx Eps", "Tx Zeta"]).await;
    let old_id = db
        .create_relation_with_span(
            &ids[0],
            &ids[1],
            &old_canon,
            Some("agent-tx"),
            Some(0.8),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
    ground_edge(&db, &old_id).await;
    let baseline_row = read_edge_row(&db, &old_id)
        .await
        .expect("seeded edge present");
    let baseline_graph = read_graph_state(&db).await;
    let now = chrono::Utc::now().timestamp();

    let new_id = {
        let guard = db.conn.lock().await;
        let conn: &libsql::Connection = &guard;
        conn.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        let (new_id, new_existed, _) = MemoryDB::create_relation_on_connection(
            conn,
            RelationWriteInput {
                from_entity: &ids[0],
                to_entity: &ids[1],
                canonical: &new_canon,
                source_agent: Some("repair"),
                confidence: Some(0.95),
                explanation: None,
                source_memory_id: None,
                span_quote: None,
                source_content: None,
                model_version: None,
                prompt_version: None,
                now,
            },
        )
        .await
        .unwrap();
        assert!(!new_existed, "replacement predicate must be a fresh mint");
        assert_ne!(new_id, old_id, "distinct predicates address distinct edges");
        let (snapshot, _) = MemoryDB::retire_relation_on_connection(conn, &old_id)
            .await
            .unwrap();
        assert!(
            snapshot.is_some(),
            "old predicate retired in the same transaction"
        );
        assert!(
            !edge_is_active_on(conn, &old_id).await,
            "old edge invalid inside the composed transaction"
        );
        assert!(
            edge_is_active_on(conn, &new_id).await,
            "new edge visible inside the composed transaction"
        );
        conn.execute("ROLLBACK", ()).await.unwrap();
        new_id
    };

    assert!(
        edge_is_active(&db, &old_id).await,
        "original edge stays active after composed rollback"
    );
    assert_eq!(
        read_edge_row(&db, &old_id).await,
        Some(baseline_row),
        "original full edge row unchanged after composed rollback"
    );
    assert!(
        read_edge_row(&db, &new_id).await.is_none(),
        "replacement edge absent after composed rollback"
    );
    assert_eq!(
        read_graph_state(&db).await,
        baseline_graph,
        "graph generation unchanged after composed rollback"
    );
}
