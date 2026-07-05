// SPDX-License-Identifier: Apache-2.0
//! Distillation phase — turn memory clusters into structured pages.
//!
//! This module owns the synthesis side of the refinery: clustering memories,
//! merging/splitting clusters via LLM, and recompiling pages from
//! source memories. Re-exported from `crate::refinery` for API stability.

use crate::db::MemoryDB;
use crate::error::WenlanError;
use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::prompts::PromptRegistry;
use crate::refinery::helpers::{
    is_all_generic_tokens, looks_like_code, looks_like_commit_message, looks_like_markup_styled,
    looks_like_path, looks_like_short_hash, looks_like_uuid,
};
use crate::sources::StabilityTier;
use crate::synthesis::refinement_queue::{resolve_proposal, ResolveStatus};
use std::collections::HashMap;
use std::sync::Arc;
use wenlan_types::requests::UpdatePageRequest;

const DISTILL_CLUSTER_DOCUMENT_MAX_SHARE_NUMERATOR: usize = 1;
const DISTILL_CLUSTER_DOCUMENT_MAX_SHARE_DENOMINATOR: usize = 2;

/// What a distillation pass is scoped to. Resolved from a free-form string
/// supplied by the user (page id, entity name, or domain value).
#[derive(Debug, Clone)]
pub enum DistillTarget {
    /// Existing page — re-distill from its current sources.
    Page(String),
    /// Scope clustering to memories belonging to one entity.
    Entity { id: String, name: String },
    /// Scope clustering to memories with a given domain.
    Domain(String),
}

/// Resolve a free-form target string into a `DistillTarget`.
///
/// Resolution order:
/// 1. Strings starting with `page_` or `concept_` are treated as page ids.
/// 2. Exact entity name match (via `MemoryDB::resolve_entity_by_name`).
/// 3. Exact registered space match.
/// 4. Otherwise `None` — caller decides whether to fail loudly or fall through.
pub async fn resolve_distill_target(
    db: &MemoryDB,
    raw: &str,
) -> Result<Option<DistillTarget>, WenlanError> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if s.starts_with("page_") || s.starts_with("concept_") {
        return Ok(Some(DistillTarget::Page(s.to_string())));
    }
    if let Some(id) = db.resolve_entity_by_name(s).await? {
        return Ok(Some(DistillTarget::Entity {
            id,
            name: s.to_string(),
        }));
    }
    if db.registered_space_or_none(Some(s)).await?.is_some() {
        return Ok(Some(DistillTarget::Domain(s.to_string())));
    }
    Ok(None)
}

/// Build the "existing page titles" hint prefix that gets prepended to
/// every distill user prompt so the LLM emits exact-match `[[Title]]`
/// wikilinks instead of inventing labels. Returns an empty string when
/// the page set is empty or the DB call errors (best-effort — the worst
/// case is the LLM invents a label that the orphan-by-count feed will
/// surface later). Capped at 100 most-recent titles so the prompt stays
/// bounded on large vaults.
pub(crate) async fn build_existing_titles_hint(db: &MemoryDB) -> String {
    let titles = db.list_active_page_titles(100).await.unwrap_or_default();
    if titles.is_empty() {
        return String::new();
    }
    let formatted = titles
        .iter()
        .map(|t| format!("[[{t}]]"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Existing pages you may reference with exact-match wikilinks: {formatted}\n\
         Use these labels verbatim when linking; only invent a new label \
         when the topic isn't already covered.\n\n"
    )
}

/// LLM cluster refinement: for entities with multiple clusters, ask the LLM to merge/split/rename.
pub(crate) async fn refine_clusters_with_llm(
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    clusters: Vec<crate::db::DistillationCluster>,
    token_limit: usize,
) -> Vec<crate::db::DistillationCluster> {
    // Group clusters by entity
    let mut by_entity: std::collections::HashMap<String, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, c) in clusters.iter().enumerate() {
        let key = c
            .entity_name
            .as_deref()
            .or(c.entity_id.as_deref())
            .unwrap_or("unlinked")
            .to_string();
        by_entity.entry(key).or_default().push(i);
    }

    // Only refine entities with 2+ clusters (single clusters = nothing to merge/split)
    let entities_to_refine: Vec<(String, Vec<usize>)> = by_entity
        .into_iter()
        .filter(|(_, indices)| indices.len() >= 2)
        .collect();

    if entities_to_refine.is_empty() {
        return clusters;
    }

    let mut result = clusters;
    let mut merged_indices: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (entity, indices) in &entities_to_refine {
        // Build one-line summaries for each cluster
        let summaries: String = indices
            .iter()
            .enumerate()
            .map(|(j, &idx)| {
                let c = &result[idx];
                let preview: String = c
                    .contents
                    .iter()
                    .take(3)
                    .map(|s| {
                        let trimmed: String = s.chars().take(60).collect();
                        format!("\"{}...\"", trimmed)
                    })
                    .collect::<Vec<_>>()
                    .join(" / ");
                format!("{}. [{} memories] {}", j, c.source_ids.len(), preview)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let user_prompt = format!("Entity: {}\n\n{}", entity, summaries);

        let response = llm
            .generate(LlmRequest {
                system_prompt: Some(prompts.refine_clusters.clone()),
                user_prompt,
                max_tokens: 512,
                temperature: 0.2,
                label: None,
                timeout_secs: None,
            })
            .await;

        match response {
            Ok(raw) => {
                let clean = crate::llm_provider::strip_think_tags(&raw);
                if let Some(json_str) = crate::engine::extract_json_array(&clean) {
                    if let Ok(actions) = serde_json::from_str::<Vec<serde_json::Value>>(&json_str) {
                        for action in &actions {
                            let act = action
                                .get("action")
                                .and_then(|v| v.as_str())
                                .unwrap_or("keep");
                            match act {
                                "merge" => {
                                    if let Some(to_merge) =
                                        action.get("clusters").and_then(|v| v.as_array())
                                    {
                                        let mut merge_indices: Vec<usize> = to_merge
                                            .iter()
                                            .filter_map(|v| v.as_u64().map(|n| n as usize))
                                            .filter(|&j| j < indices.len())
                                            .collect();
                                        merge_indices.sort_unstable();
                                        merge_indices.dedup();
                                        if merge_indices.len() >= 2 {
                                            // Guard: don't merge if the result would exceed
                                            // the token limit that sub_cluster_by_tokens split
                                            // on. This prevents the LLM from re-merging
                                            // sub-clusters into a monster that OOMs distillation.
                                            let merged_tokens: usize = merge_indices
                                                .iter()
                                                .map(|&j| result[indices[j]].estimated_tokens)
                                                .sum();
                                            if merged_tokens > token_limit {
                                                log::info!(
                                                    "[refine] skipping merge for '{}' — merged tokens {} > limit {}",
                                                    entity, merged_tokens, token_limit
                                                );
                                            } else {
                                                // Merge: combine all into the first
                                                let first = indices[merge_indices[0]];
                                                for &j in &merge_indices[1..] {
                                                    let idx = indices[j];
                                                    let extra_ids = result[idx].source_ids.clone();
                                                    let extra_contents =
                                                        result[idx].contents.clone();
                                                    result[first].source_ids.extend(extra_ids);
                                                    result[first].contents.extend(extra_contents);
                                                    result[first].estimated_tokens +=
                                                        result[idx].estimated_tokens;
                                                    merged_indices.insert(idx);
                                                }
                                                if let Some(title) =
                                                    action.get("title").and_then(|v| v.as_str())
                                                {
                                                    result[first].entity_name =
                                                        Some(title.to_string());
                                                }
                                                log::info!(
                                                    "[refine] merged {} clusters for '{}'",
                                                    merge_indices.len(),
                                                    entity
                                                );
                                            } // close else (token guard)
                                        }
                                    }
                                }
                                "rename" => {
                                    if let (Some(j), Some(title)) = (
                                        action
                                            .get("cluster")
                                            .and_then(|v| v.as_u64().map(|n| n as usize)),
                                        action.get("title").and_then(|v| v.as_str()),
                                    ) {
                                        if j < indices.len() {
                                            result[indices[j]].entity_name =
                                                Some(title.to_string());
                                            log::info!(
                                                "[refine] renamed cluster {} to '{}' for '{}'",
                                                j,
                                                title,
                                                entity
                                            );
                                        }
                                    }
                                }
                                // "keep" and "split" — split is complex (needs new clusters), defer to global_page_review
                                _ => {}
                            }
                        }
                    }
                }
            }
            Err(e) => log::warn!("[refine] LLM refinement failed for '{}': {}", entity, e),
        }
    }

    // Remove merged clusters
    if !merged_indices.is_empty() {
        result = result
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !merged_indices.contains(i))
            .map(|(_, c)| c)
            .collect();
    }

    result
}

async fn cap_document_majority_clusters(
    db: &MemoryDB,
    clusters: Vec<crate::db::DistillationCluster>,
    min_cluster_size: usize,
) -> Result<Vec<crate::db::DistillationCluster>, WenlanError> {
    let hashes = load_cluster_content_hashes(db, &clusters).await?;
    Ok(clusters
        .into_iter()
        .filter_map(|cluster| cap_one_document_majority(cluster, &hashes, min_cluster_size))
        .collect())
}

async fn load_cluster_content_hashes(
    db: &MemoryDB,
    clusters: &[crate::db::DistillationCluster],
) -> Result<HashMap<String, Option<String>>, WenlanError> {
    let mut source_ids: Vec<String> = clusters
        .iter()
        .flat_map(|cluster| cluster.source_ids.iter().cloned())
        .collect();
    source_ids.sort();
    source_ids.dedup();
    if source_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let placeholders = source_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT source_id, content_hash FROM memories WHERE source = 'memory' AND chunk_index = 0 AND source_id IN ({placeholders})"
    );
    let params: Vec<libsql::Value> = source_ids.into_iter().map(libsql::Value::Text).collect();
    let conn = db.conn.lock().await;
    let mut rows = conn
        .query(&sql, libsql::params_from_iter(params))
        .await
        .map_err(|e| WenlanError::VectorDb(format!("distill content_hash fetch: {e}")))?;

    let mut hashes = HashMap::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| WenlanError::VectorDb(format!("distill content_hash row: {e}")))?
    {
        let source_id = row.get::<String>(0).unwrap_or_default();
        let content_hash = row.get::<Option<String>>(1).unwrap_or(None);
        hashes.insert(source_id, content_hash);
    }
    Ok(hashes)
}

fn cap_one_document_majority(
    cluster: crate::db::DistillationCluster,
    hashes: &HashMap<String, Option<String>>,
    min_cluster_size: usize,
) -> Option<crate::db::DistillationCluster> {
    let original_len = cluster.source_ids.len();
    let mut retained: Vec<usize> = (0..original_len).collect();

    loop {
        if retained.len() < min_cluster_size {
            log::info!(
                "[distill] dropping document-heavy cluster after cap: {} -> {} memories (< min {})",
                original_len,
                retained.len(),
                min_cluster_size
            );
            return None;
        }

        let Some(offending_hash) = document_majority_hash(&cluster.source_ids, hashes, &retained)
        else {
            break;
        };

        if let Some(pos) = retained.iter().rposition(|&idx| {
            hash_for_source_id(&cluster.source_ids[idx], hashes) == Some(offending_hash.as_str())
        }) {
            retained.remove(pos);
        } else {
            break;
        }
    }

    if retained.len() == original_len {
        return Some(cluster);
    }

    let source_ids: Vec<String> = retained
        .iter()
        .map(|&idx| cluster.source_ids[idx].clone())
        .collect();
    let contents: Vec<String> = retained
        .iter()
        .map(|&idx| cluster.contents.get(idx).cloned().unwrap_or_default())
        .collect();
    let estimated_tokens = estimate_cluster_tokens(&contents);
    log::info!(
        "[distill] capped document-heavy cluster: {} -> {} memories",
        original_len,
        source_ids.len()
    );

    Some(crate::db::DistillationCluster {
        source_ids,
        contents,
        entity_id: cluster.entity_id,
        entity_name: cluster.entity_name,
        space: cluster.space,
        estimated_tokens,
        centroid_embedding: cluster.centroid_embedding,
    })
}

fn document_majority_hash(
    source_ids: &[String],
    hashes: &HashMap<String, Option<String>>,
    retained: &[usize],
) -> Option<String> {
    let total = retained.len();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for &idx in retained {
        if let Some(hash) = hash_for_source_id(&source_ids[idx], hashes) {
            *counts.entry(hash).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .filter(|(_, count)| {
            count * DISTILL_CLUSTER_DOCUMENT_MAX_SHARE_DENOMINATOR
                >= total * DISTILL_CLUSTER_DOCUMENT_MAX_SHARE_NUMERATOR
        })
        .max_by_key(|(_, count)| *count)
        .map(|(hash, _)| hash.to_string())
}

fn hash_for_source_id<'a>(
    source_id: &str,
    hashes: &'a HashMap<String, Option<String>>,
) -> Option<&'a str> {
    hashes
        .get(source_id)
        .and_then(|hash| hash.as_deref())
        .filter(|hash| !hash.is_empty())
}

fn estimate_cluster_tokens(contents: &[String]) -> usize {
    contents
        .iter()
        .map(|content| content.len() / 4 + 15)
        .sum::<usize>()
        + 100
}

/// Process a single distillation cluster.
///
/// Returns `Ok(true)` if a page was created, `Ok(false)` if the cluster was skipped.
/// Extracted from `distill_pages` to enable parallel cluster processing via
/// `DISTILL_CLUSTER_CONCURRENCY`.
pub async fn distill_one_cluster(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    cluster: &crate::db::DistillationCluster,
    knowledge_writer: Option<&crate::export::knowledge::KnowledgeWriter>,
) -> Result<Option<String>, WenlanError> {
    let topic = cluster
        .entity_name
        .as_deref()
        .or(cluster.space.as_deref())
        .unwrap_or("general");

    // Find the existing page that overlaps this cluster the most. Memories
    // can appear in multiple pages; the check below only prevents duplicate
    // pages, not duplicate sources.
    let overlap_match = db.find_best_overlapping_page(&cluster.source_ids).await?;
    if let Some(ref m) = overlap_match {
        let cluster_size = cluster.source_ids.len();
        let covered = m.intersection >= cluster_size || m.jaccard >= 0.8;
        if covered {
            log::info!(
                "[emergence] cluster '{}' already covered by page '{}' ({} of {} memories, Jaccard {:.2}), skipping",
                topic,
                m.page_title,
                m.intersection,
                cluster_size,
                m.jaccard
            );
            return Ok(None);
        }
        if m.intersection > 0 {
            // Partial overlap: page is stale relative to this cluster.
            // Don't emit a new page — that would be a duplicate carrying
            // different memories. Set stale_reason = "source_updated" so
            // re_distill_stale_pages picks the page up on the next sweep.
            // (increment_page_sources_updated bumps a counter nothing
            // reads; the actual refresh trigger is the stale_reason flag.)
            if let Err(e) = db.set_page_stale(&m.page_id, "source_updated").await {
                log::warn!("[emergence] could not mark page {} stale: {}", m.page_id, e);
            }
            log::info!(
                "[emergence] cluster '{}' partially overlaps page '{}' ({} new memories) — marked page stale for refresh, skipping new-page synth",
                topic,
                m.page_title,
                cluster_size - m.intersection
            );
            return Ok(None);
        }
    }

    // Source-id overlap found no duplicate. Try the scoped embedding/entity
    // matcher so a source-less seed (zero source_ids) or a topic-equivalent
    // page is still recognized. On a match we ATTACH this cluster's sources to
    // that page's evidence (content untouched) and skip synth — preventing both
    // a duplicate page and the infinite re-cluster loop (un-attached memories
    // would re-cluster, match again, skip again, forever). link_page_source
    // dual-writes page_evidence (Task 3), so the memories are marked consumed
    // in BOTH the legacy and typed provenance tables.
    if let Some(centroid) = cluster.centroid_embedding.as_deref() {
        if let Some(matched) = db
            .find_matching_page_scoped(
                cluster.entity_id.as_deref(),
                centroid,
                0.85,
                cluster.space.as_deref(),
                false, // never rewrite/attach onto hand-edited prose
            )
            .await?
        {
            let mut attached: Vec<String> = Vec::with_capacity(cluster.source_ids.len());
            for sid in &cluster.source_ids {
                match db
                    .link_page_source(&matched.id, sid, "distill_attach")
                    .await
                {
                    Ok(()) => attached.push(sid.clone()),
                    Err(e) => log::warn!(
                        "[distill] attach source {sid} -> {} failed: {e}",
                        matched.id
                    ),
                }
            }
            // P3: the scoped-match attach consumes these memories into an existing
            // page, so demote them too. Stamp ONLY the ids that actually attached —
            // never-attached memories must keep ranking normally. (Resolves P2 TODO.)
            if let Err(e) = db
                .stamp_last_distilled_at(&attached, chrono::Utc::now().timestamp())
                .await
            {
                log::warn!("[distill] stamp last_distilled_at (scoped attach) failed: {e}");
            }
            log::info!(
                "[distill] cluster '{}' attached {} sources to existing page '{}' (scoped match), skipping new-page synth",
                topic, cluster.source_ids.len(), matched.title
            );
            return Ok(None);
        }
    }

    // Clean input: strip recap headers, domain prefixes, and structured field noise
    let cleaned_contents: Vec<String> = cluster
        .contents
        .iter()
        .map(|c| {
            let mut s = c.trim().to_string();
            // Strip "Activity burst: ..." header lines
            if let Some(pos) = s.find("\n- ") {
                let prefix: String = s.chars().take(pos).collect();
                if prefix.contains("Activity burst") || prefix.contains("memories across") {
                    s = s.chars().skip(pos + 1).collect();
                }
            }
            // Strip "- [domain] " prefixes from each line
            s = s
                .lines()
                .map(|line| {
                    let trimmed = line.trim_start_matches("- ");
                    if trimmed.starts_with('[') {
                        if let Some(end) = trimmed.find("] ") {
                            trimmed[end + 2..].to_string()
                        } else {
                            line.to_string()
                        }
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            // Strip "claim: " prefix
            if let Some(rest) = s.strip_prefix("claim: ") {
                s = rest.to_string();
            }
            s
        })
        .collect();

    // Skip thin clusters — not enough substance for meaningful compilation
    let total_content_chars: usize = cleaned_contents.iter().map(|c| c.len()).sum();
    if total_content_chars < 200 {
        log::info!(
            "[compile] cluster too thin ({} chars), skipping topic='{}'",
            total_content_chars,
            topic
        );
        return Ok(None);
    }

    log::info!(
        "[distill] processing cluster: {} memories, ~{} tokens",
        cluster.source_ids.len(),
        cluster.estimated_tokens
    );

    // Build user prompt with memory IDs for source attribution.
    // Cap each memory at 800 chars so the LLM gets meaningful substance
    // without runaway context. The 800-char cap is honest: it matches the
    // amount the model can synthesize well at 2048 output tokens.
    const MEM_SNIPPET_CAP: usize = 800;
    let numbered: Vec<crate::citations::NumberedSource> = cluster
        .source_ids
        .iter()
        .zip(cleaned_contents.iter())
        .enumerate()
        .map(|(i, (id, content))| crate::citations::NumberedSource {
            index: (i + 1) as u32,
            source_kind: "memory".to_string(),
            locator: id.clone(),
            text: content.chars().take(MEM_SNIPPET_CAP).collect(),
        })
        .collect();
    let memories_block = crate::citations::build_numbered_block(&numbered);
    let titles_hint = build_existing_titles_hint(db).await;
    let user_prompt = format!("{titles_hint}Topic: {}\n\n{}", topic, memories_block);

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.distill_page.clone()),
            user_prompt,
            max_tokens: llm.recommended_max_output(),
            temperature: 0.1,
            label: Some("distill_body".into()),
            timeout_secs: None,
        })
        .await;

    match response {
        Ok(raw) if !raw.trim().is_empty() => {
            let cleaned = crate::llm_provider::strip_think_tags(&raw);
            let content = cleaned.trim().to_string();

            if content.is_empty() {
                log::warn!("[distill] empty output for topic='{}', skipping", topic);
                return Ok(None);
            }

            // Hallucination check: output must be semantically similar to input
            let texts = vec![content.clone(), cleaned_contents.join(" ")];
            if let Ok(embeddings) = db.generate_embeddings(&texts) {
                if embeddings.len() == 2 {
                    let sim = crate::db::cosine_similarity(&embeddings[0], &embeddings[1]);
                    if sim < 0.6 {
                        log::warn!(
                            "[compile] hallucination detected (sim={:.2}) for topic='{}', skipping",
                            sim,
                            topic
                        );
                        return Ok(None);
                    }
                    log::info!(
                        "[compile] quality check passed (sim={:.2}) for topic='{}'",
                        sim,
                        topic
                    );
                }
            }

            // Generate title. If LLM returns None and the only fallback is a generic
            // placeholder (e.g. "general"), skip this cluster entirely — a generic title
            // is worse than no page at all.
            let llm_title = crate::refinery::generate_short_title(llm, &content).await;
            let title = match llm_title {
                Some(t) => t,
                None if is_all_generic_tokens(topic)
                    || looks_like_markup_styled(topic)
                    || looks_like_path(topic)
                    || looks_like_code(topic)
                    || looks_like_uuid(topic)
                    || looks_like_short_hash(topic)
                    || looks_like_commit_message(topic) =>
                {
                    log::info!(
                        "[distill] no title and topic='{}' is garbage, skipping cluster",
                        topic
                    );
                    return Ok(None);
                }
                None => topic.to_string(),
            };

            // Extract summary from first bullet point
            let summary = content
                .lines()
                .find(|l| l.starts_with("- "))
                .map(|l| l.trim_start_matches("- ").to_string());

            // Verify [N] markers the LLM emitted against the numbered sources:
            // out-of-range markers are stripped, each remaining occurrence gets
            // a verified/unverified status via union-of-cited-sources overlap.
            let (content, cites, stats) =
                crate::citations::process_citation_output(&content, &numbered);
            let citations_json = serde_json::to_string(&cites).unwrap_or_else(|_| "[]".into());

            // Build source IDs as &str refs
            let source_refs: Vec<&str> = cluster.source_ids.iter().map(|s| s.as_str()).collect();
            let now = chrono::Utc::now().to_rfc3339();
            let page_id = crate::pages::new_page_id();
            log::info!("[distill] page {page_id} citations: {}", stats.summary());

            db.insert_page_with_kind(
                &page_id,
                &title,
                summary.as_deref(),
                &content,
                cluster.entity_id.as_deref(),
                cluster.space.as_deref(),
                &source_refs,
                &now,
                "distilled",
                "confirmed",
                None,
                Some(&citations_json),
            )
            .await?;

            // P3 consolidation-demotion: stamp the source memories so the ranking
            // demotion multiplier in search_memory_cross_rerank ranks them below
            // their page for the topic query (they stay in allowed_memory_ids +
            // deep recall). chrono::Utc::now().timestamp() matches the unix-seconds
            // contract of memories.last_distilled_at.
            if let Err(e) = db
                .stamp_last_distilled_at(&cluster.source_ids, chrono::Utc::now().timestamp())
                .await
            {
                log::warn!("[distill] stamp last_distilled_at failed: {e}");
            }

            log::info!(
                "[distill] distilled {} memories -> page '{}' ('{}')",
                cluster.source_ids.len(),
                title,
                content.chars().take(40).collect::<String>()
            );

            // Log activity — system-attributed, since distillation is background refinery work.
            let source_memory_ids: Vec<String> = cluster.source_ids.to_vec();
            let detail = format!(
                "created \"{}\" from {} memories",
                title,
                cluster.source_ids.len()
            );
            if let Err(e) = db
                .log_agent_activity("system", "page_create", &source_memory_ids, None, &detail)
                .await
            {
                log::warn!("[distill] log page_create activity failed: {e}");
            }

            if let Some(writer) = knowledge_writer {
                if let Ok(Some(c)) = db.get_page(&page_id).await {
                    match writer.write_page(&c) {
                        Ok(p) => log::info!("[distill] wrote page to {p}"),
                        Err(e) => log::warn!("[distill] knowledge write failed: {e}"),
                    }
                }
            }

            Ok(Some(page_id))
        }
        Ok(_) => {
            log::warn!("[distill] empty output for topic='{}'", topic);
            Ok(None)
        }
        Err(e) => {
            log::warn!("[distill] LLM error for topic='{}': {}", topic, e);
            Ok(None)
        }
    }
}

/// Outcome of a distillation pass — pages the daemon synthesized itself,
/// plus clusters it could not finish (no LLM available, or the cluster
/// exceeded the LLM's effective context budget). Callers with their own
/// LLM (e.g. the agent-driven `/distill` skill) can pick up the pending
/// clusters and finish them.
#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct DistillResult {
    /// Page ids the daemon synthesized + persisted itself this pass.
    pub created: Vec<String>,
    /// Clusters the daemon clustered but did not synthesize. Each cluster
    /// carries the source memory ids, content snippets, and entity / domain
    /// metadata so the caller has everything it needs to write a page and
    /// POST back to `/api/pages`.
    pub pending: Vec<crate::db::DistillationCluster>,
}

/// Distill memory clusters into structured concepts.
/// Memories can appear in multiple concepts. Jaccard overlap prevents duplicate concepts.
///
/// Returns the count of pages the daemon synthesized itself; refer to
/// `distill_pages_scoped` for the full `DistillResult` with `pending`.
pub async fn distill_pages(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    tuning: &crate::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
) -> Result<usize, WenlanError> {
    let r = distill_pages_scoped(db, llm, prompts, tuning, knowledge_path, None).await?;
    Ok(r.created.len())
}

/// Same as `distill_pages` but restricts clustering to a single entity, a
/// single domain, or (when `target` is `DistillTarget::Page`) re-distills one
/// existing page directly. `None` matches `distill_pages` exactly.
///
/// Returns a `DistillResult`: pages the daemon synthesized itself plus
/// clusters it couldn't finish (no LLM, or cluster too big for the LLM's
/// context). The HTTP `/api/distill` route hands the `pending` list back
/// to the caller so the agent-driven skill can synthesize the rest.
pub async fn distill_pages_scoped(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    tuning: &crate::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
    target: Option<DistillTarget>,
) -> Result<DistillResult, WenlanError> {
    if let Some(DistillTarget::Page(ref page_id)) = target {
        let updated = deep_distill_single(db, llm, prompts, page_id, knowledge_path).await?;
        return Ok(DistillResult {
            created: if updated {
                vec![page_id.clone()]
            } else {
                vec![]
            },
            pending: vec![],
        });
    }
    let (entity_id_filter, domain_filter): (Option<String>, Option<String>) = match target {
        Some(DistillTarget::Entity { id, .. }) => (Some(id), None),
        Some(DistillTarget::Domain(d)) => (None, Some(d)),
        Some(DistillTarget::Page(_)) | None => (None, None),
    };
    // No LLM available — discover clusters and hand them back as pending
    // so the caller (typically the /distill skill in Basic Memory mode)
    // can finish them with its own LLM.
    let llm = match llm {
        Some(l) if l.is_available() => l,
        _ => {
            // Use a generous budget so candidate discovery isn't gated by
            // a tiny on-device window we don't have anyway.
            let raw_clusters = db
                .find_distillation_clusters_scoped(
                    tuning.similarity_threshold,
                    tuning.page_min_cluster_size,
                    tuning.max_clusters_per_steep,
                    16_000,
                    tuning.max_unlinked_cluster_size,
                    tuning.max_grouped_cluster_size,
                    entity_id_filter.as_deref(),
                    domain_filter.as_deref(),
                )
                .await?;
            let mut raw_clusters =
                cap_document_majority_clusters(db, raw_clusters, tuning.page_min_cluster_size)
                    .await?;
            // Cap each memory's content snippet so the caller-facing payload
            // doesn't balloon past practical HTTP/MCP response sizes. The
            // synthesis path uses the same cap when building prompts; doing
            // it at the boundary keeps both code paths consistent and bounds
            // the worst-case pending payload to ~MEM_SNIPPET_CAP * total
            // memories.
            const MEM_SNIPPET_CAP: usize = 800;
            for c in raw_clusters.iter_mut() {
                for content in c.contents.iter_mut() {
                    if content.chars().count() > MEM_SNIPPET_CAP {
                        let mut truncated: String = content.chars().take(MEM_SNIPPET_CAP).collect();
                        truncated.push('…');
                        *content = truncated;
                    }
                }
            }
            return Ok(DistillResult {
                created: vec![],
                pending: raw_clusters,
            });
        }
    };

    // Each model carries its own effective synthesis limit — the max tokens it
    // can meaningfully synthesize (not just read). Research-calibrated per model
    // in on_device_models.rs and llm_provider.rs. Falls back to tuning config
    // if the provider returns the default (for backward compat).
    let token_limit = llm.synthesis_token_limit();
    let raw_clusters = db
        .find_distillation_clusters_scoped(
            tuning.similarity_threshold,
            tuning.page_min_cluster_size,
            tuning.max_clusters_per_steep,
            token_limit,
            tuning.max_unlinked_cluster_size,
            tuning.max_grouped_cluster_size,
            entity_id_filter.as_deref(),
            domain_filter.as_deref(),
        )
        .await?;
    let raw_clusters =
        cap_document_majority_clusters(db, raw_clusters, tuning.page_min_cluster_size).await?;

    // LLM cluster refinement: let LLM merge/split/rename clusters per entity
    let clusters = refine_clusters_with_llm(llm, prompts, raw_clusters, token_limit).await;
    let clusters =
        cap_document_majority_clusters(db, clusters, tuning.page_min_cluster_size).await?;

    let cluster_concurrency: usize = std::env::var("DISTILL_CLUSTER_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .min(4);

    let mut created: Vec<String> = Vec::new();

    // Create the writer once, outside the loop
    let knowledge_writer =
        knowledge_path.map(|kp| crate::export::knowledge::KnowledgeWriter::new(kp.to_path_buf()));

    if cluster_concurrency > 1 {
        let kw = knowledge_writer.as_ref();
        for chunk in clusters.chunks(cluster_concurrency) {
            let futs: Vec<_> = chunk
                .iter()
                .map(|cluster| distill_one_cluster(db, llm, prompts, cluster, kw))
                .collect();
            let results = futures::future::join_all(futs).await;
            for r in results {
                if let Some(page_id) = r? {
                    created.push(page_id);
                }
            }
        }
        return Ok(DistillResult {
            created,
            pending: vec![],
        });
    }

    for cluster in &clusters {
        let topic = cluster
            .entity_name
            .as_deref()
            .or(cluster.space.as_deref())
            .unwrap_or("general");

        // Skip if a page with very similar sources already exists (Jaccard > 0.8)
        // Memories CAN appear in multiple concepts — this only prevents duplicate concepts.
        // Asymmetry: the concurrent path (distill_one_cluster) takes the skip-and-attach
        // dedup route instead — it finds a near-match page and stamps last_distilled_at for
        // the attached ids.  This inline loop only skips on overlap, so it correctly stamps
        // nothing (no attachment was made).
        let overlap = db
            .max_page_overlap(&cluster.source_ids)
            .await
            .unwrap_or(0.0);
        if overlap > 0.8 {
            log::info!(
                "[emergence] cluster '{}' overlaps {:.0}% with existing page, skipping",
                topic,
                overlap * 100.0
            );
            continue;
        }

        // Clean input: strip recap headers, domain prefixes, and structured field noise
        let cleaned_contents: Vec<String> = cluster
            .contents
            .iter()
            .map(|c| {
                let mut s = c.trim().to_string();
                // Strip "Activity burst: ..." header lines
                if let Some(pos) = s.find("\n- ") {
                    let prefix: String = s.chars().take(pos).collect();
                    if prefix.contains("Activity burst") || prefix.contains("memories across") {
                        s = s.chars().skip(pos + 1).collect();
                    }
                }
                // Strip "- [domain] " prefixes from each line
                s = s
                    .lines()
                    .map(|line| {
                        let trimmed = line.trim_start_matches("- ");
                        if trimmed.starts_with('[') {
                            if let Some(end) = trimmed.find("] ") {
                                trimmed[end + 2..].to_string()
                            } else {
                                line.to_string()
                            }
                        } else {
                            line.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                // Strip "claim: " prefix
                if let Some(rest) = s.strip_prefix("claim: ") {
                    s = rest.to_string();
                }
                s
            })
            .collect();

        // Skip thin clusters — not enough substance for meaningful compilation
        let total_content_chars: usize = cleaned_contents.iter().map(|c| c.len()).sum();
        if total_content_chars < 200 {
            log::info!(
                "[compile] cluster too thin ({} chars), skipping topic='{}'",
                total_content_chars,
                topic
            );
            continue;
        }

        log::info!(
            "[distill] processing cluster: {} memories, ~{} tokens",
            cluster.source_ids.len(),
            cluster.estimated_tokens
        );

        // Build user prompt with memory IDs for source attribution.
        // Cap each memory at 800 chars so the LLM gets meaningful substance
        // without runaway context. The 800-char cap is honest: it matches the
        // amount the model can synthesize well at 2048 output tokens.
        const MEM_SNIPPET_CAP: usize = 800;
        let numbered: Vec<crate::citations::NumberedSource> = cluster
            .source_ids
            .iter()
            .zip(cleaned_contents.iter())
            .enumerate()
            .map(|(i, (id, content))| crate::citations::NumberedSource {
                index: (i + 1) as u32,
                source_kind: "memory".to_string(),
                locator: id.clone(),
                text: content.chars().take(MEM_SNIPPET_CAP).collect(),
            })
            .collect();
        let memories_block = crate::citations::build_numbered_block(&numbered);
        let titles_hint = build_existing_titles_hint(db).await;
        let user_prompt = format!("{titles_hint}Topic: {}\n\n{}", topic, memories_block);

        let response = llm
            .generate(LlmRequest {
                system_prompt: Some(prompts.distill_page.clone()),
                user_prompt,
                max_tokens: llm.recommended_max_output(),
                temperature: 0.1,
                label: Some("distill_body".into()),
                timeout_secs: None,
            })
            .await;

        match response {
            Ok(raw) if !raw.trim().is_empty() => {
                let cleaned = crate::llm_provider::strip_think_tags(&raw);
                let content = cleaned.trim().to_string();

                if content.is_empty() {
                    log::warn!("[distill] empty output for topic='{}', skipping", topic);
                    continue;
                }

                // Hallucination check: output must be semantically similar to input
                let texts = vec![content.clone(), cleaned_contents.join(" ")];
                if let Ok(embeddings) = db.generate_embeddings(&texts) {
                    if embeddings.len() == 2 {
                        let sim = crate::db::cosine_similarity(&embeddings[0], &embeddings[1]);
                        if sim < 0.6 {
                            log::warn!("[compile] hallucination detected (sim={:.2}) for topic='{}', skipping", sim, topic);
                            continue;
                        }
                        log::info!(
                            "[compile] quality check passed (sim={:.2}) for topic='{}'",
                            sim,
                            topic
                        );
                    }
                }

                // Generate title. If LLM returns None and the only fallback is a generic
                // placeholder (e.g. "general"), skip this cluster entirely — a generic title
                // is worse than no page at all.
                let llm_title = crate::refinery::generate_short_title(llm, &content).await;
                let title = match llm_title {
                    Some(t) => t,
                    None if is_all_generic_tokens(topic)
                        || looks_like_markup_styled(topic)
                        || looks_like_path(topic)
                        || looks_like_code(topic)
                        || looks_like_uuid(topic)
                        || looks_like_short_hash(topic)
                        || looks_like_commit_message(topic) =>
                    {
                        log::info!(
                            "[distill] no title and topic='{}' is garbage, skipping cluster",
                            topic
                        );
                        continue;
                    }
                    None => topic.to_string(),
                };

                // Extract summary from first bullet point
                let summary = content
                    .lines()
                    .find(|l| l.starts_with("- "))
                    .map(|l| l.trim_start_matches("- ").to_string());

                // Verify [N] markers the LLM emitted against the numbered sources.
                let (content, cites, stats) =
                    crate::citations::process_citation_output(&content, &numbered);
                let citations_json = serde_json::to_string(&cites).unwrap_or_else(|_| "[]".into());

                // Build source IDs as &str refs
                let source_refs: Vec<&str> =
                    cluster.source_ids.iter().map(|s| s.as_str()).collect();
                let now = chrono::Utc::now().to_rfc3339();
                let page_id = crate::pages::new_page_id();
                log::info!("[distill] page {page_id} citations: {}", stats.summary());

                db.insert_page_with_kind(
                    &page_id,
                    &title,
                    summary.as_deref(),
                    &content,
                    cluster.entity_id.as_deref(),
                    cluster.space.as_deref(),
                    &source_refs,
                    &now,
                    "distilled",
                    "confirmed",
                    None,
                    Some(&citations_json),
                )
                .await?;

                // P3 consolidation-demotion: stamp the source memories so the ranking
                // demotion multiplier in search_memory_cross_rerank ranks them below
                // their page for the topic query (they stay in allowed_memory_ids +
                // deep recall). chrono::Utc::now().timestamp() matches the unix-seconds
                // contract of memories.last_distilled_at.
                if let Err(e) = db
                    .stamp_last_distilled_at(&cluster.source_ids, chrono::Utc::now().timestamp())
                    .await
                {
                    log::warn!("[distill] stamp last_distilled_at failed: {e}");
                }

                log::info!(
                    "[distill] distilled {} memories -> page '{}' ('{}')",
                    cluster.source_ids.len(),
                    title,
                    content.chars().take(40).collect::<String>()
                );
                created.push(page_id.clone());

                // Log activity — system-attributed, since distillation is background refinery work.
                let source_memory_ids: Vec<String> = cluster.source_ids.to_vec();
                let detail = format!(
                    "created \"{}\" from {} memories",
                    title,
                    cluster.source_ids.len()
                );
                if let Err(e) = db
                    .log_agent_activity("system", "page_create", &source_memory_ids, None, &detail)
                    .await
                {
                    log::warn!("[distill] log page_create activity failed: {e}");
                }

                if let Some(ref writer) = knowledge_writer {
                    if let Ok(Some(c)) = db.get_page(&page_id).await {
                        match writer.write_page(&c) {
                            Ok(p) => log::info!("[distill] wrote page to {p}"),
                            Err(e) => log::warn!("[distill] knowledge write failed: {e}"),
                        }
                    }
                }
            }
            Ok(_) => {
                log::warn!("[distill] empty output for topic='{}'", topic);
            }
            Err(e) => {
                log::warn!("[distill] LLM error for topic='{}': {}", topic, e);
            }
        }
    }

    Ok(DistillResult {
        created,
        pending: vec![],
    })
}

/// Full Karpathy-style deep distill: emergence + orphans + recompile ALL + global review.
/// Triggered by "Distill now" button or weekly background schedule.
pub async fn deep_distill_pages(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    tuning: &crate::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
) -> Result<usize, WenlanError> {
    let llm_ref = match llm {
        Some(l) if l.is_available() => l,
        _ => return Ok(0),
    };

    let mut total = 0usize;

    // 1. Emergence — create new concepts from clusters
    let created = distill_pages(db, llm, prompts, tuning, knowledge_path)
        .await
        .unwrap_or(0);
    total += created;
    if created > 0 {
        log::info!("[deep_distill] created {} new concepts", created);
    }

    // 2. Orphan assignment — assign unlinked memories to concepts or propose new ones
    match crate::synthesis::emergence::assign_orphan_memories(
        db,
        llm_ref,
        prompts,
        tuning,
        knowledge_path,
    )
    .await
    {
        Ok(n) => {
            total += n;
            if n > 0 {
                log::info!("[deep_distill] assigned {} orphan memories", n);
            }
        }
        Err(e) => log::warn!("[deep_distill] orphan assignment failed: {}", e),
    }

    // 3. Refresh stale, non-user-edited pages. Cap 20 per fire so the refinery
    //    returns control promptly; subsequent fires drain the rest. Order = most-stale
    //    first (sources_updated_count desc).
    const DEEP_DISTILL_REFRESH_CAP: i64 = 20;
    let stale_pages = db
        .list_pages_stale("active", DEEP_DISTILL_REFRESH_CAP, 0)
        .await?;
    for page in &stale_pages {
        match recompile_single_page(db, llm_ref, prompts, page, knowledge_path).await {
            Ok(true) => total += 1,
            Ok(false) => {}
            Err(e) => log::warn!(
                "[deep_distill] recompile failed for '{}': {}",
                page.title,
                e
            ),
        }
    }

    // 4. Global review — merge/split/create analysis. Needs the full active set
    //    (not just stale pages) to propose merges/splits across all concepts.
    let all_active = db.list_pages("active", 200, 0).await?;
    if all_active.len() >= 5 {
        match crate::synthesis::emergence::global_page_review(
            db,
            llm_ref,
            prompts,
            &all_active,
            knowledge_path,
        )
        .await
        {
            Ok(n) => {
                total += n;
                if n > 0 {
                    log::info!("[deep_distill] global review applied {} changes", n);
                }
            }
            Err(e) => log::warn!("[deep_distill] global review failed: {}", e),
        }
    }

    log::info!("[deep_distill] complete: {} total changes", total);
    Ok(total)
}

/// Recompile a single page from its source memories via LLM.
pub(crate) async fn recompile_single_page(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    page: &crate::pages::Page,
    knowledge_path: Option<&std::path::Path>,
) -> Result<bool, WenlanError> {
    let memories = db
        .get_memory_contents_by_ids(&page.source_memory_ids)
        .await?;
    if memories.is_empty() {
        log::warn!(
            "[re-distill] page '{}' has no source memories, skipping",
            page.id
        );
        return Ok(false);
    }

    const MEM_SNIPPET_CAP: usize = 800;
    let numbered: Vec<crate::citations::NumberedSource> = memories
        .iter()
        .enumerate()
        .map(|(i, (id, content))| crate::citations::NumberedSource {
            index: (i + 1) as u32,
            source_kind: "memory".to_string(),
            locator: id.clone(),
            text: content.chars().take(MEM_SNIPPET_CAP).collect(),
        })
        .collect();
    let memories_block = crate::citations::build_numbered_block(&numbered);
    let titles_hint = build_existing_titles_hint(db).await;
    let user_prompt = format!("{titles_hint}Topic: {}\n\n{}", page.title, memories_block);

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.distill_page.clone()),
            user_prompt,
            max_tokens: llm.recommended_max_output(),
            temperature: 0.1,
            label: Some("distill_body".into()),
            timeout_secs: None,
        })
        .await;

    match response {
        Ok(raw) if !raw.trim().is_empty() => {
            let content = crate::llm_provider::strip_think_tags(&raw)
                .trim()
                .to_string();
            if !content.is_empty() {
                // Verify [N] markers against the numbered sources; out-of-range
                // markers are stripped from the body before it is saved.
                let (content, cites, stats) =
                    crate::citations::process_citation_output(&content, &numbered);
                log::info!(
                    "[re-distill] page '{}' citations: {}",
                    page.title,
                    stats.summary()
                );
                let result = crate::post_write::update_page(
                    db,
                    &page.id,
                    UpdatePageRequest {
                        content,
                        source_memory_ids: page.source_memory_ids.clone(),
                    },
                    "re_distill",
                    true,
                    knowledge_path,
                    None,
                )
                .await?;
                if result.wrote {
                    // `update_page` (post_write.rs) has no `citations` param
                    // until Task 6 wires the growth path, so a content write
                    // resets the column to '[]'; persist the real citation
                    // map computed from this same body as a follow-up write.
                    let citations_json =
                        serde_json::to_string(&cites).unwrap_or_else(|_| "[]".to_string());
                    if let Err(e) = db.set_page_citations(&page.id, Some(&citations_json)).await {
                        log::warn!(
                            "[re-distill] persist citations failed for '{}': {e}; resetting to NULL so the backfill sweep re-picks it",
                            page.title
                        );
                        if let Err(e2) = db.set_page_citations(&page.id, None).await {
                            log::error!(
                                "[re-distill] citations NULL fallback also failed for '{}': {e2}",
                                page.title
                            );
                        }
                    }
                    log::info!("[re-distill] refreshed page '{}'", page.title);
                    return Ok(true);
                } else {
                    if let Err(e) = db
                        .log_agent_activity(
                            "system",
                            "page_skip_user_edited",
                            std::slice::from_ref(&page.id),
                            None,
                            &format!("re_distill yielded for '{}'", page.title),
                        )
                        .await
                    {
                        log::warn!("[re-distill] activity log failed: {e}");
                    }
                    return Ok(false);
                }
            }
        }
        Ok(_) => log::warn!("[re-distill] empty output for '{}'", page.title),
        Err(e) => log::warn!("[re-distill] LLM error for '{}': {}", page.title, e),
    }
    Ok(false)
}

/// Re-distill a single page by reloading all source memories and recompiling
/// with the LLM. Returns `Ok(true)` when the page content was actually
/// rewritten, `Ok(false)` when the call was a no-op (no source memories,
/// empty LLM output) so callers can report honest counts. Returns
/// `Err(WenlanError::Llm)` only when the LLM call itself fails.
pub async fn deep_distill_single(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    page_id: &str,
    knowledge_path: Option<&std::path::Path>,
) -> Result<bool, WenlanError> {
    let llm = match llm {
        Some(l) if l.is_available() => l,
        Some(_) => {
            return Err(WenlanError::Llm(
                "LLM not available for re-distillation".into(),
            ))
        }
        None => {
            return Err(WenlanError::Llm(
                "No LLM available for re-distillation".into(),
            ))
        }
    };

    let page = db
        .get_page(page_id)
        .await?
        .ok_or_else(|| WenlanError::VectorDb(format!("Concept {} not found", page_id)))?;

    let memories = db
        .get_memory_contents_by_ids(&page.source_memory_ids)
        .await?;
    if memories.is_empty() {
        log::warn!("[distill] no source memories found for page {}", page_id);
        return Ok(false);
    }

    const MEM_SNIPPET_CAP: usize = 800;
    let numbered: Vec<crate::citations::NumberedSource> = memories
        .iter()
        .enumerate()
        .map(|(i, (id, content))| crate::citations::NumberedSource {
            index: (i + 1) as u32,
            source_kind: "memory".to_string(),
            locator: id.clone(),
            text: content.chars().take(MEM_SNIPPET_CAP).collect(),
        })
        .collect();
    let memories_block = crate::citations::build_numbered_block(&numbered);
    let user_prompt = format!("Topic: {}\n\n{}", page.title, memories_block);

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.distill_page.clone()),
            user_prompt,
            max_tokens: llm.recommended_max_output(),
            temperature: 0.1,
            label: Some("distill_body".into()),
            timeout_secs: None,
        })
        .await
        .map_err(|e| WenlanError::Llm(format!("re-distill LLM: {}", e)))?;

    let content = crate::llm_provider::strip_think_tags(&response)
        .trim()
        .to_string();

    if content.is_empty() {
        log::warn!("[distill] empty output for page '{}', skipping", page.title);
        return Ok(false);
    }

    // Verify [N] markers against the numbered sources; out-of-range markers
    // are stripped from the body before it is saved.
    let (content, cites, stats) = crate::citations::process_citation_output(&content, &numbered);
    log::info!(
        "[distill] page '{}' citations: {}",
        page.title,
        stats.summary()
    );

    let result = crate::post_write::update_page(
        db,
        page_id,
        UpdatePageRequest {
            content,
            source_memory_ids: page.source_memory_ids.clone(),
        },
        "distill",
        true,
        knowledge_path,
        None,
    )
    .await?;

    if result.wrote {
        // See recompile_single_page above: `update_page` has no `citations`
        // param until Task 6, so persist the real citation map as a
        // follow-up write rather than let the content write's '[]' reset
        // stick (which the backfill sweep, IS NULL only, would never re-visit).
        let citations_json = serde_json::to_string(&cites).unwrap_or_else(|_| "[]".to_string());
        if let Err(e) = db.set_page_citations(page_id, Some(&citations_json)).await {
            log::warn!(
                "[distill] persist citations failed for '{}': {e}; resetting to NULL so the backfill sweep re-picks it",
                page.title
            );
            if let Err(e2) = db.set_page_citations(page_id, None).await {
                log::error!(
                    "[distill] citations NULL fallback also failed for '{}': {e2}",
                    page.title
                );
            }
        }
        log::info!(
            "[distill] re-distilled page '{}' (v{}->v{})",
            page.title,
            page.version,
            page.version + 1
        );
        Ok(true)
    } else {
        if let Err(e) = db
            .log_agent_activity(
                "system",
                "page_skip_user_edited",
                &[page_id.to_string()],
                None,
                &format!("distill yielded for '{}'", page.title),
            )
            .await
        {
            log::warn!("[distill] activity log failed: {e}");
        }
        Ok(false)
    }
}

/// Apply a merge result based on the stability tier of the involved memories.
pub(crate) async fn apply_merge_by_tier(
    db: &MemoryDB,
    source_ids: &[String],
    merged_content: &str,
    proposal_id: &str,
    tier: &StabilityTier,
) -> Result<(), WenlanError> {
    match tier {
        StabilityTier::Ephemeral => {
            // Auto-apply silently
            db.apply_merge(source_ids, merged_content).await?;
            resolve_proposal(db, proposal_id, ResolveStatus::AutoApplied, "daemon").await?;
            log::info!(
                "[refinery] auto-applied merge (ephemeral) for {}",
                proposal_id
            );
        }
        StabilityTier::Standard => {
            // Auto-apply with notification (toast emitted by caller if app_handle available)
            db.apply_merge(source_ids, merged_content).await?;
            resolve_proposal(db, proposal_id, ResolveStatus::AutoApplied, "daemon").await?;
            log::info!(
                "[refinery] auto-applied merge (standard, notify) for {}",
                proposal_id
            );
        }
        StabilityTier::Protected => {
            // Queue for human review — don't auto-apply
            resolve_proposal(db, proposal_id, ResolveStatus::AwaitingReview, "daemon").await?;
            log::info!(
                "[refinery] queued merge for review (protected) for {}",
                proposal_id
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::MockProvider;
    use crate::sources::RawDocument;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn resolve_distill_target_ignores_unregistered_memory_space() {
        let (db, _db_dir) = crate::db::tests::test_db().await;
        let now_ts = chrono::Utc::now().timestamp();

        {
            let conn = db.conn.lock().await;
            conn.execute(
                "INSERT INTO memories (id, source_id, title, content, chunk_index, chunk_type, memory_type, space, source_agent, created_at, last_modified, confirmed, stability, source) \
                 VALUES (?1, ?1, ?1, 'orphaned unregistered space content', 0, 'text', 'fact', 'ghost', 'claude-code', ?2, ?2, 1, 'confirmed', 'memory')",
                libsql::params!["mem_orphan_space".to_string(), now_ts],
            )
            .await
            .unwrap();
        }

        let target = resolve_distill_target(&db, "ghost").await.unwrap();
        assert!(
            target.is_none(),
            "unregistered legacy memory labels must not resolve as distill space targets: {target:?}"
        );
    }

    #[tokio::test]
    async fn distill_one_cluster_persists_verified_and_unverified_citations() {
        let (db, _db_dir) = crate::db::tests::test_db().await;

        let cluster = crate::db::DistillationCluster {
            source_ids: vec!["mem_daemon".into(), "mem_embed".into()],
            contents: vec![
                "The Wenlan daemon binds to port 7878 by default on localhost, providing \
                 the HTTP API surface used by the CLI and the MCP bridge for all downstream \
                 tools that talk to the local memory store."
                    .to_string(),
                "FastEmbed uses the BGE-Base-EN embeddings model with 768 dimensions for \
                 vector search across every stored memory and page in the local database, \
                 combined with FTS5 for hybrid retrieval."
                    .to_string(),
            ],
            entity_id: None,
            entity_name: Some("Wenlan daemon".into()),
            space: None,
            estimated_tokens: 120,
            centroid_embedding: None,
        };

        let llm: Arc<dyn LlmProvider> = Arc::new(MockProvider::new(
            "The Wenlan daemon binds to port 7878 by default.[1]\n\n\
             Second claim: an entirely unrelated statement about the flavor of ice cream.[2]",
        ));
        let prompts = PromptRegistry::default();

        let page_id = distill_one_cluster(&db, &llm, &prompts, &cluster, None)
            .await
            .unwrap()
            .expect("cluster should synthesize a page");

        let page = db.get_page(&page_id).await.unwrap().unwrap();
        assert_eq!(page.citations.len(), 2, "citations: {:?}", page.citations);
        assert_eq!(page.citations[0].status, "verified");
        assert_eq!(page.citations[0].locator, "mem_daemon");
        assert!(page.content.contains("[1]"));
    }

    #[tokio::test]
    async fn recompile_single_page_re_projects_md_when_path_passed() {
        let (db, _db_dir) = crate::db::tests::test_db().await;
        let knowledge_dir = TempDir::new().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let now_ts = chrono::Utc::now().timestamp();

        // Insert a seed memory row directly so get_memory_contents_by_ids returns it.
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "INSERT INTO memories (id, source_id, title, content, chunk_index, chunk_type, memory_type, space, source_agent, created_at, last_modified, confirmed, stability, source) \
                 VALUES (?1, ?1, ?1, 'seed content', 0, 'text', 'fact', 'test', 'claude-code', ?2, ?2, 1, 'confirmed', 'memory')",
                libsql::params!["mem_seed".to_string(), now_ts],
            )
            .await
            .unwrap();
        }

        // Insert a page that cites that memory, then mark it stale.
        db.insert_page(
            "page_a",
            "Topic A",
            None,
            "original body",
            None,
            None,
            &["mem_seed"],
            &now,
        )
        .await
        .unwrap();
        db.set_page_stale("page_a", "source_updated").await.unwrap();

        let llm: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("recompiled body"));
        let prompts = PromptRegistry::default();
        let page = db.get_page("page_a").await.unwrap().unwrap();

        let updated = recompile_single_page(&db, &llm, &prompts, &page, Some(knowledge_dir.path()))
            .await
            .unwrap();
        assert!(updated, "recompile should write");

        // Verify md file was re-projected into the knowledge directory.
        let entries: Vec<_> = std::fs::read_dir(knowledge_dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
            .collect();
        assert_eq!(entries.len(), 1, "exactly one md file");
        let content = std::fs::read_to_string(entries[0].path()).unwrap();
        assert!(
            content.contains("recompiled body"),
            "md body should reflect LLM output"
        );
    }

    #[tokio::test]
    async fn pending_distill_clusters_cap_one_document_below_majority() {
        let (db, _db_dir) = crate::db::tests::test_db().await;
        let now = chrono::Utc::now().timestamp_millis();
        let topic = "Wenlan folder ingest distillation cluster cap";

        for i in 0..4 {
            db.upsert_documents(vec![RawDocument {
                source: "memory".to_string(),
                source_id: format!("book_chunk_{i}"),
                title: format!("Book chunk {i}"),
                content: format!("{topic} repeated book excerpt section {i}"),
                last_modified: now + i,
                memory_type: Some("fact".to_string()),
                space: Some("work".to_string()),
                entity_id: Some("ent_folder_ingest".to_string()),
                content_hash: Some("book-content-hash".to_string()),
                ..Default::default()
            }])
            .await
            .unwrap();
        }

        for i in 0..3 {
            db.upsert_documents(vec![RawDocument {
                source: "memory".to_string(),
                source_id: format!("capture_note_{i}"),
                title: format!("Capture note {i}"),
                content: format!("{topic} supporting capture note {i}"),
                last_modified: now + 10 + i,
                memory_type: Some("fact".to_string()),
                space: Some("work".to_string()),
                entity_id: Some("ent_folder_ingest".to_string()),
                content_hash: None,
                ..Default::default()
            }])
            .await
            .unwrap();
        }

        let prompts = PromptRegistry::default();
        let tuning = crate::tuning::DistillationConfig {
            similarity_threshold: 0.2,
            page_min_cluster_size: 3,
            max_grouped_cluster_size: 20,
            max_unlinked_cluster_size: 20,
            ..Default::default()
        };

        let result = distill_pages_scoped(&db, None, &prompts, &tuning, None, None)
            .await
            .unwrap();
        let cluster = result
            .pending
            .iter()
            .find(|cluster| {
                cluster
                    .source_ids
                    .iter()
                    .any(|id| id.starts_with("book_chunk_"))
            })
            .expect("expected one pending cluster containing book chunks");

        let book_count = cluster
            .source_ids
            .iter()
            .filter(|id| id.starts_with("book_chunk_"))
            .count();
        assert!(
            book_count * 2 < cluster.source_ids.len(),
            "one document must be capped below half of the assembled cluster: book_count={book_count}, cluster={:?}",
            cluster.source_ids
        );
        assert!(
            cluster.source_ids.len() >= tuning.page_min_cluster_size,
            "cluster should still form when enough non-document members remain: {:?}",
            cluster.source_ids
        );
    }

    #[tokio::test]
    async fn overlapping_cluster_attaches_to_source_less_seed_no_duplicate() {
        let (db, _dir) = crate::db::tests::test_db().await;
        let now = chrono::Utc::now().to_rfc3339();
        // 1. Source-less CONFIRMED distilled seed on topic "Tokio", in space "work".
        //    insert_page_with_kind embeds title+summary; make summary ~= the cluster
        //    centroid text so cosine >= 0.85. Long body just to be realistic.
        db.insert_page_with_kind(
            "seed_tokio",
            "Tokio async runtime",
            Some("Tokio asynchronous runtime Rust tasks scheduler reactor"),
            "Tokio is an asynchronous runtime for the Rust programming language providing a work-stealing scheduler and a reactor backed by the OS event queue.",
            None,
            Some("work"),
            &[],
            &now,
            "distilled",
            "confirmed",
            None, // workspace
            None, // citations
        ).await.unwrap();

        // 2. A cluster on the same topic with its own memory + a centroid embedding
        //    built from near-identical text (so it clears the 0.85 threshold) and
        //    content > 200 chars (so WITHOUT the attach, synth would fire + duplicate).
        let centroid = db
            .generate_embeddings(&[
                "Tokio asynchronous runtime Rust tasks scheduler reactor".to_string()
            ])
            .unwrap()
            .remove(0);
        let long_content = "Tokio is an asynchronous runtime for the Rust programming language. \
            It provides a multi-threaded work-stealing scheduler, an async TCP and UDP socket API, \
            and a reactor backed by the operating system event queue for scalable network services.".to_string();
        let cluster = crate::db::DistillationCluster {
            source_ids: vec!["mem_x".into()],
            contents: vec![long_content],
            entity_id: None,
            entity_name: Some("Tokio".into()),
            space: Some("work".into()),
            estimated_tokens: 80,
            centroid_embedding: Some(centroid),
        };

        // SANITY: the scoped matcher must actually find the seed (else the test
        // proves nothing — the attach never fires). If THIS fails, the seed
        // summary / centroid text aren't similar enough; make them more identical.
        {
            let probe = db
                .find_matching_page_scoped(
                    None,
                    cluster.centroid_embedding.as_deref().unwrap(),
                    0.85,
                    Some("work"),
                    false,
                )
                .await
                .unwrap();
            assert_eq!(probe.as_ref().map(|p| p.id.as_str()), Some("seed_tokio"),
                "precondition: scoped matcher must find the seed at 0.85 (tune seed summary vs centroid text if this fails)");
        }

        let pages_before = {
            let conn = db.conn.lock().await;
            let mut r = conn
                .query("SELECT COUNT(*) FROM pages WHERE status='active'", ())
                .await
                .unwrap();
            let row = r.next().await.unwrap().unwrap();
            row.get::<i64>(0).unwrap()
        };

        // Faithful, Tokio-topical, guard-passing synth output: nearly a verbatim
        // subset of long_content (>0.6 cosine → clears the hallucination guard)
        // and starts with "- " (summary extraction works). WITHOUT the attach,
        // this would emit a REAL duplicate page — making the no-duplicate
        // assertions load-bearing, not masked by the guard rejecting junk text.
        let llm: Arc<dyn LlmProvider> = Arc::new(MockProvider::new(
            "- Tokio is an asynchronous runtime for the Rust programming language.\n\
             - It provides a multi-threaded work-stealing scheduler, an async TCP and UDP \
             socket API, and a reactor backed by the operating system event queue.",
        ));
        let prompts = PromptRegistry::default();
        let r = distill_one_cluster(&db, &llm, &prompts, &cluster, None)
            .await
            .unwrap();
        assert!(
            r.is_none(),
            "overlapping cluster must NOT emit a new page (attach, don't synth)"
        );

        let pages_after = {
            let conn = db.conn.lock().await;
            let mut r = conn
                .query("SELECT COUNT(*) FROM pages WHERE status='active'", ())
                .await
                .unwrap();
            let row = r.next().await.unwrap().unwrap();
            row.get::<i64>(0).unwrap()
        };
        assert_eq!(pages_before, pages_after, "no duplicate page created");

        // mem_x must now be evidence on the seed (consumed → not re-clustered).
        let ev: Vec<String> = db
            .get_page_evidence("seed_tokio")
            .await
            .unwrap()
            .into_iter()
            .filter_map(|e| e.locator)
            .collect();
        assert!(
            ev.contains(&"mem_x".to_string()),
            "cluster source attached to seed evidence"
        );
    }

    #[tokio::test]
    async fn deep_distill_pages_only_recompiles_stale_non_user_edited() {
        let (db, _db_dir) = crate::db::tests::test_db().await;
        let now = chrono::Utc::now().to_rfc3339();
        let now_ts = chrono::Utc::now().timestamp();

        // Insert a seed memory row so get_memory_contents_by_ids returns it.
        {
            let conn = db.conn.lock().await;
            conn.execute(
                "INSERT INTO memories (id, source_id, title, content, chunk_index, chunk_type, memory_type, space, source_agent, created_at, last_modified, confirmed, stability, source) \
                 VALUES (?1, ?1, ?1, 'seed content', 0, 'text', 'fact', 'test', 'claude-code', ?2, ?2, 1, 'confirmed', 'memory')",
                libsql::params!["mem_1".to_string(), now_ts],
            )
            .await
            .unwrap();
        }

        // 3 pages: stale clean (page_a), stale user-edited (page_b), not stale (page_c).
        db.insert_page("page_a", "A", None, "body a", None, None, &["mem_1"], &now)
            .await
            .unwrap();
        db.insert_page("page_b", "B", None, "body b", None, None, &["mem_1"], &now)
            .await
            .unwrap();
        db.insert_page("page_c", "C", None, "body c", None, None, &["mem_1"], &now)
            .await
            .unwrap();

        db.set_page_stale("page_a", "source_updated").await.unwrap();
        db.set_page_stale("page_b", "source_updated").await.unwrap();

        // Lock page_b via fs_edit — sets user_edited=1 (require_stale=false to force).
        db.try_update_page_content_with_changelog(
            "page_b",
            "user prose b",
            &["mem_1"],
            "fs_edit",
            false,
            "user-edited",
            None,
        )
        .await
        .unwrap();

        let llm: Arc<dyn crate::llm_provider::LlmProvider> =
            Arc::new(crate::llm_provider::MockProvider::new("recompiled"));
        let prompts = crate::prompts::PromptRegistry::default();
        let tuning = crate::tuning::DistillationConfig::default();

        let _ =
            crate::synthesis::distill::deep_distill_pages(&db, Some(&llm), &prompts, &tuning, None)
                .await
                .unwrap();

        let a = db.get_page("page_a").await.unwrap().unwrap();
        let b = db.get_page("page_b").await.unwrap().unwrap();
        let c = db.get_page("page_c").await.unwrap().unwrap();
        assert_eq!(
            a.content, "recompiled",
            "stale clean page should be refreshed"
        );
        assert_eq!(
            b.content, "user prose b",
            "user-edited page must NOT be touched"
        );
        assert_eq!(c.content, "body c", "non-stale page must NOT be touched");
    }
}
