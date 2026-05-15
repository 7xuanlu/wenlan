// SPDX-License-Identifier: Apache-2.0
//! Legacy tag import from the pre-PR-B2 `spaces.db` rusqlite file.
//!
//! After PR-B2 ships, tags live in `MemoryDB.document_tags`. This module
//! exists solely to import any pre-existing `spaces.db` once on daemon
//! startup and then rename the file so it won't be re-imported.
//! Once enough users have cycled past this release, the whole module can
//! be deleted along with the rusqlite dependency.

use crate::db::MemoryDB;
use crate::error::OriginError;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

fn legacy_db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("origin")
        .join("spaces.db")
}

/// Import tags from a legacy `spaces.db` (if present) into MemoryDB,
/// then rename the file to `spaces.db.migrated-<epoch>` so we don't
/// re-import on subsequent startups. Returns the count of (source, source_id, tag)
/// triples imported.
pub async fn import_legacy_tags(db: &MemoryDB) -> Result<usize, OriginError> {
    let path = legacy_db_path();
    if !path.exists() {
        return Ok(0);
    }
    let count = import_from_path(&path, db).await?;
    let backup = path.with_file_name(format!(
        "spaces.db.migrated-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    if let Err(e) = std::fs::rename(&path, &backup) {
        log::warn!("[spaces] failed to rename legacy spaces.db after import: {e}");
    }
    Ok(count)
}

async fn import_from_path(path: &Path, db: &MemoryDB) -> Result<usize, OriginError> {
    // Read all rows synchronously via rusqlite, then write to libSQL.
    let triples: Vec<(String, String, String)> = {
        let conn = Connection::open(path)
            .map_err(|e| OriginError::Generic(format!("legacy spaces.db open: {e}")))?;
        // The legacy table may not have a `document_tags` table if the file
        // was created by a very early version. Ignore if missing.
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='document_tags'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if !table_exists {
            return Ok(0);
        }
        let mut stmt = conn
            .prepare("SELECT doc_key, tag FROM document_tags")
            .map_err(|e| OriginError::Generic(format!("legacy spaces.db prepare: {e}")))?;
        let rows = stmt
            .query_map([], |row| {
                let doc_key: String = row.get(0)?;
                let tag: String = row.get(1)?;
                Ok((doc_key, tag))
            })
            .map_err(|e| OriginError::Generic(format!("legacy spaces.db query: {e}")))?;
        let mut out = Vec::new();
        for r in rows {
            let (doc_key, tag) =
                r.map_err(|e| OriginError::Generic(format!("legacy spaces.db row: {e}")))?;
            if let Some((source, source_id)) = doc_key.split_once("::") {
                let normalized = tag.trim().to_lowercase();
                if !normalized.is_empty() {
                    out.push((source.to_string(), source_id.to_string(), normalized));
                }
            }
        }
        out
    };

    if triples.is_empty() {
        return Ok(0);
    }

    // Group by (source, source_id) so we can use set_document_tags's transaction.
    let mut by_doc: std::collections::HashMap<(String, String), Vec<String>> =
        std::collections::HashMap::new();
    for (s, sid, tag) in &triples {
        by_doc
            .entry((s.clone(), sid.clone()))
            .or_default()
            .push(tag.clone());
    }
    for ((source, source_id), tags) in by_doc {
        db.set_document_tags(&source, &source_id, tags).await?;
    }
    Ok(triples.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::NoopEmitter;
    use std::sync::Arc;

    /// Create a temporary spaces.db with a document_tags table and some rows,
    /// then assert they end up in MemoryDB after import_from_path.
    #[tokio::test]
    async fn test_import_from_path_basic() {
        let tmp = tempfile::tempdir().unwrap();
        let db_file = tmp.path().join("spaces.db");
        let mem_db_file = tmp.path().join("test.db");

        // Write a legacy spaces.db with two docs and some tags.
        {
            let conn = Connection::open(&db_file).unwrap();
            conn.execute_batch(
                "CREATE TABLE document_tags (doc_key TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (doc_key, tag));",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO document_tags VALUES ('memory::abc123', 'rust')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO document_tags VALUES ('memory::abc123', '  Tauri  ')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO document_tags VALUES ('screen::xyz', 'design')",
                [],
            )
            .unwrap();
        }

        let db = MemoryDB::new(&mem_db_file, Arc::new(NoopEmitter))
            .await
            .unwrap();

        let count = import_from_path(&db_file, &db).await.unwrap();
        assert_eq!(count, 3);

        let tags_abc = db.get_document_tags("memory", "abc123").await.unwrap();
        assert_eq!(tags_abc, vec!["rust", "tauri"]); // normalized + sorted

        let tags_xyz = db.get_document_tags("screen", "xyz").await.unwrap();
        assert_eq!(tags_xyz, vec!["design"]);
    }

    /// If the legacy file has no document_tags table, import returns 0.
    #[tokio::test]
    async fn test_import_from_path_no_table() {
        let tmp = tempfile::tempdir().unwrap();
        let db_file = tmp.path().join("spaces.db");
        let mem_db_file = tmp.path().join("test.db");

        {
            let conn = Connection::open(&db_file).unwrap();
            conn.execute_batch("CREATE TABLE spaces (id TEXT PRIMARY KEY);")
                .unwrap();
        }

        let db = MemoryDB::new(&mem_db_file, Arc::new(NoopEmitter))
            .await
            .unwrap();

        let count = import_from_path(&db_file, &db).await.unwrap();
        assert_eq!(count, 0);
    }

    /// doc_keys without "::" are silently skipped.
    #[tokio::test]
    async fn test_import_from_path_skips_bad_doc_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let db_file = tmp.path().join("spaces.db");
        let mem_db_file = tmp.path().join("test.db");

        {
            let conn = Connection::open(&db_file).unwrap();
            conn.execute_batch(
                "CREATE TABLE document_tags (doc_key TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (doc_key, tag));",
            )
            .unwrap();
            // Valid row.
            conn.execute(
                "INSERT INTO document_tags VALUES ('memory::good', 'ok')",
                [],
            )
            .unwrap();
            // Malformed row — no "::".
            conn.execute("INSERT INTO document_tags VALUES ('baddockey', 'ok')", [])
                .unwrap();
        }

        let db = MemoryDB::new(&mem_db_file, Arc::new(NoopEmitter))
            .await
            .unwrap();

        let count = import_from_path(&db_file, &db).await.unwrap();
        assert_eq!(count, 1);

        let tags = db.get_document_tags("memory", "good").await.unwrap();
        assert_eq!(tags, vec!["ok"]);
    }
}
