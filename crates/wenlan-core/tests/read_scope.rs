use std::sync::Arc;

use wenlan_core::db::MemoryDB;
use wenlan_core::events::NoopEmitter;
use wenlan_core::read_scope::{resolve_read_scope, ReadScope, ReadScopeResolveError};

async fn test_db() -> (MemoryDB, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = MemoryDB::new(dir.path(), Arc::new(NoopEmitter))
        .await
        .expect("database");
    (db, dir)
}

#[test]
fn scope_matches_only_its_binding_axis() {
    assert!(ReadScope::Global.matches(Some("work")));
    assert!(ReadScope::Global.matches(None));
    assert!(ReadScope::Space("work".into()).matches(Some("work")));
    assert!(!ReadScope::Space("work".into()).matches(Some("personal")));
    assert!(ReadScope::Uncategorized.matches(None));
    assert!(!ReadScope::Uncategorized.matches(Some("uncategorized")));
}

#[tokio::test]
async fn absent_or_blank_selector_is_global() {
    let (db, _dir) = test_db().await;

    assert_eq!(
        resolve_read_scope(&db, None).await.unwrap(),
        ReadScope::Global
    );
    assert_eq!(
        resolve_read_scope(&db, Some("  ")).await.unwrap(),
        ReadScope::Global
    );
}

#[tokio::test]
async fn registered_selector_resolves_exact_space() {
    let (db, _dir) = test_db().await;
    db.create_space("work", None, false).await.unwrap();

    assert_eq!(
        resolve_read_scope(&db, Some(" work ")).await.unwrap(),
        ReadScope::Space("work".into())
    );
}

#[tokio::test]
async fn unknown_selector_is_rejected() {
    let (db, _dir) = test_db().await;

    let error = resolve_read_scope(&db, Some("missing"))
        .await
        .expect_err("unknown selector must fail closed");
    assert!(matches!(error, ReadScopeResolveError::Unknown(name) if name == "missing"));
}

#[tokio::test]
async fn uncategorized_without_registered_collision_means_null_binding() {
    let (db, _dir) = test_db().await;

    assert_eq!(
        resolve_read_scope(&db, Some("uncategorized"))
            .await
            .unwrap(),
        ReadScope::Uncategorized
    );
}

#[tokio::test]
async fn registered_uncategorized_name_makes_null_selector_ambiguous() {
    let (db, _dir) = test_db().await;
    db.create_space("uncategorized", None, false).await.unwrap();

    let error = resolve_read_scope(&db, Some("uncategorized"))
        .await
        .expect_err("collision must fail closed");
    assert!(matches!(
        error,
        ReadScopeResolveError::AmbiguousUncategorized
    ));
}
