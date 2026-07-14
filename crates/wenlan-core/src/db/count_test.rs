use super::super::tests::test_db;

#[tokio::test]
async fn direct_count_preserves_row_fetch_failure_while_legacy_count_stays_zero() {
    // Given
    let (db, _temp) = test_db().await;
    let conn = db.conn.lock().await;
    conn.execute_batch(
        "DROP TABLE memories;
         CREATE VIEW memories AS
         SELECT json_extract('not-json', '$') AS source;",
    )
    .await
    .unwrap();
    drop(conn);
    assert_eq!(db.count().await.unwrap(), 0);

    // When
    let result = db.count_direct().await;

    // Then
    assert!(result.is_err());
}

#[tokio::test]
async fn count_decoder_rejects_empty_rows() {
    // Given
    let database = libsql::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap();
    let conn = database.connect().unwrap();
    let mut rows = conn.query("SELECT 1 WHERE 0", ()).await.unwrap();

    // When
    let result = super::read_count(&mut rows).await;

    // Then
    assert!(result.is_err());
}

#[tokio::test]
async fn count_decoder_rejects_non_integer_rows() {
    // Given
    let database = libsql::Builder::new_local(":memory:")
        .build()
        .await
        .unwrap();
    let conn = database.connect().unwrap();
    let mut rows = conn.query("SELECT 'not-a-count'", ()).await.unwrap();

    // When
    let result = super::read_count(&mut rows).await;

    // Then
    assert!(result.is_err());
}
