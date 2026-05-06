// SPDX-License-Identifier: Apache-2.0
//! Entity extraction phase: extract entities/relations from a single memory's content.

use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::prompts::PromptRegistry;
use std::sync::Arc;

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
            match crate::importer::resolve_or_create_entity(
                db,
                &mut entity_cache,
                entity,
                "post_ingest",
            )
            .await
            {
                Ok((id, _created)) => {
                    if first_entity_id.is_none() {
                        first_entity_id = Some(id);
                    }
                }
                Err(e) => log::warn!("[post_ingest] entity create failed: {e}"),
            }
        }
        for obs in &kg.observations {
            if let Some(entity_id) = entity_cache.get(&obs.entity.to_lowercase()) {
                let _ = db
                    .add_observation(entity_id, &obs.content, Some("post_ingest"), None)
                    .await;
            }
        }
        for rel in &kg.relations {
            let from_id = entity_cache.get(&rel.from.to_lowercase()).cloned();
            let to_id = entity_cache.get(&rel.to.to_lowercase()).cloned();
            if let (Some(from), Some(to)) = (from_id, to_id) {
                let _ = db
                    .create_relation(
                        &from,
                        &to,
                        &rel.relation_type,
                        Some("post_ingest"),
                        rel.confidence,
                        rel.explanation.as_deref(),
                        Some(source_id),
                    )
                    .await;
            }
        }
    }

    // Link memory to first entity
    if let Some(ref eid) = first_entity_id {
        let _ = db.update_memory_entity_id(source_id, eid).await;
    }

    Ok(first_entity_id)
}
