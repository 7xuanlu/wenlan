use super::catalog::{catalog, catalog_entry, catalog_group, LintCheckGroup};
use super::context::{
    AppliedScope, CancellationToken, ExecutionGate, LintClock, LintContext, PopulationBasis,
};
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
    #[cfg(test)]
    scenario: Option<TestScenario>,
    #[cfg(test)]
    synchronization: Option<TestSynchronization>,
    #[cfg(test)]
    memory_features: Option<super::memories::TestMemoryFeatures>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TestScenario {
    MixedQueryFailure,
    PageGroupTimeout,
    ScopedDenominators,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TestSyncPoint {
    AfterTrackerSample,
    BeforeReceipts,
}

#[cfg(test)]
pub(super) struct TestSynchronization {
    point: TestSyncPoint,
    reached: std::sync::Arc<tokio::sync::Barrier>,
    resume: std::sync::Arc<tokio::sync::Barrier>,
}

#[cfg(test)]
pub(super) struct TestSynchronizationControl {
    reached: std::sync::Arc<tokio::sync::Barrier>,
    resume: std::sync::Arc<tokio::sync::Barrier>,
}

#[cfg(test)]
impl TestSynchronization {
    pub(super) fn new(point: TestSyncPoint) -> (Self, TestSynchronizationControl) {
        let reached = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        let resume = std::sync::Arc::new(tokio::sync::Barrier::new(2));
        (
            Self {
                point,
                reached: std::sync::Arc::clone(&reached),
                resume: std::sync::Arc::clone(&resume),
            },
            TestSynchronizationControl { reached, resume },
        )
    }

    async fn hit(&self, point: TestSyncPoint) {
        if self.point == point {
            self.reached.wait().await;
            self.resume.wait().await;
        }
    }
}

#[cfg(test)]
impl TestSynchronizationControl {
    pub(super) async fn wait_until_reached(&self) {
        self.reached.wait().await;
    }

    pub(super) async fn resume(&self) {
        self.resume.wait().await;
    }
}

impl LintRunner {
    pub fn new(clock: LintClock, cancellation: CancellationToken) -> Self {
        Self {
            clock,
            gate: ExecutionGate::new(cancellation),
            #[cfg(test)]
            scenario: None,
            #[cfg(test)]
            synchronization: None,
            #[cfg(test)]
            memory_features: None,
        }
    }

    #[cfg(test)]
    pub(super) fn with_test_scenario(mut self, scenario: TestScenario) -> Self {
        self.scenario = Some(scenario);
        self
    }

    #[cfg(test)]
    pub(super) fn with_test_synchronization(
        mut self,
        synchronization: TestSynchronization,
    ) -> Self {
        self.synchronization = Some(synchronization);
        self
    }

    #[cfg(test)]
    pub(super) fn with_test_memory_features(
        mut self,
        features: super::memories::TestMemoryFeatures,
    ) -> Self {
        self.memory_features = Some(features);
        self
    }

    pub async fn run(
        &self,
        database: &MemoryDB,
        query: &LintQuery,
        page_root: Option<&Path>,
        page_projection_enabled: bool,
    ) -> Result<LintReport, LintRunError> {
        let memory_config = {
            #[cfg(test)]
            if let Some(features) = self.memory_features {
                super::memories::MemoryFeatureConfig::for_test(database, features)
            } else {
                super::memories::MemoryFeatureConfig::capture(database, page_projection_enabled)
            }
            #[cfg(not(test))]
            super::memories::MemoryFeatureConfig::capture(database, page_projection_enabled)
        };
        let projection_tracker = database.page_projection_tracker();
        let tracker_before = projection_tracker.sample();
        #[cfg(test)]
        if let Some(synchronization) = &self.synchronization {
            synchronization.hit(TestSyncPoint::AfterTrackerSample).await;
        }
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
        let context = LintContext::new(
            &snapshot,
            &scope,
            page_scan.as_ref(),
            &self.clock,
            &self.gate,
        );
        let (mut checks, page_elapsed) = if execution_failed {
            record_zero_populations(&context)?;
            (failed_results(&self.clock), std::time::Duration::ZERO)
        } else {
            self.run_groups(
                &context,
                page_projection_enabled,
                memory_config,
                page_started,
            )
            .await?
        };
        if memory_config.artifact_state_changed(database) {
            checks = inconsistent_selected_results(&self.clock, &checks, |check_id| {
                catalog_entry(check_id).is_some_and(|entry| entry.group == LintCheckGroup::Memories)
            })?;
        }
        if self.gate.check(page_elapsed).is_err() {
            checks = failed_selected_results(&self.clock, &checks, |check_id| {
                catalog_entry(check_id).is_some_and(|entry| entry.group == LintCheckGroup::Pages)
            })?;
        }
        if self.gate.check_run(self.clock.elapsed()).is_err() {
            checks = failed_results_from_checks(&self.clock, &checks)?;
        }
        validate_catalog_results(&mut checks)?;
        validate_scope_policy(&context, &checks)?;
        drop(context);
        #[cfg(test)]
        if let Some(synchronization) = &self.synchronization {
            synchronization.hit(TestSyncPoint::BeforeReceipts).await;
        }

        let page_before = page_scan
            .as_ref()
            .map(PageScan::normalized_bytes)
            .unwrap_or([0; 32]);
        let page_after = match (page_root, page_scan.as_ref()) {
            (Some(root), Some(_)) => scan_page_root(root)?.normalized_bytes(),
            _ => page_before,
        };
        let page_changed = page_before != page_after;
        let db_receipt = snapshot.finish().await?;
        let tracker_after = projection_tracker.sample();
        let tracker_unstable = tracker_before.has_active_writes()
            || tracker_after.has_active_writes()
            || tracker_before.generation() != tracker_after.generation();
        if page_changed {
            checks =
                inconsistent_selected_results(&self.clock, &checks, super::pages::uses_filesystem)?;
        }
        if page_projection_enabled && (!db_receipt.is_consistent() || tracker_unstable) {
            checks = inconsistent_selected_results(
                &self.clock,
                &checks,
                super::pages::uses_cross_store,
            )?;
        }
        validate_catalog_results(&mut checks)?;
        build_report(
            scope.into_report(),
            page_projection_enabled,
            db_receipt,
            page_before,
            page_after,
            memory_config,
            checks,
        )
    }

    async fn run_groups(
        &self,
        context: &LintContext<'_, '_>,
        page_projection_enabled: bool,
        memory_config: super::memories::MemoryFeatureConfig,
        page_started: std::time::Duration,
    ) -> Result<(Vec<LintCheckResult>, std::time::Duration), LintRunError> {
        #[cfg(test)]
        if let Some(scenario) = self.scenario {
            let results = run_test_scenario(context, scenario).await?;
            return Ok((results, self.page_elapsed(page_started)));
        }
        let mut results = super::pages::run(context, page_projection_enabled).await;
        let page_elapsed = self.page_elapsed(page_started);
        results.extend(super::memories::run(context, memory_config).await);
        Ok((results, page_elapsed))
    }

    fn page_elapsed(&self, page_started: std::time::Duration) -> std::time::Duration {
        #[cfg(test)]
        if self.scenario == Some(TestScenario::PageGroupTimeout) {
            return ExecutionGate::PAGE_BUDGET + std::time::Duration::from_millis(1);
        }
        self.clock.elapsed().saturating_sub(page_started)
    }
}

async fn validate_scope(
    snapshot: &super::snapshot::LintReadSnapshot<'_>,
    query: &LintQuery,
) -> Result<AppliedScope, LintRunError> {
    let Some(requested) = query.space.as_deref() else {
        return Ok(AppliedScope::global());
    };
    if requested == "uncategorized" {
        return Ok(AppliedScope::uncategorized());
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
    Ok(AppliedScope::registered(opaque, requested.to_string()))
}

#[cfg(test)]
async fn run_test_scenario(
    context: &LintContext<'_, '_>,
    scenario: TestScenario,
) -> Result<Vec<LintCheckResult>, LintRunError> {
    match scenario {
        TestScenario::MixedQueryFailure => {
            record_zero_populations(context)?;
            let failed = match context
                .snapshot()
                .query(
                    "SELECT CANARY_RAW_QUERY_ERROR_7f31 FROM pages",
                    libsql::params::Params::None,
                )
                .await
            {
                Err(_) => true,
                Ok(mut rows) => rows.next().await.is_err(),
            };
            if !failed {
                return Err(LintRunError::CatalogMismatch);
            }
            catalog()
                .iter()
                .enumerate()
                .map(|(index, entry)| match index {
                    0 => make_result(
                        entry.id,
                        LintOutcome::Pass,
                        LintSeverity::Info,
                        LintApplicability::Applicable,
                        LintPrecondition::Ready,
                        LintSummaryCode::CheckPassed,
                        None,
                        context.clock().duration_ms(),
                    ),
                    1 => make_result(
                        entry.id,
                        LintOutcome::Finding,
                        LintSeverity::Warning,
                        LintApplicability::Applicable,
                        LintPrecondition::Ready,
                        LintSummaryCode::FindingDetected,
                        Some(LintRecommendationCode::ReviewFinding),
                        context.clock().duration_ms(),
                    ),
                    2 => make_result(
                        entry.id,
                        LintOutcome::FailedToRun,
                        LintSeverity::Error,
                        LintApplicability::Applicable,
                        LintPrecondition::Ready,
                        LintSummaryCode::ExecutionFailed,
                        Some(LintRecommendationCode::InspectRuntime),
                        context.clock().duration_ms(),
                    ),
                    _ => make_result(
                        entry.id,
                        LintOutcome::Pass,
                        LintSeverity::Info,
                        LintApplicability::Applicable,
                        LintPrecondition::Ready,
                        LintSummaryCode::CheckPassed,
                        None,
                        context.clock().duration_ms(),
                    ),
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(LintRunError::from)
        }
        TestScenario::PageGroupTimeout => {
            let mut population = super::context::PopulationAccounting::new(101);
            for ordinal in 1..=100 {
                population.validate(ordinal, true);
            }
            if population.coverage().is_ok() {
                return Err(LintRunError::CatalogMismatch);
            }
            catalog()
                .iter()
                .map(|entry| {
                    context
                        .record_population(entry.id, PopulationBasis::Global, 101)
                        .map_err(|_| LintRunError::CatalogMismatch)?;
                    make_result_with_denominator(
                        entry.id,
                        LintOutcome::Pass,
                        LintSeverity::Info,
                        LintApplicability::Applicable,
                        LintPrecondition::Ready,
                        LintSummaryCode::CheckPassed,
                        None,
                        context.clock().duration_ms(),
                        101,
                    )
                    .map_err(LintRunError::from)
                })
                .collect()
        }
        TestScenario::ScopedDenominators => scoped_denominator_results(context),
    }
}

#[cfg(test)]
fn scoped_denominator_results(
    context: &LintContext<'_, '_>,
) -> Result<Vec<LintCheckResult>, LintRunError> {
    if !context.scope().filter().is_selected() {
        return Err(LintRunError::CatalogMismatch);
    }
    catalog()
        .iter()
        .map(|entry| {
            let selected_policy = matches!(
                entry.scope_policy,
                super::catalog::ScopePolicy::ScopedRows
                    | super::catalog::ScopePolicy::DbAnchoredProjection
            );
            let (basis, denominator) = if selected_policy {
                (PopulationBasis::SelectedScope, 3)
            } else {
                (PopulationBasis::Global, 9)
            };
            context
                .record_population(entry.id, basis, denominator)
                .map_err(|_| LintRunError::CatalogMismatch)?;
            make_result_with_denominator(
                entry.id,
                LintOutcome::Pass,
                LintSeverity::Info,
                LintApplicability::Applicable,
                LintPrecondition::Ready,
                LintSummaryCode::CheckPassed,
                None,
                context.clock().duration_ms(),
                denominator,
            )
            .map_err(LintRunError::from)
        })
        .collect()
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

pub(crate) fn configured_off_results_for_group(
    clock: &LintClock,
    group: LintCheckGroup,
) -> Vec<LintCheckResult> {
    configured_off_results(clock.clone())
        .into_iter()
        .filter(|result| catalog_entry(result.check_id()).is_some_and(|entry| entry.group == group))
        .collect()
}

pub(crate) fn prerequisite_results_for_group(
    clock: &LintClock,
    group: LintCheckGroup,
) -> Vec<LintCheckResult> {
    terminal_results_for_group(
        clock,
        group,
        LintOutcome::NotRunPrerequisite,
        LintPrecondition::MissingPrerequisite,
        LintSummaryCode::PrerequisiteUnavailable,
        LintRecommendationCode::RestorePrerequisite,
    )
}

pub(crate) fn failed_results(clock: &LintClock) -> Vec<LintCheckResult> {
    terminal_results(
        clock,
        LintOutcome::FailedToRun,
        LintPrecondition::Ready,
        LintSummaryCode::ExecutionFailed,
        LintRecommendationCode::InspectRuntime,
    )
}

pub(crate) fn failed_results_for_group(
    clock: &LintClock,
    group: LintCheckGroup,
) -> Vec<LintCheckResult> {
    terminal_results_for_group(
        clock,
        group,
        LintOutcome::FailedToRun,
        LintPrecondition::Ready,
        LintSummaryCode::ExecutionFailed,
        LintRecommendationCode::InspectRuntime,
    )
}

fn failed_results_from_checks(
    clock: &LintClock,
    checks: &[LintCheckResult],
) -> Result<Vec<LintCheckResult>, LintRunError> {
    terminal_results_from_checks(
        clock,
        checks,
        LintOutcome::FailedToRun,
        LintPrecondition::Ready,
        LintSummaryCode::ExecutionFailed,
        LintRecommendationCode::InspectRuntime,
    )
}

fn inconsistent_selected_results(
    clock: &LintClock,
    checks: &[LintCheckResult],
    affected: impl Fn(&str) -> bool,
) -> Result<Vec<LintCheckResult>, LintRunError> {
    checks
        .iter()
        .map(|check| {
            if affected(check.check_id()) {
                make_result_with_denominator(
                    check.check_id(),
                    LintOutcome::InconsistentSnapshot,
                    LintSeverity::Error,
                    LintApplicability::Applicable,
                    LintPrecondition::SnapshotUnstable,
                    LintSummaryCode::SnapshotInconsistent,
                    Some(LintRecommendationCode::RerunAfterSnapshotStabilizes),
                    clock.duration_ms(),
                    check.coverage().denominator(),
                )
                .map_err(LintRunError::from)
            } else {
                Ok(check.clone())
            }
        })
        .collect()
}

fn failed_selected_results(
    clock: &LintClock,
    checks: &[LintCheckResult],
    affected: impl Fn(&str) -> bool,
) -> Result<Vec<LintCheckResult>, LintRunError> {
    checks
        .iter()
        .map(|check| {
            if affected(check.check_id()) {
                make_result_with_denominator(
                    check.check_id(),
                    LintOutcome::FailedToRun,
                    LintSeverity::Error,
                    LintApplicability::Applicable,
                    LintPrecondition::Ready,
                    LintSummaryCode::ExecutionFailed,
                    Some(LintRecommendationCode::InspectRuntime),
                    clock.duration_ms(),
                    check.coverage().denominator(),
                )
                .map_err(LintRunError::from)
            } else {
                Ok(check.clone())
            }
        })
        .collect()
}

fn terminal_results_from_checks(
    clock: &LintClock,
    checks: &[LintCheckResult],
    outcome: LintOutcome,
    precondition: LintPrecondition,
    summary: LintSummaryCode,
    recommendation: LintRecommendationCode,
) -> Result<Vec<LintCheckResult>, LintRunError> {
    catalog()
        .iter()
        .map(|entry| {
            let denominator = checks
                .iter()
                .find(|check| check.check_id() == entry.id)
                .ok_or(LintRunError::CatalogMismatch)?
                .coverage()
                .denominator();
            make_result_with_denominator(
                entry.id,
                outcome,
                LintSeverity::Error,
                LintApplicability::Applicable,
                precondition,
                summary,
                Some(recommendation),
                clock.duration_ms(),
                denominator,
            )
            .map_err(LintRunError::from)
        })
        .collect()
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

fn terminal_results_for_group(
    clock: &LintClock,
    group: LintCheckGroup,
    outcome: LintOutcome,
    precondition: LintPrecondition,
    summary: LintSummaryCode,
    recommendation: LintRecommendationCode,
) -> Vec<LintCheckResult> {
    let applicability = if outcome == LintOutcome::NotRunPrerequisite {
        LintApplicability::NotApplicable
    } else {
        LintApplicability::Applicable
    };
    catalog_group(group)
        .map(|entry| {
            make_result(
                entry.id,
                outcome,
                LintSeverity::Error,
                applicability,
                precondition,
                summary,
                Some(recommendation),
                clock.duration_ms(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .expect("static group lint results are valid")
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
    make_result_with_denominator(
        check_id,
        outcome,
        severity,
        applicability,
        precondition,
        summary_code,
        recommendation_code,
        duration_ms,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
fn make_result_with_denominator(
    check_id: &str,
    outcome: LintOutcome,
    severity: LintSeverity,
    applicability: LintApplicability,
    precondition: LintPrecondition,
    summary_code: LintSummaryCode,
    recommendation_code: Option<LintRecommendationCode>,
    duration_ms: u64,
    denominator: u64,
) -> Result<LintCheckResult, LintContractError> {
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: check_id.to_string(),
        outcome,
        severity,
        applicability,
        precondition,
        coverage: LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            denominator,
            denominator,
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

fn validate_scope_policy(
    context: &LintContext<'_, '_>,
    checks: &[LintCheckResult],
) -> Result<(), LintRunError> {
    for check in checks {
        let entry =
            super::catalog::catalog_entry(check.check_id()).ok_or(LintRunError::CatalogMismatch)?;
        let receipt = context
            .population(check.check_id())
            .map_err(|_| LintRunError::CatalogMismatch)?
            .ok_or(LintRunError::CatalogMismatch)?;
        let selected = context.scope().filter().is_selected();
        let expected_basis = if selected
            && matches!(
                entry.scope_policy,
                super::catalog::ScopePolicy::ScopedRows
                    | super::catalog::ScopePolicy::DbAnchoredProjection
            ) {
            PopulationBasis::SelectedScope
        } else {
            PopulationBasis::Global
        };
        let global_policy = matches!(
            entry.scope_policy,
            super::catalog::ScopePolicy::GlobalAggregateOnly
                | super::catalog::ScopePolicy::GlobalOnly
        );
        if receipt.basis != expected_basis
            || receipt.denominator != check.coverage().denominator()
            || (selected && global_policy && !check.evidence().is_empty())
        {
            return Err(LintRunError::CatalogMismatch);
        }
    }
    Ok(())
}

fn record_zero_populations(context: &LintContext<'_, '_>) -> Result<(), LintRunError> {
    let selected = context.scope().filter().is_selected();
    for entry in catalog() {
        let basis = if selected
            && matches!(
                entry.scope_policy,
                super::catalog::ScopePolicy::ScopedRows
                    | super::catalog::ScopePolicy::DbAnchoredProjection
            ) {
            PopulationBasis::SelectedScope
        } else {
            PopulationBasis::Global
        };
        context
            .record_population(entry.id, basis, 0)
            .map_err(|_| LintRunError::CatalogMismatch)?;
    }
    Ok(())
}

fn build_report(
    scope: LintScope,
    page_projection_enabled: bool,
    db_receipt: SnapshotReceipt,
    page_before: [u8; 32],
    page_after: [u8; 32],
    memory_config: super::memories::MemoryFeatureConfig,
    checks: Vec<LintCheckResult>,
) -> Result<LintReport, LintRunError> {
    LintReport::try_new(
        scope,
        LintCapabilityContext::daemon_operator_endpoint(),
        receipts(db_receipt, page_before, page_after),
        LintConfigFingerprint::from_effective_config(&[
            config_selection(
                LintConfigSetting::PageProjectionEnabled,
                page_projection_enabled,
            ),
            config_selection(
                LintConfigSetting::EpisodeChannelEnabled,
                memory_config.episode,
            ),
            config_selection(LintConfigSetting::FactChannelEnabled, memory_config.fact),
            config_selection(
                LintConfigSetting::SummaryPreludeEnabled,
                memory_config.summary,
            ),
            config_selection(
                LintConfigSetting::TemporalGroundingEnabled,
                memory_config.temporal,
            ),
        ]),
        LintProducerReceipt::new(None),
        checks,
    )
    .map_err(LintRunError::from)
}

fn config_selection(setting: LintConfigSetting, enabled: bool) -> LintConfigSelection {
    LintConfigSelection::new(
        setting,
        if enabled {
            LintConfigValue::Enabled
        } else {
            LintConfigValue::Disabled
        },
    )
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
