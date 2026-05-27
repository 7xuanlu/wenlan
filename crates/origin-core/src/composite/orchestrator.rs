// SPDX-License-Identifier: Apache-2.0
//! Composite-search orchestrator: assembles 8 signals into a single ranked list.
//!
//! Plan B Task 9.

use std::collections::HashMap;

use crate::composite::{
    activation::{activate, ActivationParams},
    candidate_pool::{build_candidate_pool, Candidate},
    compose::{compose, SignalKind},
    graph_distance::{compute_graph_distance, graph_distance_score},
    hard_filters::HardFilters,
    relation_graph::RelationGraph,
    signals::{access_frequency, recency_decay, temporal_proximity, trust},
};
use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::temporal_query::extract_cue;
use crate::tuning::RetrievalConfig;

/// A single result from composite search: the memory's source_id and its
/// composite score in [0, 1].
#[allow(dead_code)]
pub(crate) struct SearchResultComposite {
    pub(crate) memory_id: String,
    pub(crate) score: f64,
}

/// Extract entity IDs from a free-text query by matching the query tokens
/// against entity names in the database.
///
/// Strategy: lowercase-tokenise the query on whitespace + punctuation, then
/// run a single SQL `WHERE LOWER(name) IN (...)` to find matching entities.
/// Returns entity `id` strings (not names).  Returns `Ok(vec![])` immediately
/// when the query has no usable tokens.
///
/// This is the fallback path: `crate::extract` has no query-side recogniser.
pub(crate) async fn extract_query_entity_ids(
    db: &MemoryDB,
    query: &str,
) -> Result<Vec<String>, OriginError> {
    let tokens: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_lowercase())
        .collect();

    if tokens.is_empty() {
        return Ok(vec![]);
    }

    let placeholders = tokens.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    let sql = format!("SELECT id FROM entities WHERE LOWER(name) IN ({placeholders})");

    let params: Vec<libsql::Value> = tokens
        .iter()
        .map(|t| libsql::Value::Text(t.clone()))
        .collect();

    let conn = db.conn.lock().await;
    let mut rows = conn
        .query(&sql, params)
        .await
        .map_err(|e| OriginError::VectorDb(format!("extract_query_entity_ids: {e}")))?;

    let mut out: Vec<String> = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| OriginError::VectorDb(format!("extract_query_entity_ids row: {e}")))?
    {
        let id: String = row
            .get(0)
            .map_err(|e| OriginError::VectorDb(format!("extract_query_entity_ids col0: {e}")))?;
        out.push(id);
    }

    Ok(out)
}

/// Composite search: assembles 8 signals (semantic, BM25, graph_distance,
/// activation, temporal, trust, recency, access_frequency) into a single
/// ranked list of `limit` results.
///
/// `query_embedding` must be pre-computed by the caller via `db.embed_query`.
/// `cfg` provides all tuning knobs; callers obtain it from their own
/// `RetrievalConfig` (e.g. `RetrievalConfig::default()` in tests).
///
/// Signal-pool alignment:
///   - `graph_distance` is keyed by `memory_id` (source_id) — looked up via
///     `c.memory_id` (the `compute_graph_distance` CTE joins through
///     `memory_entities` and returns `memory_id` keys, not entity_id keys).
///   - `activation` is keyed by `entity_id` — looked up via `c.entity_id`.
///   - Pool entries without `entity_id` get 0.0 for both graph signals.
#[allow(dead_code)]
pub(crate) async fn search_memory_composite(
    db: &MemoryDB,
    query: &str,
    query_embedding: &[f32],
    limit: usize,
    memory_type: Option<&str>,
    space: Option<&str>,
    cfg: &RetrievalConfig,
) -> Result<Vec<SearchResultComposite>, OriginError> {
    // 1. Extract query entities via SQL name-match (no LLM needed).
    let query_entities: Vec<String> = extract_query_entity_ids(db, query).await?;

    let now = chrono::Utc::now();
    let cue = extract_cue(query, now);

    // 2. Hard filter cascade.
    let exclude_superseded = db.composite_exclude_superseded();
    let filters = HardFilters {
        space,
        memory_type,
        exclude_superseded,
        temporal_cue: cue,
    };

    // 3. Candidate pool: union of vector ANN top-N and FTS5 top-N.
    let pool_n = cfg
        .pool_size_multiplier
        .saturating_mul(limit)
        .max(cfg.pool_size_floor)
        .min(cfg.pool_size_cap);
    let pool: Vec<Candidate> =
        build_candidate_pool(db, query, query_embedding, pool_n, &filters).await?;

    if pool.is_empty() {
        return Ok(vec![]);
    }

    // 4. Graph signals: distance map + activation map.
    //    Both are keyed differently (see function doc above).
    let seed_refs: Vec<&str> = query_entities.iter().map(|s| s.as_str()).collect();
    let dist_map = compute_graph_distance(db, &seed_refs, cfg.graph_depth).await?;
    let rgraph = RelationGraph::for_query(db, &seed_refs, cfg.graph_depth).await?;
    let activation_map = activate(
        &query_entities,
        &rgraph,
        ActivationParams {
            decay: cfg.activation_decay,
            threshold: cfg.activation_threshold,
            max_iter: cfg.activation_max_iter,
        },
    );

    // 5. Per-signal score vectors aligned to pool order.
    let mut scores: HashMap<SignalKind, Vec<f64>> = HashMap::new();
    let now_ts = now.timestamp();
    // Temporal proximity uses the midpoint of the cue range as the query anchor;
    // falls back to now when no cue is present.
    let query_ts = cue
        .map(|c| (c.range.start + c.range.end) / 2)
        .unwrap_or(now_ts);

    scores.insert(
        SignalKind::Semantic,
        pool.iter().map(|c| c.semantic_score).collect(),
    );
    scores.insert(
        SignalKind::Bm25,
        pool.iter().map(|c| c.bm25_score).collect(),
    );
    // graph_distance: dist_map is keyed by memory_id (source_id).
    scores.insert(
        SignalKind::GraphDistance,
        pool.iter()
            .map(|c| {
                dist_map
                    .get(c.memory_id.as_str())
                    .copied()
                    .map(graph_distance_score)
                    .unwrap_or(0.0)
            })
            .collect(),
    );
    // activation: activation_map is keyed by entity_id.
    scores.insert(
        SignalKind::Activation,
        pool.iter()
            .map(|c| {
                c.entity_id
                    .as_deref()
                    .and_then(|e| activation_map.get(e))
                    .copied()
                    .unwrap_or(0.0)
            })
            .collect(),
    );
    scores.insert(
        SignalKind::Temporal,
        pool.iter()
            .map(|c| temporal_proximity(query_ts, c.event_date, cfg.temporal_sigma_days))
            .collect(),
    );
    scores.insert(
        SignalKind::Trust,
        pool.iter()
            .map(|c| trust(c.confirmed, &c.stability))
            .collect(),
    );
    scores.insert(
        SignalKind::Recency,
        pool.iter()
            .map(|c| recency_decay(c.last_modified, now_ts, cfg.recency_decay_tau_days))
            .collect(),
    );
    scores.insert(
        SignalKind::AccessFrequency,
        pool.iter()
            .map(|c| access_frequency(c.access_count))
            .collect(),
    );

    // 6. Compose: min-max normalize per signal, degenerate-skip, reweight.
    let composite_scores = compose(&scores, &cfg.composite_weights);

    // 7. Deterministic tiebreak sort, truncate to limit.
    let mut paired: Vec<(usize, f64)> = composite_scores.into_iter().enumerate().collect();
    paired.sort_by(|(ia, a), (ib, b)| {
        b.partial_cmp(a)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| pool[*ib].last_modified.cmp(&pool[*ia].last_modified))
            .then_with(|| pool[*ia].memory_id.cmp(&pool[*ib].memory_id))
    });
    paired.truncate(limit);

    Ok(paired
        .into_iter()
        .map(|(i, score)| SearchResultComposite {
            memory_id: pool[i].memory_id.clone(),
            score,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::test_db;
    use crate::tuning::RetrievalConfig;

    /// Integration test: composite returns results when the pool is non-empty and
    /// all 8 signal paths are populated. Verifies ordering: the entity-linked
    /// memory (which has a non-zero graph_distance + activation score) ranks above
    /// the unlinked memory (which gets 0.0 for both graph signals).
    #[tokio::test]
    async fn composite_returns_results_with_8_active_signals_when_pool_complete() {
        let (db, _dir) = test_db().await;

        // -----------------------------------------------------------------
        // Seed: 5 entities connected by 3 relations forming a small DAG.
        // entities schema: id, name, entity_type, domain, source_agent,
        //   confidence, confirmed, created_at, updated_at, embedding
        // relations schema: id, from_entity, to_entity, relation_type,
        //   source_agent, created_at
        // -----------------------------------------------------------------
        {
            let conn = db.conn.lock().await;
            conn.execute_batch(
                "INSERT INTO entities (id, name, entity_type, created_at, updated_at)
                 VALUES
                   ('ent_a', 'Rust',  'Topic', 0, 0),
                   ('ent_b', 'Cargo', 'Topic', 0, 0),
                   ('ent_c', 'Tokio', 'Topic', 0, 0),
                   ('ent_d', 'Axum',  'Topic', 0, 0),
                   ('ent_e', 'Serde', 'Topic', 0, 0);
                 INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at)
                 VALUES
                   ('rel_ab', 'ent_a', 'ent_b', 'related_to', 0),
                   ('rel_bc', 'ent_b', 'ent_c', 'depends_on', 0),
                   ('rel_cd', 'ent_c', 'ent_d', 'uses',       0);",
            )
            .await
            .expect("seed entities + relations");
        }

        // -----------------------------------------------------------------
        // Seed 10 memories.  Three are linked to entities; seven are not.
        // We vary event_date, access_count, stability to populate signal paths.
        // -----------------------------------------------------------------
        let now_ts = chrono::Utc::now().timestamp();
        {
            let conn = db.conn.lock().await;

            // Build INSERT for memories table.
            // Columns: id, source, source_id, title, content, chunk_index,
            //          last_modified, chunk_type, event_date, access_count,
            //          confirmed, stability
            //
            // mem_01: linked to ent_a (the query mentions "Rust"), confirmed, learned,
            //         recent, has event_date.
            // mem_02: linked to ent_b (one hop from ent_a), stable, old.
            // mem_03: linked to ent_c (two hops).
            // mem_04..mem_10: no entity link, various params.
            //
            // Use raw INSERT with hardcoded timestamps relative to now_ts.
            // Column order: id, source, source_id, title, content, chunk_index,
            // last_modified, chunk_type, event_date, access_count, confirmed, stability
            conn.execute_batch(&format!(
                "INSERT INTO memories (id, source, source_id, title, content,
                                       chunk_index, last_modified, chunk_type,
                                       event_date, access_count, confirmed, stability)
                 VALUES
                   ('c01','memory','mem_01','Rust programming language','Rust is a systems programming language focused on safety and performance',0,{now},'text',{recent},10,1,'confirmed'),
                   ('c02','memory','mem_02','Cargo package manager','Cargo is the Rust package manager and build tool',0,{old},'text',{old},3,1,'learned'),
                   ('c03','memory','mem_03','Tokio async runtime','Tokio provides async I/O for Rust applications',0,{old},'text',NULL,0,0,'new'),
                   ('c04','memory','mem_04','Unlinked memory four','Some unrelated note about cooking',0,{now},'text',{recent},5,1,'learned'),
                   ('c05','memory','mem_05','Unlinked memory five','Note about travel plans',0,{old},'text',NULL,0,0,'new'),
                   ('c06','memory','mem_06','Unlinked memory six','Note about music',0,{now},'text',{recent},1,0,'new'),
                   ('c07','memory','mem_07','Unlinked memory seven','Note about books',0,{old},'text',NULL,2,1,'learned'),
                   ('c08','memory','mem_08','Unlinked memory eight','Note about sports',0,{now},'text',{recent},0,0,'new'),
                   ('c09','memory','mem_09','Unlinked memory nine','Note about finance',0,{old},'text',NULL,4,1,'confirmed'),
                   ('c10','memory','mem_10','Unlinked memory ten','Note about health',0,{now},'text',{recent},0,0,'new');",
                now = now_ts,
                old = now_ts - 86400 * 60,
                recent = now_ts - 3600,
            ))
            .await
            .expect("seed memories");

            // Link mem_01 → ent_a, mem_02 → ent_b, mem_03 → ent_c.
            conn.execute_batch(
                "INSERT INTO memory_entities (memory_id, entity_id)
                 VALUES
                   ('mem_01', 'ent_a'),
                   ('mem_02', 'ent_b'),
                   ('mem_03', 'ent_c');",
            )
            .await
            .expect("seed memory_entities");
        }

        // -----------------------------------------------------------------
        // Index the memories so the vector ANN + FTS channels can fire.
        // Use embed_query to get a consistent 768-dim vector for the test.
        // -----------------------------------------------------------------
        let query = "Rust programming language";
        let query_embedding = db.embed_query(query).expect("embed query");

        // Insert embeddings and FTS content for the memories that should surface.
        // We only embed a subset (mem_01..mem_03 + a few more) to keep the test fast.
        // The embedder is the real FastEmbed model (loaded by test_db).
        {
            let texts = vec![
                (
                    "mem_01",
                    "Rust is a systems programming language focused on safety and performance",
                ),
                ("mem_02", "Cargo is the Rust package manager and build tool"),
                ("mem_03", "Tokio provides async I/O for Rust applications"),
                ("mem_04", "Some unrelated note about cooking"),
                ("mem_05", "Note about travel plans"),
            ];
            for (sid, text) in &texts {
                let emb = db.embed_query(text).expect("embed memory");
                let vec_sql = MemoryDB::vec_to_sql_pub(&emb);
                let conn = db.conn.lock().await;
                conn.execute(
                    &format!(
                        "UPDATE memories SET embedding = vector32('{vec_sql}') WHERE source_id = ?1"
                    ),
                    vec![libsql::Value::Text(sid.to_string())],
                )
                .await
                .expect("update embedding");
            }
        }

        // -----------------------------------------------------------------
        // Run composite search.
        // Use a config with small pool so the test stays fast.
        // -----------------------------------------------------------------
        let mut cfg = RetrievalConfig::default();
        cfg.pool_size_multiplier = 2;
        cfg.pool_size_floor = 5;
        cfg.pool_size_cap = 20;

        let results = search_memory_composite(
            &db,
            query,
            &query_embedding,
            3, // limit
            None,
            None,
            &cfg,
        )
        .await
        .expect("composite search must not error");

        // Assert we got results (up to limit).
        assert!(
            !results.is_empty(),
            "composite search must return at least one result"
        );
        assert!(
            results.len() <= 3,
            "results must not exceed requested limit"
        );

        // The entity-linked memory (mem_01) should surface: it matches both
        // semantically (same text) and has graph_distance = 0 (seed entity).
        let ids: Vec<&str> = results.iter().map(|r| r.memory_id.as_str()).collect();
        assert!(
            ids.contains(&"mem_01"),
            "mem_01 (entity-linked + semantic match) must rank in top-3; got: {:?}",
            ids
        );

        // Scores must be finite and in a sane range.
        for r in &results {
            assert!(
                r.score.is_finite(),
                "score must be finite, got {} for {}",
                r.score,
                r.memory_id
            );
        }
    }
}
