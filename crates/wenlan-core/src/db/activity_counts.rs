// SPDX-License-Identifier: Apache-2.0
//! Counts backing `GET /api/activity`: one read-only pass over the memories,
//! enrichment-step, entity-shadow and page tables.
//!
//! Same shape as the other `impl MemoryDB` reader files (e.g.
//! `scoped_entities.rs`): pure counts, no prose, one held connection guard.

use super::MemoryDB;
use crate::WenlanError;

/// One `(step_name, status)` cell of the `enrichment_steps` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepCount {
    pub step_name: String,
    pub status: String,
    pub count: u64,
}

/// Every count `GET /api/activity` composes into an `ActivityResponse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCounts {
    /// Distinct `source_id`s in `memories` with `source = 'memory'`.
    pub memories_total: u64,
    /// `(step_name, status, count)` cells from `enrichment_steps`.
    pub step_counts: Vec<StepCount>,
    /// The "raw" count from `pipeline_status`: distinct memory `source_id`s
    /// with no `enrichment_steps` row at all.
    pub memories_without_steps: u64,
    /// Entity shadow pages that are live but unconfirmed.
    pub entities_detected: u64,
    /// Entity shadow pages that are live and confirmed.
    pub entities_confirmed: u64,
    /// Live non-entity pages.
    pub pages_active: u64,
    /// Live non-entity pages with `stale_reason` set and refreshable.
    pub pages_stale: u64,
    /// Live non-entity pages whose refresh is blocked.
    pub pages_refresh_blocked: u64,
    /// `MAX(updated_at)` over `enrichment_steps`; `None` on an empty table.
    pub last_step_updated_at: Option<i64>,
}

impl MemoryDB {
    /// Read every count `GET /api/activity` needs in one pass. Holds the
    /// connection guard for the whole function like `pipeline_status` does;
    /// every query is read-only.
    pub async fn activity_counts(&self) -> Result<ActivityCounts, WenlanError> {
        let conn = self.conn.lock().await;

        let memories_total = scalar_u64(
            &conn,
            "SELECT COUNT(DISTINCT source_id) FROM memories WHERE source = 'memory'",
            "activity_counts memories_total",
        )
        .await?;

        let mut step_rows = conn
            .query(
                "SELECT step_name, status, COUNT(*) FROM enrichment_steps \
                 GROUP BY step_name, status",
                (),
            )
            .await
            .map_err(|e| WenlanError::VectorDb(format!("activity_counts step_counts: {e}")))?;
        let mut step_counts = Vec::new();
        while let Some(row) = step_rows
            .next()
            .await
            .map_err(|e| WenlanError::VectorDb(format!("activity_counts step_counts row: {e}")))?
        {
            step_counts.push(StepCount {
                step_name: row.get::<String>(0).map_err(|e| {
                    WenlanError::VectorDb(format!("activity_counts step_counts name: {e}"))
                })?,
                status: row.get::<String>(1).map_err(|e| {
                    WenlanError::VectorDb(format!("activity_counts step_counts status: {e}"))
                })?,
                count: row
                    .get::<i64>(2)
                    .map_err(|e| {
                        WenlanError::VectorDb(format!("activity_counts step_counts count: {e}"))
                    })?
                    .max(0) as u64,
            });
        }
        drop(step_rows);

        // Same SQL as the "raw" count in `pipeline_status`.
        let memories_without_steps = scalar_u64(
            &conn,
            "SELECT COUNT(DISTINCT source_id) FROM memories WHERE source = 'memory' \
             AND source_id NOT IN (SELECT DISTINCT source_id FROM enrichment_steps)",
            "activity_counts memories_without_steps",
        )
        .await?;

        // Entity lifecycle predicates mirror `entity_status_predicate` in
        // db.rs, over the same `entity_page_map`/`pages` shadow shape the
        // scoped entity readers use.
        let entities_detected = scalar_u64(
            &conn,
            "SELECT COUNT(*) FROM entity_page_map epm JOIN pages p ON p.id = epm.page_id \
             WHERE p.kind = 'entity' \
             AND (p.status != 'archived' AND COALESCE(p.entity_confirmed, 0) = 0)",
            "activity_counts entities_detected",
        )
        .await?;
        let entities_confirmed = scalar_u64(
            &conn,
            "SELECT COUNT(*) FROM entity_page_map epm JOIN pages p ON p.id = epm.page_id \
             WHERE p.kind = 'entity' \
             AND (p.status != 'archived' AND COALESCE(p.entity_confirmed, 0) = 1)",
            "activity_counts entities_confirmed",
        )
        .await?;

        // Same SQL as `count_active_pages`.
        let pages_active = scalar_u64(
            &conn,
            "SELECT COUNT(*) FROM pages \
             WHERE status = 'active' AND COALESCE(kind, 'concept') != 'entity'",
            "activity_counts pages_active",
        )
        .await?;
        // Stale-page predicates follow `list_stale_pages_scoped`'s base shape
        // (live non-entity pages with `stale_reason` set), split by whether a
        // refresh is blocked.
        let pages_stale = scalar_u64(
            &conn,
            "SELECT COUNT(*) FROM pages \
             WHERE status = 'active' AND COALESCE(kind, 'concept') != 'entity' \
             AND stale_reason IS NOT NULL AND refresh_blocked_reason IS NULL",
            "activity_counts pages_stale",
        )
        .await?;
        let pages_refresh_blocked = scalar_u64(
            &conn,
            "SELECT COUNT(*) FROM pages \
             WHERE status = 'active' AND COALESCE(kind, 'concept') != 'entity' \
             AND stale_reason IS NOT NULL AND refresh_blocked_reason IS NOT NULL",
            "activity_counts pages_refresh_blocked",
        )
        .await?;

        let mut updated_rows = conn
            .query("SELECT MAX(updated_at) FROM enrichment_steps", ())
            .await
            .map_err(|e| {
                WenlanError::VectorDb(format!("activity_counts last_step_updated_at: {e}"))
            })?;
        let last_step_updated_at = match updated_rows.next().await.map_err(|e| {
            WenlanError::VectorDb(format!("activity_counts last_step_updated_at row: {e}"))
        })? {
            Some(row) => row.get::<Option<i64>>(0).map_err(|e| {
                WenlanError::VectorDb(format!("activity_counts last_step_updated_at get: {e}"))
            })?,
            None => None,
        };
        drop(updated_rows);

        Ok(ActivityCounts {
            memories_total,
            step_counts,
            memories_without_steps,
            entities_detected,
            entities_confirmed,
            pages_active,
            pages_stale,
            pages_refresh_blocked,
            last_step_updated_at,
        })
    }
}

async fn scalar_u64(conn: &libsql::Connection, sql: &str, what: &str) -> Result<u64, WenlanError> {
    let mut rows = conn
        .query(sql, ())
        .await
        .map_err(|e| WenlanError::VectorDb(format!("{what} query: {e}")))?;
    let row = rows
        .next()
        .await
        .map_err(|e| WenlanError::VectorDb(format!("{what} next: {e}")))?
        .ok_or_else(|| WenlanError::Generic(format!("{what}: no rows")))?;
    Ok(row
        .get::<i64>(0)
        .map_err(|e| WenlanError::VectorDb(format!("{what} get: {e}")))?
        .max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_memory(db: &MemoryDB, id: &str, source_id: &str, steps: &[(&str, &str, i64)]) {
        let conn = db.conn.lock().await;
        conn.execute(
            "INSERT INTO memories (id, content, source, source_id, title, chunk_index, \
             last_modified, chunk_type, space) \
             VALUES (?1, 'content', 'memory', ?2, 'title', 0, 0, 'text', 'work')",
            libsql::params![id, source_id],
        )
        .await
        .unwrap();
        for (step_name, status, updated_at) in steps {
            conn.execute(
                "INSERT INTO enrichment_steps (source_id, step_name, status, attempts, updated_at) \
                 VALUES (?1, ?2, ?3, 1, ?4)",
                libsql::params![source_id, *step_name, *status, *updated_at],
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn activity_counts_empty_db_is_all_zero() {
        let (db, _tmp) = super::super::tests::test_db().await;
        let counts = db.activity_counts().await.unwrap();
        assert_eq!(
            counts,
            ActivityCounts {
                memories_total: 0,
                step_counts: vec![],
                memories_without_steps: 0,
                entities_detected: 0,
                entities_confirmed: 0,
                pages_active: 0,
                pages_stale: 0,
                pages_refresh_blocked: 0,
                last_step_updated_at: None,
            }
        );
    }

    #[tokio::test]
    async fn activity_counts_groups_steps_and_splits_pages_and_entities() {
        let (db, _tmp) = super::super::tests::test_db().await;
        {
            let conn = db.conn.lock().await;
            conn.execute_batch(
                "INSERT INTO spaces (id, name, created_at, updated_at) \
                 VALUES ('space-work', 'work', 1, 1);",
            )
            .await
            .unwrap();
        }
        // One memory with a done + a failed step, one raw memory, one
        // non-memory row the reader must ignore.
        seed_memory(
            &db,
            "row-1",
            "mem-1",
            &[("title_enrich", "ok", 100), ("page_growth", "failed", 200)],
        )
        .await;
        seed_memory(&db, "row-2", "mem-2", &[]).await;
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "INSERT INTO memories (id, content, source, source_id, title, chunk_index, \
                 last_modified, chunk_type, space) \
                 VALUES ('row-3', 'content', 'note', 'note-1', 'title', 0, 0, 'text', 'work')",
                (),
            )
            .await
            .unwrap();
            conn.execute(
                "INSERT INTO pages (id, title, content, created_at, last_compiled, last_modified) \
                 VALUES ('page-active', 'Active', 'body', 0, 0, 0)",
                (),
            )
            .await
            .unwrap();
            conn.execute(
                "INSERT INTO pages (id, title, content, created_at, last_compiled, last_modified, \
                 stale_reason) \
                 VALUES ('page-stale', 'Stale', 'body', 0, 0, 0, 'source_updated')",
                (),
            )
            .await
            .unwrap();
            conn.execute(
                "INSERT INTO pages (id, title, content, created_at, last_compiled, last_modified, \
                 stale_reason, refresh_blocked_reason) \
                 VALUES ('page-blocked', 'Blocked', 'body', 0, 0, 0, 'source_updated', 'citations')",
                (),
            )
            .await
            .unwrap();
        }
        db.test_seed_entity_shadow_page(crate::db::TestEntity::new("ent-1", "One", "concept"))
            .await
            .unwrap();
        db.test_seed_entity_shadow_page(
            crate::db::TestEntity::new("ent-2", "Two", "concept").confirmed(true),
        )
        .await
        .unwrap();

        let counts = db.activity_counts().await.unwrap();
        assert_eq!(counts.memories_total, 2);
        assert_eq!(counts.memories_without_steps, 1);
        let mut cells: Vec<(&str, &str, u64)> = counts
            .step_counts
            .iter()
            .map(|cell| (cell.step_name.as_str(), cell.status.as_str(), cell.count))
            .collect();
        cells.sort();
        assert_eq!(
            cells,
            vec![("page_growth", "failed", 1), ("title_enrich", "ok", 1)]
        );
        assert_eq!(counts.entities_detected, 1);
        assert_eq!(counts.entities_confirmed, 1);
        assert_eq!(counts.pages_active, 3);
        assert_eq!(counts.pages_stale, 1);
        assert_eq!(counts.pages_refresh_blocked, 1);
        assert_eq!(counts.last_step_updated_at, Some(200));
    }
}
