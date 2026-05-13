// SPDX-License-Identifier: Apache-2.0
//! Wire types for compiled knowledge pages.

use serde::{Deserialize, Serialize};

/// A compiled knowledge page — structured, cross-referenced, backed by source memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub entity_id: Option<String>,
    pub domain: Option<String>,
    /// Kept for dual-write transition; prefer concept_sources join table for new reads.
    pub source_memory_ids: Vec<String>,
    pub version: i64,
    pub status: String,
    pub created_at: String,
    pub last_compiled: String,
    pub last_modified: String,
    /// How many source memories were updated since last distillation.
    pub sources_updated_count: i64,
    /// Why this page is stale: "source_updated" | "source_conflict" | None.
    pub stale_reason: Option<String>,
    /// True if a human has edited this page's content directly.
    pub user_edited: bool,
    /// Relevance score from search (0.0-1.0). Only populated by `search_pages`;
    /// zero for persisted/non-search contexts.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub relevance_score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_edited_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_edited_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_delta_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog: Option<String>,
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}
