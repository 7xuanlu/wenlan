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
        db.clear_page_staleness(&page.id).await?;
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
    if let Some(existing) = db.get_refinement_proposal(&id).await? {
        if matches!(existing.status.as_str(), "pending" | "awaiting_review") {
            return Ok(false);
        }
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
