use super::super::{fact_probe::FactProbe, FACT_STARVATION_ID};
use crate::lint::context::{LintContext, PopulationBasis};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintCoverage, LintEvidenceRef,
    LintMetric, LintMetricCode, LintMetricValue, LintOpaqueId, LintOutcome, LintPrecondition,
    LintRecommendationCode, LintSeverity, LintSummaryCode, LintValidationMethod,
    LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(crate) fn fact_probe(context: &LintContext<'_, '_>, probe: FactProbe) -> LintCheckResult {
    let affected = u64::try_from(probe.affected_positions.len()).unwrap_or(u64::MAX);
    let basis = if context.scope().filter().is_selected() {
        PopulationBasis::SelectedScope
    } else {
        PopulationBasis::Global
    };
    let _ = context.record_population(FACT_STARVATION_ID, basis, probe.eligible);
    let evidence = probe
        .affected_positions
        .iter()
        .take(usize::from(LINT_MAX_EVIDENCE_PER_CHECK))
        .filter_map(|position| LintOpaqueId::from_sorted_position(*position))
        .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
        .collect::<Vec<_>>();
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: FACT_STARVATION_ID.to_string(),
        outcome: if affected > 0 {
            LintOutcome::Finding
        } else {
            LintOutcome::Pass
        },
        severity: if affected > 0 {
            LintSeverity::Error
        } else {
            LintSeverity::Info
        },
        applicability: LintApplicability::Applicable,
        precondition: LintPrecondition::Ready,
        coverage: LintCoverage::new(
            LintValidationMethod::ExactAggregate,
            probe.eligible,
            probe.eligible,
            LINT_MAX_EVIDENCE_PER_CHECK,
            probe.affected_positions.len() > usize::from(LINT_MAX_EVIDENCE_PER_CHECK),
            u64::try_from(evidence.len()).unwrap_or(u64::MAX),
        )
        .expect("valid fact probe coverage"),
        metrics: vec![
            LintMetric::new(
                LintMetricCode::EligibleRecords,
                LintMetricValue::Count {
                    value: probe.eligible,
                },
            ),
            LintMetric::new(
                LintMetricCode::ObservedRecords,
                LintMetricValue::Count {
                    value: probe.eligible.saturating_sub(affected),
                },
            ),
            LintMetric::new(
                LintMetricCode::AffectedRecords,
                LintMetricValue::Count { value: affected },
            ),
        ],
        summary_code: if affected > 0 {
            LintSummaryCode::FindingDetected
        } else {
            LintSummaryCode::CheckPassed
        },
        recommendation_code: (affected > 0).then_some(LintRecommendationCode::ReviewFinding),
        evidence,
        duration_ms: context.clock().duration_ms(),
    })
    .expect("valid fact probe result")
}
