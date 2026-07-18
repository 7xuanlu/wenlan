// SPDX-License-Identifier: Apache-2.0

use super::super::tests::test_db;
use super::{CreateNodeOutcome, MemoryDB, NodePatch};

async fn seed_page(db: &MemoryDB, page_id: &str) {
    let conn = db.conn.lock().await;
    conn.execute(
        "INSERT INTO pages (id, title, content, created_at, last_compiled, last_modified) \
         VALUES (?1, 'Test Page', 'body', datetime('now'), datetime('now'), datetime('now'))",
        libsql::params![page_id],
    )
    .await
    .unwrap();
}

fn root_id(nodes: &[super::PageMapNode]) -> String {
    nodes
        .iter()
        .find(|n| n.parent_id.is_none())
        .expect("root node")
        .id
        .clone()
}

#[tokio::test]
async fn init_page_map_idempotent() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;

    let first = db.init_page_map("page-1").await.unwrap();
    assert_eq!(first.map.revision, 1);
    assert_eq!(first.nodes.len(), 1);
    let root = &first.nodes[0];
    assert!(root.parent_id.is_none());
    assert_eq!(root.ref_kind, "page");
    assert_eq!(root.ref_id, "page-1");
    assert_eq!(root.fingerprint, "page:page-1@~");

    // Second call is a no-op: same revision, same single root, no new row.
    let second = db.init_page_map("page-1").await.unwrap();
    assert_eq!(second.map.revision, 1);
    assert_eq!(second.nodes.len(), 1);
    assert_eq!(second.nodes[0].id, root.id);
}

#[tokio::test]
async fn create_map_node_fingerprint_dedup() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let map = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    let first = db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-1",
            None,
            0.0,
        )
        .await
        .unwrap();
    let created = match first {
        CreateNodeOutcome::Created(node) => node,
        other => panic!("expected Created, got {other:?}"),
    };
    assert_eq!(created.fingerprint, "memory:mem-1@~");

    // Same (ref_kind, ref_id, parent) proposed again -> Duplicate, no revision bump.
    let revision_after_first = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let second = db
        .create_map_node(
            "page-1",
            revision_after_first,
            &root,
            "memory",
            "mem-1",
            None,
            1.0,
        )
        .await
        .unwrap();
    match second {
        CreateNodeOutcome::Duplicate(node) => assert_eq!(node.id, created.id),
        other => panic!("expected Duplicate, got {other:?}"),
    }
    let revision_after_second = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    assert_eq!(revision_after_second, revision_after_first);
}

// Mutation-proof note: this test only has teeth against the tombstone branch
// in `create_map_node` (`if existing.status == "dismissed" { Tombstoned }
// else { Duplicate(existing) }`). Flipping that branch to always return
// `Duplicate(existing)` makes this test fail on the `Tombstoned` assert
// below (the mutation was applied locally to confirm the failure, then
// reverted — it is not left in the shipped code).
#[tokio::test]
async fn dismissed_tombstone_blocks_reproposal() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let map = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    let created = match db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-1",
            None,
            0.0,
        )
        .await
        .unwrap()
    {
        CreateNodeOutcome::Created(node) => node,
        other => panic!("expected Created, got {other:?}"),
    };

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    db.delete_map_node("page-1", rev, &created.id)
        .await
        .unwrap();

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let outcome = db
        .create_map_node("page-1", rev, &root, "memory", "mem-1", None, 0.0)
        .await
        .unwrap();
    assert_eq!(outcome, CreateNodeOutcome::Tombstoned);

    // Tombstoned outcome must not have inserted a second row or bumped the revision.
    let after = db.get_page_map("page-1", true).await.unwrap().unwrap();
    assert_eq!(after.map.revision, rev);
    assert_eq!(
        after
            .nodes
            .iter()
            .filter(|n| n.fingerprint == "memory:mem-1@~")
            .count(),
        1
    );
}

#[tokio::test]
async fn revision_bumps_on_every_write_and_stale_base_rejected() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let map = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);
    let stale_revision = map.map.revision;

    db.create_map_node(
        "page-1",
        stale_revision,
        &root,
        "memory",
        "mem-1",
        None,
        0.0,
    )
    .await
    .unwrap();
    let bumped = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    assert_eq!(bumped, stale_revision + 1);

    // Reusing the now-stale base_revision must be rejected as a conflict.
    let err = db
        .create_map_node(
            "page-1",
            stale_revision,
            &root,
            "memory",
            "mem-2",
            None,
            0.0,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Conflict(_)));
}

#[tokio::test]
async fn root_invariants_reject_dismiss_delete_and_reparent() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let map = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    let child = match db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-1",
            None,
            0.0,
        )
        .await
        .unwrap()
    {
        CreateNodeOutcome::Created(node) => node,
        other => panic!("expected Created, got {other:?}"),
    };

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let err = db.delete_map_node("page-1", rev, &root).await.unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));

    let err = db
        .patch_map_node(
            "page-1",
            rev,
            &root,
            NodePatch {
                parent_id: Some(child.id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));
}

#[tokio::test]
async fn reparent_rejects_cycle() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let map = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    let a = match db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-a",
            None,
            0.0,
        )
        .await
        .unwrap()
    {
        CreateNodeOutcome::Created(node) => node,
        other => panic!("expected Created, got {other:?}"),
    };
    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let b = match db
        .create_map_node("page-1", rev, &a.id, "memory", "mem-b", None, 0.0)
        .await
        .unwrap()
    {
        CreateNodeOutcome::Created(node) => node,
        other => panic!("expected Created, got {other:?}"),
    };

    // a -> root, b -> a. Re-parenting a under b would make a its own ancestor.
    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let err = db
        .patch_map_node(
            "page-1",
            rev,
            &a.id,
            NodePatch {
                parent_id: Some(b.id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));
}

#[tokio::test]
async fn reset_page_map_clears_tombstones() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let map = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    let created = match db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-1",
            None,
            0.0,
        )
        .await
        .unwrap()
    {
        CreateNodeOutcome::Created(node) => node,
        other => panic!("expected Created, got {other:?}"),
    };
    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    db.delete_map_node("page-1", rev, &created.id)
        .await
        .unwrap();

    db.reset_page_map("page-1").await.unwrap();
    assert!(db.get_page_map("page-1", true).await.unwrap().is_none());

    // Fresh init + the same fingerprint must succeed as Created, not Tombstoned.
    let map = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);
    let outcome = db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-1",
            None,
            0.0,
        )
        .await
        .unwrap();
    assert!(matches!(outcome, CreateNodeOutcome::Created(_)));
}
