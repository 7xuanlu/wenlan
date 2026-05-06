// SPDX-License-Identifier: AGPL-3.0-only
//
// Types used by the icon-overlay flow (selection icon → click → context card).
// Carried over from the now-deleted `app/src/ambient/types.rs` to keep the
// icon overlay alive until Task 7 deletes it.

use serde::{Deserialize, Serialize};

/// The kind of card surfaced near a text selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AmbientCardKind {
    PersonContext,
    DecisionReminder,
}

/// A single memory excerpt shown inside a card's source list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySnippet {
    /// Display name for the source (source_agent or domain).
    pub source: String,
    /// Short excerpt from the memory content (≤ 80 chars).
    pub text: String,
}

/// Payload emitted to the frontend via Tauri event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientCard {
    /// Unique ID for this card instance (for dismiss tracking).
    pub card_id: String,
    pub kind: AmbientCardKind,
    /// e.g. "Alice" or "CRM Selection"
    pub title: String,
    /// e.g. "Q3 Budget" or "Feb 2026"
    pub topic: String,
    /// 1-2 sentence insight.
    pub body: String,
    /// Source agent names that contributed.
    pub sources: Vec<String>,
    /// Number of memories that matched.
    pub memory_count: usize,
    /// source_id of the primary memory (for open-detail navigation).
    pub primary_source_id: String,
    /// Timestamp when card was created.
    pub created_at: u64,
    /// True while the card is a placeholder waiting for search results.
    #[serde(default)]
    pub loading: bool,
    /// Individual memory excerpts that contributed to the synthesis.
    #[serde(default)]
    pub snippets: Vec<MemorySnippet>,
}

/// Payload emitted as `"selection-card"` Tauri event.
/// Carries the card (or no-results card) + cursor position for overlay placement.
#[derive(Debug, Clone, Serialize)]
pub struct SelectionCardEvent {
    pub card: AmbientCard,
    /// Cursor X in macOS logical coordinates (0 = left of primary display).
    pub cursor_x: f64,
    /// Cursor Y in macOS logical coordinates (0 = bottom of primary display on macOS).
    pub cursor_y: f64,
}
