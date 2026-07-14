use crate::lint::context::{LintContext, PopulationBasis, PopulationLedgerError};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintContractError, LintCoverage,
    LintEvidenceRef, LintMetric, LintOpaqueId, LintOutcome, LintPrecondition,
    LintRecommendationCode, LintSeverity, LintSummaryCode, LintValidationMethod,
    LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) struct Assessment {
    pub(super) id: &'static str,
    pub(super) population: u64,
    pub(super) affected: u64,
    pub(super) evidence_positions: Vec<usize>,
    pub(super) severity: LintSeverity,
    pub(super) basis: PopulationBasis,
    pub(super) metrics: Vec<LintMetric>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum BuildError {
    #[error(transparent)]
    Contract(#[from] LintContractError),
    #[error(transparent)]
    Population(#[from] PopulationLedgerError),
}

pub(super) fn finish(
    context: &LintContext<'_, '_>,
    assessment: Assessment,
) -> Result<LintCheckResult, BuildError> {
    let finding = assessment.affected > 0;
    let basis = if assessment.basis == PopulationBasis::SelectedScope
        && !context.scope().filter().is_selected()
    {
        PopulationBasis::Global
    } else {
        assessment.basis
    };
    let evidence = if context.scope().filter().is_selected() && basis == PopulationBasis::Global {
        Vec::new()
    } else {
        assessment
            .evidence_positions
            .iter()
            .filter_map(|position| LintOpaqueId::from_sorted_position(*position))
            .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
            .collect()
    };
    let result = LintCheckResult::try_new(LintCheckResultInput {
        check_id: assessment.id.to_string(),
        outcome: if finding {
            LintOutcome::Finding
        } else {
            LintOutcome::Pass
        },
        severity: if finding {
            assessment.severity
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
            assessment.affected > u64::try_from(evidence.len()).unwrap_or(u64::MAX),
            u64::try_from(evidence.len()).unwrap_or(u64::MAX),
        )?,
        metrics: {
            let mut metrics = assessment.metrics;
            metrics.push(LintMetric::new(
                wenlan_types::lint::LintMetricCode::AffectedRecords,
                wenlan_types::lint::LintMetricValue::Count {
                    value: assessment.affected,
                },
            ));
            metrics
        },
        summary_code: if finding {
            LintSummaryCode::FindingDetected
        } else {
            LintSummaryCode::CheckPassed
        },
        recommendation_code: finding.then_some(LintRecommendationCode::ReviewFinding),
        evidence,
        duration_ms: context.clock().duration_ms(),
    })?;
    context.record_population(assessment.id, basis, assessment.population)?;
    Ok(result)
}
