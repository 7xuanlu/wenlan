// SPDX-License-Identifier: Apache-2.0

use super::super::tests::test_db;
use super::{CreateEdgeOutcome, CreateNodeOutcome, EdgePatch, MemoryDB, NodeLayout, NodePatch};

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

    let (first, created_first) = db.init_page_map("page-1").await.unwrap();
    assert!(created_first, "first call must report created = true");
    assert_eq!(first.map.revision, 1);
    assert_eq!(first.nodes.len(), 1);
    let root = &first.nodes[0];
    assert!(root.parent_id.is_none());
    assert_eq!(root.ref_kind, "page");
    assert_eq!(root.ref_id, "page-1");
    assert_eq!(root.fingerprint, "page\u{1f}page-1\u{1f}~");

    // Second call is a no-op: same revision, same single root, no new row.
    let (second, created_second) = db.init_page_map("page-1").await.unwrap();
    assert!(!created_second, "second call must report created = false");
    assert_eq!(second.map.revision, 1);
    assert_eq!(second.nodes.len(), 1);
    assert_eq!(second.nodes[0].id, root.id);
}

#[tokio::test]
async fn page_map_mutations_reject_non_active_pages_at_the_core_boundary() {
    let (db, _tmp) = test_db().await;
    let draft = db
        .create_page_draft("Draft", "Body", None, None)
        .await
        .unwrap();

    assert!(matches!(
        db.init_page_map(&draft.id).await,
        Err(crate::WenlanError::Validation(_))
    ));
    assert!(db.get_page_map(&draft.id, true).await.unwrap().is_none());

    seed_page(&db, "page-archived").await;
    let (map, _created) = db.init_page_map("page-archived").await.unwrap();
    let root = root_id(&map.nodes);
    db.archive_page("page-archived").await.unwrap();

    assert!(matches!(
        db.create_map_node(
            "page-archived",
            map.map.revision,
            &root,
            "memory",
            "mem-1",
            None,
            0.0,
        )
        .await,
        Err(crate::WenlanError::Validation(_))
    ));
    assert!(matches!(
        db.reset_page_map("page-archived").await,
        Err(crate::WenlanError::Validation(_))
    ));
    assert!(db
        .get_page_map("page-archived", true)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn create_map_node_fingerprint_dedup() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
    assert_eq!(created.fingerprint, "memory\u{1f}mem-1\u{1f}~");

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
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
            .filter(|n| n.fingerprint == "memory\u{1f}mem-1\u{1f}~")
            .count(),
        1
    );
}

#[tokio::test]
async fn revision_bumps_on_every_write_and_stale_base_rejected() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
    let (map, created_after_reset) = db.init_page_map("page-1").await.unwrap();
    assert!(created_after_reset);
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

// Dismissed rows are terminal: no patch, of any shape, can touch them — not
// even one that only re-parents (which would otherwise recompute the
// fingerprint and free the old tombstone key, letting a dismissed suggestion
// resurface under a different parent).
#[tokio::test]
async fn patch_on_dismissed_node_rejected_and_tombstone_holds() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    let other_parent = match db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-other-parent",
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
    let created = match db
        .create_map_node("page-1", rev, &root, "memory", "mem-1", None, 0.0)
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
    let fingerprint_before = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .nodes
        .into_iter()
        .find(|n| n.id == created.id)
        .unwrap()
        .fingerprint;

    // A patch touching only parent_id (no status change) must still be
    // rejected — dismissed is terminal regardless of patch contents.
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
            &created.id,
            NodePatch {
                parent_id: Some(other_parent.id.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));

    // Fingerprint must be unchanged in the DB.
    let after = db.get_page_map("page-1", true).await.unwrap().unwrap();
    let node_after = after.nodes.iter().find(|n| n.id == created.id).unwrap();
    assert_eq!(node_after.fingerprint, fingerprint_before);

    // A subsequent create under the original parent still tombstones.
    let rev = after.map.revision;
    let outcome = db
        .create_map_node("page-1", rev, &root, "memory", "mem-1", None, 0.0)
        .await
        .unwrap();
    assert_eq!(outcome, CreateNodeOutcome::Tombstoned);
}

#[tokio::test]
async fn patch_on_dismissed_edge_rejected() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
        .create_map_node("page-1", rev, &root, "memory", "mem-b", None, 0.0)
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
    let edge = match db
        .create_map_edge("page-1", rev, &a.id, &b.id, "link", None)
        .await
        .unwrap()
    {
        CreateEdgeOutcome::Created(edge) => edge,
        other => panic!("expected Created, got {other:?}"),
    };

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    db.delete_map_edge("page-1", rev, &edge.id).await.unwrap();

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let err = db
        .patch_map_edge(
            "page-1",
            rev,
            &edge.id,
            EdgePatch {
                label: Some(Some("relabel".to_string())),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));
}

// Dismissal cannot orphan: dismissing a node with a live child is rejected.
#[tokio::test]
async fn dismiss_with_live_child_rejected() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    let parent = match db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-parent",
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
    db.create_map_node("page-1", rev, &parent.id, "memory", "mem-child", None, 0.0)
        .await
        .unwrap();

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let err = db
        .delete_map_node("page-1", rev, &parent.id)
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));
}

// Creating a node under a dismissed parent is rejected.
#[tokio::test]
async fn create_under_dismissed_parent_rejected() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    let parent = match db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "mem-parent",
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
    db.delete_map_node("page-1", rev, &parent.id).await.unwrap();

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let err = db
        .create_map_node("page-1", rev, &parent.id, "memory", "mem-child", None, 0.0)
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));
}

// Creating an edge to a dismissed endpoint is rejected.
#[tokio::test]
async fn edge_to_dismissed_endpoint_rejected() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
        .create_map_node("page-1", rev, &root, "memory", "mem-b", None, 0.0)
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
    db.delete_map_node("page-1", rev, &b.id).await.unwrap();

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let err = db
        .create_map_edge("page-1", rev, &a.id, &b.id, "link", None)
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));
}

#[tokio::test]
async fn fingerprint_uses_unit_separator_and_rejects_it_in_ref_components() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
    let root = root_id(&map.nodes);

    // A ref_id containing the fingerprint separator itself is rejected.
    let err = db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "a\u{1f}b",
            None,
            0.0,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Validation(_)));

    // ref_ids that would have collided under the OLD "{kind}:{id}@{parent}"
    // scheme (colliding on '@'/':' in the ref_id) now insert as distinct rows.
    let first = match db
        .create_map_node(
            "page-1",
            map.map.revision,
            &root,
            "memory",
            "a@b",
            None,
            0.0,
        )
        .await
        .unwrap()
    {
        CreateNodeOutcome::Created(node) => node,
        other => panic!("expected Created, got {other:?}"),
    };
    assert_eq!(first.fingerprint, "memory\u{1f}a@b\u{1f}~");

    let rev = db
        .get_page_map("page-1", true)
        .await
        .unwrap()
        .unwrap()
        .map
        .revision;
    let second = match db
        .create_map_node("page-1", rev, &root, "memory", "a:b", None, 0.0)
        .await
        .unwrap()
    {
        CreateNodeOutcome::Created(node) => node,
        other => panic!("expected Created, got {other:?}"),
    };
    assert_eq!(second.fingerprint, "memory\u{1f}a:b\u{1f}~");
    assert_ne!(first.fingerprint, second.fingerprint);
}

#[tokio::test]
async fn init_page_map_created_flag_reflects_atomic_insert() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;

    let (first, created_first) = db.init_page_map("page-1").await.unwrap();
    assert!(created_first, "first call must report created = true");
    assert_eq!(first.map.revision, 1);

    let (second, created_second) = db.init_page_map("page-1").await.unwrap();
    assert!(
        !created_second,
        "second call must report created = false (idempotent no-op)"
    );
    assert_eq!(second.map.revision, 1);
}

#[tokio::test]
async fn put_page_map_layout_round_trip_pins_and_places_positioned_nodes() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
    let viewport = r#"{"x":1.0,"y":2.0,"zoom":1.5}"#;
    let positions = vec![NodeLayout {
        node_id: child.id.clone(),
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
        collapsed: true,
    }];
    let data = db
        .put_page_map_layout("page-1", rev, Some(viewport), &positions)
        .await
        .unwrap();

    assert_eq!(
        data.map.revision,
        rev + 1,
        "layout write must bump the revision"
    );
    assert_eq!(data.map.viewport.as_deref(), Some(viewport));

    let updated = data.nodes.iter().find(|n| n.id == child.id).unwrap();
    assert_eq!(updated.x, Some(10.0));
    assert_eq!(updated.y, Some(20.0));
    assert_eq!(updated.width, Some(100.0));
    assert_eq!(updated.height, Some(50.0));
    assert!(updated.placed, "positioned node must be marked placed");
    assert!(
        updated.pinned,
        "positioned node must be pinned (move with placement)"
    );
    assert!(updated.collapsed);

    // Stale base_revision is rejected.
    let err = db
        .put_page_map_layout("page-1", rev, None, &[])
        .await
        .unwrap_err();
    assert!(matches!(err, crate::WenlanError::Conflict(_)));
}

// Re-parenting onto a dismissed target parent is rejected — the same
// cannot-orphan rule as create-under-dismissed-parent, on the patch path.
#[tokio::test]
async fn reparent_onto_dismissed_parent_rejected() {
    let (db, _tmp) = test_db().await;
    seed_page(&db, "page-1").await;
    let (map, _created) = db.init_page_map("page-1").await.unwrap();
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
        .create_map_node("page-1", rev, &root, "memory", "mem-b", None, 1.0)
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
    db.delete_map_node("page-1", rev, &b.id).await.unwrap();

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
