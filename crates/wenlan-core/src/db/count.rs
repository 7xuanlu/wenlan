use super::{MemoryDB, WenlanError};

const MEMORY_COUNT_SQL: &str = "SELECT COUNT(*) FROM memories WHERE source != 'episode'";

impl MemoryDB {
    /// Total number of memories, retaining the legacy zero fallback after query creation.
    pub async fn count(&self) -> Result<u64, WenlanError> {
        let conn = self.conn.lock().await;
        let mut rows = query_count(&conn).await?;
        Ok(read_count(&mut rows).await.unwrap_or(0))
    }

    /// Total number of memories without converting direct observation failures to zero.
    pub async fn count_direct(&self) -> Result<u64, WenlanError> {
        let conn = self.conn.lock().await;
        let mut rows = query_count(&conn).await?;
        read_count(&mut rows).await
    }
}

async fn query_count(conn: &libsql::Connection) -> Result<libsql::Rows, WenlanError> {
    conn.query(MEMORY_COUNT_SQL, ())
        .await
        .map_err(|error| WenlanError::VectorDb(format!("count: {error}")))
}

pub(super) async fn read_count(rows: &mut libsql::Rows) -> Result<u64, WenlanError> {
    let row = rows
        .next()
        .await
        .map_err(|error| WenlanError::VectorDb(format!("count row: {error}")))?
        .ok_or_else(|| WenlanError::VectorDb("count row: missing aggregate".to_string()))?;
    match row
        .get_value(0)
        .map_err(|error| WenlanError::VectorDb(format!("count decode: {error}")))?
    {
        libsql::Value::Integer(count) => u64::try_from(count)
            .map_err(|error| WenlanError::VectorDb(format!("count decode: {error}"))),
        libsql::Value::Null
        | libsql::Value::Real(_)
        | libsql::Value::Text(_)
        | libsql::Value::Blob(_) => Err(WenlanError::VectorDb(
            "count decode: expected integer".to_string(),
        )),
    }
}

#[cfg(test)]
#[path = "count_test.rs"]
mod tests;
