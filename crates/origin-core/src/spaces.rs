// SPDX-License-Identifier: Apache-2.0
use crate::error::OriginError;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

// ── SpaceStore ──────────────────────────────────────────────────────────

/// Persistent store for document tag assignments and the tag library.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpaceStore {
    pub document_tags: HashMap<String, BTreeSet<String>>,
    pub tags: BTreeSet<String>,
}

impl SpaceStore {
    // ── Helpers ──────────────────────────────────────────────────────

    /// Build a document key from source and source_id.
    pub fn doc_key(source: &str, source_id: &str) -> String {
        format!("{}::{}", source, source_id)
    }

    // ── Tags ────────────────────────────────────────────────────────

    /// Set tags for a document. Normalizes to lowercase and adds to the tag library.
    pub fn set_document_tags(
        &mut self,
        source: &str,
        source_id: &str,
        tags: Vec<String>,
    ) -> Vec<String> {
        let key = Self::doc_key(source, source_id);
        let tag_set: BTreeSet<String> = tags
            .into_iter()
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();

        // Add all tags to the library
        for tag in &tag_set {
            self.tags.insert(tag.clone());
        }

        if tag_set.is_empty() {
            self.document_tags.remove(&key);
        } else {
            self.document_tags.insert(key, tag_set.clone());
        }

        tag_set.into_iter().collect()
    }

    pub fn get_document_tags(&self, source: &str, source_id: &str) -> Vec<String> {
        let key = Self::doc_key(source, source_id);
        self.document_tags
            .get(&key)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Delete a tag from the library and all document assignments.
    pub fn delete_tag(&mut self, name: &str) {
        let normalized = name.trim().to_lowercase();
        self.tags.remove(&normalized);
        for tags in self.document_tags.values_mut() {
            tags.remove(&normalized);
        }
        self.document_tags.retain(|_, v| !v.is_empty());
    }

    /// Get all tags in the library as a sorted vector.
    pub fn list_all_tags(&self) -> Vec<String> {
        self.tags.iter().cloned().collect()
    }

    // ── Cleanup ─────────────────────────────────────────────────────

    /// Remove all tag data associated with a document.
    pub fn remove_document(&mut self, source: &str, source_id: &str) {
        let key = Self::doc_key(source, source_id);
        self.document_tags.remove(&key);
    }
}

// ── Persistence functions ───────────────────────────────────────────────

/// Path to the SQLite database file.
fn db_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("origin")
        .join("spaces.db")
}

/// Path to the legacy JSON file (for migration).
fn json_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("origin")
        .join("spaces.json")
}

// ── SpaceDb ─────────────────────────────────────────────────────────────

/// SQLite-backed persistence layer for the SpaceStore.
struct SpaceDb {
    conn: Connection,
}

impl SpaceDb {
    /// Open (or create) the spaces database at the given path.
    fn open(path: &std::path::Path) -> Result<Self, OriginError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)
            .map_err(|e| OriginError::Generic(format!("spaces db open: {e}")))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| OriginError::Generic(format!("spaces db pragma: {e}")))?;
        let db = Self { conn };
        db.create_tables()?;
        Ok(db)
    }

    fn create_tables(&self) -> Result<(), OriginError> {
        self.conn
            .execute_batch(
                "
            CREATE TABLE IF NOT EXISTS spaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                icon TEXT NOT NULL DEFAULT '',
                color TEXT NOT NULL DEFAULT 'zinc',
                pinned INTEGER NOT NULL DEFAULT 0,
                auto_detected INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS space_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                pattern TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS activity_streams (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                name TEXT NOT NULL DEFAULT '',
                started_at INTEGER NOT NULL,
                ended_at INTEGER,
                app_sequence TEXT NOT NULL DEFAULT '[]'
            );

            CREATE TABLE IF NOT EXISTS document_spaces (
                doc_key TEXT PRIMARY KEY,
                space_id TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS document_tags (
                doc_key TEXT NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (doc_key, tag)
            );

            CREATE TABLE IF NOT EXISTS tags (
                name TEXT PRIMARY KEY
            );
            ",
            )
            .map_err(|e| OriginError::Generic(format!("spaces db create tables: {e}")))?;
        Ok(())
    }

    /// Write the full SpaceStore to SQLite, replacing all rows.
    fn save_all(&self, store: &SpaceStore) -> Result<(), OriginError> {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(|e| OriginError::Generic(format!("spaces db tx begin: {e}")))?;

        // Clear existing data (spaces/activity_streams/document_spaces tables remain on disk
        // but are no longer read or written — they are harmless residue from pre-B1)
        tx.execute_batch(
            "DELETE FROM document_tags;
             DELETE FROM tags;",
        )
        .map_err(|e| OriginError::Generic(format!("spaces db clear: {e}")))?;

        // ── Document tags ───────────────────────────────────────────
        for (doc_key, tags) in &store.document_tags {
            for tag in tags {
                tx.execute(
                    "INSERT INTO document_tags (doc_key, tag) VALUES (?1, ?2)",
                    params![doc_key, tag],
                )
                .map_err(|e| OriginError::Generic(format!("spaces db insert doc_tag: {e}")))?;
            }
        }

        // ── Tags library ────────────────────────────────────────────
        for tag in &store.tags {
            tx.execute("INSERT INTO tags (name) VALUES (?1)", params![tag])
                .map_err(|e| OriginError::Generic(format!("spaces db insert tag: {e}")))?;
        }

        tx.commit()
            .map_err(|e| OriginError::Generic(format!("spaces db tx commit: {e}")))?;

        Ok(())
    }

    /// Load the full SpaceStore from SQLite.
    fn load_all(&self) -> Result<SpaceStore, OriginError> {
        // ── Document tags ───────────────────────────────────────────
        let mut document_tags: HashMap<String, BTreeSet<String>> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT doc_key, tag FROM document_tags")
                .map_err(|e| OriginError::Generic(format!("spaces db select doc_tags: {e}")))?;
            let rows = stmt
                .query_map([], |row| {
                    let doc_key: String = row.get(0)?;
                    let tag: String = row.get(1)?;
                    Ok((doc_key, tag))
                })
                .map_err(|e| OriginError::Generic(format!("spaces db query doc_tags: {e}")))?;
            for row in rows {
                let (k, t) =
                    row.map_err(|e| OriginError::Generic(format!("spaces db doc_tag row: {e}")))?;
                document_tags.entry(k).or_default().insert(t);
            }
        }

        // ── Tags library ────────────────────────────────────────────
        let mut tags: BTreeSet<String> = BTreeSet::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT name FROM tags")
                .map_err(|e| OriginError::Generic(format!("spaces db select tags: {e}")))?;
            let rows = stmt
                .query_map([], |row| row.get(0))
                .map_err(|e| OriginError::Generic(format!("spaces db query tags: {e}")))?;
            for row in rows {
                tags.insert(
                    row.map_err(|e| OriginError::Generic(format!("spaces db tag row: {e}")))?,
                );
            }
        }

        Ok(SpaceStore {
            document_tags,
            tags,
        })
    }
}

// ── Public persistence API (unchanged signatures) ───────────────────────

pub fn load_spaces() -> SpaceStore {
    let db_path = db_path();
    let json_path = json_path();

    // Try opening (or creating) the SQLite database
    let db = match SpaceDb::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            log::error!("[spaces] failed to open spaces.db: {e}");
            return SpaceStore::default();
        }
    };

    // Migration: if spaces.json exists, import tag data from it into SQLite
    if json_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&json_path) {
            if let Ok(store) = serde_json::from_str::<SpaceStore>(&contents) {
                log::info!("[spaces] migrating tag data from spaces.json to SQLite");
                if let Err(e) = db.save_all(&store) {
                    log::error!("[spaces] migration save failed: {e}");
                } else {
                    // Rename the old file so we don't re-migrate
                    let migrated = json_path.with_extension("json.migrated");
                    let _ = std::fs::rename(&json_path, &migrated);
                }
                return store;
            }
        }
    }

    // Load from SQLite (returns empty defaults if DB is freshly created)
    match db.load_all() {
        Ok(store) => store,
        Err(e) => {
            log::error!("[spaces] failed to load from spaces.db: {e}");
            SpaceStore::default()
        }
    }
}

pub fn save_spaces(store: &SpaceStore) -> Result<(), OriginError> {
    let db = SpaceDb::open(&db_path())?;
    db.save_all(store)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_store_is_empty() {
        let store = SpaceStore::default();
        assert!(store.tags.is_empty());
        assert!(store.document_tags.is_empty());
    }

    #[test]
    fn test_document_tags() {
        let mut store = SpaceStore::default();

        // Set tags with mixed case
        let result = store.set_document_tags(
            "screen",
            "doc1",
            vec![
                "Rust".to_string(),
                "  TAURI  ".to_string(),
                "rust".to_string(), // duplicate after normalization
            ],
        );

        assert_eq!(result, vec!["rust", "tauri"]); // BTreeSet: sorted + deduped

        // Tags are in the library
        assert!(store.tags.contains("rust"));
        assert!(store.tags.contains("tauri"));

        // Retrieve document tags
        let tags = store.get_document_tags("screen", "doc1");
        assert_eq!(tags, vec!["rust", "tauri"]);

        // Delete a tag
        store.delete_tag("rust");
        assert!(!store.tags.contains("rust"));
        let tags = store.get_document_tags("screen", "doc1");
        assert_eq!(tags, vec!["tauri"]);
    }

    #[test]
    fn test_remove_document() {
        let mut store = SpaceStore::default();

        store.set_document_tags("screen", "doc1", vec!["rust".to_string()]);

        // Tags should exist
        assert!(!store.get_document_tags("screen", "doc1").is_empty());

        // Remove
        store.remove_document("screen", "doc1");
        assert!(store.get_document_tags("screen", "doc1").is_empty());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut store = SpaceStore::default();
        store.set_document_tags("screen", "doc1", vec!["rust".to_string()]);
        let json = serde_json::to_string(&store).unwrap();
        let deserialized: SpaceStore = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tags.len(), store.tags.len());
        assert_eq!(deserialized.document_tags.len(), store.document_tags.len());
    }

    // ── SQLite persistence tests ────────────────────────────────────

    /// Helper: open an in-memory SpaceDb for tests.
    fn test_db() -> SpaceDb {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        let db = SpaceDb { conn };
        db.create_tables().unwrap();
        db
    }

    #[test]
    fn test_sqlite_roundtrip_default_store() {
        let db = test_db();
        let original = SpaceStore::default();
        db.save_all(&original).unwrap();
        let loaded = db.load_all().unwrap();

        assert!(loaded.tags.is_empty());
        assert!(loaded.document_tags.is_empty());
    }

    #[test]
    fn test_sqlite_roundtrip_with_data() {
        let db = test_db();
        let mut store = SpaceStore::default();

        // Add tags
        store.set_document_tags(
            "screen",
            "doc1",
            vec!["rust".to_string(), "tauri".to_string()],
        );

        db.save_all(&store).unwrap();
        let loaded = db.load_all().unwrap();

        // Document tags
        assert_eq!(loaded.document_tags.len(), 1);
        let doc1_tags = loaded.document_tags.get("screen::doc1").unwrap();
        assert!(doc1_tags.contains("rust"));
        assert!(doc1_tags.contains("tauri"));

        // Tags library
        assert!(loaded.tags.contains("rust"));
        assert!(loaded.tags.contains("tauri"));
    }

    #[test]
    fn test_sqlite_save_overwrites_previous() {
        let db = test_db();

        // Save defaults (empty)
        let store1 = SpaceStore::default();
        db.save_all(&store1).unwrap();

        // Save a modified store with tags
        let mut store2 = SpaceStore::default();
        store2.set_document_tags("screen", "doc1", vec!["rust".to_string()]);
        db.save_all(&store2).unwrap();

        let loaded = db.load_all().unwrap();
        assert!(loaded.tags.contains("rust"));
        assert_eq!(loaded.document_tags.len(), 1);
    }

    #[test]
    fn test_sqlite_migration_from_json() {
        let dir = std::env::temp_dir().join(format!("origin_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let json_file = dir.join("spaces.json");
        let db_file = dir.join("spaces.db");

        // Write a JSON file with tag data
        let mut store = SpaceStore::default();
        store.set_document_tags("screen", "cap1", vec!["test".to_string()]);
        let json = serde_json::to_string_pretty(&store).unwrap();
        std::fs::write(&json_file, &json).unwrap();

        // Open the DB (no existing DB) and migrate
        let db = SpaceDb::open(&db_file).unwrap();
        // Simulate migration: load from JSON, save to SQLite
        let loaded_from_json: SpaceStore =
            serde_json::from_str(&std::fs::read_to_string(&json_file).unwrap()).unwrap();
        db.save_all(&loaded_from_json).unwrap();

        // Verify tag data survived migration
        let reloaded = db.load_all().unwrap();
        assert!(reloaded.tags.contains("test"));

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }
}
