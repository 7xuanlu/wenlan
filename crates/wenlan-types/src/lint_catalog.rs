// SPDX-License-Identifier: Apache-2.0
use super::contract::LintOpaqueId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintMetricCode {
    EligibleRecords,
    ObservedRecords,
    AffectedRecords,
    PendingRecords,
    ReturnedEvidence,
    MemoryClassifiedHeads,
    MemoryEventDatedHeads,
    MemoryEpisodeHeads,
    MemoryFactVectorHeads,
    MemoryPageLinkedHeads,
    MemorySummaryLinkedHeads,
    MemoryReembedPendingHeads,
    MemoryFailedEnrichmentSteps,
    CitationNullPages,
    CitationEmptyPages,
    CitationNonemptyPages,
    CitationVerifiedOccurrences,
    CitationUnverifiedOccurrences,
    CitationSentenceOccurrences,
    CitationParagraphOccurrences,
    CitationMemoryOccurrences,
    CitationExternalFileOccurrences,
    CitationExternalUrlOccurrences,
    CitationAuthoredOccurrences,
    PageOrphanLabels,
    PageManifestPages,
    PageManifestReferences,
    PageSourceStubs,
    PageManifestDivergences,
    ProjectPurposeArtifacts,
    ProjectSchemaArtifacts,
    ProjectIndexArtifacts,
    ProjectLogArtifacts,
    ProjectOverviewArtifacts,
    ProjectArchiveRecords,
    ProjectOutboundLinks,
    ProjectInboundLinks,
    ProjectBrokenLinks,
    KgEntities,
    KgEntitiesConfirmed,
    KgEntitiesScoped,
    KgEntitiesUncategorized,
    KgRelations,
    KgObservations,
    KgMemoryEntityLinks,
    KgDuplicateEntityNames,
    KgHubEntities,
    KgSemanticSuspicions,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintMetricStringCode {
    Ready,
    Enabled,
    Disabled,
    Present,
    Missing,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LintMetricValue {
    Count { value: u64 },
    Boolean { value: bool },
    CatalogCode { code: LintMetricStringCode },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintMetric {
    code: LintMetricCode,
    value: LintMetricValue,
}
impl LintMetric {
    pub fn new(code: LintMetricCode, value: LintMetricValue) -> Self {
        Self { code, value }
    }
    pub const fn code(&self) -> LintMetricCode {
        self.code
    }
    pub fn value(&self) -> &LintMetricValue {
        &self.value
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSummaryCode {
    CheckPassed,
    FindingDetected,
    PrerequisiteUnavailable,
    SnapshotInconsistent,
    ExecutionFailed,
    ExpectedEmpty,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintRecommendationCode {
    ReviewFinding,
    RestorePrerequisite,
    RerunAfterSnapshotStabilizes,
    InspectRuntime,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintReasonCode {
    MissingArtifact,
    InvalidCatalogState,
    ExpectedEmptySubstrate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LintSafeRootRelativePath {
    #[serde(rename = "pages")]
    PagesRoot,
    #[serde(rename = "pages/.wenlan/state.json")]
    PagesState,
    #[serde(rename = "pages/.wenlan/manifest.json")]
    PagesManifest,
    #[serde(rename = "pages/.wenlan/stubs")]
    PagesStubs,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LintEvidenceRef {
    OpaqueId {
        opaque_id: LintOpaqueId,
    },
    ReasonCode {
        reason_code: LintReasonCode,
    },
    SafeRootRelativePath {
        safe_root_relative_path: LintSafeRootRelativePath,
    },
}
impl LintEvidenceRef {
    pub(crate) const fn opaque_id(&self) -> Option<LintOpaqueId> {
        match self {
            Self::OpaqueId { opaque_id } => Some(*opaque_id),
            Self::ReasonCode { .. } | Self::SafeRootRelativePath { .. } => None,
        }
    }
}
