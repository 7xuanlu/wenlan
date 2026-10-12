// SPDX-License-Identifier: Apache-2.0
//! Durable user annotations independent of source-ingest chunk lifetimes.

use super::{commit_or_rollback, MemoryDB};
use crate::WenlanError;
use wenlan_types::source_annotations::SourceAnnotation;

/// Contexts for annotations kept with orphaned memories use a namespace that
/// cannot be selected through the public Space resolver. The Space ID itself
/// is intentionally not reused when a Space with the same name is recreated.
pub(super) fn orphaned_space_context_key(space_name: &str) -> String {
    format!("orphaned-space:{space_name}")
}

/// Move all annotations between Space contexts inside the caller's transaction.
/// Conflicting notes or linked-page lists fail closed before any row changes.
pub(super) async fn move_context_in_transaction(
    tx: &libsql::Transaction,
    from_context: &str,
    to_context: &str,
) -> Result<(), WenlanError> {
    if from_context == to_context {
        return Ok(());
    }

    let mut rows = tx
        .query(
            "SELECT source, source_id, notes, page_ids, version FROM source_annotations \
             WHERE context_key = ?1 ORDER BY source, source_id",
            libsql::params![from_context],
        )
        .await
        .map_err(|e| WenlanError::VectorDb(format!("source annotation move read: {e}")))?;
    let mut source_annotations = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| WenlanError::VectorDb(format!("source annotation move row: {e}")))?
    {
        let page_ids_json = row.get::<String>(3).unwrap_or_else(|_| "[]".to_string());
        let page_ids = serde_json::from_str::<Vec<String>>(&page_ids_json)
            .map_err(|e| WenlanError::VectorDb(format!("source annotation move page IDs: {e}")))?;
        source_annotations.push((
            row.get::<String>(0).unwrap_or_default(),
            row.get::<String>(1).unwrap_or_default(),
            row.get::<String>(2).unwrap_or_default(),
            page_ids,
            row.get::<i64>(4).unwrap_or_default().max(0) as u64,
        ));
    }
    drop(rows);

    // Preflight every collision before touching any row so callers can safely
    // perform other lifecycle updates in the same transaction afterwards.
    let mut move_plan = Vec::with_capacity(source_annotations.len());
    for (source, source_id, notes, page_ids, source_version) in &source_annotations {
        let mut target_rows = tx
            .query(
                "SELECT notes, page_ids, version FROM source_annotations \
                 WHERE source = ?1 AND source_id = ?2 AND context_key = ?3",
                libsql::params![source.as_str(), source_id.as_str(), to_context],
            )
            .await
            .map_err(|e| WenlanError::VectorDb(format!("source annotation move target: {e}")))?;
        let target =
            if let Some(row) = target_rows.next().await.map_err(|e| {
                WenlanError::VectorDb(format!("source annotation move target row: {e}"))
            })? {
                let target_pages_json = row.get::<String>(1).unwrap_or_else(|_| "[]".to_string());
                let target_pages = serde_json::from_str::<Vec<String>>(&target_pages_json)
                    .map_err(|e| {
                        WenlanError::VectorDb(format!("source annotation target page IDs: {e}"))
                    })?;
                Some((
                    row.get::<String>(0).unwrap_or_default(),
                    target_pages,
                    row.get::<i64>(2).unwrap_or_default().max(0) as u64,
                ))
            } else {
                None
            };
        drop(target_rows);

        let (next_version, target_exists) = match target {
            Some((target_notes, target_pages, target_version)) => {
                if target_notes.as_str() != notes.as_str()
                    || target_pages.as_slice() != page_ids.as_slice()
                {
                    return Err(WenlanError::Conflict(format!(
                        "source annotation {source}/{source_id} differs between Space contexts"
                    )));
                }
                let merged_version = source_version.max(&target_version);
                if *merged_version >= i64::MAX as u64 {
                    return Err(WenlanError::Conflict(format!(
                        "source annotation {source}/{source_id} version cannot be advanced"
                    )));
                }
                (*merged_version + 1, true)
            }
            None => {
                if *source_version >= i64::MAX as u64 {
                    return Err(WenlanError::Conflict(format!(
                        "source annotation {source}/{source_id} version cannot be advanced"
                    )));
                }
                (*source_version + 1, false)
            }
        };
        move_plan.push((
            source.clone(),
            source_id.clone(),
            next_version,
            target_exists,
        ));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64;
    for (source, source_id, version, target_exists) in move_plan {
        let version = version as i64;
        if target_exists {
            tx.execute(
                "UPDATE source_annotations SET version = ?1, updated_at = ?2 \
                 WHERE source = ?3 AND source_id = ?4 AND context_key = ?5",
                libsql::params![
                    version,
                    now,
                    source.as_str(),
                    source_id.as_str(),
                    to_context
                ],
            )
            .await
            .map_err(|e| WenlanError::VectorDb(format!("source annotation coalesce: {e}")))?;
            tx.execute(
                "DELETE FROM source_annotations WHERE source = ?1 AND source_id = ?2 \
                 AND context_key = ?3",
                libsql::params![source.as_str(), source_id.as_str(), from_context],
            )
            .await
            .map_err(|e| WenlanError::VectorDb(format!("source annotation source delete: {e}")))?;
        } else {
            tx.execute(
                "UPDATE source_annotations SET context_key = ?1, version = ?2, updated_at = ?3 \
                 WHERE source = ?4 AND source_id = ?5 AND context_key = ?6",
                libsql::params![
                    to_context,
                    version,
                    now,
                    source.as_str(),
                    source_id.as_str(),
                    from_context
                ],
            )
            .await
            .map_err(|e| WenlanError::VectorDb(format!("source annotation move update: {e}")))?;
        }
    }

    Ok(())
}

pub(super) async fn context_has_annotations_in_transaction(
    tx: &libsql::Transaction,
    context_key: &str,
) -> Result<bool, WenlanError> {
    let mut rows = tx
        .query(
            "SELECT 1 FROM source_annotations WHERE context_key = ?1 LIMIT 1",
            libsql::params![context_key],
        )
        .await
        .map_err(|e| WenlanError::VectorDb(format!("source annotation context check: {e}")))?;
    let exists = rows
        .next()
        .await
        .map_err(|e| WenlanError::VectorDb(format!("source annotation context row: {e}")))?
        .is_some();
    Ok(exists)
}

pub const MAX_SOURCE_ANNOTATION_NOTES_BYTES: usize = 64 * 1024;
pub const MAX_SOURCE_ANNOTATION_PAGES: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceAnnotationWrite {
    Saved(SourceAnnotation),
    VersionConflict { current_version: u64 },
}

impl MemoryDB {
    /// Read one annotation row. `context_key` is `global`, `uncategorized`, or
    /// the stable ID of a registered Space.
    pub async fn get_source_annotation(
        &self,
        source: &str,
        source_id: &str,
        context_key: &str,
    ) -> Result<Option<SourceAnnotation>, WenlanError> {
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT notes, page_ids, version, updated_at FROM source_annotations \
                 WHERE source = ?1 AND source_id = ?2 AND context_key = ?3",
                libsql::params![source, source_id, context_key],
            )
            .await
            .map_err(|e| WenlanError::VectorDb(format!("source annotation read: {e}")))?;
        let Some(row) = rows
            .next()
            .await
            .map_err(|e| WenlanError::VectorDb(format!("source annotation row: {e}")))?
        else {
            return Ok(None);
        };
        let page_ids_json = row.get::<String>(1).unwrap_or_else(|_| "[]".to_string());
        let page_ids = serde_json::from_str(&page_ids_json)
            .map_err(|e| WenlanError::VectorDb(format!("source annotation page IDs: {e}")))?;
        Ok(Some(SourceAnnotation {
            source: source.to_string(),
            source_id: source_id.to_string(),
            notes: row.get::<String>(0).unwrap_or_default(),
            page_ids,
            version: row.get::<i64>(2).unwrap_or_default().max(0) as u64,
            updated_at: row.get::<i64>(3).unwrap_or_default(),
            pages: Vec::new(),
        }))
    }

    /// Atomically compare-and-set only the supplied fields. Separate note and
    /// Page-list patches share a version so neither can silently overwrite a
    /// concurrent edit to the other field.
    pub async fn write_source_annotation(
        &self,
        source: &str,
        source_id: &str,
        context_key: &str,
        expected_version: u64,
        notes: Option<&str>,
        page_ids: Option<&[String]>,
    ) -> Result<SourceAnnotationWrite, WenlanError> {
        if let Some(notes) = notes {
            if notes.len() > MAX_SOURCE_ANNOTATION_NOTES_BYTES {
                return Err(WenlanError::Validation(format!(
                    "notes exceed {MAX_SOURCE_ANNOTATION_NOTES_BYTES} UTF-8 bytes"
                )));
            }
        }
        if let Some(page_ids) = page_ids {
            if page_ids.len() > MAX_SOURCE_ANNOTATION_PAGES {
                return Err(WenlanError::Validation(format!(
                    "page_ids exceed {MAX_SOURCE_ANNOTATION_PAGES} entries"
                )));
            }
            let unique = page_ids.iter().collect::<std::collections::HashSet<_>>();
            if unique.len() != page_ids.len() {
                return Err(WenlanError::Validation(
                    "page_ids must be unique".to_string(),
                ));
            }
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .min(i64::MAX as u64) as i64;
        let conn = self.conn.lock().await;
        conn.execute("BEGIN IMMEDIATE", ())
            .await
            .map_err(|e| WenlanError::VectorDb(format!("source annotation begin: {e}")))?;

        let result = async {
            let current = {
                let mut rows = conn
                    .query(
                        "SELECT notes, page_ids, version FROM source_annotations \
                         WHERE source = ?1 AND source_id = ?2 AND context_key = ?3",
                        libsql::params![source, source_id, context_key],
                    )
                    .await
                    .map_err(|e| {
                        WenlanError::VectorDb(format!("source annotation CAS read: {e}"))
                    })?;
                rows.next()
                    .await
                    .map_err(|e| WenlanError::VectorDb(format!("source annotation CAS row: {e}")))?
                    .map(|row| {
                        (
                            row.get::<String>(0).unwrap_or_default(),
                            row.get::<String>(1).unwrap_or_else(|_| "[]".to_string()),
                            row.get::<i64>(2).unwrap_or_default().max(0) as u64,
                        )
                    })
            };

            match current {
                Some((old_notes, old_pages_json, current_version)) => {
                    if current_version != expected_version {
                        return Ok(SourceAnnotationWrite::VersionConflict { current_version });
                    }
                    let new_notes = notes.unwrap_or(&old_notes);
                    let old_page_ids: Vec<String> =
                        serde_json::from_str(&old_pages_json).map_err(|e| {
                            WenlanError::VectorDb(format!("source annotation page IDs: {e}"))
                        })?;
                    let new_page_ids = page_ids.unwrap_or(&old_page_ids);
                    let new_page_ids_json = serde_json::to_string(new_page_ids).map_err(|e| {
                        WenlanError::VectorDb(format!("source annotation encode: {e}"))
                    })?;
                    let next_version = current_version.saturating_add(1);
                    let changed = conn
                        .execute(
                            "UPDATE source_annotations SET notes = ?1, page_ids = ?2, \
                             version = ?3, updated_at = ?4 WHERE source = ?5 AND source_id = ?6 \
                             AND context_key = ?7 AND version = ?8",
                            libsql::params![
                                new_notes,
                                new_page_ids_json,
                                next_version as i64,
                                now,
                                source,
                                source_id,
                                context_key,
                                current_version as i64
                            ],
                        )
                        .await
                        .map_err(|e| {
                            WenlanError::VectorDb(format!("source annotation update: {e}"))
                        })?;
                    if changed == 0 {
                        return Ok(SourceAnnotationWrite::VersionConflict { current_version });
                    }
                    Ok(SourceAnnotationWrite::Saved(SourceAnnotation {
                        source: source.to_string(),
                        source_id: source_id.to_string(),
                        notes: new_notes.to_string(),
                        page_ids: new_page_ids.to_vec(),
                        version: next_version,
                        updated_at: now,
                        pages: Vec::new(),
                    }))
                }
                None => {
                    if expected_version != 0 {
                        return Ok(SourceAnnotationWrite::VersionConflict { current_version: 0 });
                    }
                    let notes = notes.unwrap_or_default();
                    let page_ids = page_ids.unwrap_or_default();
                    let page_ids_json = serde_json::to_string(page_ids).map_err(|e| {
                        WenlanError::VectorDb(format!("source annotation encode: {e}"))
                    })?;
                    conn.execute(
                        "INSERT INTO source_annotations \
                         (source, source_id, context_key, notes, page_ids, version, updated_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
                        libsql::params![source, source_id, context_key, notes, page_ids_json, now],
                    )
                    .await
                    .map_err(|e| WenlanError::VectorDb(format!("source annotation insert: {e}")))?;
                    Ok(SourceAnnotationWrite::Saved(SourceAnnotation {
                        source: source.to_string(),
                        source_id: source_id.to_string(),
                        notes: notes.to_string(),
                        page_ids: page_ids.to_vec(),
                        version: 1,
                        updated_at: now,
                        pages: Vec::new(),
                    }))
                }
            }
        }
        .await;

        match result {
            Ok(SourceAnnotationWrite::Saved(annotation)) => {
                commit_or_rollback(&conn)
                    .await
                    .map_err(|e| WenlanError::VectorDb(format!("source annotation commit: {e}")))?;
                Ok(SourceAnnotationWrite::Saved(annotation))
            }
            Ok(conflict @ SourceAnnotationWrite::VersionConflict { .. }) => {
                conn.execute("ROLLBACK", ()).await.map_err(|e| {
                    WenlanError::VectorDb(format!("source annotation rollback: {e}"))
                })?;
                Ok(conflict)
            }
            Err(error) => {
                if let Err(rollback) = conn.execute("ROLLBACK", ()).await {
                    return Err(WenlanError::VectorDb(format!(
                        "source annotation rollback failed after {error}: {rollback}"
                    )));
                }
                Err(error)
            }
        }
    }
}
