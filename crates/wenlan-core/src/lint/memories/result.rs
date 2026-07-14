use super::{CheckAssessment, MemoryRecord, PARTITIONS_ID};
use crate::lint::context::{LintContext, PopulationBasis};
use wenlan_types::lint::{
    LintCheckResult, LintCheckResultInput, LintCoverage, LintEvidenceRef, LintMetric,
    LintMetricCode, LintMetricValue, LintOpaqueId, LintOutcome, LintPrecondition,
    LintRecommendationCode, LintSeverity, LintSummaryCode, LintValidationMethod,
    LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) fn finish(
    context: &LintContext<'_, '_>,
    assessment: CheckAssessment,
) -> LintCheckResult {
    let population = u64::try_from(assessment.levels.len()).unwrap_or(u64::MAX);
    let basis = if context.scope().filter().is_selected() {
        PopulationBasis::SelectedScope
    } else {
        PopulationBasis::Global
    };
    let _ = context.record_population(assessment.id, basis, population);
    let defects = assessment
        .levels
        .iter()
        .enumerate()
        .filter(|(_, level)| **level != LintSeverity::Info)
        .collect::<Vec<_>>();
    let severity = if defects
        .iter()
        .any(|(_, level)| **level == LintSeverity::Error)
    {
        LintSeverity::Error
    } else if defects.is_empty() {
        LintSeverity::Info
    } else {
        LintSeverity::Warning
    };
    let evidence = defects
        .iter()
        .take(usize::from(LINT_MAX_EVIDENCE_PER_CHECK))
        .filter_map(|(position, _)| {
            LintOpaqueId::from_sorted_position(*position)
                .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
        })
        .collect::<Vec<_>>();
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: assessment.id.to_string(),
        outcome: if defects.is_empty() {
            LintOutcome::Pass
        } else {
            LintOutcome::Finding
        },
        severity,
        applicability: assessment.applicability,
        precondition: assessment.precondition,
        coverage: LintCoverage::new(
            LintValidationMethod::FullEnumeration,
            population,
            population,
            LINT_MAX_EVIDENCE_PER_CHECK,
            defects.len() > usize::from(LINT_MAX_EVIDENCE_PER_CHECK),
            u64::try_from(evidence.len()).unwrap_or(u64::MAX),
        )
        .unwrap(),
        metrics: assessment.metrics,
        summary_code: if defects.is_empty() {
            if assessment.precondition == LintPrecondition::ConfiguredOff {
                LintSummaryCode::ExpectedEmpty
            } else {
                LintSummaryCode::CheckPassed
            }
        } else {
            LintSummaryCode::FindingDetected
        },
        recommendation_code: if defects.is_empty() {
            None
        } else {
            Some(LintRecommendationCode::ReviewFinding)
        },
        evidence,
        duration_ms: context.clock().duration_ms(),
    })
    .unwrap()
}

pub(super) fn base_metrics(eligible: usize, affected: usize) -> Vec<LintMetric> {
    vec![
        metric(LintMetricCode::EligibleRecords, eligible),
        metric(LintMetricCode::AffectedRecords, affected),
    ]
}

pub(super) fn partition_assessment(
    heads: &[&MemoryRecord],
    records: &[MemoryRecord],
) -> CheckAssessment {
    let count =
        |predicate: fn(&MemoryRecord) -> bool| heads.iter().filter(|r| predicate(r)).count();
    CheckAssessment {
        id: PARTITIONS_ID,
        levels: vec![LintSeverity::Info; heads.len()],
        applicability: wenlan_types::lint::LintApplicability::Inventory,
        precondition: LintPrecondition::Ready,
        metrics: vec![
            metric(LintMetricCode::EligibleRecords, heads.len()),
            metric(
                LintMetricCode::MemoryClassifiedHeads,
                count(|r| r.classified),
            ),
            metric(
                LintMetricCode::MemoryEventDatedHeads,
                count(|r| r.event_dated),
            ),
            metric(LintMetricCode::MemoryEpisodeHeads, count(|r| r.episode)),
            metric(LintMetricCode::MemoryFactVectorHeads, count(|r| r.fact)),
            metric(
                LintMetricCode::MemoryPageLinkedHeads,
                count(|r| r.page_link),
            ),
            metric(
                LintMetricCode::MemorySummaryLinkedHeads,
                count(|r| r.summary),
            ),
            metric(
                LintMetricCode::MemoryReembedPendingHeads,
                count(|r| r.needs_reembed),
            ),
            metric(
                LintMetricCode::MemoryFailedEnrichmentSteps,
                records.iter().map(|r| r.failed_steps as usize).sum(),
            ),
        ],
    }
}

pub(super) fn metric(code: LintMetricCode, value: usize) -> LintMetric {
    LintMetric::new(
        code,
        LintMetricValue::Count {
            value: u64::try_from(value).unwrap_or(u64::MAX),
        },
    )
}
