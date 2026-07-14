// SPDX-License-Identifier: Apache-2.0

use super::MemoryDB;
use crate::pages::Page;
use crate::read_scope::ReadScope;
use crate::WenlanError;
use std::collections::{HashMap, HashSet};

fn numbered_conditions(conditions: &[String], start: usize) -> String {
    let mut parameter = start;
    conditions
        .iter()
        .map(|condition| {
            let mut numbered = String::new();
            for character in condition.chars() {
                if character == '?' {
                    numbered.push_str(&format!("?{parameter}"));
                    parameter += 1;
                } else {
                    numbered.push(character);
                }
            }
            numbered
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

impl MemoryDB {
    /// Route-facing Page search. Selected scopes use a workspace predicate in
    /// the same query that ranks and limits candidates; Global preserves the
    /// existing indexed search path.
    pub async fn search_pages_scoped(
        &self,
        query: &str,
        limit: usize,
        page_type: Option<&str>,
        scope: &ReadScope,
    ) -> Result<Vec<Page>, WenlanError> {
        if matches!(scope, ReadScope::Global) {
            return self.search_pages(query, limit, page_type).await;
        }

        let embedding = self.get_or_compute_embedding(query)?;
        let vec_str = Self::vec_to_sql(&embedding);
        let fetch_limit = (limit * 3) as i64;
        let conn = self.conn.lock().await;

        let select = "c.id, c.title, c.summary, c.content, c.entity_id, c.space, \
                      c.source_memory_ids, c.version, c.status, c.created_at, \
                      c.last_compiled, c.last_modified, \
                      COALESCE(c.sources_updated_count, 0), c.stale_reason, \
                      COALESCE(c.user_edited, 0), COALESCE(c.changelog, '[]'), \
                      COALESCE(c.creation_kind, 'distilled'), \
                      COALESCE(c.review_status, 'confirmed'), c.workspace, c.citations";
        let mut conditions = vec!["c.status = 'active'".to_string()];
        let mut values = Vec::new();
        super::push_read_scope_filter(scope, "c.workspace", &mut conditions, &mut values);
        if let Some(page_type) = page_type {
            conditions.push("c.space = ?".to_string());
            values.push(libsql::Value::Text(page_type.to_string()));
        }
        let where_clause = numbered_conditions(&conditions, 3);

        let vector_sql = format!(
            "SELECT {select}, vector_distance_cos(c.embedding, vector32(?1)) AS dist \
             FROM pages c \
             WHERE c.embedding IS NOT NULL AND {where_clause} \
             ORDER BY dist ASC LIMIT ?2"
        );
        let mut vector_params = vec![
            libsql::Value::Text(vec_str),
            libsql::Value::Integer(fetch_limit),
        ];
        vector_params.extend(values.clone());
        let mut vector_results = Vec::new();
        match conn.query(&vector_sql, vector_params).await {
            Ok(mut rows) => {
                while let Ok(Some(row)) = rows.next().await {
                    if let Ok(page) = Self::row_to_page(&row) {
                        let distance = row.get::<f64>(20).unwrap_or(1.0);
                        vector_results.push((page.id.clone(), distance, page));
                    }
                }
            }
            Err(error) => log::warn!("[search_pages_scoped] vector search failed: {error}"),
        }

        use crate::retrieval::fts_query::{
            fts_length_exceeded, fts_recall_hardening_enabled, sanitize_fts_query,
        };
        let fts_queries = if fts_recall_hardening_enabled() {
            if fts_length_exceeded(query, 128) {
                Vec::new()
            } else {
                vec![sanitize_fts_query(query)]
            }
        } else {
            vec![query.to_string(), Self::fts_or_query(query)]
        };
        let fts_sql = format!(
            "SELECT {select} FROM pages c \
             JOIN pages_fts f ON c.rowid = f.rowid \
             WHERE pages_fts MATCH ?1 AND {where_clause} \
             ORDER BY rank LIMIT ?2"
        );
        let mut fts_results = Vec::new();
        for fts_query in fts_queries {
            let mut params = vec![
                libsql::Value::Text(fts_query),
                libsql::Value::Integer(fetch_limit),
            ];
            params.extend(values.clone());
            match conn.query(&fts_sql, params).await {
                Ok(mut rows) => {
                    while let Ok(Some(row)) = rows.next().await {
                        if let Ok(page) = Self::row_to_page(&row) {
                            fts_results.push((page.id.clone(), page));
                        }
                    }
                }
                Err(error) => {
                    log::debug!("[search_pages_scoped] FTS search failed: {error}")
                }
            }
            if !fts_results.is_empty() {
                break;
            }
        }
        drop(conn);

        let rrf_k = 60.0f32;
        let fts_weight = 0.2f32;
        let mut scores: HashMap<String, f32> = HashMap::new();
        let mut pages: HashMap<String, Page> = HashMap::new();
        for (rank, (id, distance, page)) in vector_results.into_iter().enumerate() {
            let similarity = (1.0 - distance as f32).max(0.01);
            *scores.entry(id.clone()).or_insert(0.0) += similarity / (rrf_k + rank as f32);
            pages.entry(id).or_insert(page);
        }
        for (rank, (id, page)) in fts_results.into_iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += fts_weight / (rrf_k + rank as f32);
            pages.entry(id).or_insert(page);
        }

        let theoretical_max = (1.0 + fts_weight) / rrf_k;
        for score in scores.values_mut() {
            *score = (*score / theoretical_max).min(1.0);
        }
        let mut results: Vec<Page> = pages.into_values().collect();
        results.sort_by(|left, right| {
            scores
                .get(&right.id)
                .unwrap_or(&0.0)
                .partial_cmp(scores.get(&left.id).unwrap_or(&0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.id.cmp(&right.id))
        });
        results.truncate(limit);
        for page in &mut results {
            page.relevance_score = *scores.get(&page.id).unwrap_or(&0.0);
        }
        Ok(results)
    }

    pub async fn select_visible_pages_scoped(
        &self,
        raw_pages: Vec<Page>,
        scope: &ReadScope,
        memory_source_ids: &HashSet<String>,
        caller_trust: &str,
        cap: usize,
    ) -> Vec<Page> {
        let scoped = raw_pages
            .into_iter()
            .filter(|page| scope.matches(page.workspace.as_deref()))
            .collect();
        self.select_visible_pages(scoped, None, memory_source_ids, caller_trust, cap)
            .await
    }
}
