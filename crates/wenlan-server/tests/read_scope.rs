use std::sync::Arc;

use wenlan_core::db::MemoryDB;
use wenlan_core::events::NoopEmitter;
use wenlan_core::read_scope::ReadScope;
use wenlan_server::error::ServerError;
use wenlan_server::read_scope::effective_read_scope;

async fn test_db() -> (MemoryDB, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = MemoryDB::new(dir.path(), Arc::new(NoopEmitter))
        .await
        .expect("database");
    for space in ["work", "personal"] {
        db.create_space(space, None, false).await.unwrap();
    }
    (db, dir)
}

#[tokio::test]
async fn non_empty_primary_wins_over_header() {
    let (db, _dir) = test_db().await;

    assert_eq!(
        effective_read_scope(&db, Some("work"), Some("personal"))
            .await
            .unwrap(),
        ReadScope::Space("work".into())
    );
}

#[tokio::test]
async fn empty_primary_falls_back_to_header() {
    let (db, _dir) = test_db().await;

    assert_eq!(
        effective_read_scope(&db, Some("  "), Some("personal"))
            .await
            .unwrap(),
        ReadScope::Space("personal".into())
    );
}

#[tokio::test]
async fn invalid_primary_does_not_fall_back_to_valid_header() {
    let (db, _dir) = test_db().await;

    let error = effective_read_scope(&db, Some("missing"), Some("work"))
        .await
        .expect_err("invalid primary must fail closed");
    assert!(matches!(error, ServerError::ValidationError(_)));
}

#[tokio::test]
async fn unknown_header_is_validation_error() {
    let (db, _dir) = test_db().await;

    let error = effective_read_scope(&db, None, Some("missing"))
        .await
        .expect_err("unknown header must fail closed");
    assert!(matches!(error, ServerError::ValidationError(_)));
}

#[tokio::test]
async fn registered_uncategorized_collision_is_validation_error() {
    let (db, _dir) = test_db().await;
    db.create_space("uncategorized", None, false).await.unwrap();

    let error = effective_read_scope(&db, None, Some("uncategorized"))
        .await
        .expect_err("collision must fail closed");
    assert!(matches!(error, ServerError::ValidationError(_)));
}
