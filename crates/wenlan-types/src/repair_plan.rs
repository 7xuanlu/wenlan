// SPDX-License-Identifier: Apache-2.0
//! Total lint-repair resolution contracts shared by the daemon and local clients.

use crate::{
    lint::{
        LintProducerReceipt, LintProfile, LintScope, LintSnapshotReceipts,
        LINT_CHECK_CATALOG_VERSION, LINT_REPORT_SCHEMA_VERSION,
    },
    repair::{RepairDigest, RepairLintScope, RepairManifest},
};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::{collections::BTreeSet, fmt};

pub const REPAIR_PLAN_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairPlanContractError {
    UnsupportedSchema,
    InvalidPlanId,
    InvalidReportReceipt,
    InvalidEntry,
    DuplicateOccurrence,
    InvalidReviewItem,
    InvalidSystemAction,
    InvalidBlockedResolution,
    InvalidTotals,
}

impl fmt::Display for RepairPlanContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedSchema => "unsupported repair plan schema",
            Self::InvalidPlanId => "invalid repair plan id",
            Self::InvalidReportReceipt => "invalid repair plan report receipt",
            Self::InvalidEntry => "invalid repair plan entry",
            Self::DuplicateOccurrence => "duplicate repair plan occurrence",
            Self::InvalidReviewItem => "invalid repair review item",
            Self::InvalidSystemAction => "invalid repair system action",
            Self::InvalidBlockedResolution => "invalid blocked repair resolution",
            Self::InvalidTotals => "invalid repair plan totals",
        })
    }
}

impl std::error::Error for RepairPlanContractError {}

fn valid_nonempty(value: &str) -> bool {
    !value.is_empty() && value.trim() == value
}

fn valid_plan_id(value: &str) -> bool {
    let Some(uuid) = value.strip_prefix("repair_plan_") else {
        return false;
    };
    uuid.len() == 36
        && uuid.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit()),
        })
}

fn valid_review_id(value: &str) -> bool {
    value
        .strip_prefix("lint_review_")
        .is_some_and(|digest| RepairDigest::parse(digest).is_ok())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairFindingKind {
    Deterministic,
    Semantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAffectedRecordKind {
    Memory,
    Page,
    Entity,
    Relation,
    Tag,
    PageLink,
    Schema,
    Route,
    Retrieval,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct RepairAffectedRecord {
    kind: RepairAffectedRecordKind,
    durable_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairAffectedRecordWire {
    kind: RepairAffectedRecordKind,
    durable_id: String,
}

impl RepairAffectedRecord {
    pub fn try_new(
        kind: RepairAffectedRecordKind,
        durable_id: String,
    ) -> Result<Self, RepairPlanContractError> {
        if !valid_nonempty(&durable_id) {
            return Err(RepairPlanContractError::InvalidEntry);
        }
        Ok(Self { kind, durable_id })
    }

    pub const fn kind(&self) -> RepairAffectedRecordKind {
        self.kind
    }

    pub fn durable_id(&self) -> &str {
        &self.durable_id
    }
}

impl<'de> Deserialize<'de> for RepairAffectedRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairAffectedRecordWire::deserialize(deserializer)?;
        Self::try_new(wire.kind, wire.durable_id).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairReviewItem {
    review_id: String,
    check_id: String,
    issue: String,
    choices: Vec<String>,
    suggested_research_queries: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairReviewItemWire {
    review_id: String,
    check_id: String,
    issue: String,
    choices: Vec<String>,
    #[serde(default)]
    suggested_research_queries: Vec<String>,
}

impl RepairReviewItem {
    pub fn try_new(
        review_id: String,
        check_id: String,
        issue: String,
        choices: Vec<String>,
        suggested_research_queries: Vec<String>,
    ) -> Result<Self, RepairPlanContractError> {
        if !valid_review_id(&review_id)
            || !valid_nonempty(&check_id)
            || !valid_nonempty(&issue)
            || choices.is_empty()
            || choices.iter().any(|choice| !valid_nonempty(choice))
            || suggested_research_queries
                .iter()
                .any(|query| !valid_nonempty(query))
        {
            return Err(RepairPlanContractError::InvalidReviewItem);
        }
        Ok(Self {
            review_id,
            check_id,
            issue,
            choices,
            suggested_research_queries,
        })
    }

    pub fn review_id(&self) -> &str {
        &self.review_id
    }

    pub fn check_id(&self) -> &str {
        &self.check_id
    }

    pub fn issue(&self) -> &str {
        &self.issue
    }

    pub fn choices(&self) -> &[String] {
        &self.choices
    }

    pub fn suggested_research_queries(&self) -> &[String] {
        &self.suggested_research_queries
    }
}

impl<'de> Deserialize<'de> for RepairReviewItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairReviewItemWire::deserialize(deserializer)?;
        Self::try_new(
            wire.review_id,
            wire.check_id,
            wire.issue,
            wire.choices,
            wire.suggested_research_queries,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairSystemActionKind {
    RunSchemaMigration,
    RebuildSearchIndex,
    UpdateDaemon,
    RestartDaemon,
    CorrectRouteScopeContract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairSystemAction {
    kind: RepairSystemActionKind,
    summary: String,
    evidence: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairSystemActionWire {
    kind: RepairSystemActionKind,
    summary: String,
    evidence: Vec<String>,
}

impl RepairSystemAction {
    pub fn try_new(
        kind: RepairSystemActionKind,
        summary: String,
        evidence: Vec<String>,
    ) -> Result<Self, RepairPlanContractError> {
        if !valid_nonempty(&summary)
            || evidence.is_empty()
            || evidence.iter().any(|item| !valid_nonempty(item))
        {
            return Err(RepairPlanContractError::InvalidSystemAction);
        }
        Ok(Self {
            kind,
            summary,
            evidence,
        })
    }

    pub const fn kind(&self) -> RepairSystemActionKind {
        self.kind
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn evidence(&self) -> &[String] {
        &self.evidence
    }
}

impl<'de> Deserialize<'de> for RepairSystemAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairSystemActionWire::deserialize(deserializer)?;
        Self::try_new(wire.kind, wire.summary, wire.evidence).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairBlockedReasonCode {
    UnsupportedDeterministicWriter,
    SourceIncomplete,
    SourceStale,
    AmbiguousRepairTarget,
    ConflictingRepairProposals,
    UnknownSchemaShape,
    MissingPrerequisite,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairBlocked {
    reason_code: RepairBlockedReasonCode,
    detail: String,
    next_action: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairBlockedWire {
    reason_code: RepairBlockedReasonCode,
    detail: String,
    next_action: String,
}

impl RepairBlocked {
    pub fn try_new(
        reason_code: RepairBlockedReasonCode,
        detail: String,
        next_action: String,
    ) -> Result<Self, RepairPlanContractError> {
        if !valid_nonempty(&detail) || !valid_nonempty(&next_action) {
            return Err(RepairPlanContractError::InvalidBlockedResolution);
        }
        Ok(Self {
            reason_code,
            detail,
            next_action,
        })
    }

    pub const fn reason_code(&self) -> RepairBlockedReasonCode {
        self.reason_code
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub fn next_action(&self) -> &str {
        &self.next_action
    }
}

impl<'de> Deserialize<'de> for RepairBlocked {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairBlockedWire::deserialize(deserializer)?;
        Self::try_new(wire.reason_code, wire.detail, wire.next_action).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "disposition", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepairResolution {
    Ready { manifest: Box<RepairManifest> },
    Review { review_item: RepairReviewItem },
    SystemAction { system_action: RepairSystemAction },
    Blocked { blocked: RepairBlocked },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairPlanEntry {
    finding_kind: RepairFindingKind,
    check_id: String,
    occurrence_digest: RepairDigest,
    affected_records: Vec<RepairAffectedRecord>,
    resolution: RepairResolution,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairPlanEntryWire {
    finding_kind: RepairFindingKind,
    check_id: String,
    occurrence_digest: RepairDigest,
    affected_records: Vec<RepairAffectedRecord>,
    resolution: RepairResolution,
}

impl RepairPlanEntry {
    pub fn try_new(
        finding_kind: RepairFindingKind,
        check_id: String,
        occurrence_digest: RepairDigest,
        mut affected_records: Vec<RepairAffectedRecord>,
        resolution: RepairResolution,
    ) -> Result<Self, RepairPlanContractError> {
        let target_resolution = matches!(
            resolution,
            RepairResolution::Ready { .. } | RepairResolution::Review { .. }
        );
        if !valid_nonempty(&check_id) || (target_resolution && affected_records.is_empty()) {
            return Err(RepairPlanContractError::InvalidEntry);
        }
        if let RepairResolution::Review { review_item } = &resolution {
            if review_item.check_id() != check_id {
                return Err(RepairPlanContractError::InvalidEntry);
            }
        }
        affected_records.sort();
        if affected_records.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(RepairPlanContractError::InvalidEntry);
        }
        Ok(Self {
            finding_kind,
            check_id,
            occurrence_digest,
            affected_records,
            resolution,
        })
    }

    pub const fn finding_kind(&self) -> RepairFindingKind {
        self.finding_kind
    }

    pub fn check_id(&self) -> &str {
        &self.check_id
    }

    pub fn occurrence_digest(&self) -> &RepairDigest {
        &self.occurrence_digest
    }

    pub fn affected_records(&self) -> &[RepairAffectedRecord] {
        &self.affected_records
    }

    pub fn resolution(&self) -> &RepairResolution {
        &self.resolution
    }
}

impl<'de> Deserialize<'de> for RepairPlanEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairPlanEntryWire::deserialize(deserializer)?;
        Self::try_new(
            wire.finding_kind,
            wire.check_id,
            wire.occurrence_digest,
            wire.affected_records,
            wire.resolution,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairPlanReportReceipt {
    report_schema_version: u16,
    check_catalog_version: u16,
    profile: LintProfile,
    scope: LintScope,
    snapshots: LintSnapshotReceipts,
    producer_receipt: LintProducerReceipt,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairPlanReportReceiptWire {
    report_schema_version: u16,
    check_catalog_version: u16,
    profile: LintProfile,
    scope: LintScope,
    snapshots: LintSnapshotReceipts,
    producer_receipt: LintProducerReceipt,
}

impl RepairPlanReportReceipt {
    pub fn try_new(
        profile: LintProfile,
        scope: LintScope,
        snapshots: LintSnapshotReceipts,
        producer_receipt: LintProducerReceipt,
    ) -> Result<Self, RepairPlanContractError> {
        Ok(Self {
            report_schema_version: LINT_REPORT_SCHEMA_VERSION,
            check_catalog_version: LINT_CHECK_CATALOG_VERSION,
            profile,
            scope,
            snapshots,
            producer_receipt,
        })
    }

    pub fn from_report(report: &crate::LintReport) -> Self {
        Self {
            report_schema_version: LINT_REPORT_SCHEMA_VERSION,
            check_catalog_version: LINT_CHECK_CATALOG_VERSION,
            profile: report.profile(),
            scope: report.scope().clone(),
            snapshots: report.snapshots().clone(),
            producer_receipt: report.producer_receipt().clone(),
        }
    }

    pub const fn profile(&self) -> LintProfile {
        self.profile
    }

    pub fn scope(&self) -> &LintScope {
        &self.scope
    }

    pub fn snapshots(&self) -> &LintSnapshotReceipts {
        &self.snapshots
    }

    pub fn producer_receipt(&self) -> &LintProducerReceipt {
        &self.producer_receipt
    }
}

impl<'de> Deserialize<'de> for RepairPlanReportReceipt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairPlanReportReceiptWire::deserialize(deserializer)?;
        if wire.report_schema_version != LINT_REPORT_SCHEMA_VERSION
            || wire.check_catalog_version != LINT_CHECK_CATALOG_VERSION
        {
            return Err(D::Error::custom(
                RepairPlanContractError::InvalidReportReceipt,
            ));
        }
        Self::try_new(
            wire.profile,
            wire.scope,
            wire.snapshots,
            wire.producer_receipt,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairPlanTotals {
    deterministic: u64,
    semantic: u64,
    ready: u64,
    review: u64,
    system_action: u64,
    blocked: u64,
}

impl RepairPlanTotals {
    fn from_entries(entries: &[RepairPlanEntry]) -> Result<Self, RepairPlanContractError> {
        let mut totals = Self {
            deterministic: 0,
            semantic: 0,
            ready: 0,
            review: 0,
            system_action: 0,
            blocked: 0,
        };
        for entry in entries {
            match entry.finding_kind() {
                RepairFindingKind::Deterministic => totals.deterministic += 1,
                RepairFindingKind::Semantic => totals.semantic += 1,
            }
            match entry.resolution() {
                RepairResolution::Ready { .. } => totals.ready += 1,
                RepairResolution::Review { .. } => totals.review += 1,
                RepairResolution::SystemAction { .. } => totals.system_action += 1,
                RepairResolution::Blocked { .. } => totals.blocked += 1,
            }
        }
        Ok(totals)
    }

    pub const fn deterministic(&self) -> u64 {
        self.deterministic
    }

    pub const fn semantic(&self) -> u64 {
        self.semantic
    }

    pub const fn ready(&self) -> u64 {
        self.ready
    }

    pub const fn review(&self) -> u64 {
        self.review
    }

    pub const fn system_action(&self) -> u64 {
        self.system_action
    }

    pub const fn blocked(&self) -> u64 {
        self.blocked
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairPlanDraft {
    plan_schema_version: u16,
    plan_id: String,
    scope: RepairLintScope,
    general_report_receipt: RepairPlanReportReceipt,
    #[serde(skip_serializing_if = "Option::is_none")]
    deep_report_receipt: Option<RepairPlanReportReceipt>,
    deterministic_complete: bool,
    semantic_complete: bool,
    entries: Vec<RepairPlanEntry>,
    totals: RepairPlanTotals,
}

impl RepairPlanDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        plan_id: String,
        scope: RepairLintScope,
        general_report_receipt: RepairPlanReportReceipt,
        deep_report_receipt: Option<RepairPlanReportReceipt>,
        deterministic_complete: bool,
        semantic_complete: bool,
        mut entries: Vec<RepairPlanEntry>,
    ) -> Result<Self, RepairPlanContractError> {
        let reports_valid = general_report_receipt.profile() == LintProfile::General
            && scope.matches_report_scope_kind(general_report_receipt.scope())
            && deep_report_receipt.as_ref().is_none_or(|receipt| {
                receipt.profile() == LintProfile::Deep
                    && scope.matches_report_scope_kind(receipt.scope())
                    && receipt.scope() == general_report_receipt.scope()
            });
        let semantic_valid = deep_report_receipt.is_some()
            || (!semantic_complete
                && entries
                    .iter()
                    .all(|entry| entry.finding_kind() != RepairFindingKind::Semantic));
        if !valid_plan_id(&plan_id) || !reports_valid || !semantic_valid {
            return Err(RepairPlanContractError::InvalidPlanId);
        }
        let mut occurrences = BTreeSet::new();
        if entries
            .iter()
            .any(|entry| !occurrences.insert(entry.occurrence_digest().as_str().to_string()))
        {
            return Err(RepairPlanContractError::DuplicateOccurrence);
        }
        entries.sort_by(|left, right| {
            (
                left.finding_kind(),
                left.check_id(),
                left.occurrence_digest().as_str(),
            )
                .cmp(&(
                    right.finding_kind(),
                    right.check_id(),
                    right.occurrence_digest().as_str(),
                ))
        });
        let totals = RepairPlanTotals::from_entries(&entries)?;
        Ok(Self {
            plan_schema_version: REPAIR_PLAN_SCHEMA_VERSION,
            plan_id,
            scope,
            general_report_receipt,
            deep_report_receipt,
            deterministic_complete,
            semantic_complete,
            entries,
            totals,
        })
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairPlan {
    #[serde(flatten)]
    draft: RepairPlanDraft,
    plan_digest: RepairDigest,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairPlanWire {
    plan_schema_version: u16,
    plan_id: String,
    scope: RepairLintScope,
    general_report_receipt: RepairPlanReportReceipt,
    #[serde(default)]
    deep_report_receipt: Option<RepairPlanReportReceipt>,
    deterministic_complete: bool,
    semantic_complete: bool,
    entries: Vec<RepairPlanEntry>,
    totals: RepairPlanTotals,
    plan_digest: RepairDigest,
}

impl RepairPlan {
    pub fn try_new(
        draft: RepairPlanDraft,
        plan_digest: RepairDigest,
    ) -> Result<Self, RepairPlanContractError> {
        Ok(Self { draft, plan_digest })
    }

    pub const fn schema_version(&self) -> u16 {
        self.draft.plan_schema_version
    }

    pub fn plan_id(&self) -> &str {
        &self.draft.plan_id
    }

    pub fn scope(&self) -> &RepairLintScope {
        &self.draft.scope
    }

    pub fn general_report_receipt(&self) -> &RepairPlanReportReceipt {
        &self.draft.general_report_receipt
    }

    pub fn deep_report_receipt(&self) -> Option<&RepairPlanReportReceipt> {
        self.draft.deep_report_receipt.as_ref()
    }

    pub const fn deterministic_complete(&self) -> bool {
        self.draft.deterministic_complete
    }

    pub const fn semantic_complete(&self) -> bool {
        self.draft.semantic_complete
    }

    pub fn entries(&self) -> &[RepairPlanEntry] {
        &self.draft.entries
    }

    pub const fn totals(&self) -> &RepairPlanTotals {
        &self.draft.totals
    }

    pub fn plan_digest(&self) -> &RepairDigest {
        &self.plan_digest
    }

    pub fn canonical_unsigned_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.draft.canonical_bytes()
    }
}

impl<'de> Deserialize<'de> for RepairPlan {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairPlanWire::deserialize(deserializer)?;
        if wire.plan_schema_version != REPAIR_PLAN_SCHEMA_VERSION {
            return Err(D::Error::custom(RepairPlanContractError::UnsupportedSchema));
        }
        let draft = RepairPlanDraft::try_new(
            wire.plan_id,
            wire.scope,
            wire.general_report_receipt,
            wire.deep_report_receipt,
            wire.deterministic_complete,
            wire.semantic_complete,
            wire.entries,
        )
        .map_err(D::Error::custom)?;
        if draft.totals != wire.totals {
            return Err(D::Error::custom(RepairPlanContractError::InvalidTotals));
        }
        Self::try_new(draft, wire.plan_digest).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairPlanRequest {
    scope: RepairLintScope,
    general_report: crate::LintReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    deep_report: Option<crate::LintReport>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairPlanRequestWire {
    scope: RepairLintScope,
    general_report: crate::LintReport,
    #[serde(default)]
    deep_report: Option<crate::LintReport>,
}

impl RepairPlanRequest {
    pub fn try_new(
        scope: RepairLintScope,
        general_report: crate::LintReport,
        deep_report: Option<crate::LintReport>,
    ) -> Result<Self, RepairPlanContractError> {
        let valid = general_report.profile() == LintProfile::General
            && scope.matches_report_scope_kind(general_report.scope())
            && deep_report.as_ref().is_none_or(|report| {
                report.profile() == LintProfile::Deep
                    && scope.matches_report_scope_kind(report.scope())
                    && report.scope() == general_report.scope()
            });
        if !valid {
            return Err(RepairPlanContractError::InvalidReportReceipt);
        }
        Ok(Self {
            scope,
            general_report,
            deep_report,
        })
    }

    pub fn scope(&self) -> &RepairLintScope {
        &self.scope
    }

    pub fn general_report(&self) -> &crate::LintReport {
        &self.general_report
    }

    pub fn deep_report(&self) -> Option<&crate::LintReport> {
        self.deep_report.as_ref()
    }
}

impl<'de> Deserialize<'de> for RepairPlanRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RepairPlanRequestWire::deserialize(deserializer)?;
        Self::try_new(wire.scope, wire.general_report, wire.deep_report).map_err(D::Error::custom)
    }
}
