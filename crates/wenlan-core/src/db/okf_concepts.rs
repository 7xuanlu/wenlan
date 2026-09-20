// SPDX-License-Identifier: Apache-2.0
//! OKF bundle provenance and concept links (migration 131).
//!
//! An `okf` source ingests each concept file as one source page. `pages` has
//! no metadata column and `RawDocument.metadata` is never stored, so the
//! concept's frontmatter and its outbound concept links live here, keyed by
//! the source page id. Links are stored as concept ids, not page ids, and
//! resolved on every `refresh_page_wikilinks`: a source page's stored content
//! is its digest, not the file body, and resolving at write time keeps every
//! write path (worker, lint repair, rename, drafts) producing one link set.

use super::{commit_or_rollback, MemoryDB, UNFILED_SPACE_ID};
use crate::error::WenlanError;
use crate::synthesis::wikilinks::Wikilink;

/// Stored provenance for one imported OKF concept.
#[derive(Debug, Clone, PartialEq)]
pub struct OkfConceptRecord {
    pub page_id: String,
    pub source_id: String,
    pub concept_id: String,
    /// The concept's frontmatter as parsed, unchanged. Display only.
    pub frontmatter: serde_json::Value,
    pub updated_at: i64,
}

fn concept_key(concept_id: &str) -> String {
    concept_id.to_lowercase()
}

fn db_err(label: &'static str) -> impl Fn(libsql::Error) -> WenlanError {
    move |e| WenlanError::VectorDb(format!("{label}: {e}"))
}

impl MemoryDB {
    /// Migration 131 (OKF import): `okf_concepts` holds each imported
    /// concept's frontmatter, `okf_concept_links` its outbound concept links.
    /// No foreign key to `pages`: the worker writes the row before the page
    /// so the page write's own link refresh sees the links.
    pub(super) async fn migrate_131_okf_concepts(
        &self,
        prior_version: i64,
    ) -> Result<(), WenlanError> {
        self.backup_before_migration(131, prior_version).await?;
        let conn = self.conn.lock().await;
        conn.execute("BEGIN", ())
            .await
            .map_err(db_err("m131 begin"))?;
        let result = conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS okf_concepts (
                     page_id          TEXT PRIMARY KEY,
                     source_id        TEXT NOT NULL,
                     concept_id       TEXT NOT NULL,
                     concept_key      TEXT NOT NULL,
                     frontmatter_json TEXT NOT NULL,
                     updated_at       INTEGER NOT NULL
                 );
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_okf_concepts_source_concept
                     ON okf_concepts(source_id, concept_id);
                 CREATE INDEX IF NOT EXISTS idx_okf_concepts_source_key
                     ON okf_concepts(source_id, concept_key);
                 CREATE TABLE IF NOT EXISTS okf_concept_links (
                     page_id           TEXT NOT NULL,
                     target_concept_id TEXT NOT NULL,
                     target_key        TEXT NOT NULL,
                     PRIMARY KEY (page_id, target_concept_id)
                 );
                 CREATE INDEX IF NOT EXISTS idx_okf_concept_links_target
                     ON okf_concept_links(target_key);",
            )
            .await
            .map_err(db_err("m131 create"));
        match result {
            Ok(_) => {
                commit_or_rollback(&conn)
                    .await
                    .map_err(db_err("m131 commit"))?;
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", ()).await;
                return Err(e);
            }
        }
        conn.execute("PRAGMA user_version = 131", ())
            .await
            .map_err(db_err("m131 bump"))?;
        log::info!("[migration] Migration 131 applied: okf_concepts and okf_concept_links");
        Ok(())
    }

    /// Insert or replace one concept's provenance and its outbound link set in
    /// one transaction. A row for the same concept under another page id (an
    /// id from an earlier path) is replaced.
    pub async fn upsert_okf_concept(
        &self,
        page_id: &str,
        source_id: &str,
        concept_id: &str,
        frontmatter: &serde_json::Value,
        links: &[String],
    ) -> Result<(), WenlanError> {
        let frontmatter_json = serde_json::to_string(frontmatter)
            .map_err(|e| WenlanError::VectorDb(format!("okf frontmatter encode: {e}")))?;
        let now = chrono::Utc::now().timestamp();
        let conn = self.conn.lock().await;
        conn.execute("BEGIN", ())
            .await
            .map_err(db_err("okf upsert begin"))?;
        let result: Result<(), WenlanError> = async {
            conn.execute(
                "DELETE FROM okf_concept_links WHERE page_id IN
                     (SELECT page_id FROM okf_concepts
                      WHERE source_id = ?1 AND concept_id = ?2 AND page_id != ?3)",
                libsql::params![source_id, concept_id, page_id],
            )
            .await
            .map_err(db_err("okf upsert stale links"))?;
            conn.execute(
                "DELETE FROM okf_concepts
                 WHERE source_id = ?1 AND concept_id = ?2 AND page_id != ?3",
                libsql::params![source_id, concept_id, page_id],
            )
            .await
            .map_err(db_err("okf upsert stale row"))?;
            conn.execute(
                "INSERT INTO okf_concepts
                     (page_id, source_id, concept_id, concept_key, frontmatter_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(page_id) DO UPDATE SET
                     source_id = excluded.source_id,
                     concept_id = excluded.concept_id,
                     concept_key = excluded.concept_key,
                     frontmatter_json = excluded.frontmatter_json,
                     updated_at = excluded.updated_at",
                libsql::params![
                    page_id,
                    source_id,
                    concept_id,
                    concept_key(concept_id),
                    frontmatter_json,
                    now
                ],
            )
            .await
            .map_err(db_err("okf upsert concept"))?;
            conn.execute(
                "DELETE FROM okf_concept_links WHERE page_id = ?1",
                libsql::params![page_id],
            )
            .await
            .map_err(db_err("okf upsert clear links"))?;
            for target in links {
                conn.execute(
                    "INSERT OR IGNORE INTO okf_concept_links
                         (page_id, target_concept_id, target_key)
                     VALUES (?1, ?2, ?3)",
                    libsql::params![page_id, target.as_str(), concept_key(target)],
                )
                .await
                .map_err(db_err("okf upsert link"))?;
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => {
                commit_or_rollback(&conn)
                    .await
                    .map_err(db_err("okf upsert commit"))?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", ()).await;
                Err(e)
            }
        }
    }

    /// The stored provenance for a source page, if it is an imported concept.
    pub async fn get_okf_concept(
        &self,
        page_id: &str,
    ) -> Result<Option<OkfConceptRecord>, WenlanError> {
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT page_id, source_id, concept_id, frontmatter_json, updated_at
                 FROM okf_concepts WHERE page_id = ?1",
                libsql::params![page_id],
            )
            .await
            .map_err(db_err("get_okf_concept"))?;
        let Some(row) = rows.next().await.map_err(db_err("get_okf_concept row"))? else {
            return Ok(None);
        };
        let frontmatter_json: String = row.get(3).map_err(db_err("okf frontmatter col"))?;
        Ok(Some(OkfConceptRecord {
            page_id: row.get(0).map_err(db_err("okf page_id col"))?,
            source_id: row.get(1).map_err(db_err("okf source_id col"))?,
            concept_id: row.get(2).map_err(db_err("okf concept_id col"))?,
            frontmatter: serde_json::from_str(&frontmatter_json).unwrap_or(serde_json::Value::Null),
            updated_at: row.get(4).map_err(db_err("okf updated_at col"))?,
        }))
    }

    /// The target concept ids a concept page links to, sorted.
    pub async fn okf_concept_link_targets(
        &self,
        page_id: &str,
    ) -> Result<Vec<String>, WenlanError> {
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT target_concept_id FROM okf_concept_links
                 WHERE page_id = ?1 ORDER BY target_concept_id",
                libsql::params![page_id],
            )
            .await
            .map_err(db_err("okf_concept_link_targets"))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(db_err("okf link target row"))? {
            out.push(row.get(0).map_err(db_err("okf link target col"))?);
        }
        Ok(out)
    }

    /// Remove a concept's provenance and link rows. Returns whether a concept
    /// row existed.
    pub async fn delete_okf_concept(&self, page_id: &str) -> Result<bool, WenlanError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM okf_concept_links WHERE page_id = ?1",
            libsql::params![page_id],
        )
        .await
        .map_err(db_err("delete_okf_concept links"))?;
        let removed = conn
            .execute(
                "DELETE FROM okf_concepts WHERE page_id = ?1",
                libsql::params![page_id],
            )
            .await
            .map_err(db_err("delete_okf_concept"))?;
        Ok(removed > 0)
    }

    /// Remove every concept row of a source (source removal). Returns the
    /// number of concept rows removed.
    pub async fn delete_okf_concepts_for_source(
        &self,
        source_id: &str,
    ) -> Result<u64, WenlanError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM okf_concept_links WHERE page_id IN
                 (SELECT page_id FROM okf_concepts WHERE source_id = ?1)",
            libsql::params![source_id],
        )
        .await
        .map_err(db_err("delete_okf_concepts_for_source links"))?;
        conn.execute(
            "DELETE FROM okf_concepts WHERE source_id = ?1",
            libsql::params![source_id],
        )
        .await
        .map_err(db_err("delete_okf_concepts_for_source"))
    }

    /// Move a concept row to a new page id and concept id after a same-bytes
    /// rename rebinds its source page. Returns whether a row moved.
    pub async fn rekey_okf_concept(
        &self,
        old_page_id: &str,
        new_page_id: &str,
        new_concept_id: &str,
    ) -> Result<bool, WenlanError> {
        let conn = self.conn.lock().await;
        conn.execute("BEGIN", ())
            .await
            .map_err(db_err("okf rekey begin"))?;
        let result: Result<u64, WenlanError> = async {
            if old_page_id != new_page_id {
                conn.execute(
                    "DELETE FROM okf_concept_links WHERE page_id = ?1",
                    libsql::params![new_page_id],
                )
                .await
                .map_err(db_err("okf rekey clear new links"))?;
                conn.execute(
                    "DELETE FROM okf_concepts WHERE page_id = ?1",
                    libsql::params![new_page_id],
                )
                .await
                .map_err(db_err("okf rekey clear new row"))?;
            }
            let moved = conn
                .execute(
                    "UPDATE okf_concepts
                     SET page_id = ?2, concept_id = ?3, concept_key = ?4, updated_at = ?5
                     WHERE page_id = ?1",
                    libsql::params![
                        old_page_id,
                        new_page_id,
                        new_concept_id,
                        concept_key(new_concept_id),
                        chrono::Utc::now().timestamp()
                    ],
                )
                .await
                .map_err(db_err("okf rekey row"))?;
            conn.execute(
                "UPDATE okf_concept_links SET page_id = ?2 WHERE page_id = ?1",
                libsql::params![old_page_id, new_page_id],
            )
            .await
            .map_err(db_err("okf rekey links"))?;
            Ok(moved)
        }
        .await;
        match result {
            Ok(moved) => {
                commit_or_rollback(&conn)
                    .await
                    .map_err(db_err("okf rekey commit"))?;
                Ok(moved > 0)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", ()).await;
                Err(e)
            }
        }
    }

    /// Page ids of a source's concepts linking to any of `concept_ids`,
    /// matched case-insensitively like resolution's fallback. Sorted and
    /// deduplicated.
    pub async fn okf_pages_linking_to(
        &self,
        source_id: &str,
        concept_ids: &[String],
    ) -> Result<Vec<String>, WenlanError> {
        let conn = self.conn.lock().await;
        let mut out = std::collections::BTreeSet::new();
        for concept_id in concept_ids {
            let mut rows = conn
                .query(
                    "SELECT l.page_id FROM okf_concept_links l
                     JOIN okf_concepts c ON c.page_id = l.page_id
                     WHERE c.source_id = ?1 AND l.target_key = ?2",
                    libsql::params![source_id, concept_key(concept_id)],
                )
                .await
                .map_err(db_err("okf linking pages"))?;
            while let Some(row) = rows.next().await.map_err(db_err("okf linking row"))? {
                out.insert(row.get::<String>(0).map_err(db_err("okf linking col"))?);
            }
        }
        Ok(out.into_iter().collect())
    }

    /// Every imported concept as `(page id, source id, concept id)`.
    ///
    /// The OKF export uses this to resolve a link an imported concept body
    /// still carries in its ORIGINAL bundle's terms -- `/concepts/b.md` --
    /// to wherever the export puts that concept's page now. Without it the
    /// exported bundle keeps a link to a path it does not contain.
    pub async fn okf_concept_paths(&self) -> Result<Vec<(String, String, String)>, WenlanError> {
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT page_id, source_id, concept_id FROM okf_concepts
                 ORDER BY source_id, concept_id",
                (),
            )
            .await
            .map_err(db_err("okf concept paths"))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(db_err("okf concept path row"))? {
            out.push((
                row.get::<String>(0)
                    .map_err(db_err("okf concept path col"))?,
                row.get::<String>(1)
                    .map_err(db_err("okf concept path col"))?,
                row.get::<String>(2)
                    .map_err(db_err("okf concept path col"))?,
            ));
        }
        Ok(out)
    }

    /// Re-resolve the links of every concept page of `source_id` that links
    /// to one of `concept_ids`, plus `also_page_ids`, from their stored
    /// content. Call after a concept arrives, is removed, or moves, so a
    /// forward link resolves and a link to a removed page retires its edge.
    /// Returns how many pages were refreshed.
    pub async fn refresh_okf_concept_linkers(
        &self,
        source_id: &str,
        concept_ids: &[String],
        also_page_ids: &[String],
    ) -> Result<usize, WenlanError> {
        let mut page_ids: std::collections::BTreeSet<String> = self
            .okf_pages_linking_to(source_id, concept_ids)
            .await?
            .into_iter()
            .collect();
        page_ids.extend(also_page_ids.iter().cloned());
        let mut refreshed = 0;
        for page_id in page_ids {
            let Some(page) = self.get_page(&page_id).await? else {
                continue;
            };
            self.refresh_page_wikilinks(&page_id, &page.content).await?;
            refreshed += 1;
        }
        Ok(refreshed)
    }

    /// The concept links of `page_id` resolved to pages, for merging into the
    /// page's wikilink set. Empty when the page is not an imported concept.
    ///
    /// A target resolves within the linking concept's source and within
    /// `scope` (the Space rule wikilinks use): exact concept id first, else a
    /// case-insensitive match when exactly one concept has it. The target page
    /// must be active. Anything else is an unresolved link labeled with the
    /// target concept id. A link to the page itself is dropped.
    ///
    /// The `okf_concepts` row is the liveness authority, not `stale_reason`:
    /// the sync deletes the row when a concept is removed or deprecated, and
    /// a concept that later returns keeps its page's `source_removed` mark
    /// (the source replace path never clears it) while being live again.
    pub(crate) async fn okf_links_for_page(
        &self,
        page_id: &str,
        scope: Option<&str>,
    ) -> Result<Vec<Wikilink>, WenlanError> {
        const LIVE_TARGET: &str = "SELECT c.page_id FROM okf_concepts c
             JOIN pages p ON p.id = c.page_id
             WHERE c.source_id = ?1
               AND p.status = 'active'
               AND p.space = COALESCE(?2, ?3)";

        let conn = self.conn.lock().await;
        let source_id: String = {
            let mut rows = conn
                .query(
                    "SELECT source_id FROM okf_concepts WHERE page_id = ?1",
                    libsql::params![page_id],
                )
                .await
                .map_err(db_err("okf links source"))?;
            match rows.next().await.map_err(db_err("okf links source row"))? {
                Some(row) => row.get(0).map_err(db_err("okf links source col"))?,
                None => return Ok(Vec::new()),
            }
        };
        let targets: Vec<String> = {
            let mut rows = conn
                .query(
                    "SELECT target_concept_id FROM okf_concept_links
                     WHERE page_id = ?1 ORDER BY target_concept_id",
                    libsql::params![page_id],
                )
                .await
                .map_err(db_err("okf links targets"))?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await.map_err(db_err("okf links target row"))? {
                out.push(row.get(0).map_err(db_err("okf links target col"))?);
            }
            out
        };

        let mut links = Vec::with_capacity(targets.len());
        for target in targets {
            let mut target_page_id: Option<String> = {
                let mut rows = conn
                    .query(
                        &format!("{LIVE_TARGET} AND c.concept_id = ?4 LIMIT 1"),
                        libsql::params![
                            source_id.as_str(),
                            scope,
                            UNFILED_SPACE_ID,
                            target.as_str()
                        ],
                    )
                    .await
                    .map_err(db_err("okf links exact"))?;
                match rows.next().await.map_err(db_err("okf links exact row"))? {
                    Some(row) => Some(row.get(0).map_err(db_err("okf links exact col"))?),
                    None => None,
                }
            };
            if target_page_id.is_none() {
                let mut rows = conn
                    .query(
                        &format!("{LIVE_TARGET} AND c.concept_key = ?4 LIMIT 2"),
                        libsql::params![
                            source_id.as_str(),
                            scope,
                            UNFILED_SPACE_ID,
                            concept_key(&target)
                        ],
                    )
                    .await
                    .map_err(db_err("okf links folded"))?;
                let mut candidates: Vec<String> = Vec::new();
                while let Some(row) = rows.next().await.map_err(db_err("okf links folded row"))? {
                    candidates.push(row.get(0).map_err(db_err("okf links folded col"))?);
                }
                if candidates.len() == 1 {
                    target_page_id = candidates.pop();
                }
            }
            if target_page_id.as_deref() == Some(page_id) {
                continue;
            }
            links.push(Wikilink {
                label: target,
                target_page_id,
            });
        }
        Ok(links)
    }
}

/// Merge concept links into a wikilink set without breaking `page_links`'
/// `(source_page_id, label_key)` key or minting two edges to one page: a
/// concept link is dropped when its label (case-insensitive) or its resolved
/// target is already present.
pub(crate) fn merge_okf_links(links: &mut Vec<Wikilink>, okf_links: Vec<Wikilink>) {
    for link in okf_links {
        let key = link.label.to_lowercase();
        let duplicate = links.iter().any(|existing| {
            existing.label.to_lowercase() == key
                || (link.target_page_id.is_some() && existing.target_page_id == link.target_page_id)
        });
        if !duplicate {
            links.push(link);
        }
    }
}

#[cfg(test)]
#[path = "okf_concepts_test.rs"]
mod tests;
