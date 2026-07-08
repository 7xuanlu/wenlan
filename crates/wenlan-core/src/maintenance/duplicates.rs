// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::db::MemoryDB;
use crate::error::WenlanError;
use crate::pages::Page;

const HIGH_SOURCE_OVERLAP_MIN: usize = 2;
const HIGH_SOURCE_OVERLAP_RATIO: f64 = 0.67;
const PAGE_SCAN_LIMIT: i64 = 50;

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
               AND COALESCE(a.workspace, a.space, '') = COALESCE(b.workspace, b.space, '') \
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
               AND COALESCE(a.workspace, a.space, '') = COALESCE(b.workspace, b.space, '') \
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
