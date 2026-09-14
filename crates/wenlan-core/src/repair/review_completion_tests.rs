// SPDX-License-Identifier: Apache-2.0

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn verified_review_bound_repair_resolves_its_exact_queue_row() {
    let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
    let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
    let binding = manifest
        .source()
        .review_binding()
        .expect("classification repair is review-bound");
    let before = db
        .get_refinement_proposal(binding.review_id())
        .await
        .unwrap()
        .expect("review row");
    assert_eq!(before.status, "awaiting_review");

    let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
        .await
        .unwrap();
    let (general, deep) = verification_reports(&db).await;
    record_repair_verification(
        &db,
        &store,
        exact_verify(&manifest, &apply_receipt, general, deep),
        None,
        1_721_000_002,
    )
    .await
    .unwrap();

    let after = db
        .get_refinement_proposal(binding.review_id())
        .await
        .unwrap()
        .expect("review row remains durable");
    assert_eq!(after.status, "resolved");
}

async fn queue_status(db: &MemoryDB, review_id: &str) -> Option<String> {
    let connection = db.test_primary_session().await;
    let mut rows = connection
        .query(
            "SELECT status FROM refinement_queue WHERE id=?1",
            libsql::params![review_id],
        )
        .await
        .unwrap();
    rows.next()
        .await
        .unwrap()
        .map(|row| row.get::<String>(0).unwrap())
}

fn unrelated_review_payload(occurrence: &RepairDigest, source_ids: &[String]) -> String {
    serde_json::json!({
        "action": "lint_repair_review",
        "check_id": "pages.duplicate_active_titles",
        "occurrence_digest": occurrence,
        "owner_binding_digest": lint_review_owner_binding_digest(occurrence, source_ids)
            .unwrap(),
        "issue": "An unrelated review item.",
        "choices": ["keep", "retarget"],
        "suggested_research_queries": [],
    })
    .to_string()
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn verification_replay_is_idempotent_and_preserves_unrelated_review() {
    let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
    let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
    let binding = manifest.source().review_binding().unwrap();
    let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
        .await
        .unwrap();
    let (general, deep) = verification_reports(&db).await;
    let first = record_repair_verification(
        &db,
        &store,
        exact_verify(&manifest, &apply_receipt, general.clone(), deep.clone()),
        None,
        1_721_000_002,
    )
    .await
    .unwrap();

    let unrelated_occurrence = RepairDigest::parse(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();
    let unrelated_source_ids = vec!["mem_unrelated".to_string()];
    let unrelated_id = format!("lint_review_{}", unrelated_occurrence.as_str());
    db.insert_lint_review_if_absent(
        &unrelated_id,
        &unrelated_source_ids,
        &unrelated_review_payload(&unrelated_occurrence, &unrelated_source_ids),
    )
    .await
    .unwrap();

    let second = record_repair_verification(
        &db,
        &store,
        exact_verify(&manifest, &apply_receipt, general, deep),
        None,
        1_721_000_003,
    )
    .await
    .unwrap();

    assert_eq!(first.receipt_digest(), second.receipt_digest());
    assert_eq!(queue_status(&db, binding.review_id()).await.as_deref(), Some("resolved"));
    assert_eq!(queue_status(&db, &unrelated_id).await.as_deref(), Some("awaiting_review"));
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn failed_verification_leaves_the_bound_review_open() {
    let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
    let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
    let review_id = manifest.source().review_binding().unwrap().review_id().to_string();
    let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
        .await
        .unwrap();
    let (general, deep) = verification_reports(&db).await;
    let failed_deep = fail_deep_check(deep, true);
    let result = record_repair_verification(
        &db,
        &store,
        exact_verify(&manifest, &apply_receipt, general, failed_deep),
        None,
        1_721_000_002,
    )
    .await;

    assert!(result.is_err());
    assert_eq!(queue_status(&db, &review_id).await.as_deref(), Some("awaiting_review"));
    assert!(!store
        .manifest_dir(manifest.manifest_id())
        .unwrap()
        .join(VERIFICATION_RECEIPT_FILE)
        .exists());
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn dismissed_review_is_a_completion_conflict_and_keeps_its_status() {
    let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
    let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
    let review_id = manifest.source().review_binding().unwrap().review_id().to_string();
    let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
        .await
        .unwrap();
    db.test_primary_session()
        .await
        .execute(
            "UPDATE refinement_queue SET status='dismissed' WHERE id=?1",
            libsql::params![review_id.clone()],
        )
        .await
        .unwrap();
    let (general, deep) = verification_reports(&db).await;
    let result = record_repair_verification(
        &db,
        &store,
        exact_verify(&manifest, &apply_receipt, general, deep),
        None,
        1_721_000_002,
    )
    .await;

    assert!(matches!(result, Err(WenlanError::Conflict(_))));
    assert_eq!(queue_status(&db, &review_id).await.as_deref(), Some("dismissed"));
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn rebound_review_is_a_completion_conflict_and_keeps_its_payload() {
    let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
    let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
    let binding = manifest.source().review_binding().unwrap();
    let review_id = binding.review_id().to_string();
    let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
        .await
        .unwrap();
    db.test_primary_session()
        .await
        .execute(
            "UPDATE refinement_queue SET source_ids='[\"mem_other\"]' WHERE id=?1",
            libsql::params![review_id.clone()],
        )
        .await
        .unwrap();
    let (general, deep) = verification_reports(&db).await;
    let result = record_repair_verification(
        &db,
        &store,
        exact_verify(&manifest, &apply_receipt, general, deep),
        None,
        1_721_000_002,
    )
    .await;

    assert!(matches!(result, Err(WenlanError::Conflict(_))));
    assert_eq!(queue_status(&db, &review_id).await.as_deref(), Some("awaiting_review"));
}

#[tokio::test]
#[cfg_attr(not(unix), ignore = "repair artifacts are unix-only")]
async fn missing_review_is_a_completion_conflict() {
    let (db, _db_dir, repair_root, manifest) = prepared_fixture().await;
    let store = RepairArtifactStore::new(repair_root.path().to_path_buf());
    let review_id = manifest.source().review_binding().unwrap().review_id().to_string();
    let apply_receipt = apply_repair(&db, &store, exact_apply(&manifest), 1_721_000_001)
        .await
        .unwrap();
    db.test_primary_session()
        .await
        .execute(
            "DELETE FROM refinement_queue WHERE id=?1",
            libsql::params![review_id.clone()],
        )
        .await
        .unwrap();
    let (general, deep) = verification_reports(&db).await;
    let result = record_repair_verification(
        &db,
        &store,
        exact_verify(&manifest, &apply_receipt, general, deep),
        None,
        1_721_000_002,
    )
    .await;

    assert!(matches!(result, Err(WenlanError::Conflict(_))));
    assert_eq!(queue_status(&db, &review_id).await, None);
}

#[tokio::test]
async fn unbound_manifest_completion_is_a_noop() {
    let root = tempfile::tempdir().unwrap();
    let manifest_id = "repair_550e8400-e29b-41d4-a716-446655440000";
    let manifest_dir = root.path().join(manifest_id);
    std::fs::create_dir(&manifest_dir).unwrap();
    std::fs::write(
        manifest_dir.join(MANIFEST_FILE),
        include_bytes!("../../../wenlan-types/testdata/repair/v1/manifest.json"),
    )
    .unwrap();
    let store = RepairArtifactStore::new(root.path().to_path_buf());
    let manifest = store.load_manifest(manifest_id).unwrap();
    let receipt: RepairVerificationReceipt = serde_json::from_slice(include_bytes!(
        "../../../wenlan-types/testdata/repair/v1/verification-receipt.json"
    ))
    .unwrap();
    assert!(manifest.source().review_binding().is_none());
    let (db, _db_dir) = test_db().await;
    reconcile_repair_review_completion(&db, &manifest, &receipt)
        .await
        .unwrap();
}
