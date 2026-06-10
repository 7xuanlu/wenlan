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
    #[serde(default, alias = "domain")]
    pub space: Option<String>,
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
    /// Dedicated multi-tenant scope axis (P3). Distinct from `space` (category column).
    /// Set at creation time from `CreateConceptRequest.workspace` or the
    /// `X-Origin-Space` header. NULL = no workspace constraint.
    #[serde(default)]
    pub workspace: Option<String>,
    /// Routing metadata: which mechanism created this page.
    /// One of: "distilled" | "authored" | "research" | "imported".
    /// NOT a trust signal (see `review_status` for that).
    #[serde(default = "default_creation_kind")]
    pub creation_kind: String,
    /// Trust boundary: whether this page has been confirmed as accurate.
    /// One of: "unconfirmed" | "confirmed".
    /// Distilled pages start confirmed; authored/research pages start unconfirmed.
    #[serde(default = "default_review_status")]
    pub review_status: String,
}

fn default_creation_kind() -> String {
    "distilled".to_string()
}

fn default_review_status() -> String {
    "confirmed".to_string()
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

/// Typed provenance link for a page (P2 successor to `PageSource`).
/// Backed by the `page_evidence` SQL table (migration 60).
/// Additive — `PageSource` / `page_sources` are NOT removed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PageEvidence {
    pub page_id: String,
    pub source_kind: String, // memory | external_url | external_file | authored
    pub locator: Option<String>,
    pub title: Option<String>,
    pub linked_at: i64,
    pub link_reason: Option<String>,
}
