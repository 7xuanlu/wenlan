// SPDX-License-Identifier: Apache-2.0
//! Strongly-typed phase identifiers for the refinery pipeline.
//!
//! Replaces the previous string-based `ALL_PHASES: &[&str]` constant. Compiler
//! enforces exhaustiveness in `TriggerKind::runs_phase` and catches phase-name
//! typos at call sites.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Decay,
    Promote,
    Recaps,
    Reweave,
    Reembed,
    EntityExtraction,
    CommunityDetection,
    Detect,
    Emergence,
    SummaryRollup,
    ReDistill,
    Overview,
    RefinementQueue,
    DecisionLogs,
    PruneRejections,
    Evict,
    KgRethink,
    PageMaps,
}

impl Phase {
    /// All phases in canonical order. Used by `Backstop` trigger.
    pub const ALL: &'static [Phase] = &[
        Phase::Decay,
        Phase::Promote,
        Phase::Recaps,
        Phase::Reweave,
        Phase::Reembed,
        Phase::EntityExtraction,
        Phase::CommunityDetection,
        Phase::Detect,
        Phase::Emergence,
        Phase::SummaryRollup,
        Phase::ReDistill,
        Phase::Overview,
        Phase::RefinementQueue,
        Phase::DecisionLogs,
        Phase::PruneRejections,
        Phase::Evict,
        Phase::KgRethink,
        Phase::PageMaps,
    ];

    /// Stable string identifier — preserved across the refactor for log
    /// compatibility and `PhaseResult.name` field consumers.
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Decay => "decay",
            Phase::Promote => "promote",
            Phase::Recaps => "recaps",
            Phase::Reweave => "reweave",
            Phase::Reembed => "reembed",
            Phase::EntityExtraction => "entity_extraction",
            Phase::CommunityDetection => "community_detection",
            Phase::Detect => "detect",
            Phase::Emergence => "emergence",
            Phase::SummaryRollup => "summary_rollup",
            Phase::ReDistill => "re-distill",
            Phase::Overview => "overview",
            Phase::RefinementQueue => "refinement_queue",
            Phase::DecisionLogs => "decision_logs",
            Phase::PruneRejections => "prune_rejections",
            Phase::Evict => "evict",
            Phase::KgRethink => "kg_rethink",
            Phase::PageMaps => "page_maps",
        }
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
