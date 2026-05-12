// SPDX-License-Identifier: Apache-2.0
//! Canonical write-path capability functions. Each fn owns the full create
//! flow for one kind: validation, resolve-or-create (where applicable),
//! storage primitive call, post-write enrichment (verify, log, refinery
//! enqueue). Both HTTP route handlers and daemon-internal extractors call
//! these -- eliminating drift between agent-LLM and daemon-LLM trigger paths.

use crate::db::MemoryDB;
use crate::error::OriginError;
use origin_types::requests::{AddObservationRequest, CreateEntityRequest, CreateRelationRequest};

#[derive(Debug, Clone, serde::Serialize)]
pub struct WriteResult {
    pub id: String,
    pub warnings: Vec<String>,
}

/// Create or resolve an entity. Canonical entry point for both
/// agent-triggered (`/api/memory/entities`) and daemon-internal
/// (`kg/entity_extraction.rs`) writes.
///
/// Resolution order (4-step, matches the prior `importer::resolve_or_create_entity`):
///   1. Alias lookup
///   2. Exact name search
///   3. Vector similarity (distance < 0.1 => sim > 0.9)
///   4. Create new
///
/// Post-write enrichment fires only on newly-created entities. Resolved-existing
/// returns immediately with empty warnings.
pub async fn create_entity(
    db: &MemoryDB,
    req: CreateEntityRequest,
    agent: &str,
) -> Result<WriteResult, OriginError> {
    // Pre-write validation
    let name = req.name.trim();
    if name.is_empty() {
        return Err(OriginError::Validation(
            "entity name must not be empty".into(),
        ));
    }
    let entity_type = req.entity_type.trim();
    if entity_type.is_empty() {
        return Err(OriginError::Validation(
            "entity_type must not be empty".into(),
        ));
    }
    if let Some(c) = req.confidence {
        if !(0.0..=1.0).contains(&c) {
            return Err(OriginError::Validation(format!(
                "confidence {c} out of range [0.0, 1.0]"
            )));
        }
    }

    let name_lower = name.to_lowercase();

    // Step 1: Alias lookup
    if let Some(id) = db.resolve_entity_by_alias(&name_lower).await? {
        return Ok(WriteResult {
            id,
            warnings: vec![],
        });
    }

    // Step 2: Exact name search
    let name_results = db.search_entities_by_name(name).await?;
    if let Some(existing) = name_results.first() {
        let _ = db.add_entity_alias(&name_lower, &existing.id, "auto").await;
        return Ok(WriteResult {
            id: existing.id.clone(),
            warnings: vec![],
        });
    }

    // Step 3: Vector similarity (distance < 0.1 => sim > 0.9)
    let vec_results = db.search_entities_by_vector(name, 1).await?;
    if let Some(result) = vec_results.first() {
        if result.distance < 0.1 {
            let _ = db
                .add_entity_alias(&name_lower, &result.entity.id, "auto")
                .await;
            return Ok(WriteResult {
                id: result.entity.id.clone(),
                warnings: vec![],
            });
        }
    }

    // Step 4: Create new
    let id = db
        .store_entity(
            name,
            entity_type,
            req.domain.as_deref(),
            req.source_agent.as_deref(),
            req.confidence,
        )
        .await?;

    // Post-write enrichment (LLM-free, non-blocking)
    let mut warnings: Vec<String> = Vec::new();

    // 1. Self-retrieval verification
    if let Ok(result) = crate::kg_quality::verify_entity(db, &id, name).await {
        for w in &result.warnings {
            log::warn!("[create_entity] {w}");
            warnings.push(w.clone());
        }
    }

    // 2. Merge-candidate refinery enqueue: similar entity in [0.85, 0.9) with same type
    if let Ok(results) = db.search_entities_by_vector(name, 5).await {
        for r in &results {
            if r.entity.id == id {
                continue;
            }
            if r.entity.entity_type != entity_type {
                continue;
            }
            let sim = 1.0 - r.distance;
            if (0.85..0.9).contains(&sim) {
                let id_len = id.len().min(8);
                let r_id_len = r.entity.id.len().min(8);
                let proposal_id = format!("merge_{}_{}", &id[..id_len], &r.entity.id[..r_id_len]);
                let payload = serde_json::json!({
                    "existing_id": r.entity.id,
                    "new_id": id,
                    "similarity": sim,
                })
                .to_string();
                let _ = db
                    .insert_refinement_proposal(
                        &proposal_id,
                        "entity_merge",
                        &[id.clone(), r.entity.id.clone()],
                        Some(&payload),
                        sim as f64,
                    )
                    .await;
            }
        }
    }

    // 3. Activity log
    let detail = format!("name={name}, type={entity_type}");
    if let Err(e) = db
        .log_agent_activity(
            agent,
            "entity_create",
            std::slice::from_ref(&id),
            None,
            &detail,
        )
        .await
    {
        log::warn!("[create_entity] activity log failed: {e}");
    }

    Ok(WriteResult { id, warnings })
}

/// Create a directed relation between two entities. Canonical entry for
/// both agent-triggered (`/api/memory/relations`) and daemon-internal
/// extraction.
pub async fn create_relation(
    db: &MemoryDB,
    req: CreateRelationRequest,
    agent: &str,
) -> Result<WriteResult, OriginError> {
    // Pre-write validation
    if !db.entity_exists(&req.from_entity).await? {
        return Err(OriginError::Validation(format!(
            "from_entity '{}' does not exist",
            req.from_entity
        )));
    }
    if !db.entity_exists(&req.to_entity).await? {
        return Err(OriginError::Validation(format!(
            "to_entity '{}' does not exist",
            req.to_entity
        )));
    }
    let rt = req.relation_type.trim();
    if !is_valid_snake_case_relation(rt) {
        return Err(OriginError::Validation(format!(
            "relation_type '{rt}' must be lowercase snake_case (^[a-z][a-z0-9_]*$)"
        )));
    }

    let id = db
        .create_relation(
            &req.from_entity,
            &req.to_entity,
            rt,
            req.source_agent.as_deref(),
            None,
            None,
            None,
        )
        .await?;

    // Post-write enrichment
    let mut warnings: Vec<String> = Vec::new();

    // Conflict check: existing relation between same (from, to) with different type
    if let Ok(existing) = db
        .list_relations_between(&req.from_entity, &req.to_entity)
        .await
    {
        for (existing_id, existing_type) in &existing {
            if existing_id != &id && existing_type != rt {
                let id_short: String = id.chars().take(8).collect();
                let exid_short: String = existing_id.chars().take(8).collect();
                let proposal_id = format!("rel_conflict_{id_short}_{exid_short}");
                let payload = serde_json::json!({
                    "existing_id": existing_id,
                    "new_id": id,
                    "from": req.from_entity,
                    "to": req.to_entity,
                    "old_type": existing_type,
                    "new_type": rt,
                })
                .to_string();
                let _ = db
                    .insert_refinement_proposal(
                        &proposal_id,
                        "relation_conflict",
                        &[id.clone(), existing_id.clone()],
                        Some(&payload),
                        0.7,
                    )
                    .await;
                warnings.push(format!(
                    "conflicting relation exists ({}-{}-{}); enqueued for review",
                    req.from_entity, existing_type, req.to_entity
                ));
            }
        }
    }

    // Activity log
    let detail = format!(
        "from={}, to={}, type={}",
        req.from_entity, req.to_entity, rt
    );
    if let Err(e) = db
        .log_agent_activity(
            agent,
            "relation_create",
            std::slice::from_ref(&id),
            None,
            &detail,
        )
        .await
    {
        log::warn!("[create_relation] activity log failed: {e}");
    }

    Ok(WriteResult { id, warnings })
}

/// Add an observation to an existing entity. Canonical entry for both
/// agent-triggered (`/api/memory/observations`) and daemon-internal callers.
pub async fn add_observation(
    db: &MemoryDB,
    req: AddObservationRequest,
    agent: &str,
) -> Result<WriteResult, OriginError> {
    // Pre-write validation
    if !db.entity_exists(&req.entity_id).await? {
        return Err(OriginError::Validation(format!(
            "entity_id '{}' does not exist",
            req.entity_id
        )));
    }
    let content = req.content.trim();
    if content.chars().count() < 5 {
        return Err(OriginError::Validation(
            "observation content must be at least 5 characters".into(),
        ));
    }
    if let Some(c) = req.confidence {
        if !(0.0..=1.0).contains(&c) {
            return Err(OriginError::Validation(format!(
                "confidence {c} out of range [0.0, 1.0]"
            )));
        }
    }

    let id = db
        .add_observation(
            &req.entity_id,
            content,
            req.source_agent.as_deref(),
            req.confidence,
        )
        .await?;

    // Activity log (no verify step yet — observations have no canonical quality check)
    let detail = format!("entity_id={}, content_len={}", req.entity_id, content.len());
    if let Err(e) = db
        .log_agent_activity(
            agent,
            "observation_add",
            std::slice::from_ref(&id),
            None,
            &detail,
        )
        .await
    {
        log::warn!("[add_observation] activity log failed: {e}");
    }

    Ok(WriteResult {
        id,
        warnings: vec![],
    })
}

fn is_valid_snake_case_relation(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::NoopEmitter;
    use std::sync::Arc;

    async fn test_db() -> (MemoryDB, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = MemoryDB::new(&path, Arc::new(NoopEmitter)).await.unwrap();
        (db, dir)
    }

    #[tokio::test]
    async fn create_entity_rejects_empty_name() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "".to_string(),
            entity_type: "person".to_string(),
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_rejects_empty_type() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "".to_string(),
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_rejects_out_of_range_confidence() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: Some(1.5),
        };
        assert!(matches!(
            create_entity(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_entity_happy_path_returns_id() {
        let (db, _dir) = test_db().await;
        let req = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: Some(0.9),
        };
        let result = create_entity(&db, req, "test").await.unwrap();
        assert!(!result.id.is_empty());
    }

    #[tokio::test]
    async fn create_entity_resolves_to_existing_by_name() {
        let (db, _dir) = test_db().await;
        let req1 = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        let first = create_entity(&db, req1, "test").await.unwrap();
        let req2 = CreateEntityRequest {
            name: "Alice".to_string(),
            entity_type: "person".to_string(),
            domain: None,
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        let second = create_entity(&db, req2, "test").await.unwrap();
        assert_eq!(first.id, second.id);
    }

    #[tokio::test]
    async fn create_relation_rejects_missing_from_entity() {
        let (db, _dir) = test_db().await;
        let req = CreateRelationRequest {
            from_entity: "missing-1".to_string(),
            to_entity: "missing-2".to_string(),
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
        };
        assert!(matches!(
            create_relation(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_relation_rejects_bad_relation_type() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req = CreateRelationRequest {
            from_entity: alice,
            to_entity: bob,
            relation_type: "Knows!".to_string(),
            source_agent: Some("test".to_string()),
        };
        assert!(matches!(
            create_relation(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn create_relation_happy_path() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let bob = db
            .store_entity("Bob", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req = CreateRelationRequest {
            from_entity: alice,
            to_entity: bob,
            relation_type: "knows".to_string(),
            source_agent: Some("test".to_string()),
        };
        let result = create_relation(&db, req, "test").await.unwrap();
        assert!(!result.id.is_empty());
    }

    #[tokio::test]
    async fn add_observation_rejects_missing_entity() {
        let (db, _dir) = test_db().await;
        let req = AddObservationRequest {
            entity_id: "no-such-entity".to_string(),
            content: "Alice prefers Rust".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            add_observation(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn add_observation_rejects_short_content() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req = AddObservationRequest {
            entity_id: alice,
            content: "hi".to_string(),
            source_agent: Some("test".to_string()),
            confidence: None,
        };
        assert!(matches!(
            add_observation(&db, req, "test").await,
            Err(OriginError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn add_observation_happy_path() {
        let (db, _dir) = test_db().await;
        let alice = db
            .store_entity("Alice", "person", None, Some("test"), None)
            .await
            .unwrap();
        let req = AddObservationRequest {
            entity_id: alice.clone(),
            content: "Alice prefers Rust over Python".to_string(),
            source_agent: Some("test".to_string()),
            confidence: Some(0.9),
        };
        let result = add_observation(&db, req, "test").await.unwrap();
        assert!(!result.id.is_empty());

        // Verify the observation was actually persisted
        let observations = db
            .get_observations_for_entities(&[alice], 10)
            .await
            .unwrap();
        assert_eq!(observations.len(), 1);
        assert!(observations[0].content.contains("Alice prefers Rust"));
    }
}
