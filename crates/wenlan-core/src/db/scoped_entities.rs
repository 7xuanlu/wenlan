// SPDX-License-Identifier: Apache-2.0

use super::{Entity, EntitySearchResult, MemoryDB, SearchResult};
use crate::read_scope::ReadScope;
use crate::WenlanError;
use std::collections::HashMap;

impl MemoryDB {
    pub(super) async fn filter_entity_ids_scoped(
        &self,
        entity_ids: &[String],
        scope: &ReadScope,
    ) -> Result<Vec<String>, WenlanError> {
        if matches!(scope, ReadScope::Global) || entity_ids.is_empty() {
            return Ok(entity_ids.to_vec());
        }
        let placeholders = (1..=entity_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let scope_index = entity_ids.len() + 1;
        let (scope_sql, scope_value) = match scope {
            ReadScope::Space(space) => (
                format!("AND space = ?{scope_index}"),
                Some(libsql::Value::Text(space.clone())),
            ),
            ReadScope::Uncategorized => ("AND space IS NULL".to_string(), None),
            ReadScope::Global => unreachable!(),
        };
        let sql = format!("SELECT id FROM entities WHERE id IN ({placeholders}) {scope_sql}");
        let mut params: Vec<libsql::Value> = entity_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        if let Some(value) = scope_value {
            params.push(value);
        }
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(&sql, params)
            .await
            .map_err(|error| WenlanError::VectorDb(format!("filter_entity_ids_scoped: {error}")))?;
        let mut visible = std::collections::HashSet::new();
        while let Ok(Some(row)) = rows.next().await {
            if let Ok(id) = row.get::<String>(0) {
                visible.insert(id);
            }
        }
        Ok(entity_ids
            .iter()
            .filter(|id| visible.contains(id.as_str()))
            .cloned()
            .collect())
    }

    /// Route/retrieval-facing Entity vector search. The Global path retains
    /// DiskANN; selected scopes rank only rows satisfying `entities.space`.
    pub async fn search_entities_by_vector_scoped(
        &self,
        query: &str,
        limit: usize,
        scope: &ReadScope,
    ) -> Result<Vec<EntitySearchResult>, WenlanError> {
        if matches!(scope, ReadScope::Global) {
            return self.search_entities_by_vector(query, limit).await;
        }

        let embedding = self.get_or_compute_embedding(query)?;
        let vec_str = Self::vec_to_sql(&embedding);
        let (scope_sql, scope_value) = match scope {
            ReadScope::Space(space) => {
                ("AND e.space = ?3", Some(libsql::Value::Text(space.clone())))
            }
            ReadScope::Uncategorized => ("AND e.space IS NULL", None),
            ReadScope::Global => unreachable!(),
        };
        let sql = format!(
            "SELECT e.id, e.name, e.entity_type, e.space, e.source_agent, e.confidence, \
                    e.confirmed, e.created_at, e.updated_at, \
                    vector_distance_cos(e.embedding, vector32(?1)) AS distance \
             FROM entities e \
             WHERE e.embedding IS NOT NULL {scope_sql} \
             ORDER BY distance ASC LIMIT ?2"
        );
        let mut params = vec![
            libsql::Value::Text(vec_str),
            libsql::Value::Integer(limit as i64),
        ];
        if let Some(value) = scope_value {
            params.push(value);
        }
        let conn = self.conn.lock().await;
        let mut rows = conn.query(&sql, params).await.map_err(|error| {
            WenlanError::VectorDb(format!("search_entities_by_vector_scoped: {error}"))
        })?;
        let mut results = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            results.push(EntitySearchResult {
                entity: Entity {
                    id: row.get(0).unwrap_or_default(),
                    name: row.get(1).unwrap_or_default(),
                    entity_type: row.get(2).unwrap_or_default(),
                    space: row.get::<Option<String>>(3).unwrap_or(None),
                    source_agent: row.get::<Option<String>>(4).unwrap_or(None),
                    confidence: row.get::<Option<f64>>(5).unwrap_or(None).map(|v| v as f32),
                    confirmed: row.get::<i64>(6).unwrap_or(0) != 0,
                    created_at: row.get(7).unwrap_or(0),
                    updated_at: row.get(8).unwrap_or(0),
                },
                distance: row.get::<f64>(9).unwrap_or(1.0) as f32,
            });
        }
        Ok(results)
    }

    pub async fn get_memories_for_entities_scoped(
        &self,
        ranked_anchor_ids: &[String],
        limit: usize,
        scope: &ReadScope,
    ) -> Result<Vec<SearchResult>, WenlanError> {
        if matches!(scope, ReadScope::Global) {
            return self
                .get_memories_for_entities(ranked_anchor_ids, limit)
                .await;
        }
        if ranked_anchor_ids.is_empty() {
            return Ok(Vec::new());
        }

        let anchor_rank: HashMap<&str, usize> = ranked_anchor_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect();
        let placeholders = (1..=ranked_anchor_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let scope_index = ranked_anchor_ids.len() + 1;
        let (scope_sql, scope_value) = match scope {
            ReadScope::Space(space) => (
                format!("AND c.space = ?{scope_index}"),
                Some(libsql::Value::Text(space.clone())),
            ),
            ReadScope::Uncategorized => ("AND c.space IS NULL".to_string(), None),
            ReadScope::Global => unreachable!(),
        };
        let sql = format!(
            "SELECT c.id, c.content, c.source, c.source_id, c.title, c.summary, c.url,
                    c.chunk_index, c.last_modified, c.chunk_type, c.language, c.byte_start,
                    c.byte_end, c.semantic_unit, c.memory_type, c.space, c.source_agent,
                    c.confidence, c.confirmed, c.stability, c.supersedes,
                    c.entity_id, c.quality, c.is_recap, c.supersede_mode,
                    c.structured_fields, c.retrieval_cue, c.source_text,
                    c.version, c.pending_revision,
                    0.0, c.importance, c.event_date, c.content_hash, me.entity_id
             FROM memories c
             JOIN memory_entities me ON me.memory_id = c.source_id
             WHERE me.entity_id IN ({placeholders})
               AND c.source = 'memory' AND c.chunk_index = 0 {scope_sql}"
        );
        let mut params: Vec<libsql::Value> = ranked_anchor_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        if let Some(value) = scope_value {
            params.push(value);
        }
        let conn = self.conn.lock().await;
        let mut rows = conn.query(&sql, params).await.map_err(|error| {
            WenlanError::VectorDb(format!("get_memories_for_entities_scoped: {error}"))
        })?;
        let mut best = HashMap::new();
        while let Ok(Some(row)) = rows.next().await {
            let entity_id: String = row.get(34).unwrap_or_default();
            let rank = anchor_rank
                .get(entity_id.as_str())
                .copied()
                .unwrap_or(usize::MAX);
            let Ok(result) = Self::row_to_search_result(&row, 0.0) else {
                continue;
            };
            best.entry(result.source_id.clone())
                .and_modify(|(best_rank, _): &mut (usize, SearchResult)| {
                    *best_rank = (*best_rank).min(rank);
                })
                .or_insert((rank, result));
        }
        let mut ordered: Vec<(usize, SearchResult)> = best.into_values().collect();
        ordered.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.source_id.cmp(&right.1.source_id))
        });
        ordered.truncate(limit);
        Ok(ordered.into_iter().map(|(_, result)| result).collect())
    }

    pub(super) async fn get_observations_for_entities_scoped(
        &self,
        entity_ids: &[String],
        limit: usize,
        scope: &ReadScope,
    ) -> Result<Vec<SearchResult>, WenlanError> {
        if matches!(scope, ReadScope::Global) {
            return self.get_observations_for_entities(entity_ids, limit).await;
        }
        let visible_ids = self.filter_entity_ids_scoped(entity_ids, scope).await?;
        if visible_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=visible_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let limit_parameter = visible_ids.len() + 1;
        let sql = format!(
            "SELECT o.id, o.content, e.name, o.created_at, o.source_agent, o.confidence \
             FROM observations o JOIN entities e ON o.entity_id = e.id \
             WHERE o.entity_id IN ({placeholders}) \
             ORDER BY o.confidence DESC NULLS LAST, o.created_at DESC \
             LIMIT ?{limit_parameter}"
        );
        let mut params: Vec<libsql::Value> = visible_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();
        params.push(libsql::Value::Integer(limit as i64));
        let conn = self.conn.lock().await;
        let mut rows = conn.query(&sql, params).await.map_err(|error| {
            WenlanError::VectorDb(format!("get_observations_for_entities_scoped: {error}"))
        })?;
        let mut results = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            let id: String = row.get(0).unwrap_or_default();
            let entity_name: String = row.get(2).unwrap_or_default();
            results.push(SearchResult {
                id: id.clone(),
                content: row.get(1).unwrap_or_default(),
                source: "knowledge_graph".to_string(),
                source_id: format!("obs_{id}"),
                title: entity_name.clone(),
                url: None,
                chunk_index: 0,
                last_modified: row.get(3).unwrap_or(0),
                score: 0.0,
                chunk_type: None,
                language: None,
                semantic_unit: None,
                memory_type: None,
                space: None,
                source_agent: row.get::<Option<String>>(4).unwrap_or(None),
                confidence: row.get::<Option<f64>>(5).unwrap_or(None).map(|v| v as f32),
                confirmed: None,
                stability: None,
                supersedes: None,
                summary: None,
                entity_id: None,
                entity_name: Some(entity_name),
                quality: None,
                importance: None,
                event_date: None,
                is_archived: false,
                is_recap: false,
                structured_fields: None,
                retrieval_cue: None,
                source_text: None,
                content_hash: None,
                raw_score: 0.0,
                version: 0,
                pending_revision: false,
                merged_from: None,
                last_delta_summary: None,
            });
        }
        Ok(results)
    }
}
