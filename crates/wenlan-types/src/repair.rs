// SPDX-License-Identifier: Apache-2.0
//! Approval-gated repair contracts shared by the daemon and local clients.

use crate::{
    lint::{
        LintDigest, LintProducerReceipt, LintProfile, LintScope, LintScopeKind, LintSemanticAction,
        LintSemanticFinding, LintSnapshotReceipts, LINT_CHECK_CATALOG_VERSION,
        LINT_REPORT_SCHEMA_VERSION,
    },
    LintReport, MemoryType,
};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::{fmt, path::Component, path::Path};

pub const REPAIR_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const REPAIR_ROLLBACK_FORMAT_VERSION: u16 = 1;
pub const REPAIR_RECEIPT_SCHEMA_VERSION: u16 = 1;
pub const REPAIR_CLASSIFICATION_CHECK_ID: &str = "memories.semantic.classification";

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
pub struct RepairPostAssertions {
    target_check_id: String,
    target_evidence_id: LintDigest,
    general_baseline: Vec<RepairCheckBaseline>,
    deep_baseline: Vec<RepairCheckBaseline>,
    require_complete_general: bool,
    require_complete_deep: bool,
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
    require_complete_general: bool,
    require_complete_deep: bool,
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
            require_complete_general: true,
            require_complete_deep: true,
            reject_new_actionable: true,
            reject_new_incomplete: true,
            allowed_non_target_check_deltas,
        })
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
            || !wire.require_complete_deep
            || !wire.reject_new_actionable
            || !wire.reject_new_incomplete
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
            || !deep_report.complete()
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
        if wire.receipt_schema_version != REPAIR_RECEIPT_SCHEMA_VERSION {
            return Err(D::Error::custom(RepairContractError::InvalidReceipt));
        }
        let draft = RepairApplyReceiptDraft::try_new(
            wire.manifest_id,
            wire.manifest_digest,
            wire.applied_at,
            wire.before_target_receipt,
            wire.after_target_receipt,
            wire.non_target_before,
            wire.non_target_after,
            wire.actual_effects,
            wire.writer,
        )
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
            || !deep_report.complete()
            || general_report.scope() != deep_report.scope()
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
        if wire.receipt_schema_version != REPAIR_RECEIPT_SCHEMA_VERSION {
            return Err(D::Error::custom(RepairContractError::InvalidReceipt));
        }
        let draft = RepairVerificationReceiptDraft::try_new(
            wire.manifest_id,
            wire.manifest_digest,
            wire.apply_receipt_digest,
            wire.verified_at,
            wire.general_snapshots,
            wire.deep_snapshots,
        )
        .map_err(D::Error::custom)?;
        Ok(Self::from_draft(draft, wire.receipt_digest))
    }
}
