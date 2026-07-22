// SPDX-License-Identifier: Apache-2.0
//! Closed serde graph for repair artifacts persisted with schema version 1.
//!
//! These definitions intentionally duplicate the v1 wire contract. Do not
//! import or alias live lint, repair, or domain types here: their evolution
//! must not change how already-persisted bytes decode or canonicalize.
//! Keep the complete graph in this file unless the architecture test is
//! extended to inspect every additional frozen-v1 source file.

use serde::{Deserialize, Serialize};

// Historical producer versions encoded by repair manifest schema v1.
pub(super) const FROZEN_LINT_REPORT_SCHEMA_VERSION_V1: u16 = 4;
pub(super) const FROZEN_LINT_CHECK_CATALOG_VERSION_V1: u16 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrozenRepairDigestV1(pub(super) String);

impl FrozenRepairDigestV1 {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrozenLintDigestV1(pub(super) String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrozenLintCommitReceiptV1(pub(super) String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FrozenLintOpaqueIdV1(pub(super) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintScopeKindV1 {
    Global,
    Registered,
    Uncategorized,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenLintScopeV1 {
    pub(super) kind: FrozenLintScopeKindV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) opaque_scope_ref: Option<FrozenLintOpaqueIdV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintDbSnapshotModeV1 {
    TransactionalReadOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintPageSnapshotModeV1 {
    BestEffort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenLintDbSnapshotReceiptV1 {
    pub(super) mode: FrozenLintDbSnapshotModeV1,
    pub(super) analysis_digest: FrozenLintDigestV1,
    pub(super) post_run_digest: Option<FrozenLintDigestV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenLintPageSnapshotReceiptV1 {
    pub(super) mode: FrozenLintPageSnapshotModeV1,
    pub(super) before_scan_digest: FrozenLintDigestV1,
    pub(super) after_scan_digest: Option<FrozenLintDigestV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenLintSnapshotReceiptsV1 {
    pub(super) db: FrozenLintDbSnapshotReceiptV1,
    pub(super) pages: FrozenLintPageSnapshotReceiptV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenLintProducerReceiptV1 {
    pub(super) runtime_commit: Option<FrozenLintCommitReceiptV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintSemanticActionV1 {
    ReclassifyMemory,
    ReviewContradiction,
    ReviewStaleness,
    SupersedeMemory,
    AddMemoryEntityLink,
    RemoveMemoryEntityLink,
    AddEntityRelation,
    RemoveEntityRelation,
    ReviewPageClaim,
    AddPageEvidence,
    RemovePageEvidence,
    ReviewRetrieval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintSemanticReasonCodeV1 {
    ClassificationMismatch,
    PotentialContradiction,
    PotentialStaleness,
    MentionWithoutLink,
    ExistingLinkMismatch,
    SharedContextWithoutRelation,
    ExistingRelationMismatch,
    PotentialUnfaithfulClaim,
    PotentialInadequateProvenance,
    ClaimOverlapWithoutEvidence,
    ExistingEvidenceMismatch,
    PotentialRetrievalMiss,
    DanglingOwner,
    TemporalEvolution,
    RelatedButNotEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintSemanticProviderRouteV1 {
    OnDevice,
    ConfiguredExternal,
    CallingAgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenLintSemanticFindingV1 {
    pub(super) candidate_id: FrozenLintOpaqueIdV1,
    pub(super) proposed_action: FrozenLintSemanticActionV1,
    pub(super) reason_code: FrozenLintSemanticReasonCodeV1,
    pub(super) confidence_basis_points: u16,
    pub(super) provider_route: FrozenLintSemanticProviderRouteV1,
    pub(super) evidence_ids: Vec<FrozenLintDigestV1>,
    pub(super) counterevidence_ids: Vec<FrozenLintDigestV1>,
    pub(super) unresolved_disagreement: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintOutcomeV1 {
    Pass,
    Finding,
    NotRunPrerequisite,
    InconsistentSnapshot,
    FailedToRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintGateEffectV1 {
    Actionable,
    Advisory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenLintReasonCodeV1 {
    MissingArtifact,
    InvalidCatalogState,
    ExpectedEmptySubstrate,
    InvalidSourceConfiguration,
    TerminalOperationFailure,
    ExpiredRetry,
    InvalidOperationState,
    DurableNoProgress,
    SemanticProviderUnavailable,
    InsufficientSemanticEvidence,
    SemanticExecutionFailure,
    SemanticAgentAdjudicationRequired,
    SemanticAgentWorkStale,
    SemanticAgentSubmissionInvalid,
    SemanticCandidateGenerationFailure,
    SemanticPopulationIncomplete,
    SemanticDisagreementUnresolved,
    SemanticSecondJudgeRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::enum_variant_names)]
pub enum FrozenLintSafeRootRelativePathV1 {
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrozenLintEvidenceRefV1 {
    OpaqueId {
        opaque_id: FrozenLintOpaqueIdV1,
    },
    ReasonCode {
        reason_code: FrozenLintReasonCodeV1,
    },
    SafeRootRelativePath {
        safe_root_relative_path: FrozenLintSafeRootRelativePathV1,
    },
    SemanticFinding {
        finding: FrozenLintSemanticFindingV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrozenRepairLintScopeV1 {
    Global {},
    Registered { space: String },
    Uncategorized {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrozenRepairScopeV1 {
    Registered { space: String },
    Uncategorized {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrozenRepairTargetV1 {
    Memory {
        source_id: String,
        scope: FrozenRepairScopeV1,
    },
}

impl FrozenRepairTargetV1 {
    pub fn memory_source_id(&self) -> &str {
        match self {
            Self::Memory { source_id, .. } => source_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairSourceV1 {
    pub(super) report_schema_version: u16,
    pub(super) check_catalog_version: u16,
    pub(super) lint_scope: FrozenRepairLintScopeV1,
    pub(super) report_scope: FrozenLintScopeV1,
    pub(super) check_id: String,
    pub(super) finding: FrozenLintSemanticFindingV1,
    pub(super) general_snapshots: FrozenLintSnapshotReceiptsV1,
    pub(super) deep_snapshots: FrozenLintSnapshotReceiptsV1,
    pub(super) general_producer_receipt: FrozenLintProducerReceiptV1,
    pub(super) deep_producer_receipt: FrozenLintProducerReceiptV1,
    pub(super) agent_work_digest: FrozenLintDigestV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairExpectedStateV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) version: Option<i64>,
    pub(super) canonical_receipt: FrozenRepairDigestV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenRepairWriterV1 {
    ReclassifyMemory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrozenMemoryTypeV1 {
    Identity,
    Preference,
    Decision,
    Lesson,
    Gotcha,
    Fact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrozenRepairMutationV1 {
    ReclassifyMemory {
        before_memory_type: Option<FrozenMemoryTypeV1>,
        after_memory_type: FrozenMemoryTypeV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrozenRepairMemoryFieldV1 {
    MemoryType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairAllowedEffectsV1 {
    pub(super) owner: FrozenRepairTargetV1,
    pub(super) fields: Vec<FrozenRepairMemoryFieldV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairRollbackReferenceV1 {
    pub(super) format_version: u16,
    pub(super) relative_path: String,
    pub(super) digest: FrozenRepairDigestV1,
}

impl FrozenRepairRollbackReferenceV1 {
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn digest(&self) -> &FrozenRepairDigestV1 {
        &self.digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairCheckBaselineV1 {
    pub(super) check_id: String,
    pub(super) outcome: FrozenLintOutcomeV1,
    pub(super) gate_effect: FrozenLintGateEffectV1,
    pub(super) evidence: Vec<FrozenLintEvidenceRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairPostAssertionsV1 {
    pub(super) target_check_id: String,
    pub(super) target_evidence_id: FrozenLintDigestV1,
    pub(super) general_baseline: Vec<FrozenRepairCheckBaselineV1>,
    pub(super) deep_baseline: Vec<FrozenRepairCheckBaselineV1>,
    pub(super) require_complete_general: bool,
    pub(super) require_complete_deep: bool,
    pub(super) reject_new_actionable: bool,
    pub(super) reject_new_incomplete: bool,
    pub(super) allowed_non_target_check_deltas: Vec<String>,
}

/// The first daemon-persisted manifest-v1 assertion shape (commit 08faba7f),
/// before non-target check baselines were added without a schema bump.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairPostAssertionsPreBaselineV1 {
    pub(super) target_check_id: String,
    pub(super) target_evidence_id: FrozenLintDigestV1,
    pub(super) require_complete_general: bool,
    pub(super) require_complete_deep: bool,
    pub(super) reject_new_actionable: bool,
    pub(super) reject_new_incomplete: bool,
    pub(super) allowed_non_target_check_deltas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairManifestDraftV1 {
    pub(super) manifest_schema_version: u16,
    pub(super) manifest_id: String,
    pub(super) prepared_at: i64,
    pub(super) source: FrozenRepairSourceV1,
    pub(super) target: FrozenRepairTargetV1,
    pub(super) expected_state: FrozenRepairExpectedStateV1,
    pub(super) writer: FrozenRepairWriterV1,
    pub(super) mutation: FrozenRepairMutationV1,
    pub(super) allowed_effects: FrozenRepairAllowedEffectsV1,
    pub(super) rollback: FrozenRepairRollbackReferenceV1,
    pub(super) post_assertions: FrozenRepairPostAssertionsV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairManifestV1 {
    #[serde(flatten)]
    pub(super) draft: FrozenRepairManifestDraftV1,
    pub(super) manifest_digest: FrozenRepairDigestV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairManifestDraftPreBaselineV1 {
    pub(super) manifest_schema_version: u16,
    pub(super) manifest_id: String,
    pub(super) prepared_at: i64,
    pub(super) source: FrozenRepairSourceV1,
    pub(super) target: FrozenRepairTargetV1,
    pub(super) expected_state: FrozenRepairExpectedStateV1,
    pub(super) writer: FrozenRepairWriterV1,
    pub(super) mutation: FrozenRepairMutationV1,
    pub(super) allowed_effects: FrozenRepairAllowedEffectsV1,
    pub(super) rollback: FrozenRepairRollbackReferenceV1,
    pub(super) post_assertions: FrozenRepairPostAssertionsPreBaselineV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairManifestPreBaselineV1 {
    #[serde(flatten)]
    pub(super) draft: FrozenRepairManifestDraftPreBaselineV1,
    pub(super) manifest_digest: FrozenRepairDigestV1,
}

impl FrozenRepairManifestPreBaselineV1 {
    pub fn manifest_id(&self) -> &str {
        &self.draft.manifest_id
    }

    pub fn manifest_digest(&self) -> &FrozenRepairDigestV1 {
        &self.manifest_digest
    }

    pub fn rollback(&self) -> &FrozenRepairRollbackReferenceV1 {
        &self.draft.rollback
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.draft)
    }
}

impl FrozenRepairManifestV1 {
    pub fn manifest_id(&self) -> &str {
        &self.draft.manifest_id
    }

    pub fn manifest_digest(&self) -> &FrozenRepairDigestV1 {
        &self.manifest_digest
    }

    pub fn target(&self) -> &FrozenRepairTargetV1 {
        &self.draft.target
    }

    pub fn rollback(&self) -> &FrozenRepairRollbackReferenceV1 {
        &self.draft.rollback
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.draft)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairRollbackArtifactV1 {
    pub(super) format_version: u16,
    pub(super) table: String,
    pub(super) source_id: String,
    pub(super) columns: Vec<String>,
    pub(super) rows: Vec<Vec<String>>,
}

impl FrozenRepairRollbackArtifactV1 {
    pub fn format_version(&self) -> u16 {
        self.format_version
    }

    pub fn table(&self) -> &str {
        &self.table
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn rows(&self) -> &[Vec<String>] {
        &self.rows
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairApplyReceiptDraftV1 {
    pub(super) receipt_schema_version: u16,
    pub(super) manifest_id: String,
    pub(super) manifest_digest: FrozenRepairDigestV1,
    pub(super) applied_at: i64,
    pub(super) before_target_receipt: FrozenRepairDigestV1,
    pub(super) after_target_receipt: FrozenRepairDigestV1,
    pub(super) non_target_before: FrozenRepairDigestV1,
    pub(super) non_target_after: FrozenRepairDigestV1,
    pub(super) actual_effects: FrozenRepairAllowedEffectsV1,
    pub(super) writer: FrozenRepairWriterV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairApplyReceiptV1 {
    #[serde(flatten)]
    pub(super) draft: FrozenRepairApplyReceiptDraftV1,
    pub(super) receipt_digest: FrozenRepairDigestV1,
}

impl FrozenRepairApplyReceiptV1 {
    pub fn manifest_id(&self) -> &str {
        &self.draft.manifest_id
    }

    pub fn manifest_digest(&self) -> &FrozenRepairDigestV1 {
        &self.draft.manifest_digest
    }

    pub fn receipt_digest(&self) -> &FrozenRepairDigestV1 {
        &self.receipt_digest
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.draft)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairVerificationReceiptDraftV1 {
    pub(super) receipt_schema_version: u16,
    pub(super) manifest_id: String,
    pub(super) manifest_digest: FrozenRepairDigestV1,
    pub(super) apply_receipt_digest: FrozenRepairDigestV1,
    pub(super) verified_at: i64,
    pub(super) general_snapshots: FrozenLintSnapshotReceiptsV1,
    pub(super) deep_snapshots: FrozenLintSnapshotReceiptsV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenRepairVerificationReceiptV1 {
    #[serde(flatten)]
    pub(super) draft: FrozenRepairVerificationReceiptDraftV1,
    pub(super) receipt_digest: FrozenRepairDigestV1,
}

impl FrozenRepairVerificationReceiptV1 {
    pub fn manifest_id(&self) -> &str {
        &self.draft.manifest_id
    }

    pub fn manifest_digest(&self) -> &FrozenRepairDigestV1 {
        &self.draft.manifest_digest
    }

    pub fn apply_receipt_digest(&self) -> &FrozenRepairDigestV1 {
        &self.draft.apply_receipt_digest
    }

    pub fn receipt_digest(&self) -> &FrozenRepairDigestV1 {
        &self.receipt_digest
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.draft)
    }
}
