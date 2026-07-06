// SPDX-License-Identifier: Apache-2.0

mod duplicates;

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

    let payload = serde_json::json!({
        "left_page_id": pair.left_id,
        "right_page_id": pair.right_id,
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
        &[pair.left_id.clone(), pair.right_id.clone()],
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
            max_per_tick: 5,
        }
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
