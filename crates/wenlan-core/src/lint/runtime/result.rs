use crate::lint::context::{LintContext, PopulationBasis, PopulationLedgerError};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintContractError, LintCoverage,
    LintMetric, LintMetricCode, LintMetricValue, LintOutcome, LintPrecondition,
    LintRecommendationCode, LintSeverity, LintSummaryCode, LintValidationMethod,
    LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) struct Assessment {
    id: &'static str,
    population: u64,
    affected: u64,
    observed: Option<u64>,
    metrics: Vec<LintMetric>,
}

pub(super) enum PendingAssessment {
    Ready(Assessment),
    ExpectedEmpty(Assessment),
    Failed(&'static str),
}

impl Assessment {
    pub(super) const fn new(id: &'static str, population: u64, affected: u64) -> Self {
        Self {
            id,
            population,
            affected,
            observed: None,
            metrics: Vec::new(),
        }
    }

    pub(super) fn with_observed(mut self, observed: u64) -> Self {
        self.observed = Some(observed);
        self
    }

    pub(super) fn with_metric(mut self, code: LintMetricCode, value: u64) -> Self {
        self.metrics.push(metric(code, value));
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum BuildError {
    #[error(transparent)]
    Contract(#[from] LintContractError),
    #[error(transparent)]
    Population(#[from] PopulationLedgerError),
}

pub(super) fn finish_pending(
    context: &LintContext<'_, '_>,
    pending: PendingAssessment,
) -> Result<LintCheckResult, BuildError> {
    match pending {
        PendingAssessment::Ready(assessment) => finish(context, assessment),
        PendingAssessment::ExpectedEmpty(assessment) => expected_empty(context, assessment),
        PendingAssessment::Failed(id) => failed(context, id),
    }
}

pub(super) fn finish(
    context: &LintContext<'_, '_>,
    assessment: Assessment,
) -> Result<LintCheckResult, BuildError> {
    let finding = assessment.affected > 0;
    let mut metrics = assessment.metrics;
    metrics.push(metric(LintMetricCode::AffectedRecords, assessment.affected));
    if let Some(observed) = assessment.observed {
        metrics.push(metric(LintMetricCode::ObservedRecords, observed));
    }
    let result = LintCheckResult::try_new(LintCheckResultInput {
        check_id: assessment.id.to_string(),
        outcome: if finding {
            LintOutcome::Finding
        } else {
            LintOutcome::Pass
        },
        severity: if finding {
            LintSeverity::Warning
        } else {
            LintSeverity::Info
        },
        applicability: if finding {
            LintApplicability::Applicable
        } else {
            LintApplicability::Inventory
        },
        precondition: LintPrecondition::Ready,
        coverage: LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            assessment.population,
            assessment.population,
            LINT_MAX_EVIDENCE_PER_CHECK,
            false,
            0,
        )?,
        metrics,
        summary_code: if finding {
            LintSummaryCode::FindingDetected
        } else {
            LintSummaryCode::CheckPassed
        },
        recommendation_code: finding.then_some(LintRecommendationCode::InspectRuntime),
        evidence: Vec::new(),
        duration_ms: context.clock().duration_ms(),
    })?;
    context.record_population(
        assessment.id,
        PopulationBasis::Global,
        assessment.population,
    )?;
    Ok(result)
}

fn expected_empty(
    context: &LintContext<'_, '_>,
    assessment: Assessment,
) -> Result<LintCheckResult, BuildError> {
    let mut metrics = assessment.metrics;
    metrics.push(metric(LintMetricCode::AffectedRecords, 0));
    if let Some(observed) = assessment.observed {
        metrics.push(metric(LintMetricCode::ObservedRecords, observed));
    }
    let result = LintCheckResult::try_new(LintCheckResultInput {
        check_id: assessment.id.to_string(),
        outcome: LintOutcome::Pass,
        severity: LintSeverity::Info,
        applicability: LintApplicability::ExpectedEmpty,
        precondition: LintPrecondition::ConfiguredOff,
        coverage: LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            assessment.population,
            assessment.population,
            LINT_MAX_EVIDENCE_PER_CHECK,
            false,
            0,
        )?,
        metrics,
        summary_code: LintSummaryCode::ExpectedEmpty,
        recommendation_code: None,
        evidence: Vec::new(),
        duration_ms: context.clock().duration_ms(),
    })?;
    context.record_population(
        assessment.id,
        PopulationBasis::Global,
        assessment.population,
    )?;
    Ok(result)
}

fn metric(code: LintMetricCode, value: u64) -> LintMetric {
    LintMetric::new(code, LintMetricValue::Count { value })
}

fn failed(context: &LintContext<'_, '_>, id: &'static str) -> Result<LintCheckResult, BuildError> {
    let result = LintCheckResult::try_new(LintCheckResultInput {
        check_id: id.to_string(),
        outcome: LintOutcome::FailedToRun,
        severity: LintSeverity::Error,
        applicability: LintApplicability::Applicable,
        precondition: LintPrecondition::Ready,
        coverage: LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            0,
            0,
            LINT_MAX_EVIDENCE_PER_CHECK,
            false,
            0,
        )?,
        metrics: Vec::new(),
        summary_code: LintSummaryCode::ExecutionFailed,
        recommendation_code: Some(LintRecommendationCode::InspectRuntime),
        evidence: Vec::new(),
        duration_ms: context.clock().duration_ms(),
    })?;
    context.record_population(id, PopulationBasis::Global, 0)?;
    Ok(result)
}
