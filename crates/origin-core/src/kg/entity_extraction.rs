// SPDX-License-Identifier: Apache-2.0
//! Entity extraction phase: extract entities/relations from a single memory's content.

use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::prompts::PromptRegistry;
use std::sync::Arc;

/// Extract KG entities from `content` via LLM, create/upsert them in the DB,
/// and return the list of entity ids. Does **not** write to `memory_entities` —
/// the caller is responsible for linking them to a specific memory.
///
/// This is the extraction-only primitive used by `run_enrichment_sweep` (and any
/// other caller that controls when/how linkage happens).
pub async fn extract_entities_for_content(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    content: &str,
) -> Result<Vec<String>, OriginError> {
    let truncated: String = content.chars().take(500).collect();
    let numbered = format!("1. {}", truncated);

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.extract_knowledge_graph.clone()),
            user_prompt: numbered,
            max_tokens: 512,
            temperature: 0.3,
            label: None,
            timeout_secs: None,
        })
        .await
        .map_err(|e| OriginError::Llm(format!("entity extraction: {}", e)))?;

    let batch = [(0usize, content.to_string())];
    let kg_results = crate::extract::parse_kg_response(&response, &batch);

    let mut entity_cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    for kg in &kg_results {
        for entity in &kg.entities {
            let name_lower = entity.name.to_lowercase();
            if entity_cache.contains_key(&name_lower) {
                continue;
            }
            let req = origin_types::requests::CreateEntityRequest {
                name: entity.name.clone(),
                entity_type: entity.entity_type.clone(),
                space: None,
                source_agent: Some("post_ingest".to_string()),
                confidence: None,
            };
            match crate::post_write::create_entity(db, req, "post_ingest").await {
                Ok(result) => {
                    entity_cache.insert(name_lower, result.id);
                }
                Err(e) => log::warn!("[extract_entities_for_content] entity create failed: {e}"),
            }
        }
        for obs in &kg.observations {
            if let Some(entity_id) = entity_cache.get(&obs.entity.to_lowercase()) {
                let req = origin_types::requests::AddObservationRequest {
                    entity_id: entity_id.clone(),
                    content: obs.content.clone(),
                    source_agent: Some("post_ingest".to_string()),
                    confidence: None,
                };
                if let Err(e) = crate::post_write::add_observation(db, req, "post_ingest").await {
                    log::warn!("[extract_entities_for_content] add_observation failed: {e}");
                }
            }
        }
        for rel in &kg.relations {
            let from_id = entity_cache.get(&rel.from.to_lowercase()).cloned();
            let to_id = entity_cache.get(&rel.to.to_lowercase()).cloned();
            if let (Some(from), Some(to)) = (from_id, to_id) {
                let req = origin_types::requests::CreateRelationRequest {
                    from_entity: from,
                    to_entity: to,
                    relation_type: rel.relation_type.clone(),
                    source_agent: Some("post_ingest".to_string()),
                    confidence: rel.confidence,
                    explanation: rel.explanation.clone(),
                    source_memory_id: None,
                };
                if let Err(e) = crate::post_write::create_relation(db, req, "post_ingest").await {
                    log::warn!("[extract_entities_for_content] create_relation failed: {e}");
                }
            }
        }
    }

    Ok(entity_cache.into_values().collect())
}

/// Extract entities from a single memory via LLM. Returns the primary entity_id if one was created/found.
pub async fn extract_single_memory_entities(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
    source_id: &str,
    content: &str,
) -> Result<Option<String>, OriginError> {
    let truncated: String = content.chars().take(500).collect();
    let numbered = format!("1. {}", truncated);

    let response = llm
        .generate(LlmRequest {
            system_prompt: Some(prompts.extract_knowledge_graph.clone()),
            user_prompt: numbered,
            max_tokens: 512,
            temperature: 0.3,
            label: None,
            timeout_secs: None,
        })
        .await
        .map_err(|e| OriginError::Llm(format!("entity extraction: {}", e)))?;

    let batch = [(0usize, content.to_string())];
    let kg_results = crate::extract::parse_kg_response(&response, &batch);

    let mut entity_cache: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    let mut first_entity_id: Option<String> = None;

    for kg in &kg_results {
        for entity in &kg.entities {
            let name_lower = entity.name.to_lowercase();
            // In-batch cache (per-extraction-pass dedup; capability fn doesn't take a cache)
            if let Some(id) = entity_cache.get(&name_lower) {
                if first_entity_id.is_none() {
                    first_entity_id = Some(id.clone());
                }
                continue;
            }
            let req = origin_types::requests::CreateEntityRequest {
                name: entity.name.clone(),
                entity_type: entity.entity_type.clone(),
                space: None,
                source_agent: Some("post_ingest".to_string()),
                confidence: None,
            };
            match crate::post_write::create_entity(db, req, "post_ingest").await {
                Ok(result) => {
                    entity_cache.insert(name_lower, result.id.clone());
                    if first_entity_id.is_none() {
                        first_entity_id = Some(result.id);
                    }
                }
                Err(e) => log::warn!("[post_ingest] entity create failed: {e}"),
            }
        }
        for obs in &kg.observations {
            if let Some(entity_id) = entity_cache.get(&obs.entity.to_lowercase()) {
                let req = origin_types::requests::AddObservationRequest {
                    entity_id: entity_id.clone(),
                    content: obs.content.clone(),
                    source_agent: Some("post_ingest".to_string()),
                    confidence: None,
                };
                if let Err(e) = crate::post_write::add_observation(db, req, "post_ingest").await {
                    log::warn!("[post_ingest] add_observation failed: {e}");
                }
            }
        }
        for rel in &kg.relations {
            let from_id = entity_cache.get(&rel.from.to_lowercase()).cloned();
            let to_id = entity_cache.get(&rel.to.to_lowercase()).cloned();
            if let (Some(from), Some(to)) = (from_id, to_id) {
                let req = origin_types::requests::CreateRelationRequest {
                    from_entity: from,
                    to_entity: to,
                    relation_type: rel.relation_type.clone(),
                    source_agent: Some("post_ingest".to_string()),
                    confidence: rel.confidence,
                    explanation: rel.explanation.clone(),
                    source_memory_id: Some(source_id.to_string()),
                };
                if let Err(e) = crate::post_write::create_relation(db, req, "post_ingest").await {
                    log::warn!("[post_ingest] create_relation failed: {e}");
                }
            }
        }
    }

    // Link memory to first entity (1-1 legacy column).
    if let Some(ref eid) = first_entity_id {
        let _ = db.update_memory_entity_id(source_id, eid).await;
    }

    // Write all extracted entities into the many-to-many junction table.
    if !entity_cache.is_empty() {
        let ids: Vec<&str> = entity_cache.values().map(|s| s.as_str()).collect();
        if let Err(e) = db.link_memory_entities(source_id, &ids).await {
            log::warn!("[post_ingest] link_memory_entities failed: {e}");
        }
    }

    Ok(first_entity_id)
}
