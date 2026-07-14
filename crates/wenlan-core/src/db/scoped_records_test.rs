use super::tests::test_db;
use crate::read_scope::ReadScope;

#[tokio::test]
async fn scoped_chunks_propagate_database_failures() {
    let (db, _temp) = test_db().await;
    let conn = db.conn.lock().await;
    conn.execute_batch("DROP TABLE memories;").await.unwrap();
    drop(conn);

    let result = db.get_chunks_scoped("missing", &ReadScope::Global).await;

    assert!(
        result.is_err(),
        "query failure must not become an empty success"
    );
}
