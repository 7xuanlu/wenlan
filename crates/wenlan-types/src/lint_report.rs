// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintTotals {
    checks: u32,
    passed: u32,
    findings: u32,
    actionable_findings: u32,
    advisory_findings: u32,
    incomplete: u32,
}
impl LintTotals {
    fn from_checks(checks: &[LintCheckResult]) -> Result<Self, LintContractError> {
        let checks_count =
            u32::try_from(checks.len()).map_err(|_| LintContractError::TooManyChecks)?;
        let mut totals = Self {
            checks: checks_count,
            passed: 0,
            findings: 0,
            actionable_findings: 0,
            advisory_findings: 0,
            incomplete: 0,
        };
        for check in checks {
            match check.outcome {
                LintOutcome::Pass => totals.passed += 1,
                LintOutcome::Finding => {
                    totals.findings += 1;
                    match check.gate_effect() {
                        LintGateEffect::Actionable => totals.actionable_findings += 1,
                        LintGateEffect::Advisory => totals.advisory_findings += 1,
                    }
                }
                LintOutcome::NotRunPrerequisite
                | LintOutcome::InconsistentSnapshot
                | LintOutcome::FailedToRun => totals.incomplete += 1,
            }
        }
        Ok(totals)
    }
    pub const fn checks(&self) -> u32 {
        self.checks
    }
    pub const fn findings(&self) -> u32 {
        self.findings
    }
    pub const fn actionable_findings(&self) -> u32 {
        self.actionable_findings
    }
    pub const fn advisory_findings(&self) -> u32 {
        self.advisory_findings
    }
    pub const fn passed(&self) -> u32 {
        self.passed
    }
    pub const fn incomplete(&self) -> u32 {
        self.incomplete
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintReport {
    report_schema_version: u16,
    check_catalog_version: u16,
    profile: LintProfile,
    scope: LintScope,
    capability_context: LintCapabilityContext,
    snapshots: LintSnapshotReceipts,
    config_fingerprint: LintConfigFingerprint,
    producer_receipt: LintProducerReceipt,
    checks: Vec<LintCheckResult>,
    totals: LintTotals,
    complete: bool,
}
#[derive(Deserialize)]
struct LintReportWire {
    report_schema_version: u16,
    check_catalog_version: u16,
    profile: LintProfile,
    scope: LintScope,
    capability_context: LintCapabilityContext,
    snapshots: LintSnapshotReceipts,
    config_fingerprint: LintConfigFingerprint,
    producer_receipt: LintProducerReceipt,
    checks: Vec<LintCheckResult>,
    totals: LintTotals,
    complete: bool,
}
impl LintReport {
    pub fn try_new(
        scope: LintScope,
        capability_context: LintCapabilityContext,
        snapshots: LintSnapshotReceipts,
        config_fingerprint: LintConfigFingerprint,
        producer_receipt: LintProducerReceipt,
        checks: Vec<LintCheckResult>,
    ) -> Result<Self, LintContractError> {
        Self::try_new_for_profile(
            LintProfile::General,
            scope,
            capability_context,
            snapshots,
            config_fingerprint,
            producer_receipt,
            checks,
        )
    }

    pub fn try_new_for_profile(
        profile: LintProfile,
        scope: LintScope,
        capability_context: LintCapabilityContext,
        snapshots: LintSnapshotReceipts,
        config_fingerprint: LintConfigFingerprint,
        producer_receipt: LintProducerReceipt,
        mut checks: Vec<LintCheckResult>,
    ) -> Result<Self, LintContractError> {
        checks.sort_by(|left, right| left.check_id().cmp(right.check_id()));
        let totals = LintTotals::from_checks(&checks)?;
        let complete = checks.iter().all(|check| check.outcome.is_complete());
        Ok(Self {
            report_schema_version: LINT_REPORT_SCHEMA_VERSION,
            check_catalog_version: LINT_CHECK_CATALOG_VERSION,
            profile,
            scope,
            capability_context,
            snapshots,
            config_fingerprint,
            producer_receipt,
            checks,
            totals,
            complete,
        })
    }
    pub const fn complete(&self) -> bool {
        self.complete
    }
    pub const fn profile(&self) -> LintProfile {
        self.profile
    }
    pub const fn totals(&self) -> &LintTotals {
        &self.totals
    }
    pub fn checks(&self) -> &[LintCheckResult] {
        &self.checks
    }
    pub fn scope(&self) -> &LintScope {
        &self.scope
    }
    pub const fn capability_context(&self) -> LintCapabilityContext {
        self.capability_context
    }
    pub fn snapshots(&self) -> &LintSnapshotReceipts {
        &self.snapshots
    }
    pub fn config_fingerprint(&self) -> &LintConfigFingerprint {
        &self.config_fingerprint
    }
    pub fn producer_receipt(&self) -> &LintProducerReceipt {
        &self.producer_receipt
    }
}
impl<'de> Deserialize<'de> for LintReport {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintReportWire::deserialize(deserializer)?;
        if wire.report_schema_version != LINT_REPORT_SCHEMA_VERSION {
            return Err(D::Error::custom(LintContractError::UnsupportedReportSchema));
        }
        if wire.check_catalog_version != LINT_CHECK_CATALOG_VERSION {
            return Err(D::Error::custom(LintContractError::UnsupportedCheckCatalog));
        }
        let expected_checks = match wire.profile {
            LintProfile::General => LINT_GENERAL_CHECK_COUNT,
            LintProfile::Deep => LINT_DEEP_CHECK_COUNT,
        };
        let unique_ids = wire
            .checks
            .iter()
            .map(LintCheckResult::check_id)
            .collect::<BTreeSet<_>>();
        if wire.checks.len() != expected_checks || unique_ids.len() != wire.checks.len() {
            return Err(D::Error::custom(LintContractError::InvalidCatalogShape));
        }
        let report = Self::try_new_for_profile(
            wire.profile,
            wire.scope,
            wire.capability_context,
            wire.snapshots,
            wire.config_fingerprint,
            wire.producer_receipt,
            wire.checks,
        )
        .map_err(D::Error::custom)?;
        if report.totals != wire.totals {
            return Err(D::Error::custom(LintContractError::InvalidTotals));
        }
        if report.complete != wire.complete {
            return Err(D::Error::custom(LintContractError::InvalidCompleteness));
        }
        Ok(report)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<LintProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
}

impl LintQuery {
    pub const fn new(profile: Option<LintProfile>, space: Option<String>) -> Self {
        Self { profile, space }
    }

    pub fn applied_profile(&self) -> LintProfile {
        self.profile.unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintRequestQuery {
    #[serde(flatten)]
    lint: LintQuery,
    #[serde(default, skip_serializing_if = "is_false")]
    external_egress: bool,
}

impl LintRequestQuery {
    pub const fn new(lint: LintQuery, external_egress: bool) -> Self {
        Self {
            lint,
            external_egress,
        }
    }

    pub const fn lint(&self) -> &LintQuery {
        &self.lint
    }

    pub const fn external_egress(&self) -> bool {
        self.external_egress
    }
}

const fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintErrorResponse {
    error: String,
}

impl LintErrorResponse {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }

    pub fn error(&self) -> &str {
        &self.error
    }
}
