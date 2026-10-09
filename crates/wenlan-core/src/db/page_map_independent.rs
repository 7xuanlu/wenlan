// SPDX-License-Identifier: Apache-2.0
//! Storage support for map-owned idea nodes.

use crate::WenlanError;

/// Validate an independent idea reference and return its canonical UUID.
/// The caller must validate before initializing or otherwise mutating a map.
pub fn canonical_idea_id(ref_id: &str, label: Option<&str>) -> Result<String, &'static str> {
    if label.is_none_or(|value| value.trim().is_empty()) {
        return Err("idea label must be non-empty");
    }
    uuid::Uuid::parse_str(ref_id)
        .map(|id| id.hyphenated().to_string())
        .map_err(|_| "idea ref_id must be a UUID")
}

pub fn validate_idea_label(label: Option<&str>) -> Result<(), &'static str> {
    if label.is_some_and(|value| !value.trim().is_empty()) {
        Ok(())
    } else {
        Err("idea label must be non-empty")
    }
}

impl super::MemoryDB {
    /// Reconciles both schema-133 histories and adds map-owned idea nodes.
    /// The public 133 lineage already has `page_history.title`; private Dev
    /// 133 databases may already have idea nodes but lack that column. Inspect
    /// schema shape instead of inferring it from `user_version`.
    pub(crate) async fn migrate_134_page_map_ideas(&self) -> Result<(), WenlanError> {
        let conn = self.conn.lock().await;
        let mut version_rows = conn
            .query("PRAGMA user_version", ())
            .await
            .map_err(|e| WenlanError::VectorDb(format!("m134 read user_version: {e}")))?;
        let version = version_rows
            .next()
            .await
            .map_err(|e| WenlanError::VectorDb(format!("m134 read user_version row: {e}")))?
            .and_then(|row| row.get::<i64>(0).ok())
            .unwrap_or(0);
        drop(version_rows);
        if version >= 134 {
            return Ok(());
        }

        let has_history_title = {
            let mut columns = conn
                .query("PRAGMA table_info(page_history)", ())
                .await
                .map_err(|e| WenlanError::VectorDb(format!("m134 inspect page_history: {e}")))?;
            let mut found = false;
            while let Some(row) = columns
                .next()
                .await
                .map_err(|e| WenlanError::VectorDb(format!("m134 inspect page_history row: {e}")))?
            {
                if row.get::<String>(1).ok().as_deref() == Some("title") {
                    found = true;
                }
            }
            found
        };

        let mut foreign_key_rows = conn
            .query("PRAGMA foreign_keys", ())
            .await
            .map_err(|e| WenlanError::VectorDb(format!("m134 read foreign_keys: {e}")))?;
        let foreign_keys_enabled = foreign_key_rows
            .next()
            .await
            .map_err(|e| WenlanError::VectorDb(format!("m134 read foreign_keys row: {e}")))?
            .and_then(|row| row.get::<i64>(0).ok())
            .unwrap_or(0)
            != 0;
        drop(foreign_key_rows);

        if foreign_keys_enabled {
            conn.execute("PRAGMA foreign_keys = OFF", ())
                .await
                .map_err(|e| WenlanError::VectorDb(format!("m134 disable foreign_keys: {e}")))?;
        }

        let migration_result = async {
            conn.execute("BEGIN IMMEDIATE", ())
                .await
                .map_err(|e| WenlanError::VectorDb(format!("m134 begin: {e}")))?;

            let result = async {
                if !has_history_title {
                    conn.execute("ALTER TABLE page_history ADD COLUMN title TEXT", ())
                        .await
                        .map_err(|e| {
                            WenlanError::VectorDb(format!("m134 add page_history.title: {e}"))
                        })?;
                }
                conn.execute(
                    "UPDATE page_history
                        SET title=(
                            SELECT p.title FROM pages p
                             WHERE p.id=page_history.page_id
                               AND p.version=page_history.version
                               AND p.content=page_history.content
                               AND p.source_memory_ids=page_history.source_memory_ids
                        )
                      WHERE title IS NULL
                        AND EXISTS (
                            SELECT 1 FROM pages p
                             WHERE p.id=page_history.page_id
                               AND p.version=page_history.version
                               AND p.content=page_history.content
                               AND p.source_memory_ids=page_history.source_memory_ids
                        )",
                    (),
                )
                .await
                .map_err(|e| {
                    WenlanError::VectorDb(format!("m134 backfill exact page_history titles: {e}"))
                })?;
                conn.execute_batch(
                    "CREATE TABLE page_map_nodes__m134 (
                        id          TEXT PRIMARY KEY,
                        page_id     TEXT NOT NULL REFERENCES page_maps(page_id) ON DELETE CASCADE,
                        parent_id   TEXT REFERENCES page_map_nodes__m134(id),
                        rank        REAL NOT NULL DEFAULT 0,
                        ref_kind    TEXT NOT NULL CHECK (ref_kind IN ('memory','entity','page','section','idea')),
                        ref_id      TEXT NOT NULL,
                        label       TEXT,
                        status      TEXT NOT NULL DEFAULT 'active'
                                    CHECK (status IN ('suggested','active','dismissed')),
                        pinned      INTEGER NOT NULL DEFAULT 0,
                        placed      INTEGER NOT NULL DEFAULT 0,
                        collapsed   INTEGER NOT NULL DEFAULT 0,
                        x REAL, y REAL, width REAL, height REAL,
                        fingerprint TEXT NOT NULL,
                        provenance  TEXT,
                        created_at  TEXT DEFAULT (datetime('now')),
                        updated_at  TEXT DEFAULT (datetime('now')),
                        CHECK (ref_kind <> 'idea' OR (label IS NOT NULL AND length(trim(label)) > 0))
                    );

                    INSERT INTO page_map_nodes__m134 (
                        id, page_id, parent_id, rank, ref_kind, ref_id, label, status,
                        pinned, placed, collapsed, x, y, width, height, fingerprint,
                        provenance, created_at, updated_at
                    ) SELECT
                        id, page_id, parent_id, rank, ref_kind, ref_id, label, status,
                        pinned, placed, collapsed, x, y, width, height, fingerprint,
                        provenance, created_at, updated_at
                    FROM page_map_nodes;

                    CREATE TABLE page_map_edges__m134 (
                        id         TEXT PRIMARY KEY,
                        page_id    TEXT NOT NULL REFERENCES page_maps(page_id) ON DELETE CASCADE,
                        from_node  TEXT NOT NULL REFERENCES page_map_nodes__m134(id) ON DELETE CASCADE,
                        to_node    TEXT NOT NULL REFERENCES page_map_nodes__m134(id) ON DELETE CASCADE,
                        kind       TEXT NOT NULL DEFAULT 'link' CHECK (kind IN ('link','suggested')),
                        label      TEXT,
                        status     TEXT NOT NULL DEFAULT 'active'
                                   CHECK (status IN ('suggested','active','dismissed')),
                        provenance TEXT,
                        created_at TEXT DEFAULT (datetime('now')),
                        CHECK (from_node <> to_node),
                        UNIQUE (page_id, from_node, to_node, kind)
                    );

                    INSERT INTO page_map_edges__m134 (
                        id, page_id, from_node, to_node, kind, label, status, provenance, created_at
                    ) SELECT
                        id, page_id, from_node, to_node, kind, label, status, provenance, created_at
                    FROM page_map_edges;

                    DROP TABLE page_map_edges;
                    DROP TABLE page_map_nodes;
                    ALTER TABLE page_map_nodes__m134 RENAME TO page_map_nodes;
                    ALTER TABLE page_map_edges__m134 RENAME TO page_map_edges;
                    CREATE UNIQUE INDEX idx_pmn_fp ON page_map_nodes(page_id, fingerprint);
                    CREATE INDEX idx_pmn_page ON page_map_nodes(page_id, status);
                    PRAGMA user_version = 134;",
                )
                .await
                .map_err(|e| WenlanError::VectorDb(format!("m134 rebuild page map tables: {e}")))?;

                for table in ["page_map_nodes", "page_map_edges"] {
                    let mut fk_check = conn
                        .query(&format!("PRAGMA foreign_key_check({table})"), ())
                        .await
                        .map_err(|e| {
                            WenlanError::VectorDb(format!(
                                "m134 foreign_key_check({table}): {e}"
                            ))
                        })?;
                    let has_violation = fk_check
                        .next()
                        .await
                        .map_err(|e| {
                            WenlanError::VectorDb(format!(
                                "m134 foreign_key_check({table}) row: {e}"
                            ))
                        })?
                        .is_some();
                    drop(fk_check);
                    if has_violation {
                        return Err(WenlanError::VectorDb(format!(
                            "m134 foreign_key_check found a violation in {table}"
                        )));
                    }
                }

                conn.execute("COMMIT", ())
                    .await
                    .map_err(|e| WenlanError::VectorDb(format!("m134 commit: {e}")))?;
                Ok(())
            }
            .await;

            if result.is_err() {
                let _ = conn.execute("ROLLBACK", ()).await;
            }
            result
        }
        .await;

        let restore_result = if foreign_keys_enabled {
            conn.execute("PRAGMA foreign_keys = ON", ())
                .await
                .map(|_| ())
                .map_err(|e| WenlanError::VectorDb(format!("m134 restore foreign_keys: {e}")))
        } else {
            Ok(())
        };
        migration_result?;
        restore_result?;
        log::info!(
            "[migration] Migration 134 applied: page history titles and map-owned idea nodes"
        );
        Ok(())
    }
}
