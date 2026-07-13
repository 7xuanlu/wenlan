use crate::lint::context::{LintContext, PopulationBasis};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintCoverage, LintMetric,
    LintMetricCode, LintMetricValue, LintOutcome, LintPrecondition, LintSeverity, LintSummaryCode,
    LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(crate) fn inventory(
    context: &LintContext<'_, '_>,
    id: &'static str,
    values: Vec<(LintMetricCode, u64)>,
) -> LintCheckResult {
    let population = values.iter().map(|(_, value)| *value).sum();
    let _ = context.record_population(id, PopulationBasis::Global, population);
    let metrics = values
        .into_iter()
        .map(|(code, value)| LintMetric::new(code, LintMetricValue::Count { value }))
        .collect();
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: id.to_string(),
        outcome: LintOutcome::Pass,
        severity: LintSeverity::Info,
        applicability: LintApplicability::Inventory,
        precondition: LintPrecondition::Ready,
        coverage: LintCoverage::new(
            LintValidationMethod::ExactAggregate,
            population,
            population,
            LINT_MAX_EVIDENCE_PER_CHECK,
            false,
            0,
        )
        .expect("valid inventory coverage"),
        metrics,
        summary_code: LintSummaryCode::CheckPassed,
        recommendation_code: None,
        evidence: Vec::new(),
        duration_ms: context.clock().duration_ms(),
    })
    .expect("valid inventory result")
}
