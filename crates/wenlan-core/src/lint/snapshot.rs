use crate::db::MemoryDB;
use sha2::{Digest, Sha256};
use std::future::Future;
use std::marker::PhantomData;

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("lint snapshot database operation failed")]
    Database(#[from] libsql::Error),
    #[error("lint snapshot has no active transaction")]
    Closed,
    #[error("lint snapshot query returned no row")]
    EmptyRow,
    #[error("lint snapshot structural digest input is too large")]
    DigestLength,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralDigest([u8; 32]);

impl StructuralDigest {
    pub fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotReceipt {
    analysis_digest: StructuralDigest,
    post_run_digest: StructuralDigest,
    analysis_data_version: i64,
    post_run_data_version: i64,
}

impl SnapshotReceipt {
    pub fn analysis_digest(self) -> StructuralDigest {
        self.analysis_digest
    }

    pub fn post_run_digest(self) -> StructuralDigest {
        self.post_run_digest
    }

    pub fn is_consistent(self) -> bool {
        self.analysis_digest == self.post_run_digest
            && self.analysis_data_version == self.post_run_data_version
    }
}

/// Rows remain borrowed from their snapshot until they are dropped.
///
/// ```compile_fail,E0505
/// use wenlan_core::lint::snapshot::{LintReadSnapshot, SnapshotError};
///
/// async fn consume_after_finish(
///     snapshot: LintReadSnapshot<'_>,
/// ) -> Result<(), SnapshotError> {
///     let mut rows = snapshot
///         .query("SELECT 1", libsql::params::Params::None)
///         .await?;
///     snapshot.finish().await?;
///     let _ = rows.next().await?;
///     Ok(())
/// }
/// ```
pub struct LintRows<'snapshot> {
    rows: libsql::Rows,
    snapshot: PhantomData<&'snapshot ()>,
}

impl LintRows<'_> {
    pub async fn next(&mut self) -> Result<Option<libsql::Row>, SnapshotError> {
        self.rows.next().await.map_err(SnapshotError::from)
    }
}

pub struct LintReadSnapshot<'database> {
    database: &'database libsql::Database,
    observer: libsql::Connection,
    transaction: Option<libsql::Transaction>,
    analysis_digest: Option<StructuralDigest>,
    analysis_data_version: i64,
}

impl<'database> LintReadSnapshot<'database> {
    pub(crate) async fn open(database: &'database libsql::Database) -> Result<Self, SnapshotError> {
        Self::open_unpinned(database).await?.pin_analysis().await
    }

    pub(crate) async fn open_unpinned(
        database: &'database libsql::Database,
    ) -> Result<Self, SnapshotError> {
        let observer = database.connect()?;
        let analysis_data_version = scalar_i64(&observer, "PRAGMA data_version").await?;
        let connection = database.connect()?;
        connection.execute("PRAGMA query_only = ON", ()).await?;
        let transaction = connection
            .transaction_with_behavior(libsql::TransactionBehavior::ReadOnly)
            .await?;

        Ok(Self {
            database,
            observer,
            transaction: Some(transaction),
            analysis_digest: None,
            analysis_data_version,
        })
    }

    pub(crate) async fn pin_analysis(mut self) -> Result<Self, SnapshotError> {
        let transaction = self.transaction.as_ref().ok_or(SnapshotError::Closed)?;
        self.analysis_digest = Some(structural_digest(transaction).await?);
        Ok(self)
    }

    pub async fn query<'snapshot>(
        &'snapshot self,
        sql: &str,
        params: libsql::params::Params,
    ) -> Result<LintRows<'snapshot>, SnapshotError> {
        let rows = self
            .transaction
            .as_ref()
            .ok_or(SnapshotError::Closed)?
            .query(sql, params)
            .await
            .map_err(SnapshotError::from)?;
        Ok(LintRows {
            rows,
            snapshot: PhantomData,
        })
    }

    pub async fn finish(self) -> Result<SnapshotReceipt, SnapshotError> {
        self.finish_inner(|| std::future::ready(())).await
    }

    async fn finish_inner<F, Fut>(
        mut self,
        post_snapshot_pinned: F,
    ) -> Result<SnapshotReceipt, SnapshotError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ()>,
    {
        let analysis_digest = self.analysis_digest.ok_or(SnapshotError::Closed)?;
        let transaction = self.transaction.take().ok_or(SnapshotError::Closed)?;
        transaction.rollback().await?;
        let post_run_digest = fresh_structural_digest(self.database, post_snapshot_pinned).await?;
        let post_run_data_version = scalar_i64(&self.observer, "PRAGMA data_version").await?;

        Ok(SnapshotReceipt {
            analysis_digest,
            post_run_digest,
            analysis_data_version: self.analysis_data_version,
            post_run_data_version,
        })
    }

    #[cfg(test)]
    async fn finish_with_post_snapshot_hook<F, Fut>(
        self,
        post_snapshot_pinned: F,
    ) -> Result<SnapshotReceipt, SnapshotError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ()>,
    {
        self.finish_inner(post_snapshot_pinned).await
    }
}

impl MemoryDB {
    pub async fn open_lint_snapshot(&self) -> Result<LintReadSnapshot<'_>, SnapshotError> {
        LintReadSnapshot::open(&self._db).await
    }

    pub(crate) async fn open_unpinned_lint_snapshot(
        &self,
    ) -> Result<LintReadSnapshot<'_>, SnapshotError> {
        LintReadSnapshot::open_unpinned(&self._db).await
    }
}

async fn fresh_structural_digest<F, Fut>(
    database: &libsql::Database,
    snapshot_pinned: F,
) -> Result<StructuralDigest, SnapshotError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let connection = database.connect()?;
    connection.execute("PRAGMA query_only = ON", ()).await?;
    let transaction = connection
        .transaction_with_behavior(libsql::TransactionBehavior::ReadOnly)
        .await?;
    let digest = structural_digest_with_hook(&transaction, snapshot_pinned).await?;
    transaction.rollback().await?;
    Ok(digest)
}

async fn structural_digest(
    connection: &libsql::Connection,
) -> Result<StructuralDigest, SnapshotError> {
    structural_digest_with_hook(connection, || std::future::ready(())).await
}

async fn structural_digest_with_hook<F, Fut>(
    connection: &libsql::Connection,
    snapshot_pinned: F,
) -> Result<StructuralDigest, SnapshotError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    let schema_version = scalar_i64(connection, "PRAGMA schema_version").await?;
    snapshot_pinned().await;
    let mut digest = Sha256::new();
    digest_bytes(&mut digest, b"lint-db-structural-digest-v2")?;
    digest_i64(&mut digest, schema_version);
    digest_i64(
        &mut digest,
        scalar_i64(connection, "PRAGMA user_version").await?,
    );

    let mut rows = connection
        .query(
            "SELECT type, name, COALESCE(sql, '') FROM sqlite_schema WHERE type IN ('index', 'table', 'trigger', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY type, name",
            (),
        )
        .await?;
    let mut tables = Vec::new();

    while let Some(row) = rows.next().await? {
        let object_type = row.get::<String>(0)?;
        let name = row.get::<String>(1)?;
        let sql = row.get::<String>(2)?;
        digest_bytes(&mut digest, object_type.as_bytes())?;
        digest_bytes(&mut digest, name.as_bytes())?;
        digest_bytes(&mut digest, sql.as_bytes())?;
        if object_type == "table" && !sql.to_ascii_lowercase().starts_with("create virtual table") {
            tables.push(name);
        }
    }
    drop(rows);

    for table in tables {
        digest_bytes(&mut digest, table.as_bytes())?;
        let count_query = format!("SELECT COUNT(*) FROM {}", quote_identifier(&table));
        digest_i64(&mut digest, scalar_i64(connection, &count_query).await?);
    }

    Ok(StructuralDigest(digest.finalize().into()))
}

async fn scalar_i64(connection: &libsql::Connection, sql: &str) -> Result<i64, SnapshotError> {
    let mut rows = connection.query(sql, ()).await?;
    let row = rows.next().await?.ok_or(SnapshotError::EmptyRow)?;
    Ok(row.get::<i64>(0)?)
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn digest_bytes(digest: &mut Sha256, value: &[u8]) -> Result<(), SnapshotError> {
    let length = u64::try_from(value.len()).map_err(|_| SnapshotError::DigestLength)?;
    digest.update(length.to_le_bytes());
    digest.update(value);
    Ok(())
}

fn digest_i64(digest: &mut Sha256, value: i64) {
    digest.update(value.to_le_bytes());
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;
