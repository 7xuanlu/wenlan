// SPDX-License-Identifier: Apache-2.0
//! Distillation phase — turn memory clusters into structured concept pages.
//!
//! This module owns the synthesis side of the refinery: clustering memories,
//! merging/splitting clusters via LLM, and recompiling concept pages from
//! source memories. Re-exported from `crate::refinery` for API stability.

use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::prompts::PromptRegistry;
use crate::refinery::helpers::{
    is_all_generic_tokens, looks_like_code, looks_like_commit_message, looks_like_markup_styled,
    looks_like_path, looks_like_short_hash, looks_like_uuid,
};
use crate::sources::StabilityTier;
use std::sync::Arc;

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
                                // "keep" and "split" — split is complex (needs new clusters), defer to global_concept_review
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

/// Process a single distillation cluster.
///
/// Returns `Ok(true)` if a concept was created, `Ok(false)` if the cluster was skipped.
/// Extracted from `distill_pages` to enable parallel cluster processing via
/// `DISTILL_CLUSTER_CONCURRENCY`.
pub(crate) async fn distill_one_cluster(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    cluster: &crate::db::DistillationCluster,
    knowledge_writer: Option<&crate::export::knowledge::KnowledgeWriter>,
) -> Result<bool, OriginError> {
    let topic = cluster
        .entity_name
        .as_deref()
        .or(cluster.domain.as_deref())
        .unwrap_or("general");

    // Skip if a concept with very similar sources already exists (Jaccard > 0.8)
    // Memories CAN appear in multiple concepts — this only prevents duplicate concepts
    let overlap = db
        .max_page_overlap(&cluster.source_ids)
        .await
        .unwrap_or(0.0);
    if overlap > 0.8 {
        log::info!(
            "[emergence] cluster '{}' overlaps {:.0}% with existing concept, skipping",
            topic,
            overlap * 100.0
        );
        return Ok(false);
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
        return Ok(false);
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
    let memories_block: String = cluster
        .source_ids
        .iter()
        .zip(cleaned_contents.iter())
        .map(|(id, content)| {
            let snippet: String = content.chars().take(MEM_SNIPPET_CAP).collect();
            let snippet = if content.chars().count() > MEM_SNIPPET_CAP {
                format!("{}...", snippet.trim_end())
            } else {
                snippet
            };
            format!("[{}] {}", id, snippet)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let user_prompt = format!("Topic: {}\n\n{}", topic, memories_block);

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.distill_concept.clone()),
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
                return Ok(false);
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
                        return Ok(false);
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
            // is worse than no concept at all.
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
                    return Ok(false);
                }
                None => topic.to_string(),
            };

            // Extract summary from first bullet point
            let summary = content
                .lines()
                .find(|l| l.starts_with("- "))
                .map(|l| l.trim_start_matches("- ").to_string());

            // Build source IDs as &str refs
            let source_refs: Vec<&str> = cluster.source_ids.iter().map(|s| s.as_str()).collect();
            let now = chrono::Utc::now().to_rfc3339();
            let page_id = crate::pages::Page::new_id();

            db.insert_page(
                &page_id,
                &title,
                summary.as_deref(),
                &content,
                cluster.entity_id.as_deref(),
                cluster.domain.as_deref(),
                &source_refs,
                &now,
            )
            .await?;

            log::info!(
                "[distill] distilled {} memories -> concept '{}' ('{}')",
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
                .log_agent_activity(
                    "system",
                    "concept_create",
                    &source_memory_ids,
                    None,
                    &detail,
                )
                .await
            {
                log::warn!("[distill] log concept_create activity failed: {e}");
            }

            if let Some(writer) = knowledge_writer {
                if let Ok(Some(c)) = db.get_page(&page_id).await {
                    match writer.write_concept(&c) {
                        Ok(p) => log::info!("[distill] wrote concept to {p}"),
                        Err(e) => log::warn!("[distill] knowledge write failed: {e}"),
                    }
                }
            }

            Ok(true)
        }
        Ok(_) => {
            log::warn!("[distill] empty output for topic='{}'", topic);
            Ok(false)
        }
        Err(e) => {
            log::warn!("[distill] LLM error for topic='{}': {}", topic, e);
            Ok(false)
        }
    }
}

/// Distill memory clusters into structured concepts.
/// Memories can appear in multiple concepts. Jaccard overlap prevents duplicate concepts.
pub async fn distill_pages(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    tuning: &crate::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
) -> Result<usize, OriginError> {
    let llm = match llm {
        Some(l) if l.is_available() => l,
        _ => return Ok(0),
    };

    // Each model carries its own effective synthesis limit — the max tokens it
    // can meaningfully synthesize (not just read). Research-calibrated per model
    // in on_device_models.rs and llm_provider.rs. Falls back to tuning config
    // if the provider returns the default (for backward compat).
    let token_limit = llm.synthesis_token_limit();
    let raw_clusters = db
        .find_distillation_clusters(
            tuning.similarity_threshold,
            tuning.concept_min_cluster_size,
            tuning.max_clusters_per_steep,
            token_limit,
            tuning.max_unlinked_cluster_size,
        )
        .await?;

    // LLM cluster refinement: let LLM merge/split/rename clusters per entity
    let clusters = refine_clusters_with_llm(llm, prompts, raw_clusters, token_limit).await;

    let cluster_concurrency: usize = std::env::var("DISTILL_CLUSTER_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
        .min(4);

    let mut distilled = 0usize;

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
                if r? {
                    distilled += 1;
                }
            }
        }
        return Ok(distilled);
    }

    for cluster in &clusters {
        let topic = cluster
            .entity_name
            .as_deref()
            .or(cluster.domain.as_deref())
            .unwrap_or("general");

        // Skip if a concept with very similar sources already exists (Jaccard > 0.8)
        // Memories CAN appear in multiple concepts — this only prevents duplicate concepts
        let overlap = db
            .max_page_overlap(&cluster.source_ids)
            .await
            .unwrap_or(0.0);
        if overlap > 0.8 {
            log::info!(
                "[emergence] cluster '{}' overlaps {:.0}% with existing concept, skipping",
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
        let memories_block: String = cluster
            .source_ids
            .iter()
            .zip(cleaned_contents.iter())
            .map(|(id, content)| {
                let snippet: String = content.chars().take(MEM_SNIPPET_CAP).collect();
                let snippet = if content.chars().count() > MEM_SNIPPET_CAP {
                    format!("{}...", snippet.trim_end())
                } else {
                    snippet
                };
                format!("[{}] {}", id, snippet)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let user_prompt = format!("Topic: {}\n\n{}", topic, memories_block);

        let response = llm
            .generate(LlmRequest {
                system_prompt: Some(prompts.distill_concept.clone()),
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
                // is worse than no concept at all.
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

                // Build source IDs as &str refs
                let source_refs: Vec<&str> =
                    cluster.source_ids.iter().map(|s| s.as_str()).collect();
                let now = chrono::Utc::now().to_rfc3339();
                let page_id = crate::pages::Page::new_id();

                db.insert_page(
                    &page_id,
                    &title,
                    summary.as_deref(),
                    &content,
                    cluster.entity_id.as_deref(),
                    cluster.domain.as_deref(),
                    &source_refs,
                    &now,
                )
                .await?;

                log::info!(
                    "[distill] distilled {} memories -> concept '{}' ('{}')",
                    cluster.source_ids.len(),
                    title,
                    content.chars().take(40).collect::<String>()
                );
                distilled += 1;

                // Log activity — system-attributed, since distillation is background refinery work.
                let source_memory_ids: Vec<String> = cluster.source_ids.to_vec();
                let detail = format!(
                    "created \"{}\" from {} memories",
                    title,
                    cluster.source_ids.len()
                );
                if let Err(e) = db
                    .log_agent_activity(
                        "system",
                        "concept_create",
                        &source_memory_ids,
                        None,
                        &detail,
                    )
                    .await
                {
                    log::warn!("[distill] log concept_create activity failed: {e}");
                }

                if let Some(ref writer) = knowledge_writer {
                    if let Ok(Some(c)) = db.get_page(&page_id).await {
                        match writer.write_concept(&c) {
                            Ok(p) => log::info!("[distill] wrote concept to {p}"),
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

    Ok(distilled)
}

/// Full Karpathy-style deep distill: emergence + orphans + recompile ALL + global review.
/// Triggered by "Distill now" button or weekly background schedule.
pub async fn deep_distill_pages(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    tuning: &crate::tuning::DistillationConfig,
    knowledge_path: Option<&std::path::Path>,
) -> Result<usize, OriginError> {
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
    match crate::refinery::assign_orphan_memories(db, llm_ref, prompts, tuning, knowledge_path)
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

    // 3. Recompile ALL active concepts (not just changed ones — full refresh)
    let all_active = db.list_pages("active", 200, 0).await?;
    for concept in &all_active {
        match recompile_single_page(db, llm_ref, prompts, concept).await {
            Ok(true) => total += 1,
            Ok(false) => {}
            Err(e) => log::warn!(
                "[deep_distill] recompile failed for '{}': {}",
                concept.title,
                e
            ),
        }
    }

    // 4. Global review — merge/split/create analysis
    if all_active.len() >= 5 {
        match crate::refinery::global_page_review(db, llm_ref, prompts, &all_active).await {
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

/// Recompile a single concept from its source memories via LLM.
pub(crate) async fn recompile_single_page(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    concept: &crate::pages::Page,
) -> Result<bool, OriginError> {
    let memories = db
        .get_memory_contents_by_ids(&concept.source_memory_ids)
        .await?;
    if memories.is_empty() {
        log::warn!(
            "[re-distill] concept '{}' has no source memories, skipping",
            concept.id
        );
        return Ok(false);
    }

    const MEM_SNIPPET_CAP: usize = 800;
    let memories_block: String = memories
        .iter()
        .map(|(id, content)| {
            let snippet: String = content.chars().take(MEM_SNIPPET_CAP).collect();
            let snippet = if content.chars().count() > MEM_SNIPPET_CAP {
                format!("{}...", snippet.trim_end())
            } else {
                snippet
            };
            format!("[{}] {}", id, snippet)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let user_prompt = format!("Topic: {}\n\n{}", concept.title, memories_block);

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.distill_concept.clone()),
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
                let source_refs: Vec<&str> = concept
                    .source_memory_ids
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                db.update_page_content(&concept.id, &content, &source_refs, "re_distill")
                    .await?;
                log::info!("[re-distill] refreshed concept '{}'", concept.title);
                return Ok(true);
            }
        }
        Ok(_) => log::warn!("[re-distill] empty output for '{}'", concept.title),
        Err(e) => log::warn!("[re-distill] LLM error for '{}': {}", concept.title, e),
    }
    Ok(false)
}

/// Re-distill a single concept by reloading all source memories and recompiling with LLM.
pub async fn deep_distill_single(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    page_id: &str,
) -> Result<(), OriginError> {
    let llm = match llm {
        Some(l) if l.is_available() => l,
        Some(_) => {
            return Err(OriginError::Llm(
                "LLM not available for re-distillation".into(),
            ))
        }
        None => {
            return Err(OriginError::Llm(
                "No LLM available for re-distillation".into(),
            ))
        }
    };

    let concept = db
        .get_page(page_id)
        .await?
        .ok_or_else(|| OriginError::VectorDb(format!("Concept {} not found", page_id)))?;

    let memories = db
        .get_memory_contents_by_ids(&concept.source_memory_ids)
        .await?;
    if memories.is_empty() {
        log::warn!("[distill] no source memories found for concept {}", page_id);
        return Ok(());
    }

    const MEM_SNIPPET_CAP: usize = 800;
    let memories_block: String = memories
        .iter()
        .map(|(id, content)| {
            let snippet: String = content.chars().take(MEM_SNIPPET_CAP).collect();
            let snippet = if content.chars().count() > MEM_SNIPPET_CAP {
                format!("{}...", snippet.trim_end())
            } else {
                snippet
            };
            format!("[{}] {}", id, snippet)
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let user_prompt = format!("Topic: {}\n\n{}", concept.title, memories_block);

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.distill_concept.clone()),
            user_prompt,
            max_tokens: llm.recommended_max_output(),
            temperature: 0.1,
            label: Some("distill_body".into()),
            timeout_secs: None,
        })
        .await
        .map_err(|e| OriginError::Llm(format!("re-distill LLM: {}", e)))?;

    let content = crate::llm_provider::strip_think_tags(&response)
        .trim()
        .to_string();

    if content.is_empty() {
        log::warn!(
            "[distill] empty output for concept '{}', skipping",
            concept.title
        );
        return Ok(());
    }

    let source_refs: Vec<&str> = concept
        .source_memory_ids
        .iter()
        .map(|s| s.as_str())
        .collect();
    db.update_page_content(page_id, &content, &source_refs, "distill")
        .await?;

    log::info!(
        "[distill] re-distilled concept '{}' (v{}->v{})",
        concept.title,
        concept.version,
        concept.version + 1
    );
    Ok(())
}

/// Apply a merge result based on the stability tier of the involved memories.
pub(crate) async fn apply_merge_by_tier(
    db: &MemoryDB,
    source_ids: &[String],
    merged_content: &str,
    proposal_id: &str,
    tier: &StabilityTier,
) -> Result<(), OriginError> {
    match tier {
        StabilityTier::Ephemeral => {
            // Auto-apply silently
            db.apply_merge(source_ids, merged_content).await?;
            db.resolve_refinement(proposal_id, "auto_applied").await?;
            log::info!(
                "[refinery] auto-applied merge (ephemeral) for {}",
                proposal_id
            );
        }
        StabilityTier::Standard => {
            // Auto-apply with notification (toast emitted by caller if app_handle available)
            db.apply_merge(source_ids, merged_content).await?;
            db.resolve_refinement(proposal_id, "auto_applied").await?;
            log::info!(
                "[refinery] auto-applied merge (standard, notify) for {}",
                proposal_id
            );
        }
        StabilityTier::Protected => {
            // Queue for human review — don't auto-apply
            db.resolve_refinement(proposal_id, "awaiting_review")
                .await?;
            log::info!(
                "[refinery] queued merge for review (protected) for {}",
                proposal_id
            );
        }
    }
    Ok(())
}
