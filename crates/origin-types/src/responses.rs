// SPDX-License-Identifier: Apache-2.0
//! API response types for all HTTP endpoints.

use crate::entities::{Entity, EntitySearchResult};
use crate::memory::{IndexedFileInfo, MemoryItem, MemoryStats, SearchResult};
use crate::pages::Page;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ===== Memory CRUD =====

#[derive(Debug, Serialize, Deserialize)]
pub struct StoreMemoryResponse {
    pub source_id: String,
    pub chunks_created: usize,
    /// Memory type at the moment of persistence. If caller did not supply
    /// one and enrichment is pending, this is a placeholder (`"fact"`) —
    /// check `enrichment` field to know whether to expect it to change.
    pub memory_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    /// Schema-validation issues — actionable by the agent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// How structured fields were populated. "agent" | "llm" | "none" | "unknown" (forward-compat default).
    #[serde(default = "default_extraction_method")]
    pub extraction_method: String,
    /// Enrichment state for the memory. `"pending"` when background
    /// classification + entity extraction + concept linking will run;
    /// `"not_needed"` when no LLM is available and the memory stays as
    /// caller-supplied. Machine-readable — Tauri app uses this to drive
    /// polling / live-update UI, MCP callers can choose to relay state.
    /// Defaulted for backward compatibility with pre-async-enrichment clients.
    #[serde(default)]
    pub enrichment: String,
    /// Prose cue for caller agents — safe to relay to the user verbatim.
    /// Communicates that Origin is compiling the memory into reusable
    /// context in the background, so callers don't treat `None` enriched
    /// fields as failure. Empty when the store completed fully sync.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hint: String,
    /// Source IDs of protected memories now flagged for human revision
    /// because this capture's topic-match upsert fired against them. Empty
    /// when no contradictions detected. Skills should surface these inline
    /// to the user with accept/dismiss verbs (see `accept_revision` /
    /// `dismiss_revision` MCP tools).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggered_revisions: Vec<String>,
}

fn default_extraction_method() -> String {
    "unknown".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMemoryResponse {
    pub results: Vec<SearchResult>,
    pub took_ms: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListMemoriesResponse {
    pub memories: Vec<IndexedFileInfo>,
}

/// Shared wire format for any `deleted: bool` response.
///
/// Reused by:
/// - `DELETE /api/memory/delete/{id}` (server/memory.rs)
/// - `DELETE /api/documents/{source}/{source_id}` (server/ingest.rs)
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfirmResponse {
    pub confirmed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReclassifyMemoryResponse {
    pub source_id: String,
    pub memory_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryStatsResponse {
    pub stats: MemoryStats,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NurtureCardsResponse {
    pub cards: Vec<MemoryItem>,
}

// ===== General search/context =====

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub db_initialized: bool,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResponse {
    pub is_running: bool,
    pub files_indexed: u64,
    pub files_total: u64,
    pub sources_connected: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub took_ms: f64,
}

#[doc(hidden)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextSuggestion {
    pub content: String,
    pub score: f32,
    pub source: String,
}

#[doc(hidden)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextResponse {
    pub suggestions: Vec<ContextSuggestion>,
    pub took_ms: f64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TierTokenEstimates {
    pub tier1_identity: usize,
    pub tier2_project: usize,
    pub tier3_relevant: usize,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileContext {
    pub narrative: String,
    pub identity: Vec<String>,
    pub preferences: Vec<String>,
    /// Deprecated: goal taxonomy folded into Identity by migration 45 (Phase 0).
    /// Always empty — daemon does not emit goal-typed memories. Field stays for
    /// wire backward compat; will be removed in 0.4.
    #[deprecated(
        since = "0.3.2",
        note = "Goal taxonomy folded into Identity by migration 45 (Phase 0). \
                Always empty. Will be removed in 0.4."
    )]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub goals: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KnowledgeContext {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub relevant_memories: Vec<SearchResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph_context: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatContextResponse {
    pub context: String,
    pub profile: ProfileContext,
    pub knowledge: KnowledgeContext,
    pub took_ms: f64,
    pub token_estimates: TierTokenEstimates,
}

// ===== Profile & Agents =====

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub bio: Option<String>,
    pub avatar_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub agent_type: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub trust_level: String,
    pub last_seen_at: Option<i64>,
    pub memory_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

// ===== Knowledge graph =====

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateEntityResponse {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[doc(hidden)]
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRelationResponse {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddObservationResponse {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[doc(hidden)]
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePageResponse {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListEntitiesResponse {
    pub entities: Vec<Entity>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchEntitiesResponse {
    pub results: Vec<EntitySearchResult>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchPagesResponse {
    pub pages: Vec<Page>,
}

/// Wikilink graph centered on a single page. Outbound = labels parsed
/// out of this page's body; `target_page_id` is `None` for orphans.
/// Inbound = active pages whose body cites this title.
#[derive(Debug, Serialize, Deserialize)]
pub struct PageLinksResponse {
    pub outbound: Vec<PageLinkOutbound>,
    pub inbound: Vec<PageLinkInbound>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PageLinkOutbound {
    pub label: String,
    /// `None` when the resolver couldn't find a matching active page —
    /// surfaces in the orphan-by-count feed via /api/pages/orphan-links.
    pub target_page_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PageLinkInbound {
    pub source_page_id: String,
    pub label: String,
}

// ===== Import =====

#[derive(Debug, Serialize, Deserialize)]
pub struct ImportMemoriesResponse {
    pub imported: usize,
    pub skipped: usize,
    pub breakdown: HashMap<String, usize>,
    pub entities_created: usize,
    pub observations_added: usize,
    pub relations_created: usize,
    pub batch_id: String,
}

// ===== Steep =====

/// How loud Origin should be about a phase's output.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Nudge {
    Silent,
    Ambient,
    Notable,
    Wow,
}

/// Result of a single phase within a steep cycle.
#[doc(hidden)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseResult {
    pub name: String,
    pub duration_ms: u64,
    pub items_processed: usize,
    pub error: Option<String>,
    pub nudge: Nudge,
    pub headline: Option<String>,
}

#[doc(hidden)]
#[derive(Debug, Serialize, Deserialize)]
pub struct SteepResponse {
    pub memories_decayed: u64,
    pub recaps_generated: u32,
    pub distilled: u32,
    pub pending_remaining: u32,
    pub phases: Vec<PhaseResult>,
}

// ===== Config =====

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigResponse {
    pub skip_apps: Vec<String>,
    pub skip_title_patterns: Vec<String>,
    pub private_browsing_detection: bool,
    pub setup_completed: bool,
    pub clipboard_enabled: bool,
    pub screen_capture_enabled: bool,
    pub remote_access_enabled: bool,
    /// Anthropic model used for fast/routine tasks (e.g. classification, tagging).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routine_model: Option<String>,
    /// Anthropic model used for synthesis tasks (e.g. distillation, narrative).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthesis_model: Option<String>,
    /// Base URL for an OpenAI-compatible external LLM endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_llm_endpoint: Option<String>,
    /// Model identifier to use with the external LLM endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_llm_model: Option<String>,
}

// ===== Indexed files / chunks =====

#[derive(Debug, Serialize, Deserialize)]
pub struct IndexedFilesResponse {
    pub files: Vec<IndexedFileInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteCountResponse {
    pub deleted: usize,
}

// ===== Entity / Observation =====

#[derive(Debug, Serialize, Deserialize)]
pub struct SuccessResponse {
    pub ok: bool,
}

// ===== Memory detail =====

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryDetailResponse {
    pub memory: Option<MemoryItem>,
}

/// Detailed chunk-level view of a stored memory, returned by `/api/chunks/{source_id}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryDetail {
    pub id: String,
    pub content: String,
    pub title: String,
    pub source_id: String,
    pub chunk_index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_end: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// A pending revision waiting for human approval (Protected tier supersede).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRevision {
    pub source_id: String,
    pub content: String,
    pub source_agent: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VersionChainResponse {
    pub versions: Vec<crate::memory::MemoryVersionItem>,
}

// ===== Tags =====

#[derive(Debug, Serialize, Deserialize)]
pub struct TagsResponse {
    pub tags: Vec<String>,
}

// ===== Activity =====

#[derive(Debug, Serialize, Deserialize)]
pub struct ActivityResponse {
    pub activities: Vec<crate::memory::AgentActivityRow>,
}

// ===== Decisions =====

#[derive(Debug, Serialize, Deserialize)]
pub struct DecisionsResponse {
    pub decisions: Vec<MemoryItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DecisionDomainsResponse {
    pub domains: Vec<String>,
}

// ===== Pinned =====

#[derive(Debug, Serialize, Deserialize)]
pub struct PinnedMemoriesResponse {
    pub memories: Vec<MemoryItem>,
}

// ===== Ingest =====

#[derive(Debug, Serialize, Deserialize)]
pub struct IngestResponse {
    pub chunks_created: usize,
    pub document_id: String,
}

// Note: ingest's `DELETE /api/documents/{source}/{source_id}` reuses the
// `DeleteResponse { deleted: bool }` defined above — same wire format.

// ===== Concept Export =====

/// Statistics from a bulk page export operation (POST /api/pages/export).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ExportStats {
    pub exported: usize,
    pub skipped: usize,
    pub failed: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ExportPageResponse {
    pub path: String,
}

// ===== Knowledge Directory =====

#[derive(Debug, Deserialize, Serialize)]
pub struct KnowledgePathResponse {
    pub path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct KnowledgeCountResponse {
    pub count: u64,
}

// ===== Revision history =====

/// One entry in a memory's supersede chain, returned by `/api/memory/{id}/revisions`.
///
/// `depth = 0` is the current (most-recent) memory; higher depths are older
/// predecessors. `delta_summary` is `None` for the deepest entry (no predecessor
/// to diff against) and computed heuristically for all shallower entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRevisionEntry {
    pub source_id: String,
    pub depth: i64,
    pub title: String,
    pub content_preview: String,
    pub last_modified: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersede_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_summary: Option<String>,
}

/// Response envelope for `/api/memory/{id}/revisions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListMemoryRevisionsResponse {
    pub current_source_id: String,
    pub chain_depth: i64,
    pub entries: Vec<MemoryRevisionEntry>,
}

/// One entry in a page's version changelog, returned by `/api/pages/{id}/revisions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageChangelogEntry {
    pub version: i64,
    pub at: i64,
    pub edited_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incoming_source_ids: Option<Vec<String>>,
}

/// Response envelope for `/api/pages/{id}/revisions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPageRevisionsResponse {
    pub page_id: String,
    pub current_version: i64,
    pub user_edited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
    pub entries: Vec<PageChangelogEntry>,
}

// ===== Sources =====

#[doc(hidden)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatsResponse {
    pub files_found: usize,
    pub ingested: usize,
    pub skipped: usize,
    pub errors: usize,
}

// ===== Refinement proposals =====

/// The action type for a background-refinery proposal.
///
/// Used as the `action` tag in [`RefinementPayload`].
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalAction {
    EntityMerge,
    RelationConflict,
    DetectContradiction,
    SuggestEntity,
    DedupMerge,
}

/// Tagged-union payload emitted by the background refinery.
///
/// Each variant carries exactly the fields needed for that action type.
/// Decoded at the route boundary so downstream consumers (MCP wrappers,
/// agent skills) can pattern-match instead of inspecting raw JSON strings.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum RefinementPayload {
    EntityMerge {
        existing_id: String,
        new_id: String,
        similarity: f64,
    },
    RelationConflict {
        existing_id: String,
        new_id: String,
        from: String,
        to: String,
        old_type: String,
        new_type: String,
    },
    DetectContradiction,
    SuggestEntity {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name_hint: Option<String>,
    },
    DedupMerge,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RefinementProposalSummary {
    pub id: String,
    pub action: ProposalAction,
    pub source_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<RefinementPayload>,
    pub confidence: f64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ListRefinementsResponse {
    pub proposals: Vec<RefinementProposalSummary>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RejectRefinementResponse {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptRefinementResponse {
    pub id: String,
    pub action_applied: String,
}

#[cfg(test)]
mod refinement_wire_tests {
    use super::*;

    #[test]
    fn proposal_action_serde_round_trip() {
        let cases = [
            ("\"entity_merge\"", ProposalAction::EntityMerge),
            ("\"relation_conflict\"", ProposalAction::RelationConflict),
            (
                "\"detect_contradiction\"",
                ProposalAction::DetectContradiction,
            ),
            ("\"suggest_entity\"", ProposalAction::SuggestEntity),
            ("\"dedup_merge\"", ProposalAction::DedupMerge),
        ];
        for (json, expected) in cases {
            let parsed: ProposalAction = serde_json::from_str(json).unwrap();
            assert_eq!(parsed, expected, "deserialize {json}");
            let back = serde_json::to_string(&expected).unwrap();
            assert_eq!(back, json, "serialize {expected:?}");
        }
    }

    #[test]
    fn refinement_payload_entity_merge_round_trip() {
        let json =
            r#"{"action":"entity_merge","existing_id":"e1","new_id":"e2","similarity":0.87}"#;
        let parsed: RefinementPayload = serde_json::from_str(json).unwrap();
        match parsed {
            RefinementPayload::EntityMerge {
                ref existing_id,
                ref new_id,
                similarity,
            } => {
                assert_eq!(existing_id, "e1");
                assert_eq!(new_id, "e2");
                assert!((similarity - 0.87).abs() < 1e-9);
            }
            _ => panic!("expected EntityMerge variant"),
        }
        let back = serde_json::to_value(&parsed).unwrap();
        assert_eq!(back["action"], "entity_merge");
        assert_eq!(back["existing_id"], "e1");
    }

    #[test]
    fn refinement_payload_dedup_merge_no_fields() {
        let json = r#"{"action":"dedup_merge"}"#;
        let parsed: RefinementPayload = serde_json::from_str(json).unwrap();
        assert!(matches!(parsed, RefinementPayload::DedupMerge));
    }

    #[test]
    fn refinement_payload_relation_conflict_round_trip() {
        let json = r#"{"action":"relation_conflict","existing_id":"r1","new_id":"r2","from":"e_a","to":"e_b","old_type":"works_at","new_type":"founded"}"#;
        let parsed: RefinementPayload = serde_json::from_str(json).unwrap();
        match parsed {
            RefinementPayload::RelationConflict {
                ref existing_id,
                ref new_id,
                ref from,
                ref to,
                ref old_type,
                ref new_type,
            } => {
                assert_eq!(existing_id, "r1");
                assert_eq!(new_id, "r2");
                assert_eq!(from, "e_a");
                assert_eq!(to, "e_b");
                assert_eq!(old_type, "works_at");
                assert_eq!(new_type, "founded");
            }
            _ => panic!("expected RelationConflict"),
        }
        let back = serde_json::to_value(&parsed).unwrap();
        assert_eq!(back["from"], "e_a");
        assert_eq!(back["to"], "e_b");
    }

    #[test]
    fn refinement_payload_detect_contradiction_unit_variant() {
        let json = r#"{"action":"detect_contradiction"}"#;
        let parsed: RefinementPayload = serde_json::from_str(json).unwrap();
        assert!(matches!(parsed, RefinementPayload::DetectContradiction));
    }

    #[test]
    fn refinement_payload_suggest_entity_with_name_hint() {
        let json = r#"{"action":"suggest_entity","name_hint":"PostgreSQL"}"#;
        let parsed: RefinementPayload = serde_json::from_str(json).unwrap();
        match parsed {
            RefinementPayload::SuggestEntity { ref name_hint } => {
                assert_eq!(name_hint.as_deref(), Some("PostgreSQL"));
            }
            _ => panic!("expected SuggestEntity"),
        }
    }

    #[test]
    fn refinement_payload_suggest_entity_without_name_hint() {
        let json = r#"{"action":"suggest_entity"}"#;
        let parsed: RefinementPayload = serde_json::from_str(json).unwrap();
        assert!(matches!(
            parsed,
            RefinementPayload::SuggestEntity { name_hint: None }
        ));
    }

    #[test]
    fn list_refinements_response_round_trip() {
        let resp = ListRefinementsResponse {
            proposals: vec![RefinementProposalSummary {
                id: "ref_1".into(),
                action: ProposalAction::EntityMerge,
                source_ids: vec!["a".into(), "b".into()],
                payload: Some(RefinementPayload::EntityMerge {
                    existing_id: "a".into(),
                    new_id: "b".into(),
                    similarity: 0.86,
                }),
                confidence: 0.86,
                created_at: "2026-05-12T00:00:00Z".into(),
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: ListRefinementsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.proposals.len(), 1);
        assert_eq!(parsed.proposals[0].id, "ref_1");
        assert!(matches!(
            parsed.proposals[0].action,
            ProposalAction::EntityMerge
        ));
    }

    #[test]
    fn reject_refinement_response_round_trip() {
        let resp = RejectRefinementResponse { id: "ref_x".into() };
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: RejectRefinementResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "ref_x");
    }
}

/// One orphaned page link label aggregated across sources.
///
/// `count` is how many distinct source pages reference this label
/// without a matching target page existing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrphanLink {
    pub label: String,
    pub count: i64,
}

/// Response for `GET /api/pages/orphan-links`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrphanLinksResponse {
    pub min_count: usize,
    pub orphan_labels: Vec<OrphanLink>,
}

/// One pending revision awaiting human accept/dismiss.
///
/// `target_source_id` is the memory being revised; pass it to
/// `accept_pending_revision` or `dismiss_pending_revision`.
/// `revision_source_id` is the staged revision row itself, exposed
/// for diagnostics and round-tripping (not for the action call).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingRevisionItem {
    pub target_source_id: String,
    pub revision_source_id: String,
    pub revision_content: String,
    pub source_agent: Option<String>,
    pub last_modified: i64,
}

// ===== Curation mutate responses (Spec C-2) =====

/// Response returned by `POST /api/memory/entity-suggestions/{id}/approve`.
/// Carries the created (or resolved) entity id, link count, and the wrote flag
/// from the nested `create_entity` capability fn. `wrote: false` means the
/// suggestion resolved to an existing entity via alias/exact-name/vector match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntitySuggestionApproveResponse {
    pub suggestion_id: String,
    pub entity_id: String,
    pub entity_name: String,
    pub memories_linked: u32,
    pub wrote: bool,
}

/// Response returned by `POST /api/memory/entity-suggestions/{id}/dismiss`.
/// `wrote: true` always (404 on missing/already-resolved).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntitySuggestionDismissResponse {
    pub suggestion_id: String,
    pub wrote: bool,
}

/// Response returned by `POST /api/memory/revision/{id}/accept`.
/// Carries the now-consumed revision row id so agents can correlate with
/// their `list_pending_revisions` cache.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionAcceptResponse {
    pub target_source_id: String,
    pub revision_source_id: String,
    pub wrote: bool,
}

/// Response returned by `POST /api/memory/revision/{id}/dismiss`.
/// `wrote: true` always (404 on missing).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevisionDismissResponse {
    pub target_source_id: String,
    pub wrote: bool,
}

/// Response returned by `POST /api/memory/contradiction/{source_id}/dismiss`.
/// `wrote: true` is best-effort: the daemon's underlying DB method silently
/// no-ops when no rows match. Wrapper cannot distinguish dismiss-of-existing
/// from dismiss-of-nothing without an extra SELECT (out of scope).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContradictionDismissResponse {
    pub source_id: String,
    pub wrote: bool,
}

#[cfg(test)]
mod mutation_response_tests {
    use super::*;

    #[test]
    fn entity_suggestion_approve_response_serializes_byte_identical() {
        let r = EntitySuggestionApproveResponse {
            suggestion_id: "ref_123".into(),
            entity_id: "ent_456".into(),
            entity_name: "Acme Corp".into(),
            memories_linked: 7,
            wrote: true,
        };
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(
            json,
            r#"{"suggestion_id":"ref_123","entity_id":"ent_456","entity_name":"Acme Corp","memories_linked":7,"wrote":true}"#
        );
    }

    #[test]
    fn entity_suggestion_dismiss_response_serializes_byte_identical() {
        let r = EntitySuggestionDismissResponse {
            suggestion_id: "ref_123".into(),
            wrote: true,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"suggestion_id":"ref_123","wrote":true}"#
        );
    }

    #[test]
    fn revision_accept_response_serializes_byte_identical() {
        let r = RevisionAcceptResponse {
            target_source_id: "mem_target".into(),
            revision_source_id: "mem_rev".into(),
            wrote: true,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"target_source_id":"mem_target","revision_source_id":"mem_rev","wrote":true}"#
        );
    }

    #[test]
    fn revision_dismiss_response_serializes_byte_identical() {
        let r = RevisionDismissResponse {
            target_source_id: "mem_target".into(),
            wrote: true,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"target_source_id":"mem_target","wrote":true}"#
        );
    }

    #[test]
    fn contradiction_dismiss_response_serializes_byte_identical() {
        let r = ContradictionDismissResponse {
            source_id: "mem_abc".into(),
            wrote: true,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"source_id":"mem_abc","wrote":true}"#
        );
    }

    #[test]
    fn entity_suggestion_approve_response_round_trips() {
        let r = EntitySuggestionApproveResponse {
            suggestion_id: "ref_123".into(),
            entity_id: "ent_456".into(),
            entity_name: "Acme".into(),
            memories_linked: 0,
            wrote: false,
        };
        let json = serde_json::to_string(&r).unwrap();
        let decoded: EntitySuggestionApproveResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_memory_response_deserializes_without_extraction_method() {
        // Forward-compat: older server responses (pre-D9) omit extraction_method entirely.
        let json = r#"{
            "source_id": "mem_abc",
            "chunks_created": 3,
            "memory_type": "fact"
        }"#;
        let parsed: StoreMemoryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.source_id, "mem_abc");
        assert_eq!(parsed.chunks_created, 3);
        assert_eq!(parsed.memory_type, "fact");
        assert_eq!(parsed.extraction_method, "unknown");
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn store_memory_response_deserializes_with_all_fields() {
        let json = r#"{
            "source_id": "mem_abc",
            "chunks_created": 3,
            "memory_type": "fact",
            "warnings": ["decision memory missing claim"],
            "extraction_method": "llm"
        }"#;
        let parsed: StoreMemoryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.warnings.len(), 1);
        assert_eq!(parsed.extraction_method, "llm");
    }

    #[test]
    fn store_memory_response_exposes_enrichment_and_hint() {
        // Post-async-refactor shape: the daemon returns immediately after
        // upsert and reports deferred enrichment via `enrichment` + `hint`.
        let json = r#"{
            "source_id": "mem_xyz",
            "chunks_created": 1,
            "memory_type": "fact",
            "warnings": [],
            "extraction_method": "unknown",
            "enrichment": "pending",
            "hint": "Stored. Origin is compiling classification + concept links in the background (~2s). Recall will surface the enriched form shortly."
        }"#;
        let parsed: StoreMemoryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.enrichment, "pending");
        assert!(parsed.hint.contains("compiling"));
    }

    #[test]
    fn store_memory_response_defaults_enrichment_for_older_responses() {
        // Backward-compat: existing clients (origin-mcp, Tauri app) that
        // deserialize pre-async-refactor responses must keep working.
        let json = r#"{
            "source_id": "mem_old",
            "chunks_created": 1,
            "memory_type": "fact"
        }"#;
        let parsed: StoreMemoryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.enrichment, ""); // default
        assert_eq!(parsed.hint, ""); // default
    }

    #[test]
    fn store_memory_response_roundtrips_not_needed_state() {
        // Daemon reports `not_needed` when no LLM is available. Hint is empty
        // (skip_serializing_if) so the JSON shrinks accordingly.
        let response = StoreMemoryResponse {
            source_id: "mem_no_llm".into(),
            chunks_created: 1,
            memory_type: "fact".into(),
            entity_id: None,
            quality: None,
            warnings: vec![],
            extraction_method: "none".into(),
            enrichment: "not_needed".into(),
            hint: String::new(),
            triggered_revisions: vec![],
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"enrichment\":\"not_needed\""));
        assert!(
            !json.contains("\"hint\""),
            "empty hint must be skipped on the wire, got: {json}"
        );
        let parsed: StoreMemoryResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.enrichment, "not_needed");
        assert_eq!(parsed.hint, "");
    }

    #[test]
    fn store_memory_response_triggered_revisions_serializes_when_non_empty() {
        let r = StoreMemoryResponse {
            source_id: "mem_new".into(),
            chunks_created: 1,
            memory_type: "fact".into(),
            entity_id: None,
            quality: None,
            warnings: vec![],
            extraction_method: "none".into(),
            enrichment: "not_needed".into(),
            hint: String::new(),
            triggered_revisions: vec!["mem_target_abc".to_string()],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            json.contains("\"triggered_revisions\":[\"mem_target_abc\"]"),
            "triggered_revisions must appear in JSON when non-empty, got: {json}"
        );
    }

    #[test]
    fn store_memory_response_triggered_revisions_skips_when_empty() {
        let r = StoreMemoryResponse {
            source_id: "mem_new".into(),
            chunks_created: 1,
            memory_type: "fact".into(),
            entity_id: None,
            quality: None,
            warnings: vec![],
            extraction_method: "none".into(),
            enrichment: "not_needed".into(),
            hint: String::new(),
            triggered_revisions: vec![],
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(
            !json.contains("triggered_revisions"),
            "triggered_revisions must be absent from JSON when empty, got: {json}"
        );
    }

    #[test]
    fn chat_context_response_roundtrips_with_empty_knowledge_sections() {
        // ProfileContext.goals is deprecated; constructing it directly here
        // for wire roundtrip coverage until 0.4 drops the field entirely.
        #[allow(deprecated)]
        let profile = ProfileContext {
            narrative: "n".into(),
            identity: vec![],
            preferences: vec![],
            goals: vec![],
        };
        let response = ChatContextResponse {
            context: "context".into(),
            profile,
            knowledge: KnowledgeContext {
                pages: vec![],
                decisions: vec![],
                relevant_memories: vec![],
                graph_context: vec![],
            },
            took_ms: 1.0,
            token_estimates: TierTokenEstimates {
                tier1_identity: 1,
                tier2_project: 2,
                tier3_relevant: 3,
                total: 6,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: ChatContextResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.knowledge.pages.is_empty());
        assert!(parsed.knowledge.decisions.is_empty());
        assert!(parsed.knowledge.relevant_memories.is_empty());
        assert!(parsed.knowledge.graph_context.is_empty());
    }

    #[test]
    fn orphan_links_response_golden_string() {
        let resp = OrphanLinksResponse {
            min_count: 2,
            orphan_labels: vec![OrphanLink {
                label: "Rust".to_string(),
                count: 3,
            }],
        };
        let s = serde_json::to_string(&resp).unwrap();
        assert_eq!(
            s,
            r#"{"min_count":2,"orphan_labels":[{"label":"Rust","count":3}]}"#
        );
    }

    #[test]
    fn orphan_links_response_empty_round_trip() {
        let resp = OrphanLinksResponse {
            min_count: 1,
            orphan_labels: vec![],
        };
        let decoded: OrphanLinksResponse =
            serde_json::from_str(&serde_json::to_string(&resp).unwrap()).unwrap();
        assert_eq!(decoded, resp);
    }

    #[test]
    fn pending_revision_item_round_trip() {
        let item = PendingRevisionItem {
            target_source_id: "mem_target".into(),
            revision_source_id: "mem_rev".into(),
            revision_content: "new body".into(),
            source_agent: Some("claude-code".into()),
            last_modified: 1_715_000_000,
        };
        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["target_source_id"], "mem_target");
        assert_eq!(json["revision_source_id"], "mem_rev");
        assert_eq!(json["revision_content"], "new body");
        let decoded: PendingRevisionItem = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, item);
    }
}
