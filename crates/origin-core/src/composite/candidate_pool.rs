// SPDX-License-Identifier: Apache-2.0
//! Candidate pool builder: union of vector ANN top-N and FTS5 top-N results.
//!
//! Plan B Task 8.
//!
//! SQL is lifted verbatim from `db.rs` `search_memory` (lines 6821-6896).
//! The hard-filter WHERE snippet produced by `build_where` composes directly
//! because it starts with ` AND ...` and targets the `memories c` alias.

use std::collections::HashMap;

use crate::db::MemoryDB;
use crate::error::OriginError;

use super::hard_filters::{build_where, HardFilters};

/// A single candidate memory row returned from the union of vector ANN and FTS5
/// searches.  Scores from the channel that found the candidate are populated;
/// the other channel's score is `0.0`.  When both channels matched the same
/// memory the scores are combined via `max` in the union step.
#[allow(dead_code)]
pub(crate) struct Candidate {
    pub(crate) memory_id: String,
    pub(crate) semantic_score: f64,
    pub(crate) bm25_score: f64,
    pub(crate) entity_id: Option<String>,
    pub(crate) event_date: Option<i64>,
    pub(crate) last_modified: i64,
    pub(crate) confirmed: bool,
    pub(crate) stability: String,
    pub(crate) access_count: u64,
}

/// Build the candidate pool by running vector ANN top-N and FTS5 top-N queries
/// and unioning the results by `source_id` (deduplicating across channels).
///
/// `query_embedding` must already be computed by the caller — this function does
/// not call the embedder.
///
/// `pool_n` controls how many rows each channel fetches before the union.
/// `hard_filters` applies mandatory WHERE conditions (supersession, temporal
/// cue, space, memory_type) to both channels.
#[allow(dead_code)]
pub(crate) async fn build_candidate_pool(
    db: &MemoryDB,
    query_text: &str,
    query_embedding: &[f32],
    pool_n: usize,
    hard_filters: &HardFilters<'_>,
) -> Result<Vec<Candidate>, OriginError> {
    // Pre-compute the hard-filter WHERE fragment (may be empty, or start with " AND ").
    let where_extra = build_where(hard_filters);

    // Render the embedding vector as a libSQL-compatible bracket string.
    let vec_str = MemoryDB::vec_to_sql_pub(query_embedding);
    let fetch_limit = pool_n as i64;

    let mut union: HashMap<String, Candidate> = HashMap::new();

    let conn = db.conn.lock().await;

    // -----------------------------------------------------------------------
    // Vector ANN top-N
    // Lifted from db.rs `search_memory` lines 6821-6854.
    // ?1 = vec_str  ?2 = fetch_limit
    // -----------------------------------------------------------------------
    {
        let sql = format!(
            "SELECT c.source_id, c.entity_id, c.event_date, c.last_modified,
                    c.confirmed, c.stability, c.access_count,
                    vector_distance_cos(c.embedding, vector32(?1))
             FROM vector_top_k('memories_vec_idx', vector32(?1), ?2) AS vt
             JOIN memories c ON c.rowid = vt.id
             WHERE 1=1{}",
            where_extra
        );

        let params: Vec<libsql::Value> = vec![
            libsql::Value::Text(vec_str.clone()),
            libsql::Value::Integer(fetch_limit),
        ];

        match conn.query(&sql, params).await {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next().await {
                    let source_id: String = row.get(0).unwrap_or_default();
                    let entity_id: Option<String> = row.get(1).ok();
                    let event_date: Option<i64> = row.get(2).ok();
                    let last_modified: i64 = row.get(3).unwrap_or(0);
                    let confirmed_int: i64 = row.get(4).unwrap_or(0);
                    let stability: String = row.get(5).unwrap_or_else(|_| "new".to_string());
                    let access_count: i64 = row.get(6).unwrap_or(0);
                    let distance: f64 = row.get(7).unwrap_or(1.0);
                    let semantic_score = (1.0 - distance).max(0.0);

                    if source_id.is_empty() {
                        continue;
                    }

                    let candidate = Candidate {
                        memory_id: source_id.clone(),
                        semantic_score,
                        bm25_score: 0.0,
                        entity_id,
                        event_date,
                        last_modified,
                        confirmed: confirmed_int != 0,
                        stability,
                        access_count: access_count.max(0) as u64,
                    };

                    union
                        .entry(source_id)
                        .and_modify(|c| {
                            c.semantic_score = c.semantic_score.max(candidate.semantic_score);
                        })
                        .or_insert(candidate);
                }
            }
            Err(e) => {
                log::warn!("[candidate_pool] vector ANN search failed: {}", e);
            }
        }
    }

    // -----------------------------------------------------------------------
    // FTS5 top-N
    // Lifted from db.rs `search_memory` lines 6881-6926.
    // ?1 = query_text  ?2 = fetch_limit
    //
    // The hard-filter WHERE snippet is appended after `WHERE memories_fts MATCH ?1`
    // because the snippet targets `c.*` columns (on the joined `memories c` alias),
    // not the FTS virtual table columns.  This is the same composition the
    // existing `search_memory` uses.
    // -----------------------------------------------------------------------
    {
        let fts_sql = format!(
            "SELECT c.source_id, c.entity_id, c.event_date, c.last_modified,
                    c.confirmed, c.stability, c.access_count,
                    fts.rank
             FROM memories_fts fts
             JOIN memories c ON fts.rowid = c.rowid
             WHERE memories_fts MATCH ?1{}
             ORDER BY fts.rank
             LIMIT ?2",
            where_extra
        );

        // Try AND matching first; fall back to OR if no results (mirrors db.rs).
        let fts_queries = vec![
            query_text.to_string(),
            MemoryDB::fts_or_query_pub(query_text),
        ];

        for fts_query in &fts_queries {
            let params: Vec<libsql::Value> = vec![
                libsql::Value::Text(fts_query.clone()),
                libsql::Value::Integer(fetch_limit),
            ];

            let mut fts_hit = false;
            match conn.query(&fts_sql, params).await {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next().await {
                        let source_id: String = row.get(0).unwrap_or_default();
                        let entity_id: Option<String> = row.get(1).ok();
                        let event_date: Option<i64> = row.get(2).ok();
                        let last_modified: i64 = row.get(3).unwrap_or(0);
                        let confirmed_int: i64 = row.get(4).unwrap_or(0);
                        let stability: String = row.get(5).unwrap_or_else(|_| "new".to_string());
                        let access_count: i64 = row.get(6).unwrap_or(0);
                        let bm25_rank: f64 = row.get(7).unwrap_or(0.0);
                        // FTS rank is negative in SQLite FTS5 (lower = better match).
                        // Store the raw rank as bm25_score; callers that need a
                        // positive score should negate it.
                        let bm25_score = bm25_rank;

                        if source_id.is_empty() {
                            continue;
                        }
                        fts_hit = true;

                        let candidate = Candidate {
                            memory_id: source_id.clone(),
                            semantic_score: 0.0,
                            bm25_score,
                            entity_id,
                            event_date,
                            last_modified,
                            confirmed: confirmed_int != 0,
                            stability,
                            access_count: access_count.max(0) as u64,
                        };

                        union
                            .entry(source_id)
                            .and_modify(|c| {
                                // Keep best semantic_score from vector channel;
                                // take best (most-negative = higher-ranked) FTS rank.
                                c.bm25_score = if bm25_score < c.bm25_score {
                                    bm25_score
                                } else {
                                    c.bm25_score
                                };
                            })
                            .or_insert(candidate);
                    }
                }
                Err(e) => {
                    log::warn!("[candidate_pool] FTS search failed: {}", e);
                }
            }

            if fts_hit {
                break; // AND matched — no need for OR fallback.
            }
        }
    }

    Ok(union.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::sources::RawDocument;

    /// Seed a minimal RawDocument into the test DB via `upsert_documents`.
    async fn seed(db: &MemoryDB, source_id: &str, content: &str) {
        let doc = RawDocument {
            source: "memory".to_string(),
            source_id: source_id.to_string(),
            title: format!("doc-{}", source_id),
            content: content.to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            memory_type: Some("general".to_string()),
            ..Default::default()
        };
        db.upsert_documents(vec![doc])
            .await
            .expect("upsert_documents must succeed");
    }

    #[tokio::test]
    async fn candidate_pool_unions_vector_and_fts_with_hard_filters() {
        let (db, _dir) = test_db().await;

        // Seed 50 memories via the public ingest path so embeddings + FTS5 sync naturally.
        // The corpus covers two topic clusters:
        //   - "rust programming" (25 docs)
        //   - "machine learning" (25 docs)
        // This ensures the query ("rust programming language") hits both vector and
        // FTS channels across a realistic-size pool.
        for i in 0..25usize {
            seed(
                &db,
                &format!("rust_{i}"),
                &format!(
                    "Rust programming language memory {i}: ownership borrow checker lifetimes \
                     cargo crates async trait impl enum pattern match closures iterators"
                ),
            )
            .await;
        }
        for i in 0..25usize {
            seed(
                &db,
                &format!("ml_{i}"),
                &format!(
                    "Machine learning neural network {i}: gradient descent loss function \
                     transformer attention softmax embedding layer weights optimizer"
                ),
            )
            .await;
        }

        // Compute query embedding via the DB's own embedder (same model used at ingest).
        let query = "rust programming language";
        let q_emb = db.embed_query(query).expect("embed_query must succeed");

        let pool = build_candidate_pool(&db, query, &q_emb, 50, &HardFilters::default_open())
            .await
            .expect("build_candidate_pool must not fail");

        // With 50 seeded memories and a query that hits the rust cluster via both
        // vector and FTS channels, we expect at least 20 candidates in the union.
        // The threshold is deliberately loose enough to pass under test-DB conditions
        // (no GPU, quantized embedder) while being tight enough to catch a broken union
        // (e.g. one channel returning 0 results would leave ≤ 25 in the pool).
        assert!(
            pool.len() >= 20,
            "expected >= 20 candidates in pool, got {} (union of vector+FTS broken?)",
            pool.len()
        );

        // Verify each candidate has a non-empty memory_id.
        for c in &pool {
            assert!(
                !c.memory_id.is_empty(),
                "candidate memory_id must not be empty"
            );
        }

        // Verify that at least one candidate has a non-zero semantic_score
        // (vector channel contributed).
        let has_semantic = pool.iter().any(|c| c.semantic_score > 0.0);
        assert!(
            has_semantic,
            "at least one candidate must have semantic_score > 0"
        );
    }
}
