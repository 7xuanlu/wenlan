// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::db::MemoryDB;
use crate::error::WenlanError;
use crate::pages::Page;

const HIGH_SOURCE_OVERLAP_MIN: usize = 2;
const HIGH_SOURCE_OVERLAP_RATIO: f64 = 0.67;
const PAGE_SCAN_LIMIT: i64 = 50;
pub(super) const AUTOMATIC_PAIR_BUDGET: usize = 128;
pub(super) const AUTOMATIC_SOURCE_CAP: usize = 256;

#[derive(Debug, Clone)]
struct PageSourceSet {
    page: Page,
    source_ids: HashSet<String>,
}

#[derive(Debug, Clone)]
pub(super) struct NearDuplicatePair {
    pub(super) left_id: String,
    pub(super) right_id: String,
    pub(super) similarity: Option<f64>,
    pub(super) source_overlap: usize,
    pub(super) source_overlap_ratio: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub(super) struct NearDuplicateCursor {
    pub(super) left_id: String,
    pub(super) right_id: String,
}

#[derive(Debug)]
pub(super) struct NearDuplicateSlice {
    pub(super) candidate: Option<NearDuplicatePair>,
    pub(super) next_cursor: Option<NearDuplicateCursor>,
    pub(super) more: bool,
    pub(super) pairs_examined: usize,
    pub(super) pages_examined: usize,
    pub(super) source_rows_examined: usize,
    pub(super) truncated: bool,
}

#[derive(Debug)]
struct BoundedPairRow {
    left_id: String,
    right_id: String,
    left_embedding: Vec<f32>,
    right_embedding: Vec<f32>,
    left_fallback_sources: Vec<String>,
    right_fallback_sources: Vec<String>,
    eligible: bool,
}

/// Scan a stable keyset window of Page pairs. Unlike the foreground ranking
/// query below, distance is computed only after the pair window has been
/// bounded. Source evidence is independently capped per Page; an overflow is
/// never treated as partial overlap because that could create false-positive
/// merge cards.
pub(super) async fn scan_near_duplicate_slice(
    db: &MemoryDB,
    page_match_threshold: f64,
    cursor: Option<&NearDuplicateCursor>,
) -> Result<NearDuplicateSlice, WenlanError> {
    let conn = db.conn.lock().await;
    let mut sql = String::from(
        "SELECT a.id, b.id, a.embedding, b.embedding, \
                a.source_memory_ids, b.source_memory_ids, \
                CASE WHEN a.status = 'active' \
                           AND b.status = 'active' \
                           AND COALESCE(a.review_status, 'confirmed') = 'confirmed' \
                           AND COALESCE(b.review_status, 'confirmed') = 'confirmed' \
                           AND COALESCE(a.workspace, a.space, '') = COALESCE(b.workspace, b.space, '') \
                           AND lower(a.title) != 'overview' \
                           AND lower(b.title) != 'overview' \
                     THEN 1 ELSE 0 END AS eligible \
         FROM pages a \
         JOIN pages b ON a.id < b.id \
         WHERE 1 = 1",
    );
    let mut bind = Vec::<libsql::Value>::new();
    if let Some(cursor) = cursor {
        sql.push_str(" AND (a.id > ? OR (a.id = ? AND b.id > ?))");
        bind.push(libsql::Value::Text(cursor.left_id.clone()));
        bind.push(libsql::Value::Text(cursor.left_id.clone()));
        bind.push(libsql::Value::Text(cursor.right_id.clone()));
    }
    sql.push_str(" ORDER BY a.id, b.id LIMIT ?");
    bind.push(libsql::Value::Integer(AUTOMATIC_PAIR_BUDGET as i64));

    let mut rows = conn
        .query(&sql, libsql::params_from_iter(bind))
        .await
        .map_err(|error| WenlanError::VectorDb(format!("bounded Page pair scan: {error}")))?;
    let mut pair_rows = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|error| WenlanError::VectorDb(format!("bounded Page pair row: {error}")))?
    {
        let decode_embedding = |index| {
            row.get::<Vec<u8>>(index)
                .unwrap_or_default()
                .chunks_exact(4)
                .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                .collect::<Vec<_>>()
        };
        let decode_sources = |index| {
            row.get::<String>(index)
                .ok()
                .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
                .unwrap_or_default()
        };
        pair_rows.push(BoundedPairRow {
            left_id: row.get::<String>(0).unwrap_or_default(),
            right_id: row.get::<String>(1).unwrap_or_default(),
            left_embedding: decode_embedding(2),
            right_embedding: decode_embedding(3),
            left_fallback_sources: decode_sources(4),
            right_fallback_sources: decode_sources(5),
            eligible: row.get::<i64>(6).unwrap_or(0) != 0,
        });
    }
    drop(rows);

    let mut fallback_sources = HashMap::<String, Vec<String>>::new();
    for pair in &pair_rows {
        if !pair.eligible {
            continue;
        }
        fallback_sources
            .entry(pair.left_id.clone())
            .or_insert_with(|| pair.left_fallback_sources.clone());
        fallback_sources
            .entry(pair.right_id.clone())
            .or_insert_with(|| pair.right_fallback_sources.clone());
    }

    let mut source_sets = HashMap::<String, (HashSet<String>, bool)>::new();
    let mut source_rows_examined = 0usize;
    let mut truncated = false;
    for (page_id, fallback) in fallback_sources {
        let mut source_rows = conn
            .query(
                "SELECT memory_source_id FROM page_sources \
                 WHERE page_id = ?1 ORDER BY memory_source_id LIMIT ?2",
                libsql::params![page_id.as_str(), (AUTOMATIC_SOURCE_CAP + 1) as i64],
            )
            .await
            .map_err(|error| {
                WenlanError::VectorDb(format!("bounded Page sources for '{page_id}': {error}"))
            })?;
        let mut source_ids = Vec::new();
        while let Some(row) = source_rows.next().await.map_err(|error| {
            WenlanError::VectorDb(format!("bounded Page source row for '{page_id}': {error}"))
        })? {
            source_ids.push(row.get::<String>(0).unwrap_or_default());
        }
        if source_ids.is_empty() {
            source_ids.extend(fallback.into_iter().take(AUTOMATIC_SOURCE_CAP + 1));
        }
        source_rows_examined += source_ids.len();
        let page_truncated = source_ids.len() > AUTOMATIC_SOURCE_CAP;
        truncated |= page_truncated;
        source_ids.truncate(AUTOMATIC_SOURCE_CAP);
        source_sets.insert(page_id, (source_ids.into_iter().collect(), page_truncated));
    }

    let pages_examined = source_sets.len();
    let pairs_examined = pair_rows.len();
    let mut next_cursor = None;
    let mut candidate = None;
    let mut stopped_early = false;
    for (index, pair) in pair_rows.iter().enumerate() {
        next_cursor = Some(NearDuplicateCursor {
            left_id: pair.left_id.clone(),
            right_id: pair.right_id.clone(),
        });
        if !pair.eligible {
            continue;
        }
        let similarity = (!pair.left_embedding.is_empty() && !pair.right_embedding.is_empty())
            .then(|| crate::db::cosine_similarity(&pair.left_embedding, &pair.right_embedding));
        let (left_sources, left_truncated) = source_sets
            .get(&pair.left_id)
            .expect("every bounded pair has a left source set");
        let (right_sources, right_truncated) = source_sets
            .get(&pair.right_id)
            .expect("every bounded pair has a right source set");
        let (source_overlap, source_overlap_ratio) = if *left_truncated || *right_truncated {
            (0, 0.0)
        } else {
            let overlap = left_sources.intersection(right_sources).count();
            let smaller = left_sources.len().min(right_sources.len());
            let ratio = if smaller == 0 {
                0.0
            } else {
                overlap as f64 / smaller as f64
            };
            (overlap, ratio)
        };
        let embedding_match = similarity.is_some_and(|value| value >= page_match_threshold);
        let source_match = source_overlap >= HIGH_SOURCE_OVERLAP_MIN
            && source_overlap_ratio >= HIGH_SOURCE_OVERLAP_RATIO;
        if embedding_match || source_match {
            candidate = Some(NearDuplicatePair {
                left_id: pair.left_id.clone(),
                right_id: pair.right_id.clone(),
                similarity,
                source_overlap,
                source_overlap_ratio,
            });
            stopped_early = index + 1 < pair_rows.len();
            break;
        }
    }
    let more = stopped_early || pair_rows.len() == AUTOMATIC_PAIR_BUDGET;

    Ok(NearDuplicateSlice {
        candidate,
        next_cursor,
        more,
        pairs_examined,
        pages_examined,
        source_rows_examined,
        truncated,
    })
}

pub(super) async fn detect_near_duplicate_pages(
    db: &MemoryDB,
    page_match_threshold: f64,
    limit: usize,
) -> Result<Vec<NearDuplicatePair>, WenlanError> {
    detect_near_duplicate_pages_inner(db, page_match_threshold, Some(limit)).await
}

pub(super) async fn detect_all_near_duplicate_pages(
    db: &MemoryDB,
    page_match_threshold: f64,
) -> Result<Vec<NearDuplicatePair>, WenlanError> {
    detect_near_duplicate_pages_inner(db, page_match_threshold, None).await
}

async fn detect_near_duplicate_pages_inner(
    db: &MemoryDB,
    page_match_threshold: f64,
    limit: Option<usize>,
) -> Result<Vec<NearDuplicatePair>, WenlanError> {
    let mut pairs: HashMap<(String, String), NearDuplicatePair> = HashMap::new();
    for pair in embedding_near_duplicate_pairs(db, page_match_threshold, limit).await? {
        pairs.insert((pair.left_id.clone(), pair.right_id.clone()), pair);
    }

    for pair in source_overlap_pairs(db, limit).await? {
        pairs
            .entry((pair.left_id.clone(), pair.right_id.clone()))
            .and_modify(|existing| {
                existing.source_overlap = existing.source_overlap.max(pair.source_overlap);
                existing.source_overlap_ratio =
                    existing.source_overlap_ratio.max(pair.source_overlap_ratio);
            })
            .or_insert(pair);
    }

    let mut out: Vec<NearDuplicatePair> = pairs.into_values().collect();
    out.sort_by(|left, right| {
        let l = left.similarity.unwrap_or(left.source_overlap_ratio);
        let r = right.similarity.unwrap_or(right.source_overlap_ratio);
        r.partial_cmp(&l).unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(limit) = limit {
        out.truncate(limit);
    }
    Ok(out)
}

async fn embedding_near_duplicate_pairs(
    db: &MemoryDB,
    page_match_threshold: f64,
    limit: Option<usize>,
) -> Result<Vec<NearDuplicatePair>, WenlanError> {
    let conn = db.conn.lock().await;
    let sql = match limit {
        Some(_) => {
            "SELECT a.id, b.id, vector_distance_cos(a.embedding, b.embedding) AS dist \
             FROM pages a \
             JOIN pages b ON a.id < b.id \
             WHERE a.status = 'active' \
               AND b.status = 'active' \
               AND a.embedding IS NOT NULL \
               AND b.embedding IS NOT NULL \
               AND COALESCE(a.review_status, 'confirmed') = 'confirmed' \
               AND COALESCE(b.review_status, 'confirmed') = 'confirmed' \
               AND a.space = b.space \
               AND lower(a.title) != 'overview' \
               AND lower(b.title) != 'overview' \
               AND vector_distance_cos(a.embedding, b.embedding) <= ?1 \
             ORDER BY dist ASC \
             LIMIT ?2"
        }
        None => {
            "SELECT a.id, b.id, vector_distance_cos(a.embedding, b.embedding) AS dist \
             FROM pages a \
             JOIN pages b ON a.id < b.id \
             WHERE a.status = 'active' \
               AND b.status = 'active' \
               AND a.embedding IS NOT NULL \
               AND b.embedding IS NOT NULL \
               AND COALESCE(a.review_status, 'confirmed') = 'confirmed' \
               AND COALESCE(b.review_status, 'confirmed') = 'confirmed' \
               AND a.space = b.space \
               AND lower(a.title) != 'overview' \
               AND lower(b.title) != 'overview' \
               AND vector_distance_cos(a.embedding, b.embedding) <= ?1 \
             ORDER BY dist ASC"
        }
    };
    let threshold = (1.0 - page_match_threshold).max(0.0);
    let mut rows = match limit {
        Some(limit) => {
            conn.query(sql, libsql::params![threshold, limit as i64])
                .await
        }
        None => conn.query(sql, libsql::params![threshold]).await,
    }
    .map_err(|e| WenlanError::VectorDb(format!("page near-duplicate query: {e}")))?;

    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| WenlanError::VectorDb(format!("page near-duplicate row: {e}")))?
    {
        let left: String = row
            .get(0)
            .map_err(|e| WenlanError::VectorDb(format!("near-dup left id: {e}")))?;
        let right: String = row
            .get(1)
            .map_err(|e| WenlanError::VectorDb(format!("near-dup right id: {e}")))?;
        let dist: f64 = row.get(2).unwrap_or(1.0);
        out.push(NearDuplicatePair {
            left_id: left,
            right_id: right,
            similarity: Some(1.0 - dist),
            source_overlap: 0,
            source_overlap_ratio: 0.0,
        });
    }
    Ok(out)
}

async fn source_overlap_pairs(
    db: &MemoryDB,
    limit: Option<usize>,
) -> Result<Vec<NearDuplicatePair>, WenlanError> {
    let pages = list_page_source_sets(db, limit.map(|n| n as i64)).await?;
    let mut pairs = Vec::new();
    for (index, left) in pages.iter().enumerate() {
        for right in pages.iter().skip(index + 1) {
            if page_workspace(&left.page) != page_workspace(&right.page) {
                continue;
            }
            let overlap = left.source_ids.intersection(&right.source_ids).count();
            let smaller = left.source_ids.len().min(right.source_ids.len());
            if smaller == 0 {
                continue;
            }
            let ratio = overlap as f64 / smaller as f64;
            if overlap >= HIGH_SOURCE_OVERLAP_MIN && ratio >= HIGH_SOURCE_OVERLAP_RATIO {
                pairs.push(NearDuplicatePair {
                    left_id: left.page.id.clone(),
                    right_id: right.page.id.clone(),
                    similarity: None,
                    source_overlap: overlap,
                    source_overlap_ratio: ratio,
                });
            }
            if limit.is_some_and(|limit| pairs.len() >= limit) {
                return Ok(pairs);
            }
        }
    }
    Ok(pairs)
}

async fn list_page_source_sets(
    db: &MemoryDB,
    limit: Option<i64>,
) -> Result<Vec<PageSourceSet>, WenlanError> {
    let pages = db
        .list_pages("active", limit.unwrap_or(i64::MAX).max(PAGE_SCAN_LIMIT), 0)
        .await?;
    let mut out = Vec::new();
    for page in pages {
        if page.title.eq_ignore_ascii_case("overview") || page.review_status != "confirmed" {
            continue;
        }
        let sources = db.get_page_sources(&page.id).await?;
        let ids: HashSet<String> = if sources.is_empty() {
            page.source_memory_ids.iter().cloned().collect()
        } else {
            sources.into_iter().map(|s| s.memory_source_id).collect()
        };
        out.push(PageSourceSet {
            page,
            source_ids: ids,
        });
    }
    Ok(out)
}

fn page_workspace(page: &Page) -> Option<&str> {
    page.workspace.as_deref().or(page.space.as_deref())
}
