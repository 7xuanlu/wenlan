// SPDX-License-Identifier: Apache-2.0
//! Approval-gated repair contracts shared by the daemon and local clients.

mod frozen_v1;

use crate::{
    lint::{
        LintCommitReceipt, LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest, LintEvidenceRef,
        LintGateEffect, LintOpaqueId, LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt,
        LintProducerReceipt, LintProfile, LintReasonCode, LintSafeRootRelativePath, LintScope,
        LintScopeKind, LintSemanticAction, LintSemanticFinding, LintSemanticProviderRoute,
        LintSemanticReasonCode, LintSnapshotReceipts, LINT_CHECK_CATALOG_VERSION,
        LINT_REPORT_SCHEMA_VERSION,
    },
    LintReport, MemoryType,
};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::{fmt, path::Component, path::Path};

use frozen_v1::{
    FrozenRepairApplyReceiptV1, FrozenRepairManifestPreBaselineV1, FrozenRepairManifestV1,
    FrozenRepairRollbackArtifactV1, FrozenRepairVerificationReceiptV1,
};

pub const REPAIR_MANIFEST_SCHEMA_VERSION: u16 = 2;
pub const REPAIR_ROLLBACK_FORMAT_VERSION: u16 = 1;
pub const REPAIR_RECEIPT_SCHEMA_VERSION: u16 = 2;
pub const REPAIR_CLASSIFICATION_CHECK_ID: &str = "memories.semantic.classification";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoredRepairDigestRef<'a>(&'a str);

impl<'a> StoredRepairDigestRef<'a> {
    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

#[derive(Deserialize)]
struct StoredManifestVersionProbe {
    manifest_schema_version: u16,
}

#[derive(Deserialize)]
struct StoredRollbackVersionProbe {
    format_version: u16,
}

#[derive(Deserialize)]
struct StoredReceiptVersionProbe {
    receipt_schema_version: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredRepairManifest {
    V1(FrozenRepairManifestV1),
    V1PreBaseline(FrozenRepairManifestPreBaselineV1),
    V2(RepairManifest),
}

impl StoredRepairManifest {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        match serde_json::from_slice::<StoredManifestVersionProbe>(bytes)?.manifest_schema_version {
            1 => match serde_json::from_slice(bytes) {
                Ok(manifest) => Ok(Self::V1(manifest)),
                Err(current_error) => serde_json::from_slice(bytes)
                    .map(Self::V1PreBaseline)
                    .map_err(|_| current_error),
            },
            2 => serde_json::from_slice(bytes).map(Self::V2),
            version => Err(serde_json::Error::custom(format!(
                "unsupported repair manifest schema version {version}"
            ))),
        }
    }

    pub fn manifest_id(&self) -> &str {
        match self {
            Self::V1(manifest) => manifest.manifest_id(),
            Self::V1PreBaseline(manifest) => manifest.manifest_id(),
            Self::V2(manifest) => manifest.manifest_id(),
        }
    }

    pub fn manifest_digest(&self) -> StoredRepairDigestRef<'_> {
        match self {
            Self::V1(manifest) => StoredRepairDigestRef(manifest.manifest_digest().as_str()),
            Self::V1PreBaseline(manifest) => {
                StoredRepairDigestRef(manifest.manifest_digest().as_str())
            }
            Self::V2(manifest) => StoredRepairDigestRef(manifest.manifest_digest().as_str()),
        }
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Self::V1(manifest) => manifest.canonical_unsigned_bytes(),
            Self::V1PreBaseline(manifest) => manifest.canonical_unsigned_bytes(),
            Self::V2(manifest) => manifest.canonical_unsigned_bytes(),
        }
    }

    pub fn rollback_digest(&self) -> StoredRepairDigestRef<'_> {
        match self {
            Self::V1(manifest) => StoredRepairDigestRef(manifest.rollback().digest().as_str()),
            Self::V1PreBaseline(manifest) => {
                StoredRepairDigestRef(manifest.rollback().digest().as_str())
            }
            Self::V2(manifest) => StoredRepairDigestRef(manifest.rollback().digest().as_str()),
        }
    }

    pub fn persisted_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Self::V1(manifest) => serde_json::to_vec_pretty(manifest),
            Self::V1PreBaseline(manifest) => serde_json::to_vec_pretty(manifest),
            Self::V2(manifest) => serde_json::to_vec_pretty(manifest),
        }
    }

    pub fn verify_and_try_into_current(
        self,
        verify: impl FnOnce(&[u8], StoredRepairDigestRef<'_>) -> bool,
    ) -> Result<RepairManifest, RepairContractError> {
        let canonical = self
            .canonical_unsigned_bytes()
            .map_err(|_| RepairContractError::InvalidManifest)?;
        if !verify(&canonical, self.manifest_digest()) {
            return Err(RepairContractError::InvalidDigest);
        }
        match self {
            Self::V1(manifest) => frozen_manifest_v1_into_current(manifest),
            Self::V1PreBaseline(manifest) => frozen_manifest_pre_baseline_v1_into_current(manifest),
            Self::V2(manifest) => Ok(manifest),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredRepairRollbackArtifact {
    V1(FrozenRepairRollbackArtifactV1),
}

impl StoredRepairRollbackArtifact {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        match serde_json::from_slice::<StoredRollbackVersionProbe>(bytes)?.format_version {
            1 => serde_json::from_slice(bytes).map(Self::V1),
            version => Err(serde_json::Error::custom(format!(
                "unsupported repair rollback format version {version}"
            ))),
        }
    }

    pub fn persisted_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Self::V1(rollback) => serde_json::to_vec_pretty(rollback),
        }
    }

    pub fn as_v1(&self) -> &FrozenRepairRollbackArtifactV1 {
        match self {
            Self::V1(rollback) => rollback,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredRepairApplyReceipt {
    V1(FrozenRepairApplyReceiptV1),
    V2(RepairApplyReceipt),
}

impl StoredRepairApplyReceipt {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        match serde_json::from_slice::<StoredReceiptVersionProbe>(bytes)?.receipt_schema_version {
            1 => serde_json::from_slice(bytes).map(Self::V1),
            2 => serde_json::from_slice(bytes).map(Self::V2),
            version => Err(serde_json::Error::custom(format!(
                "unsupported repair receipt schema version {version}"
            ))),
        }
    }

    pub fn receipt_digest(&self) -> StoredRepairDigestRef<'_> {
        match self {
            Self::V1(receipt) => StoredRepairDigestRef(receipt.receipt_digest().as_str()),
            Self::V2(receipt) => StoredRepairDigestRef(receipt.receipt_digest().as_str()),
        }
    }

    pub fn manifest_id(&self) -> &str {
        match self {
            Self::V1(receipt) => receipt.manifest_id(),
            Self::V2(receipt) => receipt.manifest_id(),
        }
    }

    pub fn manifest_digest(&self) -> StoredRepairDigestRef<'_> {
        match self {
            Self::V1(receipt) => StoredRepairDigestRef(receipt.manifest_digest().as_str()),
            Self::V2(receipt) => StoredRepairDigestRef(receipt.manifest_digest().as_str()),
        }
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Self::V1(receipt) => receipt.canonical_unsigned_bytes(),
            Self::V2(receipt) => receipt.canonical_unsigned_bytes(),
        }
    }

    pub fn persisted_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Self::V1(receipt) => serde_json::to_vec_pretty(receipt),
            Self::V2(receipt) => serde_json::to_vec_pretty(receipt),
        }
    }

    pub fn verify_and_try_into_current(
        self,
        verify: impl FnOnce(&[u8], StoredRepairDigestRef<'_>) -> bool,
    ) -> Result<RepairApplyReceipt, RepairContractError> {
        let canonical = self
            .canonical_unsigned_bytes()
            .map_err(|_| RepairContractError::InvalidReceipt)?;
        if !verify(&canonical, self.receipt_digest()) {
            return Err(RepairContractError::InvalidDigest);
        }
        match self {
            Self::V1(receipt) => frozen_apply_receipt_v1_into_current(receipt),
            Self::V2(receipt) => Ok(receipt),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredRepairVerificationReceipt {
    V1(FrozenRepairVerificationReceiptV1),
    V2(RepairVerificationReceipt),
}

impl StoredRepairVerificationReceipt {
    pub fn from_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        match serde_json::from_slice::<StoredReceiptVersionProbe>(bytes)?.receipt_schema_version {
            1 => serde_json::from_slice(bytes).map(Self::V1),
            2 => serde_json::from_slice(bytes).map(Self::V2),
            version => Err(serde_json::Error::custom(format!(
                "unsupported repair receipt schema version {version}"
            ))),
        }
    }

    pub fn receipt_digest(&self) -> StoredRepairDigestRef<'_> {
        match self {
            Self::V1(receipt) => StoredRepairDigestRef(receipt.receipt_digest().as_str()),
            Self::V2(receipt) => StoredRepairDigestRef(receipt.receipt_digest().as_str()),
        }
    }

    pub fn manifest_id(&self) -> &str {
        match self {
            Self::V1(receipt) => receipt.manifest_id(),
            Self::V2(receipt) => receipt.manifest_id(),
        }
    }

    pub fn manifest_digest(&self) -> StoredRepairDigestRef<'_> {
        match self {
            Self::V1(receipt) => StoredRepairDigestRef(receipt.manifest_digest().as_str()),
            Self::V2(receipt) => StoredRepairDigestRef(receipt.manifest_digest().as_str()),
        }
    }

    pub fn apply_receipt_digest(&self) -> StoredRepairDigestRef<'_> {
        match self {
            Self::V1(receipt) => StoredRepairDigestRef(receipt.apply_receipt_digest().as_str()),
            Self::V2(receipt) => StoredRepairDigestRef(receipt.apply_receipt_digest().as_str()),
        }
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Self::V1(receipt) => receipt.canonical_unsigned_bytes(),
            Self::V2(receipt) => receipt.canonical_unsigned_bytes(),
        }
    }

    pub fn persisted_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        match self {
            Self::V1(receipt) => serde_json::to_vec_pretty(receipt),
            Self::V2(receipt) => serde_json::to_vec_pretty(receipt),
        }
    }

    pub fn verify_and_try_into_current(
        self,
        verify: impl FnOnce(&[u8], StoredRepairDigestRef<'_>) -> bool,
    ) -> Result<RepairVerificationReceipt, RepairContractError> {
        let canonical = self
            .canonical_unsigned_bytes()
            .map_err(|_| RepairContractError::InvalidReceipt)?;
        if !verify(&canonical, self.receipt_digest()) {
            return Err(RepairContractError::InvalidDigest);
        }
        match self {
            Self::V1(receipt) => frozen_verification_receipt_v1_into_current(receipt),
            Self::V2(receipt) => Ok(receipt),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairContractError {
    InvalidDigest,
    InvalidManifestId,
    InvalidSource,
    InvalidTarget,
    InvalidExpectedState,
    UnsupportedWriter,
    InvalidMutation,
    InvalidAllowedEffects,
    InvalidRollbackArtifact,
    InvalidPostAssertions,
    UnsupportedManifestSchema,
    InvalidManifest,
    InvalidPrepareRequest,
    InvalidApplyRequest,
    InvalidVerifyRequest,
    InvalidReceipt,
}

impl RepairContractError {
    const fn code(self) -> &'static str {
        match self {
            Self::InvalidDigest => "invalid_repair_digest",
            Self::InvalidManifestId => "invalid_repair_manifest_id",
            Self::InvalidSource => "invalid_repair_source",
            Self::InvalidTarget => "invalid_repair_target",
            Self::InvalidExpectedState => "invalid_repair_expected_state",
            Self::UnsupportedWriter => "unsupported_repair_writer",
            Self::InvalidMutation => "invalid_repair_mutation",
            Self::InvalidAllowedEffects => "invalid_repair_allowed_effects",
            Self::InvalidRollbackArtifact => "invalid_repair_rollback_artifact",
            Self::InvalidPostAssertions => "invalid_repair_post_assertions",
            Self::UnsupportedManifestSchema => "unsupported_repair_manifest_schema",
            Self::InvalidManifest => "invalid_repair_manifest",
            Self::InvalidPrepareRequest => "invalid_prepare_repair_request",
            Self::InvalidApplyRequest => "invalid_apply_repair_request",
            Self::InvalidVerifyRequest => "invalid_verify_repair_request",
            Self::InvalidReceipt => "invalid_repair_receipt",
        }
    }
}

impl fmt::Display for RepairContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for RepairContractError {}

fn frozen_repair_digest_v1(
    digest: frozen_v1::FrozenRepairDigestV1,
) -> Result<RepairDigest, RepairContractError> {
    RepairDigest::parse(&digest.0)
}

fn frozen_lint_digest_v1(
    digest: frozen_v1::FrozenLintDigestV1,
) -> Result<LintDigest, RepairContractError> {
    LintDigest::from_hex(&digest.0).map_err(|_| RepairContractError::InvalidSource)
}

fn frozen_lint_opaque_id_v1(
    id: frozen_v1::FrozenLintOpaqueIdV1,
) -> Result<LintOpaqueId, RepairContractError> {
    let position =
        id.0.checked_sub(1)
            .and_then(|value| usize::try_from(value).ok())
            .and_then(LintOpaqueId::from_sorted_position)
            .ok_or(RepairContractError::InvalidSource)?;
    Ok(position)
}

fn frozen_lint_scope_v1(
    scope: frozen_v1::FrozenLintScopeV1,
) -> Result<LintScope, RepairContractError> {
    match (scope.kind, scope.opaque_scope_ref) {
        (frozen_v1::FrozenLintScopeKindV1::Global, None) => Ok(LintScope::global()),
        (frozen_v1::FrozenLintScopeKindV1::Registered, Some(reference)) => {
            Ok(LintScope::registered(frozen_lint_opaque_id_v1(reference)?))
        }
        (frozen_v1::FrozenLintScopeKindV1::Uncategorized, None) => Ok(LintScope::uncategorized()),
        _ => Err(RepairContractError::InvalidSource),
    }
}

fn frozen_lint_snapshots_v1(
    snapshots: frozen_v1::FrozenLintSnapshotReceiptsV1,
) -> Result<LintSnapshotReceipts, RepairContractError> {
    let db = snapshots.db;
    let db_mode = match db.mode {
        frozen_v1::FrozenLintDbSnapshotModeV1::TransactionalReadOnly => {
            LintDbSnapshotMode::TransactionalReadOnly
        }
    };
    let pages = snapshots.pages;
    let page_mode = match pages.mode {
        frozen_v1::FrozenLintPageSnapshotModeV1::BestEffort => LintPageSnapshotMode::BestEffort,
    };
    Ok(LintSnapshotReceipts::new(
        LintDbSnapshotReceipt::new(
            db_mode,
            frozen_lint_digest_v1(db.analysis_digest)?,
            db.post_run_digest.map(frozen_lint_digest_v1).transpose()?,
        ),
        LintPageSnapshotReceipt::new(
            page_mode,
            frozen_lint_digest_v1(pages.before_scan_digest)?,
            pages
                .after_scan_digest
                .map(frozen_lint_digest_v1)
                .transpose()?,
        ),
    ))
}

fn frozen_lint_producer_receipt_v1(
    receipt: frozen_v1::FrozenLintProducerReceiptV1,
) -> Result<LintProducerReceipt, RepairContractError> {
    let commit = receipt
        .runtime_commit
        .map(|commit| {
            LintCommitReceipt::new(&commit.0).map_err(|_| RepairContractError::InvalidSource)
        })
        .transpose()?;
    Ok(LintProducerReceipt::new(commit))
}

fn frozen_semantic_action_v1(action: frozen_v1::FrozenLintSemanticActionV1) -> LintSemanticAction {
    match action {
        frozen_v1::FrozenLintSemanticActionV1::ReclassifyMemory => {
            LintSemanticAction::ReclassifyMemory
        }
        frozen_v1::FrozenLintSemanticActionV1::ReviewContradiction => {
            LintSemanticAction::ReviewContradiction
        }
        frozen_v1::FrozenLintSemanticActionV1::ReviewStaleness => {
            LintSemanticAction::ReviewStaleness
        }
        frozen_v1::FrozenLintSemanticActionV1::SupersedeMemory => {
            LintSemanticAction::SupersedeMemory
        }
        frozen_v1::FrozenLintSemanticActionV1::AddMemoryEntityLink => {
            LintSemanticAction::AddMemoryEntityLink
        }
        frozen_v1::FrozenLintSemanticActionV1::RemoveMemoryEntityLink => {
            LintSemanticAction::RemoveMemoryEntityLink
        }
        frozen_v1::FrozenLintSemanticActionV1::AddEntityRelation => {
            LintSemanticAction::AddEntityRelation
        }
        frozen_v1::FrozenLintSemanticActionV1::RemoveEntityRelation => {
            LintSemanticAction::RemoveEntityRelation
        }
        frozen_v1::FrozenLintSemanticActionV1::ReviewPageClaim => {
            LintSemanticAction::ReviewPageClaim
        }
        frozen_v1::FrozenLintSemanticActionV1::AddPageEvidence => {
            LintSemanticAction::AddPageEvidence
        }
        frozen_v1::FrozenLintSemanticActionV1::RemovePageEvidence => {
            LintSemanticAction::RemovePageEvidence
        }
        frozen_v1::FrozenLintSemanticActionV1::ReviewRetrieval => {
            LintSemanticAction::ReviewRetrieval
        }
    }
}

fn frozen_semantic_reason_v1(
    reason: frozen_v1::FrozenLintSemanticReasonCodeV1,
) -> LintSemanticReasonCode {
    match reason {
        frozen_v1::FrozenLintSemanticReasonCodeV1::ClassificationMismatch => {
            LintSemanticReasonCode::ClassificationMismatch
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::PotentialContradiction => {
            LintSemanticReasonCode::PotentialContradiction
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::PotentialStaleness => {
            LintSemanticReasonCode::PotentialStaleness
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::MentionWithoutLink => {
            LintSemanticReasonCode::MentionWithoutLink
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::ExistingLinkMismatch => {
            LintSemanticReasonCode::ExistingLinkMismatch
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::SharedContextWithoutRelation => {
            LintSemanticReasonCode::SharedContextWithoutRelation
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::ExistingRelationMismatch => {
            LintSemanticReasonCode::ExistingRelationMismatch
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::PotentialUnfaithfulClaim => {
            LintSemanticReasonCode::PotentialUnfaithfulClaim
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::PotentialInadequateProvenance => {
            LintSemanticReasonCode::PotentialInadequateProvenance
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::ClaimOverlapWithoutEvidence => {
            LintSemanticReasonCode::ClaimOverlapWithoutEvidence
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::ExistingEvidenceMismatch => {
            LintSemanticReasonCode::ExistingEvidenceMismatch
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::PotentialRetrievalMiss => {
            LintSemanticReasonCode::PotentialRetrievalMiss
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::DanglingOwner => {
            LintSemanticReasonCode::DanglingOwner
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::TemporalEvolution => {
            LintSemanticReasonCode::TemporalEvolution
        }
        frozen_v1::FrozenLintSemanticReasonCodeV1::RelatedButNotEvidence => {
            LintSemanticReasonCode::RelatedButNotEvidence
        }
    }
}

fn frozen_semantic_provider_v1(
    provider: frozen_v1::FrozenLintSemanticProviderRouteV1,
) -> LintSemanticProviderRoute {
    match provider {
        frozen_v1::FrozenLintSemanticProviderRouteV1::OnDevice => {
            LintSemanticProviderRoute::OnDevice
        }
        frozen_v1::FrozenLintSemanticProviderRouteV1::ConfiguredExternal => {
            LintSemanticProviderRoute::ConfiguredExternal
        }
        frozen_v1::FrozenLintSemanticProviderRouteV1::CallingAgent => {
            LintSemanticProviderRoute::CallingAgent
        }
    }
}

fn frozen_semantic_finding_v1(
    finding: frozen_v1::FrozenLintSemanticFindingV1,
) -> Result<LintSemanticFinding, RepairContractError> {
    LintSemanticFinding::try_new_with_disagreement(
        frozen_lint_opaque_id_v1(finding.candidate_id)?,
        frozen_semantic_action_v1(finding.proposed_action),
        frozen_semantic_reason_v1(finding.reason_code),
        finding.confidence_basis_points,
        frozen_semantic_provider_v1(finding.provider_route),
        finding
            .evidence_ids
            .into_iter()
            .map(frozen_lint_digest_v1)
            .collect::<Result<Vec<_>, _>>()?,
        finding
            .counterevidence_ids
            .into_iter()
            .map(frozen_lint_digest_v1)
            .collect::<Result<Vec<_>, _>>()?,
        finding.unresolved_disagreement,
    )
    .map_err(|_| RepairContractError::InvalidSource)
}

fn frozen_lint_outcome_v1(outcome: frozen_v1::FrozenLintOutcomeV1) -> LintOutcome {
    match outcome {
        frozen_v1::FrozenLintOutcomeV1::Pass => LintOutcome::Pass,
        frozen_v1::FrozenLintOutcomeV1::Finding => LintOutcome::Finding,
        frozen_v1::FrozenLintOutcomeV1::NotRunPrerequisite => LintOutcome::NotRunPrerequisite,
        frozen_v1::FrozenLintOutcomeV1::InconsistentSnapshot => LintOutcome::InconsistentSnapshot,
        frozen_v1::FrozenLintOutcomeV1::FailedToRun => LintOutcome::FailedToRun,
    }
}

fn frozen_lint_gate_effect_v1(effect: frozen_v1::FrozenLintGateEffectV1) -> LintGateEffect {
    match effect {
        frozen_v1::FrozenLintGateEffectV1::Actionable => LintGateEffect::Actionable,
        frozen_v1::FrozenLintGateEffectV1::Advisory => LintGateEffect::Advisory,
    }
}

fn frozen_lint_reason_v1(reason: frozen_v1::FrozenLintReasonCodeV1) -> LintReasonCode {
    match reason {
        frozen_v1::FrozenLintReasonCodeV1::MissingArtifact => LintReasonCode::MissingArtifact,
        frozen_v1::FrozenLintReasonCodeV1::InvalidCatalogState => {
            LintReasonCode::InvalidCatalogState
        }
        frozen_v1::FrozenLintReasonCodeV1::ExpectedEmptySubstrate => {
            LintReasonCode::ExpectedEmptySubstrate
        }
        frozen_v1::FrozenLintReasonCodeV1::InvalidSourceConfiguration => {
            LintReasonCode::InvalidSourceConfiguration
        }
        frozen_v1::FrozenLintReasonCodeV1::TerminalOperationFailure => {
            LintReasonCode::TerminalOperationFailure
        }
        frozen_v1::FrozenLintReasonCodeV1::ExpiredRetry => LintReasonCode::ExpiredRetry,
        frozen_v1::FrozenLintReasonCodeV1::InvalidOperationState => {
            LintReasonCode::InvalidOperationState
        }
        frozen_v1::FrozenLintReasonCodeV1::DurableNoProgress => LintReasonCode::DurableNoProgress,
        frozen_v1::FrozenLintReasonCodeV1::SemanticProviderUnavailable => {
            LintReasonCode::SemanticProviderUnavailable
        }
        frozen_v1::FrozenLintReasonCodeV1::InsufficientSemanticEvidence => {
            LintReasonCode::InsufficientSemanticEvidence
        }
        frozen_v1::FrozenLintReasonCodeV1::SemanticExecutionFailure => {
            LintReasonCode::SemanticExecutionFailure
        }
        frozen_v1::FrozenLintReasonCodeV1::SemanticAgentAdjudicationRequired => {
            LintReasonCode::SemanticAgentAdjudicationRequired
        }
        frozen_v1::FrozenLintReasonCodeV1::SemanticAgentWorkStale => {
            LintReasonCode::SemanticAgentWorkStale
        }
        frozen_v1::FrozenLintReasonCodeV1::SemanticAgentSubmissionInvalid => {
            LintReasonCode::SemanticAgentSubmissionInvalid
        }
        frozen_v1::FrozenLintReasonCodeV1::SemanticCandidateGenerationFailure => {
            LintReasonCode::SemanticCandidateGenerationFailure
        }
        frozen_v1::FrozenLintReasonCodeV1::SemanticPopulationIncomplete => {
            LintReasonCode::SemanticPopulationIncomplete
        }
        frozen_v1::FrozenLintReasonCodeV1::SemanticDisagreementUnresolved => {
            LintReasonCode::SemanticDisagreementUnresolved
        }
        frozen_v1::FrozenLintReasonCodeV1::SemanticSecondJudgeRequired => {
            LintReasonCode::SemanticSecondJudgeRequired
        }
    }
}

fn frozen_lint_safe_path_v1(
    path: frozen_v1::FrozenLintSafeRootRelativePathV1,
) -> LintSafeRootRelativePath {
    match path {
        frozen_v1::FrozenLintSafeRootRelativePathV1::PagesRoot => {
            LintSafeRootRelativePath::PagesRoot
        }
        frozen_v1::FrozenLintSafeRootRelativePathV1::PagesState => {
            LintSafeRootRelativePath::PagesState
        }
        frozen_v1::FrozenLintSafeRootRelativePathV1::PagesManifest => {
            LintSafeRootRelativePath::PagesManifest
        }
        frozen_v1::FrozenLintSafeRootRelativePathV1::PagesStubs => {
            LintSafeRootRelativePath::PagesStubs
        }
    }
}

fn frozen_lint_evidence_v1(
    evidence: frozen_v1::FrozenLintEvidenceRefV1,
) -> Result<LintEvidenceRef, RepairContractError> {
    match evidence {
        frozen_v1::FrozenLintEvidenceRefV1::OpaqueId { opaque_id } => {
            Ok(LintEvidenceRef::OpaqueId {
                opaque_id: frozen_lint_opaque_id_v1(opaque_id)?,
            })
        }
        frozen_v1::FrozenLintEvidenceRefV1::ReasonCode { reason_code } => {
            Ok(LintEvidenceRef::ReasonCode {
                reason_code: frozen_lint_reason_v1(reason_code),
            })
        }
        frozen_v1::FrozenLintEvidenceRefV1::SafeRootRelativePath {
            safe_root_relative_path,
        } => Ok(LintEvidenceRef::SafeRootRelativePath {
            safe_root_relative_path: frozen_lint_safe_path_v1(safe_root_relative_path),
        }),
        frozen_v1::FrozenLintEvidenceRefV1::SemanticFinding { finding } => {
            Ok(LintEvidenceRef::SemanticFinding {
                finding: frozen_semantic_finding_v1(finding)?,
            })
        }
    }
}

fn frozen_repair_lint_scope_v1(
    scope: frozen_v1::FrozenRepairLintScopeV1,
) -> Result<RepairLintScope, RepairContractError> {
    match scope {
        frozen_v1::FrozenRepairLintScopeV1::Global {} => Ok(RepairLintScope::global()),
        frozen_v1::FrozenRepairLintScopeV1::Registered { space } => {
            RepairLintScope::registered(space)
        }
        frozen_v1::FrozenRepairLintScopeV1::Uncategorized {} => {
            Ok(RepairLintScope::uncategorized())
        }
    }
}

fn frozen_repair_scope_v1(
    scope: frozen_v1::FrozenRepairScopeV1,
) -> Result<RepairScope, RepairContractError> {
    match scope {
        frozen_v1::FrozenRepairScopeV1::Registered { space } => RepairScope::registered(space),
        frozen_v1::FrozenRepairScopeV1::Uncategorized {} => Ok(RepairScope::uncategorized()),
    }
}

fn frozen_repair_target_v1(
    target: frozen_v1::FrozenRepairTargetV1,
) -> Result<RepairTarget, RepairContractError> {
    match target {
        frozen_v1::FrozenRepairTargetV1::Memory { source_id, scope } => {
            RepairTarget::memory(source_id, frozen_repair_scope_v1(scope)?)
        }
    }
}

fn frozen_repair_source_v1(
    source: frozen_v1::FrozenRepairSourceV1,
) -> Result<RepairSource, RepairContractError> {
    if source.report_schema_version != frozen_v1::FROZEN_LINT_REPORT_SCHEMA_VERSION_V1
        || source.check_catalog_version != frozen_v1::FROZEN_LINT_CHECK_CATALOG_VERSION_V1
        || source.check_id != REPAIR_CLASSIFICATION_CHECK_ID
    {
        return Err(RepairContractError::InvalidSource);
    }
    RepairSource::try_new(
        frozen_repair_lint_scope_v1(source.lint_scope)?,
        frozen_lint_scope_v1(source.report_scope)?,
        frozen_semantic_finding_v1(source.finding)?,
        frozen_lint_snapshots_v1(source.general_snapshots)?,
        frozen_lint_snapshots_v1(source.deep_snapshots)?,
        frozen_lint_producer_receipt_v1(source.general_producer_receipt)?,
        frozen_lint_producer_receipt_v1(source.deep_producer_receipt)?,
        frozen_lint_digest_v1(source.agent_work_digest)?,
    )
}

fn frozen_expected_state_v1(
    expected: frozen_v1::FrozenRepairExpectedStateV1,
) -> Result<RepairExpectedState, RepairContractError> {
    RepairExpectedState::try_new(
        expected.version,
        frozen_repair_digest_v1(expected.canonical_receipt)?,
    )
}

fn frozen_writer_v1(writer: frozen_v1::FrozenRepairWriterV1) -> RepairWriter {
    match writer {
        frozen_v1::FrozenRepairWriterV1::ReclassifyMemory => RepairWriter::ReclassifyMemory,
    }
}

fn frozen_memory_type_v1(memory_type: frozen_v1::FrozenMemoryTypeV1) -> MemoryType {
    match memory_type {
        frozen_v1::FrozenMemoryTypeV1::Identity => MemoryType::Identity,
        frozen_v1::FrozenMemoryTypeV1::Preference => MemoryType::Preference,
        frozen_v1::FrozenMemoryTypeV1::Decision => MemoryType::Decision,
        frozen_v1::FrozenMemoryTypeV1::Lesson => MemoryType::Lesson,
        frozen_v1::FrozenMemoryTypeV1::Gotcha => MemoryType::Gotcha,
        frozen_v1::FrozenMemoryTypeV1::Fact => MemoryType::Fact,
    }
}

fn frozen_mutation_v1(
    mutation: frozen_v1::FrozenRepairMutationV1,
) -> Result<RepairMutation, RepairContractError> {
    match mutation {
        frozen_v1::FrozenRepairMutationV1::ReclassifyMemory {
            before_memory_type,
            after_memory_type,
        } => RepairMutation::from_memory_types(
            before_memory_type.map(frozen_memory_type_v1),
            frozen_memory_type_v1(after_memory_type),
        ),
    }
}

fn frozen_allowed_effects_v1(
    effects: frozen_v1::FrozenRepairAllowedEffectsV1,
) -> Result<RepairAllowedEffects, RepairContractError> {
    if effects.fields != [frozen_v1::FrozenRepairMemoryFieldV1::MemoryType] {
        return Err(RepairContractError::InvalidAllowedEffects);
    }
    Ok(RepairAllowedEffects::memory_type(frozen_repair_target_v1(
        effects.owner,
    )?))
}

fn frozen_rollback_reference_v1(
    rollback: frozen_v1::FrozenRepairRollbackReferenceV1,
) -> Result<RepairRollbackArtifact, RepairContractError> {
    if rollback.format_version != 1 {
        return Err(RepairContractError::InvalidRollbackArtifact);
    }
    RepairRollbackArtifact::try_new(
        rollback.relative_path,
        frozen_repair_digest_v1(rollback.digest)?,
    )
}

fn frozen_check_baseline_v1(
    baseline: frozen_v1::FrozenRepairCheckBaselineV1,
) -> Result<RepairCheckBaseline, RepairContractError> {
    RepairCheckBaseline::try_new(
        baseline.check_id,
        frozen_lint_outcome_v1(baseline.outcome),
        frozen_lint_gate_effect_v1(baseline.gate_effect),
        baseline
            .evidence
            .into_iter()
            .map(frozen_lint_evidence_v1)
            .collect::<Result<Vec<_>, _>>()?,
    )
}

fn frozen_post_assertions_v1(
    assertions: frozen_v1::FrozenRepairPostAssertionsV1,
) -> Result<RepairPostAssertions, RepairContractError> {
    if assertions.target_check_id != REPAIR_CLASSIFICATION_CHECK_ID
        || !assertions.require_complete_general
        || !assertions.require_complete_deep
        || !assertions.reject_new_actionable
        || !assertions.reject_new_incomplete
    {
        return Err(RepairContractError::InvalidPostAssertions);
    }
    RepairPostAssertions::try_new_legacy_v1(
        frozen_lint_digest_v1(assertions.target_evidence_id)?,
        assertions
            .general_baseline
            .into_iter()
            .map(frozen_check_baseline_v1)
            .collect::<Result<Vec<_>, _>>()?,
        assertions
            .deep_baseline
            .into_iter()
            .map(frozen_check_baseline_v1)
            .collect::<Result<Vec<_>, _>>()?,
        assertions.allowed_non_target_check_deltas,
    )
}

fn frozen_post_assertions_pre_baseline_v1(
    assertions: frozen_v1::FrozenRepairPostAssertionsPreBaselineV1,
) -> Result<RepairPostAssertions, RepairContractError> {
    if assertions.target_check_id != REPAIR_CLASSIFICATION_CHECK_ID
        || !assertions.require_complete_general
        || !assertions.require_complete_deep
        || !assertions.reject_new_actionable
        || !assertions.reject_new_incomplete
        || assertions
            .allowed_non_target_check_deltas
            .iter()
            .any(|value| !valid_nonempty(value))
        || !assertions
            .allowed_non_target_check_deltas
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    {
        return Err(RepairContractError::InvalidPostAssertions);
    }
    Ok(RepairPostAssertions {
        target_check_id: REPAIR_CLASSIFICATION_CHECK_ID.to_string(),
        target_evidence_id: frozen_lint_digest_v1(assertions.target_evidence_id)?,
        general_baseline: Vec::new(),
        deep_baseline: Vec::new(),
        verification_policy: RepairVerificationPolicy::LegacyWholeReports,
        require_complete_general: true,
        reject_new_actionable: true,
        reject_new_incomplete: true,
        allowed_non_target_check_deltas: assertions.allowed_non_target_check_deltas,
    })
}

fn frozen_manifest_v1_into_current(
    manifest: FrozenRepairManifestV1,
) -> Result<RepairManifest, RepairContractError> {
    let frozen_v1::FrozenRepairManifestV1 {
        draft,
        manifest_digest,
    } = manifest;
    if draft.manifest_schema_version != 1 {
        return Err(RepairContractError::UnsupportedManifestSchema);
    }
    let draft = RepairManifestDraft::try_new(
        draft.manifest_id,
        draft.prepared_at,
        frozen_repair_source_v1(draft.source)?,
        frozen_repair_target_v1(draft.target)?,
        frozen_expected_state_v1(draft.expected_state)?,
        frozen_writer_v1(draft.writer),
        frozen_mutation_v1(draft.mutation)?,
        frozen_allowed_effects_v1(draft.allowed_effects)?,
        frozen_rollback_reference_v1(draft.rollback)?,
        frozen_post_assertions_v1(draft.post_assertions)?,
    )?;
    RepairManifest::try_new(draft, frozen_repair_digest_v1(manifest_digest)?)
}

fn frozen_manifest_pre_baseline_v1_into_current(
    manifest: FrozenRepairManifestPreBaselineV1,
) -> Result<RepairManifest, RepairContractError> {
    let frozen_v1::FrozenRepairManifestPreBaselineV1 {
        draft,
        manifest_digest,
    } = manifest;
    if draft.manifest_schema_version != 1 {
        return Err(RepairContractError::UnsupportedManifestSchema);
    }
    let draft = RepairManifestDraft::try_new(
        draft.manifest_id,
        draft.prepared_at,
        frozen_repair_source_v1(draft.source)?,
        frozen_repair_target_v1(draft.target)?,
        frozen_expected_state_v1(draft.expected_state)?,
        frozen_writer_v1(draft.writer),
        frozen_mutation_v1(draft.mutation)?,
        frozen_allowed_effects_v1(draft.allowed_effects)?,
        frozen_rollback_reference_v1(draft.rollback)?,
        frozen_post_assertions_pre_baseline_v1(draft.post_assertions)?,
    )?;
    RepairManifest::try_new(draft, frozen_repair_digest_v1(manifest_digest)?)
}

fn frozen_apply_receipt_v1_into_current(
    receipt: FrozenRepairApplyReceiptV1,
) -> Result<RepairApplyReceipt, RepairContractError> {
    let frozen_v1::FrozenRepairApplyReceiptV1 {
        draft,
        receipt_digest,
    } = receipt;
    if draft.receipt_schema_version != 1 {
        return Err(RepairContractError::InvalidReceipt);
    }
    let draft = RepairApplyReceiptDraft::try_new_legacy_v1(
        draft.manifest_id,
        frozen_repair_digest_v1(draft.manifest_digest)?,
        draft.applied_at,
        frozen_repair_digest_v1(draft.before_target_receipt)?,
        frozen_repair_digest_v1(draft.after_target_receipt)?,
        frozen_repair_digest_v1(draft.non_target_before)?,
        frozen_repair_digest_v1(draft.non_target_after)?,
        frozen_allowed_effects_v1(draft.actual_effects)?,
        frozen_writer_v1(draft.writer),
    )?;
    Ok(RepairApplyReceipt::from_draft(
        draft,
        frozen_repair_digest_v1(receipt_digest)?,
    ))
}

fn frozen_verification_receipt_v1_into_current(
    receipt: FrozenRepairVerificationReceiptV1,
) -> Result<RepairVerificationReceipt, RepairContractError> {
    let frozen_v1::FrozenRepairVerificationReceiptV1 {
        draft,
        receipt_digest,
    } = receipt;
    if draft.receipt_schema_version != 1 {
        return Err(RepairContractError::InvalidReceipt);
    }
    let draft = RepairVerificationReceiptDraft::try_new_legacy_v1(
        draft.manifest_id,
        frozen_repair_digest_v1(draft.manifest_digest)?,
        frozen_repair_digest_v1(draft.apply_receipt_digest)?,
        draft.verified_at,
        frozen_lint_snapshots_v1(draft.general_snapshots)?,
        frozen_lint_snapshots_v1(draft.deep_snapshots)?,
    )?;
    Ok(RepairVerificationReceipt::from_draft(
        draft,
        frozen_repair_digest_v1(receipt_digest)?,
    ))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
        })
}

fn valid_manifest_id(value: &str) -> bool {
    let Some(uuid) = value.strip_prefix("repair_") else {
        return false;
    };
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit()),
        })
}

fn valid_nonempty(value: &str) -> bool {
    !value.is_empty() && value.trim() == value
}

fn parse_memory_type(value: &str) -> Result<MemoryType, RepairContractError> {
    if !MemoryType::all_values().contains(&value) {
        return Err(RepairContractError::InvalidMutation);
    }
    value
        .parse()
        .map_err(|_| RepairContractError::InvalidMutation)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RepairDigest(String);

impl RepairDigest {
    pub fn parse(value: &str) -> Result<Self, RepairContractError> {
        if is_lower_hex(value, 64) {
            Ok(Self(value.to_string()))
        } else {
            Err(RepairContractError::InvalidDigest)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RepairDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(&String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairLintScope {
    Global,
    Registered { space: String },
    Uncategorized,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RepairLintScopeWire {
    Global,
    Registered { space: String },
    Uncategorized,
}

impl RepairLintScope {
    pub const fn global() -> Self {
        Self::Global
    }

    pub fn registered(space: String) -> Result<Self, RepairContractError> {
        if valid_nonempty(&space) {
            Ok(Self::Registered { space })
        } else {
            Err(RepairContractError::InvalidSource)
        }
    }

    pub const fn uncategorized() -> Self {
        Self::Uncategorized
    }

    pub fn space(&self) -> Option<&str> {
        match self {
            Self::Registered { space } => Some(space),
            Self::Global | Self::Uncategorized => None,
        }
    }

    pub fn matches_report_scope_kind(&self, report_scope: &LintScope) -> bool {
        matches!(
            (self, report_scope.kind()),
            (Self::Global, LintScopeKind::Global)
                | (Self::Registered { .. }, LintScopeKind::Registered)
                | (Self::Uncategorized, LintScopeKind::Uncategorized)
        )
    }
}

impl<'de> Deserialize<'de> for RepairLintScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RepairLintScopeWire::deserialize(deserializer)? {
            RepairLintScopeWire::Global => Ok(Self::global()),
            RepairLintScopeWire::Registered { space } => Self::registered(space),
            RepairLintScopeWire::Uncategorized => Ok(Self::uncategorized()),
        }
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairScope {
    Registered { space: String },
    Uncategorized,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RepairScopeWire {
    Registered { space: String },
    Uncategorized,
}

impl RepairScope {
    pub fn registered(space: String) -> Result<Self, RepairContractError> {
        if valid_nonempty(&space) {
            Ok(Self::Registered { space })
        } else {
            Err(RepairContractError::InvalidTarget)
        }
    }

    pub const fn uncategorized() -> Self {
        Self::Uncategorized
    }

    pub fn space(&self) -> Option<&str> {
        match self {
            Self::Registered { space } => Some(space),
            Self::Uncategorized => None,
        }
    }
}

impl<'de> Deserialize<'de> for RepairScope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RepairScopeWire::deserialize(deserializer)? {
            RepairScopeWire::Registered { space } => Self::registered(space),
            RepairScopeWire::Uncategorized => Ok(Self::uncategorized()),
        }
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairTarget {
    Memory {
        source_id: String,
        scope: RepairScope,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RepairTargetWire {
    Memory {
        source_id: String,
        scope: RepairScope,
    },
}

impl RepairTarget {
    pub fn memory(source_id: String, scope: RepairScope) -> Result<Self, RepairContractError> {
        if valid_nonempty(&source_id) {
            Ok(Self::Memory { source_id, scope })
        } else {
            Err(RepairContractError::InvalidTarget)
        }
    }

    pub fn memory_source_id(&self) -> &str {
        match self {
            Self::Memory { source_id, .. } => source_id,
        }
    }

    pub fn scope(&self) -> &RepairScope {
        match self {
            Self::Memory { scope, .. } => scope,
        }
    }
}

impl<'de> Deserialize<'de> for RepairTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RepairTargetWire::deserialize(deserializer)? {
            RepairTargetWire::Memory { source_id, scope } => Self::memory(source_id, scope),
        }
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairSource {
    report_schema_version: u16,
    check_catalog_version: u16,
    lint_scope: RepairLintScope,
    report_scope: LintScope,
    check_id: String,
    finding: LintSemanticFinding,
    general_snapshots: LintSnapshotReceipts,
    deep_snapshots: LintSnapshotReceipts,
    general_producer_receipt: LintProducerReceipt,
    deep_producer_receipt: LintProducerReceipt,
    agent_work_digest: LintDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairSourceWire {
    report_schema_version: u16,
    check_catalog_version: u16,
    lint_scope: RepairLintScope,
    report_scope: LintScope,
    check_id: String,
    finding: LintSemanticFinding,
    general_snapshots: LintSnapshotReceipts,
    deep_snapshots: LintSnapshotReceipts,
    general_producer_receipt: LintProducerReceipt,
    deep_producer_receipt: LintProducerReceipt,
    agent_work_digest: LintDigest,
}

impl RepairSource {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        lint_scope: RepairLintScope,
        report_scope: LintScope,
        finding: LintSemanticFinding,
        general_snapshots: LintSnapshotReceipts,
        deep_snapshots: LintSnapshotReceipts,
        general_producer_receipt: LintProducerReceipt,
        deep_producer_receipt: LintProducerReceipt,
        agent_work_digest: LintDigest,
    ) -> Result<Self, RepairContractError> {
        if !lint_scope.matches_report_scope_kind(&report_scope)
            || finding.proposed_action() != LintSemanticAction::ReclassifyMemory
            || finding.unresolved_disagreement()
            || finding.evidence_ids().is_empty()
        {
            return Err(RepairContractError::InvalidSource);
        }
        Ok(Self {
            report_schema_version: LINT_REPORT_SCHEMA_VERSION,
            check_catalog_version: LINT_CHECK_CATALOG_VERSION,
            lint_scope,
            report_scope,
            check_id: REPAIR_CLASSIFICATION_CHECK_ID.to_string(),
            finding,
            general_snapshots,
            deep_snapshots,
            general_producer_receipt,
            deep_producer_receipt,
            agent_work_digest,
        })
    }

    pub fn lint_scope(&self) -> &RepairLintScope {
        &self.lint_scope
    }

    pub fn report_scope(&self) -> &LintScope {
        &self.report_scope
    }

    pub fn finding(&self) -> &LintSemanticFinding {
        &self.finding
    }

    pub fn check_id(&self) -> &str {
        &self.check_id
    }

    pub fn general_snapshots(&self) -> &LintSnapshotReceipts {
        &self.general_snapshots
    }

    pub fn deep_snapshots(&self) -> &LintSnapshotReceipts {
        &self.deep_snapshots
    }

    pub fn agent_work_digest(&self) -> &LintDigest {
        &self.agent_work_digest
    }
}

impl<'de> Deserialize<'de> for RepairSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairSourceWire::deserialize(deserializer)?;
        if wire.report_schema_version != LINT_REPORT_SCHEMA_VERSION
            || wire.check_catalog_version != LINT_CHECK_CATALOG_VERSION
            || wire.check_id != REPAIR_CLASSIFICATION_CHECK_ID
        {
            return Err(D::Error::custom(RepairContractError::InvalidSource));
        }
        Self::try_new(
            wire.lint_scope,
            wire.report_scope,
            wire.finding,
            wire.general_snapshots,
            wire.deep_snapshots,
            wire.general_producer_receipt,
            wire.deep_producer_receipt,
            wire.agent_work_digest,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairExpectedState {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<i64>,
    canonical_receipt: RepairDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairExpectedStateWire {
    version: Option<i64>,
    canonical_receipt: RepairDigest,
}

impl RepairExpectedState {
    pub fn try_new(
        version: Option<i64>,
        canonical_receipt: RepairDigest,
    ) -> Result<Self, RepairContractError> {
        if version.is_some_and(|version| version < 0) {
            return Err(RepairContractError::InvalidExpectedState);
        }
        Ok(Self {
            version,
            canonical_receipt,
        })
    }

    pub const fn version(&self) -> Option<i64> {
        self.version
    }

    pub fn canonical_receipt(&self) -> &RepairDigest {
        &self.canonical_receipt
    }
}

impl<'de> Deserialize<'de> for RepairExpectedState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairExpectedStateWire::deserialize(deserializer)?;
        Self::try_new(wire.version, wire.canonical_receipt).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairWriter {
    ReclassifyMemory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairMutation {
    ReclassifyMemory {
        before_memory_type: Option<MemoryType>,
        after_memory_type: MemoryType,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RepairMutationWire {
    ReclassifyMemory {
        before_memory_type: Option<MemoryType>,
        after_memory_type: MemoryType,
    },
}

impl RepairMutation {
    pub fn try_reclassify(
        before_memory_type: Option<&str>,
        after_memory_type: &str,
    ) -> Result<Self, RepairContractError> {
        let before_memory_type = before_memory_type.map(parse_memory_type).transpose()?;
        let after_memory_type = parse_memory_type(after_memory_type)?;
        Self::from_memory_types(before_memory_type, after_memory_type)
    }

    pub fn from_memory_types(
        before_memory_type: Option<MemoryType>,
        after_memory_type: MemoryType,
    ) -> Result<Self, RepairContractError> {
        if before_memory_type.as_ref() == Some(&after_memory_type) {
            return Err(RepairContractError::InvalidMutation);
        }
        Ok(Self::ReclassifyMemory {
            before_memory_type,
            after_memory_type,
        })
    }

    pub fn before_memory_type(&self) -> Option<&str> {
        match self {
            Self::ReclassifyMemory {
                before_memory_type, ..
            } => before_memory_type.as_ref().map(|value| match value {
                MemoryType::Identity => "identity",
                MemoryType::Preference => "preference",
                MemoryType::Decision => "decision",
                MemoryType::Lesson => "lesson",
                MemoryType::Gotcha => "gotcha",
                MemoryType::Fact => "fact",
            }),
        }
    }

    pub fn after_memory_type(&self) -> &str {
        match self {
            Self::ReclassifyMemory {
                after_memory_type, ..
            } => match after_memory_type {
                MemoryType::Identity => "identity",
                MemoryType::Preference => "preference",
                MemoryType::Decision => "decision",
                MemoryType::Lesson => "lesson",
                MemoryType::Gotcha => "gotcha",
                MemoryType::Fact => "fact",
            },
        }
    }
}

impl<'de> Deserialize<'de> for RepairMutation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RepairMutationWire::deserialize(deserializer)? {
            RepairMutationWire::ReclassifyMemory {
                before_memory_type,
                after_memory_type,
            } => Self::from_memory_types(before_memory_type, after_memory_type),
        }
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairMemoryField {
    MemoryType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairAllowedEffects {
    owner: RepairTarget,
    fields: Vec<RepairMemoryField>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairAllowedEffectsWire {
    owner: RepairTarget,
    fields: Vec<RepairMemoryField>,
}

impl RepairAllowedEffects {
    pub fn memory_type(owner: RepairTarget) -> Self {
        Self {
            owner,
            fields: vec![RepairMemoryField::MemoryType],
        }
    }

    pub fn owner(&self) -> &RepairTarget {
        &self.owner
    }

    pub fn fields(&self) -> &[RepairMemoryField] {
        &self.fields
    }
}

impl<'de> Deserialize<'de> for RepairAllowedEffects {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairAllowedEffectsWire::deserialize(deserializer)?;
        if wire.fields != [RepairMemoryField::MemoryType] {
            return Err(D::Error::custom(RepairContractError::InvalidAllowedEffects));
        }
        Ok(Self {
            owner: wire.owner,
            fields: wire.fields,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairRollbackArtifact {
    format_version: u16,
    relative_path: String,
    digest: RepairDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairRollbackArtifactWire {
    format_version: u16,
    relative_path: String,
    digest: RepairDigest,
}

impl RepairRollbackArtifact {
    pub fn try_new(
        relative_path: String,
        digest: RepairDigest,
    ) -> Result<Self, RepairContractError> {
        let path = Path::new(&relative_path);
        if !valid_nonempty(&relative_path)
            || path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(RepairContractError::InvalidRollbackArtifact);
        }
        Ok(Self {
            format_version: REPAIR_ROLLBACK_FORMAT_VERSION,
            relative_path,
            digest,
        })
    }

    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    pub fn digest(&self) -> &RepairDigest {
        &self.digest
    }
}

impl<'de> Deserialize<'de> for RepairRollbackArtifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairRollbackArtifactWire::deserialize(deserializer)?;
        if wire.format_version != REPAIR_ROLLBACK_FORMAT_VERSION {
            return Err(D::Error::custom(
                RepairContractError::InvalidRollbackArtifact,
            ));
        }
        Self::try_new(wire.relative_path, wire.digest).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairVerificationPolicy {
    LegacyWholeReports,
    ApplicableChecks {
        required_deep_check_ids: Vec<String>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RepairVerificationPolicyWire {
    LegacyWholeReports,
    ApplicableChecks {
        required_deep_check_ids: Vec<String>,
    },
}

impl RepairVerificationPolicy {
    fn applicable_classification() -> Self {
        Self::ApplicableChecks {
            required_deep_check_ids: vec![REPAIR_CLASSIFICATION_CHECK_ID.to_string()],
        }
    }

    pub fn required_deep_check_ids(&self) -> Option<&[String]> {
        match self {
            Self::LegacyWholeReports => None,
            Self::ApplicableChecks {
                required_deep_check_ids,
            } => Some(required_deep_check_ids),
        }
    }

    pub const fn requires_whole_reports(&self) -> bool {
        matches!(self, Self::LegacyWholeReports)
    }
}

impl<'de> Deserialize<'de> for RepairVerificationPolicy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairVerificationPolicyWire::deserialize(deserializer)?;
        match wire {
            RepairVerificationPolicyWire::LegacyWholeReports => Ok(Self::LegacyWholeReports),
            RepairVerificationPolicyWire::ApplicableChecks {
                required_deep_check_ids,
            } if required_deep_check_ids == [REPAIR_CLASSIFICATION_CHECK_ID] => {
                Ok(Self::applicable_classification())
            }
            RepairVerificationPolicyWire::ApplicableChecks { .. } => {
                Err(D::Error::custom(RepairContractError::InvalidPostAssertions))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairPostAssertions {
    target_check_id: String,
    target_evidence_id: LintDigest,
    general_baseline: Vec<RepairCheckBaseline>,
    deep_baseline: Vec<RepairCheckBaseline>,
    verification_policy: RepairVerificationPolicy,
    require_complete_general: bool,
    reject_new_actionable: bool,
    reject_new_incomplete: bool,
    allowed_non_target_check_deltas: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairPostAssertionsWire {
    target_check_id: String,
    target_evidence_id: LintDigest,
    general_baseline: Vec<RepairCheckBaseline>,
    deep_baseline: Vec<RepairCheckBaseline>,
    verification_policy: RepairVerificationPolicy,
    require_complete_general: bool,
    reject_new_actionable: bool,
    reject_new_incomplete: bool,
    allowed_non_target_check_deltas: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairCheckBaseline {
    check_id: String,
    outcome: crate::lint::LintOutcome,
    gate_effect: crate::lint::LintGateEffect,
    evidence: Vec<crate::lint::LintEvidenceRef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairCheckBaselineWire {
    check_id: String,
    outcome: crate::lint::LintOutcome,
    gate_effect: crate::lint::LintGateEffect,
    evidence: Vec<crate::lint::LintEvidenceRef>,
}

impl RepairCheckBaseline {
    pub fn try_new(
        check_id: String,
        outcome: crate::lint::LintOutcome,
        gate_effect: crate::lint::LintGateEffect,
        evidence: Vec<crate::lint::LintEvidenceRef>,
    ) -> Result<Self, RepairContractError> {
        if !valid_nonempty(&check_id) {
            return Err(RepairContractError::InvalidPostAssertions);
        }
        Ok(Self {
            check_id,
            outcome,
            gate_effect,
            evidence,
        })
    }

    pub fn check_id(&self) -> &str {
        &self.check_id
    }

    pub const fn outcome(&self) -> crate::lint::LintOutcome {
        self.outcome
    }

    pub const fn gate_effect(&self) -> crate::lint::LintGateEffect {
        self.gate_effect
    }

    pub fn evidence(&self) -> &[crate::lint::LintEvidenceRef] {
        &self.evidence
    }
}

impl<'de> Deserialize<'de> for RepairCheckBaseline {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairCheckBaselineWire::deserialize(deserializer)?;
        Self::try_new(wire.check_id, wire.outcome, wire.gate_effect, wire.evidence)
            .map_err(D::Error::custom)
    }
}

fn valid_check_baseline(values: &[RepairCheckBaseline]) -> bool {
    !values.is_empty()
        && values
            .windows(2)
            .all(|pair| pair[0].check_id() < pair[1].check_id())
}

impl RepairPostAssertions {
    pub fn try_new(
        target_evidence_id: LintDigest,
        general_baseline: Vec<RepairCheckBaseline>,
        deep_baseline: Vec<RepairCheckBaseline>,
        allowed_non_target_check_deltas: Vec<String>,
    ) -> Result<Self, RepairContractError> {
        if !valid_check_baseline(&general_baseline)
            || !valid_check_baseline(&deep_baseline)
            || allowed_non_target_check_deltas
                .iter()
                .any(|value| !valid_nonempty(value))
            || !allowed_non_target_check_deltas
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        {
            return Err(RepairContractError::InvalidPostAssertions);
        }
        Ok(Self {
            target_check_id: REPAIR_CLASSIFICATION_CHECK_ID.to_string(),
            target_evidence_id,
            general_baseline,
            deep_baseline,
            verification_policy: RepairVerificationPolicy::applicable_classification(),
            require_complete_general: true,
            reject_new_actionable: true,
            reject_new_incomplete: true,
            allowed_non_target_check_deltas,
        })
    }

    fn try_new_legacy_v1(
        target_evidence_id: LintDigest,
        general_baseline: Vec<RepairCheckBaseline>,
        deep_baseline: Vec<RepairCheckBaseline>,
        allowed_non_target_check_deltas: Vec<String>,
    ) -> Result<Self, RepairContractError> {
        let mut assertions = Self::try_new(
            target_evidence_id,
            general_baseline,
            deep_baseline,
            allowed_non_target_check_deltas,
        )?;
        assertions.verification_policy = RepairVerificationPolicy::LegacyWholeReports;
        Ok(assertions)
    }

    pub fn target_evidence_id(&self) -> &LintDigest {
        &self.target_evidence_id
    }

    pub fn general_baseline(&self) -> &[RepairCheckBaseline] {
        &self.general_baseline
    }

    pub fn deep_baseline(&self) -> &[RepairCheckBaseline] {
        &self.deep_baseline
    }

    pub const fn verification_policy(&self) -> &RepairVerificationPolicy {
        &self.verification_policy
    }

    pub fn allowed_non_target_check_deltas(&self) -> &[String] {
        &self.allowed_non_target_check_deltas
    }
}

impl<'de> Deserialize<'de> for RepairPostAssertions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairPostAssertionsWire::deserialize(deserializer)?;
        if wire.target_check_id != REPAIR_CLASSIFICATION_CHECK_ID
            || !wire.require_complete_general
            || !wire.reject_new_actionable
            || !wire.reject_new_incomplete
            || wire.verification_policy.requires_whole_reports()
        {
            return Err(D::Error::custom(RepairContractError::InvalidPostAssertions));
        }
        Self::try_new(
            wire.target_evidence_id,
            wire.general_baseline,
            wire.deep_baseline,
            wire.allowed_non_target_check_deltas,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairManifestDraft {
    manifest_schema_version: u16,
    manifest_id: String,
    prepared_at: i64,
    source: RepairSource,
    target: RepairTarget,
    expected_state: RepairExpectedState,
    writer: RepairWriter,
    mutation: RepairMutation,
    allowed_effects: RepairAllowedEffects,
    rollback: RepairRollbackArtifact,
    post_assertions: RepairPostAssertions,
}

impl RepairManifestDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        manifest_id: String,
        prepared_at: i64,
        source: RepairSource,
        target: RepairTarget,
        expected_state: RepairExpectedState,
        writer: RepairWriter,
        mutation: RepairMutation,
        allowed_effects: RepairAllowedEffects,
        rollback: RepairRollbackArtifact,
        post_assertions: RepairPostAssertions,
    ) -> Result<Self, RepairContractError> {
        if !valid_manifest_id(&manifest_id) || prepared_at <= 0 {
            return Err(RepairContractError::InvalidManifest);
        }
        if writer != RepairWriter::ReclassifyMemory
            || !matches!(mutation, RepairMutation::ReclassifyMemory { .. })
        {
            return Err(RepairContractError::UnsupportedWriter);
        }
        if allowed_effects.owner() != &target
            || !source
                .finding()
                .evidence_ids()
                .contains(post_assertions.target_evidence_id())
        {
            return Err(RepairContractError::InvalidManifest);
        }
        Ok(Self {
            manifest_schema_version: REPAIR_MANIFEST_SCHEMA_VERSION,
            manifest_id,
            prepared_at,
            source,
            target,
            expected_state,
            writer,
            mutation,
            allowed_effects,
            rollback,
            post_assertions,
        })
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairManifest {
    #[serde(flatten)]
    draft: RepairManifestDraft,
    manifest_digest: RepairDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairManifestWire {
    manifest_schema_version: u16,
    manifest_id: String,
    prepared_at: i64,
    source: RepairSource,
    target: RepairTarget,
    expected_state: RepairExpectedState,
    writer: RepairWriter,
    mutation: RepairMutation,
    allowed_effects: RepairAllowedEffects,
    rollback: RepairRollbackArtifact,
    post_assertions: RepairPostAssertions,
    manifest_digest: RepairDigest,
}

impl RepairManifest {
    pub fn try_new(
        draft: RepairManifestDraft,
        manifest_digest: RepairDigest,
    ) -> Result<Self, RepairContractError> {
        Ok(Self {
            draft,
            manifest_digest,
        })
    }

    pub fn manifest_id(&self) -> &str {
        &self.draft.manifest_id
    }

    pub const fn prepared_at(&self) -> i64 {
        self.draft.prepared_at
    }

    pub fn source(&self) -> &RepairSource {
        &self.draft.source
    }

    pub fn target(&self) -> &RepairTarget {
        &self.draft.target
    }

    pub fn expected_state(&self) -> &RepairExpectedState {
        &self.draft.expected_state
    }

    pub const fn writer(&self) -> RepairWriter {
        self.draft.writer
    }

    pub fn mutation(&self) -> &RepairMutation {
        &self.draft.mutation
    }

    pub fn allowed_effects(&self) -> &RepairAllowedEffects {
        &self.draft.allowed_effects
    }

    pub fn rollback(&self) -> &RepairRollbackArtifact {
        &self.draft.rollback
    }

    pub fn post_assertions(&self) -> &RepairPostAssertions {
        &self.draft.post_assertions
    }

    pub fn manifest_digest(&self) -> &RepairDigest {
        &self.manifest_digest
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.draft.canonical_bytes()
    }
}

impl<'de> Deserialize<'de> for RepairManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairManifestWire::deserialize(deserializer)?;
        if wire.manifest_schema_version != REPAIR_MANIFEST_SCHEMA_VERSION {
            return Err(D::Error::custom(
                RepairContractError::UnsupportedManifestSchema,
            ));
        }
        let draft = RepairManifestDraft::try_new(
            wire.manifest_id,
            wire.prepared_at,
            wire.source,
            wire.target,
            wire.expected_state,
            wire.writer,
            wire.mutation,
            wire.allowed_effects,
            wire.rollback,
            wire.post_assertions,
        )
        .map_err(D::Error::custom)?;
        Self::try_new(draft, wire.manifest_digest).map_err(D::Error::custom)
    }
}

fn applicable_deep_complete(report: &LintReport) -> bool {
    report
        .checks()
        .iter()
        .find(|check| check.check_id() == REPAIR_CLASSIFICATION_CHECK_ID)
        .is_some_and(|check| matches!(check.outcome(), LintOutcome::Pass | LintOutcome::Finding))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrepareRepairRequest {
    lint_scope: RepairLintScope,
    general_report: LintReport,
    deep_report: LintReport,
    selected_finding: LintSemanticFinding,
    after_memory_type: MemoryType,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareRepairRequestWire {
    lint_scope: RepairLintScope,
    general_report: LintReport,
    deep_report: LintReport,
    selected_finding: LintSemanticFinding,
    after_memory_type: MemoryType,
}

impl PrepareRepairRequest {
    pub fn try_new(
        lint_scope: RepairLintScope,
        general_report: LintReport,
        deep_report: LintReport,
        selected_finding: LintSemanticFinding,
        after_memory_type: MemoryType,
    ) -> Result<Self, RepairContractError> {
        if !lint_scope.matches_report_scope_kind(general_report.scope())
            || !lint_scope.matches_report_scope_kind(deep_report.scope())
            || general_report.profile() != LintProfile::General
            || deep_report.profile() != LintProfile::Deep
            || !general_report.complete()
            || !applicable_deep_complete(&deep_report)
            || general_report.scope() != deep_report.scope()
            || selected_finding.proposed_action() != LintSemanticAction::ReclassifyMemory
            || selected_finding.unresolved_disagreement()
            || deep_report.agent_work().is_none()
        {
            return Err(RepairContractError::InvalidPrepareRequest);
        }
        Ok(Self {
            lint_scope,
            general_report,
            deep_report,
            selected_finding,
            after_memory_type,
        })
    }

    pub fn lint_scope(&self) -> &RepairLintScope {
        &self.lint_scope
    }

    pub fn general_report(&self) -> &LintReport {
        &self.general_report
    }

    pub fn deep_report(&self) -> &LintReport {
        &self.deep_report
    }

    pub fn selected_finding(&self) -> &LintSemanticFinding {
        &self.selected_finding
    }

    pub fn after_memory_type(&self) -> &MemoryType {
        &self.after_memory_type
    }
}

impl<'de> Deserialize<'de> for PrepareRepairRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PrepareRepairRequestWire::deserialize(deserializer)?;
        Self::try_new(
            wire.lint_scope,
            wire.general_report,
            wire.deep_report,
            wire.selected_finding,
            wire.after_memory_type,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Intent-binding request for cooperating agent workflows. The exact phrase is
/// deliberately not a local authentication or malicious-process boundary.
pub struct ApplyRepairRequest {
    manifest_id: String,
    approved_manifest_digest: RepairDigest,
    approval: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyRepairRequestWire {
    manifest_id: String,
    approved_manifest_digest: RepairDigest,
    approval: String,
}

impl ApplyRepairRequest {
    pub fn try_new(
        manifest_id: String,
        approved_manifest_digest: RepairDigest,
        approval: String,
    ) -> Result<Self, RepairContractError> {
        let expected = format!(
            "apply repair {} {}",
            manifest_id,
            approved_manifest_digest.as_str()
        );
        if !valid_manifest_id(&manifest_id) || approval != expected {
            return Err(RepairContractError::InvalidApplyRequest);
        }
        Ok(Self {
            manifest_id,
            approved_manifest_digest,
            approval,
        })
    }

    pub fn manifest_id(&self) -> &str {
        &self.manifest_id
    }

    pub fn approved_manifest_digest(&self) -> &RepairDigest {
        &self.approved_manifest_digest
    }

    pub fn approval(&self) -> &str {
        &self.approval
    }
}

impl<'de> Deserialize<'de> for ApplyRepairRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ApplyRepairRequestWire::deserialize(deserializer)?;
        Self::try_new(
            wire.manifest_id,
            wire.approved_manifest_digest,
            wire.approval,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairApplyReceiptDraft {
    receipt_schema_version: u16,
    manifest_id: String,
    manifest_digest: RepairDigest,
    applied_at: i64,
    before_target_receipt: RepairDigest,
    after_target_receipt: RepairDigest,
    non_target_before: RepairDigest,
    non_target_after: RepairDigest,
    #[serde(skip_serializing_if = "Option::is_none")]
    post_apply_db_digest: Option<RepairDigest>,
    actual_effects: RepairAllowedEffects,
    writer: RepairWriter,
}

impl RepairApplyReceiptDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        manifest_id: String,
        manifest_digest: RepairDigest,
        applied_at: i64,
        before_target_receipt: RepairDigest,
        after_target_receipt: RepairDigest,
        non_target_before: RepairDigest,
        non_target_after: RepairDigest,
        post_apply_db_digest: RepairDigest,
        actual_effects: RepairAllowedEffects,
        writer: RepairWriter,
    ) -> Result<Self, RepairContractError> {
        if !valid_manifest_id(&manifest_id)
            || applied_at <= 0
            || before_target_receipt == after_target_receipt
            || non_target_before != non_target_after
            || writer != RepairWriter::ReclassifyMemory
        {
            return Err(RepairContractError::InvalidReceipt);
        }
        Ok(Self {
            receipt_schema_version: REPAIR_RECEIPT_SCHEMA_VERSION,
            manifest_id,
            manifest_digest,
            applied_at,
            before_target_receipt,
            after_target_receipt,
            non_target_before,
            non_target_after,
            post_apply_db_digest: Some(post_apply_db_digest),
            actual_effects,
            writer,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn try_new_legacy_v1(
        manifest_id: String,
        manifest_digest: RepairDigest,
        applied_at: i64,
        before_target_receipt: RepairDigest,
        after_target_receipt: RepairDigest,
        non_target_before: RepairDigest,
        non_target_after: RepairDigest,
        actual_effects: RepairAllowedEffects,
        writer: RepairWriter,
    ) -> Result<Self, RepairContractError> {
        if !valid_manifest_id(&manifest_id)
            || applied_at <= 0
            || before_target_receipt == after_target_receipt
            || non_target_before != non_target_after
            || writer != RepairWriter::ReclassifyMemory
        {
            return Err(RepairContractError::InvalidReceipt);
        }
        Ok(Self {
            receipt_schema_version: 1,
            manifest_id,
            manifest_digest,
            applied_at,
            before_target_receipt,
            after_target_receipt,
            non_target_before,
            non_target_after,
            post_apply_db_digest: None,
            actual_effects,
            writer,
        })
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairApplyReceipt {
    #[serde(flatten)]
    draft: RepairApplyReceiptDraft,
    receipt_digest: RepairDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairApplyReceiptWire {
    receipt_schema_version: u16,
    manifest_id: String,
    manifest_digest: RepairDigest,
    applied_at: i64,
    before_target_receipt: RepairDigest,
    after_target_receipt: RepairDigest,
    non_target_before: RepairDigest,
    non_target_after: RepairDigest,
    #[serde(default)]
    post_apply_db_digest: Option<RepairDigest>,
    actual_effects: RepairAllowedEffects,
    writer: RepairWriter,
    receipt_digest: RepairDigest,
}

impl RepairApplyReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        manifest_id: String,
        manifest_digest: RepairDigest,
        applied_at: i64,
        before_target_receipt: RepairDigest,
        after_target_receipt: RepairDigest,
        non_target_before: RepairDigest,
        non_target_after: RepairDigest,
        post_apply_db_digest: RepairDigest,
        actual_effects: RepairAllowedEffects,
        writer: RepairWriter,
        receipt_digest: RepairDigest,
    ) -> Result<Self, RepairContractError> {
        let draft = RepairApplyReceiptDraft::try_new(
            manifest_id,
            manifest_digest,
            applied_at,
            before_target_receipt,
            after_target_receipt,
            non_target_before,
            non_target_after,
            post_apply_db_digest,
            actual_effects,
            writer,
        )?;
        Ok(Self::from_draft(draft, receipt_digest))
    }

    pub fn from_draft(draft: RepairApplyReceiptDraft, receipt_digest: RepairDigest) -> Self {
        Self {
            draft,
            receipt_digest,
        }
    }

    pub fn manifest_id(&self) -> &str {
        &self.draft.manifest_id
    }

    pub fn manifest_digest(&self) -> &RepairDigest {
        &self.draft.manifest_digest
    }

    pub fn receipt_digest(&self) -> &RepairDigest {
        &self.receipt_digest
    }

    pub fn actual_effects(&self) -> &RepairAllowedEffects {
        &self.draft.actual_effects
    }

    pub fn before_target_receipt(&self) -> &RepairDigest {
        &self.draft.before_target_receipt
    }

    pub fn after_target_receipt(&self) -> &RepairDigest {
        &self.draft.after_target_receipt
    }

    pub fn non_target_before(&self) -> &RepairDigest {
        &self.draft.non_target_before
    }

    pub fn non_target_after(&self) -> &RepairDigest {
        &self.draft.non_target_after
    }

    pub fn post_apply_db_digest(&self) -> Option<&RepairDigest> {
        self.draft.post_apply_db_digest.as_ref()
    }

    pub const fn writer(&self) -> RepairWriter {
        self.draft.writer
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.draft.canonical_bytes()
    }
}

impl<'de> Deserialize<'de> for RepairApplyReceipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairApplyReceiptWire::deserialize(deserializer)?;
        let draft = match wire.receipt_schema_version {
            1 if wire.post_apply_db_digest.is_none() => RepairApplyReceiptDraft::try_new_legacy_v1(
                wire.manifest_id,
                wire.manifest_digest,
                wire.applied_at,
                wire.before_target_receipt,
                wire.after_target_receipt,
                wire.non_target_before,
                wire.non_target_after,
                wire.actual_effects,
                wire.writer,
            ),
            REPAIR_RECEIPT_SCHEMA_VERSION => RepairApplyReceiptDraft::try_new(
                wire.manifest_id,
                wire.manifest_digest,
                wire.applied_at,
                wire.before_target_receipt,
                wire.after_target_receipt,
                wire.non_target_before,
                wire.non_target_after,
                wire.post_apply_db_digest
                    .ok_or(RepairContractError::InvalidReceipt)
                    .map_err(D::Error::custom)?,
                wire.actual_effects,
                wire.writer,
            ),
            _ => Err(RepairContractError::InvalidReceipt),
        }
        .map_err(D::Error::custom)?;
        Ok(Self::from_draft(draft, wire.receipt_digest))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifyRepairRequest {
    manifest_id: String,
    manifest_digest: RepairDigest,
    apply_receipt_digest: RepairDigest,
    general_report: LintReport,
    deep_report: LintReport,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifyRepairRequestWire {
    manifest_id: String,
    manifest_digest: RepairDigest,
    apply_receipt_digest: RepairDigest,
    general_report: LintReport,
    deep_report: LintReport,
}

impl VerifyRepairRequest {
    pub fn try_new(
        manifest_id: String,
        manifest_digest: RepairDigest,
        apply_receipt_digest: RepairDigest,
        general_report: LintReport,
        deep_report: LintReport,
    ) -> Result<Self, RepairContractError> {
        if !valid_manifest_id(&manifest_id)
            || general_report.profile() != LintProfile::General
            || deep_report.profile() != LintProfile::Deep
            || !general_report.complete()
            || !applicable_deep_complete(&deep_report)
            || general_report.scope() != deep_report.scope()
            || general_report.producer_receipt() != deep_report.producer_receipt()
        {
            return Err(RepairContractError::InvalidVerifyRequest);
        }
        Ok(Self {
            manifest_id,
            manifest_digest,
            apply_receipt_digest,
            general_report,
            deep_report,
        })
    }

    pub fn manifest_id(&self) -> &str {
        &self.manifest_id
    }

    pub fn manifest_digest(&self) -> &RepairDigest {
        &self.manifest_digest
    }

    pub fn apply_receipt_digest(&self) -> &RepairDigest {
        &self.apply_receipt_digest
    }

    pub fn general_report(&self) -> &LintReport {
        &self.general_report
    }

    pub fn deep_report(&self) -> &LintReport {
        &self.deep_report
    }
}

impl<'de> Deserialize<'de> for VerifyRepairRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = VerifyRepairRequestWire::deserialize(deserializer)?;
        Self::try_new(
            wire.manifest_id,
            wire.manifest_digest,
            wire.apply_receipt_digest,
            wire.general_report,
            wire.deep_report,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairVerificationReceiptDraft {
    receipt_schema_version: u16,
    manifest_id: String,
    manifest_digest: RepairDigest,
    apply_receipt_digest: RepairDigest,
    verified_at: i64,
    general_snapshots: LintSnapshotReceipts,
    deep_snapshots: LintSnapshotReceipts,
}

impl RepairVerificationReceiptDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        manifest_id: String,
        manifest_digest: RepairDigest,
        apply_receipt_digest: RepairDigest,
        verified_at: i64,
        general_snapshots: LintSnapshotReceipts,
        deep_snapshots: LintSnapshotReceipts,
    ) -> Result<Self, RepairContractError> {
        if !valid_manifest_id(&manifest_id) || verified_at <= 0 {
            return Err(RepairContractError::InvalidReceipt);
        }
        Ok(Self {
            receipt_schema_version: REPAIR_RECEIPT_SCHEMA_VERSION,
            manifest_id,
            manifest_digest,
            apply_receipt_digest,
            verified_at,
            general_snapshots,
            deep_snapshots,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn try_new_legacy_v1(
        manifest_id: String,
        manifest_digest: RepairDigest,
        apply_receipt_digest: RepairDigest,
        verified_at: i64,
        general_snapshots: LintSnapshotReceipts,
        deep_snapshots: LintSnapshotReceipts,
    ) -> Result<Self, RepairContractError> {
        if !valid_manifest_id(&manifest_id) || verified_at <= 0 {
            return Err(RepairContractError::InvalidReceipt);
        }
        Ok(Self {
            receipt_schema_version: 1,
            manifest_id,
            manifest_digest,
            apply_receipt_digest,
            verified_at,
            general_snapshots,
            deep_snapshots,
        })
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairVerificationReceipt {
    #[serde(flatten)]
    draft: RepairVerificationReceiptDraft,
    receipt_digest: RepairDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairVerificationReceiptWire {
    receipt_schema_version: u16,
    manifest_id: String,
    manifest_digest: RepairDigest,
    apply_receipt_digest: RepairDigest,
    verified_at: i64,
    general_snapshots: LintSnapshotReceipts,
    deep_snapshots: LintSnapshotReceipts,
    receipt_digest: RepairDigest,
}

impl RepairVerificationReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        manifest_id: String,
        manifest_digest: RepairDigest,
        apply_receipt_digest: RepairDigest,
        verified_at: i64,
        general_snapshots: LintSnapshotReceipts,
        deep_snapshots: LintSnapshotReceipts,
        receipt_digest: RepairDigest,
    ) -> Result<Self, RepairContractError> {
        let draft = RepairVerificationReceiptDraft::try_new(
            manifest_id,
            manifest_digest,
            apply_receipt_digest,
            verified_at,
            general_snapshots,
            deep_snapshots,
        )?;
        Ok(Self::from_draft(draft, receipt_digest))
    }

    pub fn from_draft(draft: RepairVerificationReceiptDraft, receipt_digest: RepairDigest) -> Self {
        Self {
            draft,
            receipt_digest,
        }
    }

    pub fn receipt_digest(&self) -> &RepairDigest {
        &self.receipt_digest
    }

    pub fn manifest_id(&self) -> &str {
        &self.draft.manifest_id
    }

    pub fn manifest_digest(&self) -> &RepairDigest {
        &self.draft.manifest_digest
    }

    pub fn apply_receipt_digest(&self) -> &RepairDigest {
        &self.draft.apply_receipt_digest
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.draft.canonical_bytes()
    }
}

impl<'de> Deserialize<'de> for RepairVerificationReceipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairVerificationReceiptWire::deserialize(deserializer)?;
        let draft = match wire.receipt_schema_version {
            1 => RepairVerificationReceiptDraft::try_new_legacy_v1(
                wire.manifest_id,
                wire.manifest_digest,
                wire.apply_receipt_digest,
                wire.verified_at,
                wire.general_snapshots,
                wire.deep_snapshots,
            ),
            REPAIR_RECEIPT_SCHEMA_VERSION => RepairVerificationReceiptDraft::try_new(
                wire.manifest_id,
                wire.manifest_digest,
                wire.apply_receipt_digest,
                wire.verified_at,
                wire.general_snapshots,
                wire.deep_snapshots,
            ),
            _ => Err(RepairContractError::InvalidReceipt),
        }
        .map_err(D::Error::custom)?;
        Ok(Self::from_draft(draft, wire.receipt_digest))
    }
}
