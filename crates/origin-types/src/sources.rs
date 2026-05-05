// SPDX-License-Identifier: Apache-2.0
//! Document source types -- MemoryType enum, RawDocument, SourceType, SyncStatus.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Closed taxonomy of memory facets -- validated at API boundary.
/// Stored as lowercase TEXT in SQLite.
///
/// Six user-facing types after taxonomy refactor:
/// - Identity, Preference -- about the user (Protected tier)
/// - Decision -- choices made (Standard tier)
/// - Lesson -- positive learnings (Standard tier)
/// - Gotcha -- traps/warnings/negative learnings (Standard tier)
/// - Fact -- durable knowledge (Standard tier)
///
/// Goal is deprecated: incoming "goal" parses as Identity (aspirations
/// are part of who the user is). Existing rows migrate via DB migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryType {
    Identity,
    Preference,
    Decision,
    Lesson,
    Gotcha,
    Fact,
}

impl MemoryType {
    /// All valid lowercase string values (6 canonical types).
    pub fn all_values() -> &'static [&'static str] {
        &[
            "identity",
            "preference",
            "decision",
            "lesson",
            "gotcha",
            "fact",
        ]
    }

    /// Check if input is the "profile" high-level alias (case-insensitive).
    /// Used by the store flow to detect when async LLM sub-classification is needed.
    pub fn is_profile_alias(s: &str) -> bool {
        s.eq_ignore_ascii_case("profile")
    }

    /// Check if input is the "knowledge" high-level alias (case-insensitive).
    /// Knowledge expands to fact | lesson | gotcha and needs sub-classification.
    pub fn is_knowledge_alias(s: &str) -> bool {
        s.eq_ignore_ascii_case("knowledge")
    }
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Identity => "identity",
            Self::Preference => "preference",
            Self::Decision => "decision",
            Self::Lesson => "lesson",
            Self::Gotcha => "gotcha",
            Self::Fact => "fact",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for MemoryType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "identity" => Ok(Self::Identity),
            "preference" => Ok(Self::Preference),
            "decision" => Ok(Self::Decision),
            "lesson" => Ok(Self::Lesson),
            "gotcha" => Ok(Self::Gotcha),
            "fact" => Ok(Self::Fact),
            // Deprecated: "goal" folds into Identity (aspirations = identity).
            "goal" => Ok(Self::Identity),
            // High-level alias: "profile" needs async LLM sub-classification
            "profile" => Err(
                "profile requires sub-classification into identity or preference -- use classify_memory_type".to_string()
            ),
            // High-level alias: "knowledge" needs sub-classification (fact | lesson | gotcha)
            "knowledge" => Err(
                "knowledge requires sub-classification into fact, lesson, or gotcha -- use classify_memory_type".to_string()
            ),
            // Backward compat: removed types map to Fact
            "correction" | "custom" | "recap" => Ok(Self::Fact),
            _ => Err(format!(
                "invalid memory_type '{}', valid values: {}",
                s,
                Self::all_values().join(", ")
            )),
        }
    }
}

/// Stability tiers determine supersede behavior, confidence defaults, and retrieval decay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StabilityTier {
    /// identity, preference -- supersede requires human confirmation
    Protected,
    /// fact, decision, lesson, gotcha -- supersede auto-applies unconfirmed
    Standard,
    /// catch-all for unknown / removed types -- supersede auto-applies silently
    Ephemeral,
}

/// Map a memory type string to its stability tier. NULL -> Ephemeral.
pub fn stability_tier(memory_type: Option<&str>) -> StabilityTier {
    match memory_type {
        Some("identity") | Some("preference") => StabilityTier::Protected,
        Some("fact") | Some("decision") | Some("lesson") | Some("gotcha") => {
            StabilityTier::Standard
        }
        // Deprecated: "goal" still in DB rows pre-migration -> treat as Identity (Protected).
        Some("goal") => StabilityTier::Protected,
        _ => StabilityTier::Ephemeral,
    }
}

/// A raw document fetched from any source, ready for chunking and embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawDocument {
    /// Source identifier ("gmail", "notion", "local_files", etc.)
    pub source: String,
    /// Unique ID within the source (message ID, page ID, file path)
    pub source_id: String,
    /// Document title (filename, subject line, page title)
    pub title: String,
    /// LLM-generated summary (stored separately from chunk content)
    pub summary: Option<String>,
    /// Plain text content
    pub content: String,
    /// Deep link back to the source (URL, file path)
    pub url: Option<String>,
    /// Unix timestamp of last modification
    pub last_modified: i64,
    /// Additional metadata
    pub metadata: HashMap<String, String>,

    // --- Memory layer fields (all optional for backward compat) ---
    /// Memory category: "preference", "decision", "fact", "goal", "relationship"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_type: Option<String>,
    /// Domain context: "work", "personal", "health", or "project:<name>"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Which AI agent stored this memory (e.g. "claude-code", "chatgpt")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_agent: Option<String>,
    /// Confidence score (0.0-1.0) assigned by the storing agent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Whether a human has confirmed this memory
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed: Option<bool>,
    /// Stability tier: "new", "learned", or "confirmed"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stability: Option<String>,
    /// source_id of the memory this entry supersedes (version chain)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    /// Whether this is a pending revision awaiting human approval (Protected tier supersede)
    #[serde(default)]
    pub pending_revision: bool,
    /// Link to a knowledge graph entity (nullable, cascade handled manually)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Quality assessment: "low", "medium", "high" (NULL = unassessed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    /// Whether this memory is a recap/summary of other memories
    #[serde(default)]
    pub is_recap: bool,
    /// Deprecated: enrichment status is now derived from the `enrichment_steps` table.
    /// This field is ignored on INSERT. Kept for API compatibility with downstream consumers.
    #[serde(default = "default_enrichment_status")]
    pub enrichment_status: String,
    /// How superseded content is handled: "hide" (default) or "archive" (visible but muted)
    #[serde(default = "default_supersede_mode")]
    pub supersede_mode: String,
    /// JSON object with type-specific structured fields (e.g. {"claim": "...", "context": "..."})
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_fields: Option<String>,
    /// LLM-generated question this memory answers -- embedded for vector search
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_cue: Option<String>,
    /// Original prose content, preserved when structured_fields are promoted to primary content
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_text: Option<String>,
}

fn default_enrichment_status() -> String {
    "raw".to_string()
}

fn default_supersede_mode() -> String {
    "hide".to_string()
}

impl Default for RawDocument {
    fn default() -> Self {
        Self {
            source: String::new(),
            source_id: String::new(),
            title: String::new(),
            summary: None,
            content: String::new(),
            url: None,
            last_modified: 0,
            metadata: HashMap::new(),
            memory_type: None,
            domain: None,
            source_agent: None,
            confidence: None,
            confirmed: None,
            stability: None,
            supersedes: None,
            pending_revision: false,
            entity_id: None,
            quality: None,
            is_recap: false,
            enrichment_status: "raw".to_string(),
            supersede_mode: "hide".to_string(),
            structured_fields: None,
            retrieval_cue: None,
            source_text: None,
        }
    }
}

/// Persisted source type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Obsidian,
    Directory,
}

impl SourceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Obsidian => "obsidian",
            Self::Directory => "directory",
        }
    }
}

/// Sync status for a connected source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncStatus {
    Active,
    Paused,
    Error(String),
}
