use super::catalog::catalog;
use super::context::{CancellationToken, ExecutionGate, LintClock};
use super::pages::fs::{scan_page_root, PageFsError, PageScan};
use super::snapshot::{SnapshotError, SnapshotReceipt};
use crate::db::MemoryDB;
use std::collections::BTreeSet;
use std::path::Path;
use wenlan_types::lint::{
    LintApplicability, LintCapabilityContext, LintCheckResult, LintCheckResultInput,
    LintConfigFingerprint, LintConfigSelection, LintConfigSetting, LintConfigValue,
    LintContractError, LintCoverage, LintDbSnapshotMode, LintDbSnapshotReceipt, LintDigest,
    LintOutcome, LintPageSnapshotMode, LintPageSnapshotReceipt, LintPrecondition,
    LintProducerReceipt, LintQuery, LintRecommendationCode, LintReport, LintScope, LintSeverity,
    LintSnapshotReceipts, LintSummaryCode, LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
};

#[derive(Debug, thiserror::Error)]
pub enum LintRunError {
    #[error("invalid_scope")]
    InvalidScope,
    #[error("lint catalog/result mismatch")]
    CatalogMismatch,
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    PageScan(#[from] PageFsError),
    #[error(transparent)]
    Contract(#[from] LintContractError),
}

pub struct LintRunner {
    clock: LintClock,
    gate: ExecutionGate,
}

impl LintRunner {
    pub fn new(clock: LintClock, cancellation: CancellationToken) -> Self {
        Self {
            clock,
            gate: ExecutionGate::new(cancellation),
        }
    }

    pub async fn run(
        &self,
        database: &MemoryDB,
        query: &LintQuery,
        page_root: Option<&Path>,
        page_projection_enabled: bool,
    ) -> Result<LintReport, LintRunError> {
        let snapshot = database.open_unpinned_lint_snapshot().await?;
        let scope = validate_scope(&snapshot, query).await?;
        let snapshot = snapshot.pin_analysis().await?;
        let mut execution_failed = self.gate.check_run(self.clock.elapsed()).is_err();

        let page_started = self.clock.elapsed();
        let page_scan = if page_projection_enabled && !execution_failed {
            match page_root.map(scan_page_root).transpose() {
                Ok(scan) => scan,
                Err(_) => {
                    execution_failed = true;
                    None
                }
            }
        } else {
            None
        };
        let mut checks = if execution_failed {
            failed_results(&self.clock)
        } else if page_projection_enabled {
            prerequisite_results(&self.clock)
        } else {
            configured_off_results(self.clock.clone())
        };
        let page_elapsed = self.clock.elapsed().saturating_sub(page_started);
        if self.gate.check(page_elapsed).is_err()
            || self.gate.check_run(self.clock.elapsed()).is_err()
        {
            checks = failed_results(&self.clock);
        }
        validate_catalog_results(&mut checks)?;
        validate_scoped_evidence(&scope, &checks)?;

        let page_before = page_scan
            .as_ref()
            .map(PageScan::normalized_bytes)
            .unwrap_or([0; 32]);
        let page_after = match (page_root, page_scan.as_ref()) {
            (Some(root), Some(_)) => scan_page_root(root)?.normalized_bytes(),
            _ => page_before,
        };
        if page_before != page_after {
            checks = inconsistent_results(&self.clock);
        }
        let db_receipt = snapshot.finish().await?;
        if !db_receipt.is_consistent() && page_projection_enabled {
            checks = inconsistent_results(&self.clock);
        }
        validate_catalog_results(&mut checks)?;
        validate_scoped_evidence(&scope, &checks)?;
        build_report(
            scope,
            page_projection_enabled,
            db_receipt,
            page_before,
            page_after,
            checks,
        )
    }
}

async fn validate_scope(
    snapshot: &super::snapshot::LintReadSnapshot<'_>,
    query: &LintQuery,
) -> Result<LintScope, LintRunError> {
    let Some(requested) = query.space.as_deref() else {
        return Ok(LintScope::global());
    };
    if requested == "uncategorized" {
        return Ok(LintScope::uncategorized());
    }
    let mut rows = snapshot
        .query(
            "SELECT (SELECT COUNT(*) FROM spaces prior WHERE prior.name < current.name) FROM spaces current WHERE current.name = ?1 LIMIT 1",
            libsql::params::Params::Positional(vec![libsql::Value::Text(requested.to_string())]),
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return Err(LintRunError::InvalidScope);
    };
    let ordinal = row.get::<i64>(0).map_err(SnapshotError::from)?;
    let position = usize::try_from(ordinal).map_err(|_| LintRunError::InvalidScope)?;
    let opaque = wenlan_types::lint::LintOpaqueId::from_sorted_position(position)
        .ok_or(LintRunError::InvalidScope)?;
    Ok(LintScope::registered(opaque))
}

pub fn configured_off_results(clock: LintClock) -> Vec<LintCheckResult> {
    catalog()
        .iter()
        .map(|entry| {
            make_result(
                entry.id,
                LintOutcome::Pass,
                LintSeverity::Info,
                LintApplicability::ExpectedEmpty,
                LintPrecondition::ConfiguredOff,
                LintSummaryCode::ExpectedEmpty,
                None,
                clock.duration_ms(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("static configured-off lint results are valid")
}

fn prerequisite_results(clock: &LintClock) -> Vec<LintCheckResult> {
    catalog()
        .iter()
        .map(|entry| {
            make_result(
                entry.id,
                LintOutcome::NotRunPrerequisite,
                LintSeverity::Error,
                LintApplicability::NotApplicable,
                LintPrecondition::MissingPrerequisite,
                LintSummaryCode::PrerequisiteUnavailable,
                Some(LintRecommendationCode::RestorePrerequisite),
                clock.duration_ms(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("static prerequisite lint results are valid")
}

fn failed_results(clock: &LintClock) -> Vec<LintCheckResult> {
    terminal_results(
        clock,
        LintOutcome::FailedToRun,
        LintPrecondition::Ready,
        LintSummaryCode::ExecutionFailed,
        LintRecommendationCode::InspectRuntime,
    )
}

fn inconsistent_results(clock: &LintClock) -> Vec<LintCheckResult> {
    terminal_results(
        clock,
        LintOutcome::InconsistentSnapshot,
        LintPrecondition::SnapshotUnstable,
        LintSummaryCode::SnapshotInconsistent,
        LintRecommendationCode::RerunAfterSnapshotStabilizes,
    )
}

fn terminal_results(
    clock: &LintClock,
    outcome: LintOutcome,
    precondition: LintPrecondition,
    summary: LintSummaryCode,
    recommendation: LintRecommendationCode,
) -> Vec<LintCheckResult> {
    catalog()
        .iter()
        .map(|entry| {
            make_result(
                entry.id,
                outcome,
                LintSeverity::Error,
                LintApplicability::Applicable,
                precondition,
                summary,
                Some(recommendation),
                clock.duration_ms(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("static terminal lint results are valid")
}

#[allow(clippy::too_many_arguments)]
fn make_result(
    check_id: &str,
    outcome: LintOutcome,
    severity: LintSeverity,
    applicability: LintApplicability,
    precondition: LintPrecondition,
    summary_code: LintSummaryCode,
    recommendation_code: Option<LintRecommendationCode>,
    duration_ms: u64,
) -> Result<LintCheckResult, LintContractError> {
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: check_id.to_string(),
        outcome,
        severity,
        applicability,
        precondition,
        coverage: LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            0,
            0,
            LINT_MAX_EVIDENCE_PER_CHECK,
            false,
            0,
        )?,
        metrics: Vec::new(),
        summary_code,
        recommendation_code,
        evidence: Vec::new(),
        duration_ms,
    })
}

pub fn validate_catalog_results(checks: &mut [LintCheckResult]) -> Result<(), LintRunError> {
    checks.sort_by(|left, right| left.check_id().cmp(right.check_id()));
    let expected = catalog().iter().map(|entry| entry.id).collect::<Vec<_>>();
    let actual = checks
        .iter()
        .map(LintCheckResult::check_id)
        .collect::<Vec<_>>();
    let unique = actual.iter().copied().collect::<BTreeSet<_>>();
    if actual != expected || unique.len() != actual.len() {
        return Err(LintRunError::CatalogMismatch);
    }
    Ok(())
}

fn validate_scoped_evidence(
    scope: &LintScope,
    checks: &[LintCheckResult],
) -> Result<(), LintRunError> {
    if scope.kind() == wenlan_types::lint::LintScopeKind::Global {
        return Ok(());
    }
    for check in checks {
        let entry =
            super::catalog::catalog_entry(check.check_id()).ok_or(LintRunError::CatalogMismatch)?;
        if matches!(
            entry.scope_policy,
            super::catalog::ScopePolicy::GlobalAggregateOnly
                | super::catalog::ScopePolicy::GlobalOnly
        ) && !check.evidence().is_empty()
        {
            return Err(LintRunError::CatalogMismatch);
        }
    }
    Ok(())
}

fn build_report(
    scope: LintScope,
    page_projection_enabled: bool,
    db_receipt: SnapshotReceipt,
    page_before: [u8; 32],
    page_after: [u8; 32],
    checks: Vec<LintCheckResult>,
) -> Result<LintReport, LintRunError> {
    LintReport::try_new(
        scope,
        LintCapabilityContext::daemon_operator_endpoint(),
        receipts(db_receipt, page_before, page_after),
        LintConfigFingerprint::from_effective_config(&[LintConfigSelection::new(
            LintConfigSetting::PageProjectionEnabled,
            if page_projection_enabled {
                LintConfigValue::Enabled
            } else {
                LintConfigValue::Disabled
            },
        )]),
        LintProducerReceipt::new(None),
        checks,
    )
    .map_err(LintRunError::from)
}

fn receipts(
    db: SnapshotReceipt,
    page_before: [u8; 32],
    page_after: [u8; 32],
) -> LintSnapshotReceipts {
    LintSnapshotReceipts::new(
        LintDbSnapshotReceipt::new(
            LintDbSnapshotMode::TransactionalReadOnly,
            digest(db.analysis_digest().as_bytes()),
            Some(digest(db.post_run_digest().as_bytes())),
        ),
        LintPageSnapshotReceipt::new(
            LintPageSnapshotMode::BestEffort,
            digest(page_before),
            Some(digest(page_after)),
        ),
    )
}

fn digest(bytes: [u8; 32]) -> LintDigest {
    LintDigest::from_u64(u64::from_le_bytes(
        bytes[..8].try_into().expect("eight bytes"),
    ))
}

#[cfg(test)]
pub(super) fn synthetic_report(checks: Vec<LintCheckResult>) -> LintReport {
    let zero = LintDigest::from_u64(0);
    LintReport::try_new(
        LintScope::global(),
        LintCapabilityContext::daemon_operator_endpoint(),
        LintSnapshotReceipts::new(
            LintDbSnapshotReceipt::new(
                LintDbSnapshotMode::TransactionalReadOnly,
                zero.clone(),
                Some(zero.clone()),
            ),
            LintPageSnapshotReceipt::new(
                LintPageSnapshotMode::BestEffort,
                zero.clone(),
                Some(zero),
            ),
        ),
        LintConfigFingerprint::from_effective_config(&[]),
        LintProducerReceipt::new(None),
        checks,
    )
    .expect("synthetic report is valid")
}
