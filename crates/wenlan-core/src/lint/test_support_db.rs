use super::TestOracleError;
use crate::lint::snapshot::LintReadSnapshot;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DbSemanticFingerprint {
    tables: BTreeMap<String, [u8; 32]>,
}

impl fmt::Debug for DbSemanticFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DbSemanticFingerprint")
            .field("table_count", &self.tables.len())
            .finish()
    }
}

impl DbSemanticFingerprint {
    pub(crate) async fn capture(snapshot: &LintReadSnapshot<'_>) -> Result<Self, TestOracleError> {
        let tables = table_names(snapshot).await?;
        let mut fingerprints = BTreeMap::new();
        for table in tables {
            fingerprints.insert(table.clone(), fingerprint_table(snapshot, &table).await?);
        }
        Ok(Self {
            tables: fingerprints,
        })
    }

    pub(crate) fn table_names(&self) -> impl Iterator<Item = &str> {
        self.tables.keys().map(String::as_str)
    }

    pub(crate) fn table(&self, name: &str) -> Option<[u8; 32]> {
        self.tables.get(name).copied()
    }
}

async fn table_names(snapshot: &LintReadSnapshot<'_>) -> Result<Vec<String>, TestOracleError> {
    let mut rows = snapshot
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            libsql::params::Params::None,
        )
        .await?;
    let mut tables = Vec::new();
    while let Some(row) = rows.next().await? {
        tables.push(row.get::<String>(0)?);
    }
    Ok(tables)
}

async fn fingerprint_table(
    snapshot: &LintReadSnapshot<'_>,
    table: &str,
) -> Result<[u8; 32], TestOracleError> {
    let columns = column_names(snapshot, table).await?;
    let mut digest = Sha256::new();
    digest_value(&mut digest, b"wenlan-lint-table-v1");
    digest_value(&mut digest, table.as_bytes());
    for column in &columns {
        digest_value(&mut digest, column.as_bytes());
    }
    if columns.is_empty() {
        return Ok(digest.finalize().into());
    }

    let values = columns
        .iter()
        .map(|column| format!("quote({})", quote_identifier(column)))
        .collect::<Vec<_>>();
    let ordering = (1..=columns.len())
        .map(|index| index.to_string())
        .collect::<Vec<_>>();
    let sql = format!(
        "SELECT {} FROM {} ORDER BY {}",
        values.join(", "),
        quote_identifier(table),
        ordering.join(", ")
    );
    let mut rows = snapshot.query(&sql, libsql::params::Params::None).await?;
    while let Some(row) = rows.next().await? {
        digest_value(&mut digest, b"row");
        for index in 0..columns.len() {
            let index = i32::try_from(index).map_err(|_| TestOracleError::ColumnCount)?;
            digest_value(&mut digest, row.get::<String>(index)?.as_bytes());
        }
    }
    Ok(digest.finalize().into())
}

async fn column_names(
    snapshot: &LintReadSnapshot<'_>,
    table: &str,
) -> Result<Vec<String>, TestOracleError> {
    let sql = format!("PRAGMA table_info({})", quote_literal(table));
    let mut rows = snapshot.query(&sql, libsql::params::Params::None).await?;
    let mut columns = Vec::new();
    while let Some(row) = rows.next().await? {
        columns.push(row.get::<String>(1)?);
    }
    Ok(columns)
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn digest_value(digest: &mut Sha256, value: &[u8]) {
    let length = u64::try_from(value.len()).unwrap_or(u64::MAX);
    digest.update(length.to_le_bytes());
    digest.update(value);
}
