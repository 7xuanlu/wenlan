// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintTotals {
    checks: u32,
    passed: u32,
    findings: u32,
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
            incomplete: 0,
        };
        for check in checks {
            match check.outcome {
                LintOutcome::Pass => totals.passed += 1,
                LintOutcome::Finding => totals.findings += 1,
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
        mut checks: Vec<LintCheckResult>,
    ) -> Result<Self, LintContractError> {
        checks.sort_by(|left, right| left.check_id().cmp(right.check_id()));
        let totals = LintTotals::from_checks(&checks)?;
        let complete = checks.iter().all(|check| check.outcome.is_complete());
        Ok(Self {
            report_schema_version: LINT_REPORT_SCHEMA_VERSION,
            check_catalog_version: LINT_CHECK_CATALOG_VERSION,
            profile: LintProfile::Deterministic,
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
        if wire.profile != LintProfile::Deterministic {
            return Err(D::Error::custom(LintContractError::UnsupportedReportSchema));
        }
        let report = Self::try_new(
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
    pub space: Option<String>,
}
