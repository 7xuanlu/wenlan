// SPDX-License-Identifier: Apache-2.0

mod duplicates;
mod page_merge_order;

#[cfg(test)]
mod survivor_tests;

use std::path::Path;
use std::sync::Arc;

use crate::db::MemoryDB;
use crate::error::WenlanError;
use crate::llm_provider::LlmProvider;
use crate::post_write::page_is_human_owned;
use crate::prompts::PromptRegistry;
use crate::synthesis::distill::{refresh_page, RefreshReason};

#[derive(Debug, Clone)]
pub struct MaintenanceTickConfig {
    pub page_match_threshold: f64,
    pub formation_threshold: f64,
    pub page_min_cluster_size: usize,
    pub token_limit: usize,
    pub max_unlinked_cluster_size: usize,
    pub max_grouped_cluster_size: usize,
    pub max_per_tick: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MaintenanceTickResult {
    pub merge_cards_emitted: usize,
    pub stale_machine_refreshed: usize,
    pub stale_machine_queued: usize,
    pub stale_human_cards: usize,
    pub stale_human_queued: usize,
    pub orphan_labels_checked: usize,
    pub overview_refreshed: usize,
    pub discovery_cards_emitted: usize,
}

pub async fn run_maintenance_tick(
    db: &MemoryDB,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    config: &MaintenanceTickConfig,
    knowledge_path: Option<&Path>,
) -> Result<MaintenanceTickResult, WenlanError> {
    let mut result = MaintenanceTickResult::default();
    let max_per_tick = config.max_per_tick.max(1);

    let near_duplicates =
        duplicates::detect_near_duplicate_pages(db, config.page_match_threshold, max_per_tick)
            .await?;
    for pair in near_duplicates.iter().take(max_per_tick) {
        if emit_page_merge_card(db, pair).await? {
            result.merge_cards_emitted += 1;
        }
    }

    result.orphan_labels_checked = db.list_orphan_link_labels(1).await?.len();

    let discovery_clusters = db
        .find_cross_space_distillation_clusters(
            config.formation_threshold,
            config.page_min_cluster_size,
            max_per_tick,
            config.token_limit,
            config.max_unlinked_cluster_size,
            config.max_grouped_cluster_size,
        )
        .await?;
    for cluster in discovery_clusters.iter().take(max_per_tick) {
        if emit_cross_space_discovery_card(db, &cluster.source_ids).await? {
            result.discovery_cards_emitted += 1;
        }
    }

    let Some(provider) = llm.filter(|provider| provider.is_available()) else {
        let stale = db.list_stale_pages("source_updated").await?;
        for page in stale.iter().take(max_per_tick) {
            if page_is_human_owned(page) {
                result.stale_human_queued += 1;
            } else {
                result.stale_machine_queued += 1;
            }
        }
        return Ok(result);
    };

    let stale = db.list_stale_pages("source_updated").await?;
    for page in stale.iter().take(max_per_tick) {
        let human_owned = page_is_human_owned(page);
        let outcome = refresh_page(
            db,
            provider,
            prompts,
            &page.id,
            RefreshReason::SourceChanged,
            knowledge_path,
        )
        .await?;
        if outcome.wrote || outcome.gated {
            db.clear_page_staleness(&page.id).await?;
        }
        if human_owned || outcome.gated {
            result.stale_human_cards += usize::from(outcome.gated);
        } else {
            result.stale_machine_refreshed += usize::from(outcome.wrote);
        }
    }

    let overview = crate::synthesis::overview::refresh_overview_page(
        db,
        provider,
        prompts,
        "maintenance",
        knowledge_path,
    )
    .await?;
    result.overview_refreshed = usize::from(overview.wrote);

    Ok(result)
}

async fn emit_page_merge_card(
    db: &MemoryDB,
    pair: &duplicates::NearDuplicatePair,
) -> Result<bool, WenlanError> {
    let id = page_merge_card_id(&pair.left_id, &pair.right_id);
    if db.get_refinement_proposal(&id).await?.is_some() {
        return Ok(false);
    }
    let Some(order) = page_merge_order::order_survivor(db, &pair.left_id, &pair.right_id).await?
    else {
        return Ok(false);
    };
    let survivor_id = order.survivor_id;
    let absorbed_id = order.absorbed_id;

    let payload = serde_json::json!({
        "left_page_id": &survivor_id,
        "right_page_id": &absorbed_id,
        "similarity": pair.similarity,
        "source_overlap": pair.source_overlap,
        "source_overlap_ratio": pair.source_overlap_ratio,
    })
    .to_string();
    let confidence = pair
        .similarity
        .unwrap_or(pair.source_overlap_ratio)
        .clamp(0.0, 1.0);
    db.insert_refinement_proposal(
        &id,
        "page_merge",
        &[survivor_id, absorbed_id],
        Some(&payload),
        confidence,
    )
    .await?;
    db.resolve_refinement_if_open(&id, "awaiting_review")
        .await?;
    Ok(true)
}

fn page_merge_card_id(left: &str, right: &str) -> String {
    let (first, second) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    format!(
        "page_merge_{}_{}",
        stable_fragment(first),
        stable_fragment(second)
    )
}

async fn emit_cross_space_discovery_card(
    db: &MemoryDB,
    source_ids: &[String],
) -> Result<bool, WenlanError> {
    let spaces = spaces_for_sources(db, source_ids).await?;
    if spaces.len() < 2 {
        return Ok(false);
    }

    let id = cross_space_discovery_card_id(source_ids);
    if db.get_refinement_proposal(&id).await?.is_some() {
        return Ok(false);
    }

    let payload = serde_json::json!({
        "memory_count": source_ids.len(),
        "spaces": spaces,
        "allowed_actions": ["dismiss", "pick_space"],
    })
    .to_string();
    db.insert_refinement_proposal(
        &id,
        "cross_space_discovery",
        source_ids,
        Some(&payload),
        1.0,
    )
    .await?;
    db.resolve_refinement_if_open(&id, "awaiting_review")
        .await?;
    Ok(true)
}

async fn spaces_for_sources(
    db: &MemoryDB,
    source_ids: &[String],
) -> Result<Vec<String>, WenlanError> {
    let mut spaces = std::collections::BTreeSet::new();
    for source_id in source_ids {
        if let Some(space) = db.get_memory_space(source_id).await? {
            if !space.is_empty() {
                spaces.insert(space);
            }
        }
    }
    Ok(spaces.into_iter().collect())
}

fn cross_space_discovery_card_id(source_ids: &[String]) -> String {
    let mut ids = source_ids.to_vec();
    ids.sort();
    let mut hash = 0xcbf29ce484222325u64;
    for id in ids {
        for byte in id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("cross_space_discovery_{hash:016x}")
}

fn stable_fragment(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::events::NoopEmitter;
    use crate::llm_provider::{LlmBackend, LlmError, LlmProvider, LlmRequest};
    use std::sync::Arc;

    struct TestProvider {
        body: String,
    }

    #[async_trait::async_trait]
    impl LlmProvider for TestProvider {
        async fn generate(&self, _request: LlmRequest) -> Result<String, LlmError> {
            Ok(self.body.clone())
        }

        fn is_available(&self) -> bool {
            true
        }

        fn name(&self) -> &str {
            "maintenance-test"
        }

        fn backend(&self) -> LlmBackend {
            LlmBackend::Api
        }

        fn kind(&self) -> &'static str {
            "mock"
        }
    }

    async fn new_test_db() -> (MemoryDB, tempfile::TempDir) {
        let db_dir = tempfile::tempdir().unwrap();
        let db = MemoryDB::new(db_dir.path(), Arc::new(NoopEmitter))
            .await
            .unwrap();
        (db, db_dir)
    }

    async fn store_test_memory(db: &MemoryDB, id: &str, content: &str) {
        db.upsert_documents(vec![wenlan_types::RawDocument {
            source: "memory".to_string(),
            source_id: id.to_string(),
            title: id.to_string(),
            content: content.to_string(),
            last_modified: chrono::Utc::now().timestamp(),
            memory_type: Some("fact".to_string()),
            source_agent: Some("test".to_string()),
            confirmed: Some(true),
            ..Default::default()
        }])
        .await
        .unwrap();
    }

    async fn insert_test_page(db: &MemoryDB, id: &str, content: &str, source_ids: &[&str]) {
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page_with_kind(
            id,
            id,
            None,
            content,
            None,
            None,
            source_ids,
            &now,
            "research",
            "confirmed",
            Some("work"),
            Some("[]"),
        )
        .await
        .unwrap();
    }

    fn config() -> MaintenanceTickConfig {
        MaintenanceTickConfig {
            page_match_threshold: 0.85,
            formation_threshold: 0.60,
            page_min_cluster_size: 3,
            token_limit: 3500,
            max_unlinked_cluster_size: 20,
            max_grouped_cluster_size: 20,
            max_per_tick: 5,
        }
    }

    fn discovery_config() -> MaintenanceTickConfig {
        MaintenanceTickConfig {
            page_match_threshold: 0.85,
            formation_threshold: 0.80,
            page_min_cluster_size: 3,
            token_limit: 3500,
            max_unlinked_cluster_size: 20,
            max_grouped_cluster_size: 20,
            max_per_tick: 5,
        }
    }

    fn vec_to_sql(v: &[f32]) -> String {
        let mut out = String::with_capacity(v.len() * 10);
        out.push('[');
        for (i, value) in v.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            use std::fmt::Write;
            let _ = write!(out, "{value:.6}");
        }
        out.push(']');
        out
    }

    fn unit_vec(axis: usize) -> Vec<f32> {
        let mut v = vec![0.0; 768];
        v[axis] = 1.0;
        v
    }

    async fn insert_staging_memory(
        db: &MemoryDB,
        source_id: &str,
        content: &str,
        space: &str,
        embedding: &[f32],
        last_modified: i64,
    ) {
        let embedding_sql = vec_to_sql(embedding);
        let conn = db.conn.lock().await;
        conn.execute(
            "INSERT INTO memories (
                id, content, source, source_id, title, chunk_index, last_modified,
                chunk_type, memory_type, space, source_agent, confidence,
                confirmed, word_count, enrichment_status, quality, is_recap,
                supersede_mode, stability, embedding
             )
             VALUES (
                ?1, ?2, 'memory', ?1, ?3, 0, ?4, 'document', 'fact',
                ?5, 'codex-test', 0.9, 1, 8, 'enriched', 'high', 0,
                'hide', 'learned', vector32(?6)
             )",
            libsql::params![
                source_id,
                content,
                content.chars().take(40).collect::<String>(),
                last_modified,
                space,
                embedding_sql.as_str(),
            ],
        )
        .await
        .unwrap();
    }

    async fn active_page_count(db: &MemoryDB) -> i64 {
        let conn = db.conn.lock().await;
        let mut rows = conn
            .query("SELECT COUNT(*) FROM pages WHERE status = 'active'", ())
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
    }

    async fn page_source_count(db: &MemoryDB) -> i64 {
        let conn = db.conn.lock().await;
        let mut rows = conn
            .query("SELECT COUNT(*) FROM page_sources", ())
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
    }

    #[tokio::test]
    async fn cross_space_topic_emits_one_discovery_card_without_page_mutation() {
        let (db, _db_dir) = new_test_db().await;
        let embedding = unit_vec(42);
        let now = chrono::Utc::now().timestamp();
        for (source_id, space) in [
            ("cross_disc_work_a", "work"),
            ("cross_disc_personal_a", "personal"),
            ("cross_disc_work_b", "work"),
        ] {
            insert_staging_memory(
                &db,
                source_id,
                "Incremental Rust compilation cache tuning for shared developer machines.",
                space,
                &embedding,
                now,
            )
            .await;
        }

        let result = run_maintenance_tick(
            &db,
            None,
            &PromptRegistry::default(),
            &discovery_config(),
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.discovery_cards_emitted, 1);
        assert_eq!(active_page_count(&db).await, 0);
        assert_eq!(page_source_count(&db).await, 0);

        let discovery_cards: Vec<_> = db
            .get_pending_refinements()
            .await
            .unwrap()
            .into_iter()
            .filter(|p| p.action == "cross_space_discovery")
            .collect();
        assert_eq!(
            discovery_cards.len(),
            1,
            "one cross-space cluster should produce exactly one discovery card"
        );
        assert_eq!(discovery_cards[0].source_ids.len(), 3);
        let payload = discovery_cards[0]
            .payload
            .as_deref()
            .expect("discovery card should carry a typed payload");
        assert!(
            payload.contains("\"memory_count\":3"),
            "payload should describe the card prompt facts, got {payload}"
        );
        assert!(
            payload.contains("\"work\"") && payload.contains("\"personal\""),
            "payload should name every space involved, got {payload}"
        );
    }

    #[tokio::test]
    async fn dismissed_cross_space_discovery_card_stays_dismissed_across_next_tick() {
        let (db, _db_dir) = new_test_db().await;
        let embedding = unit_vec(43);
        let now = chrono::Utc::now().timestamp();
        for (source_id, space) in [
            ("cross_dismiss_work_a", "work"),
            ("cross_dismiss_personal_a", "personal"),
            ("cross_dismiss_work_b", "work"),
        ] {
            insert_staging_memory(
                &db,
                source_id,
                "Cross-space topic cards should not resurrect after dismissal.",
                space,
                &embedding,
                now,
            )
            .await;
        }

        let first = run_maintenance_tick(
            &db,
            None,
            &PromptRegistry::default(),
            &discovery_config(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(first.discovery_cards_emitted, 1);
        let card = db
            .get_pending_refinements()
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.action == "cross_space_discovery")
            .expect("cross-space discovery card exists");
        db.resolve_refinement_if_open(&card.id, "dismissed")
            .await
            .unwrap();

        let second = run_maintenance_tick(
            &db,
            None,
            &PromptRegistry::default(),
            &discovery_config(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(second.discovery_cards_emitted, 0);
        let dismissed = db
            .get_refinement_proposal(&card.id)
            .await
            .unwrap()
            .expect("dismissed discovery card remains");
        assert_eq!(dismissed.status, "dismissed");
    }

    #[tokio::test]
    async fn dismissed_page_merge_card_stays_dismissed_across_next_tick() {
        let (db, _db_dir) = new_test_db().await;
        let source = "Rust ownership prevents data races at compile time.";
        for id in ["mem_dup_a", "mem_dup_b", "mem_dup_c"] {
            store_test_memory(&db, id, source).await;
        }
        insert_test_page(
            &db,
            "page_dup_a",
            source,
            &["mem_dup_a", "mem_dup_b", "mem_dup_c"],
        )
        .await;
        insert_test_page(
            &db,
            "page_dup_b",
            source,
            &["mem_dup_a", "mem_dup_b", "mem_dup_c"],
        )
        .await;

        let first = run_maintenance_tick(&db, None, &PromptRegistry::default(), &config(), None)
            .await
            .unwrap();
        assert_eq!(first.merge_cards_emitted, 1);
        let card = db
            .get_pending_refinements()
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.action == "page_merge")
            .expect("page_merge card exists");
        db.resolve_refinement_if_open(&card.id, "dismissed")
            .await
            .unwrap();

        let second = run_maintenance_tick(&db, None, &PromptRegistry::default(), &config(), None)
            .await
            .unwrap();
        assert_eq!(second.merge_cards_emitted, 0);
        let dismissed = db
            .get_refinement_proposal(&card.id)
            .await
            .unwrap()
            .expect("dismissed card remains");
        assert_eq!(dismissed.status, "dismissed");
    }

    #[tokio::test]
    async fn stale_machine_page_survives_noop_refresh_for_retry() {
        let (db, _db_dir) = new_test_db().await;
        let source = "Rust ownership prevents data races at compile time.";
        store_test_memory(&db, "mem_stale", source).await;
        insert_test_page(&db, "page_stale", "Old machine prose.", &["mem_stale"]).await;
        db.set_page_stale("page_stale", "source_updated")
            .await
            .unwrap();

        let llm: Arc<dyn LlmProvider> = Arc::new(TestProvider {
            body: String::new(),
        });
        let result =
            run_maintenance_tick(&db, Some(&llm), &PromptRegistry::default(), &config(), None)
                .await
                .unwrap();

        assert_eq!(result.stale_machine_refreshed, 0);
        let page = db
            .get_page("page_stale")
            .await
            .unwrap()
            .expect("stale page remains");
        assert_eq!(page.stale_reason.as_deref(), Some("source_updated"));
    }
}
