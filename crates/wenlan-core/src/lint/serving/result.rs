use super::{ChannelAssessment, OBSERVABILITY_ID, RERANKER_ID, ROUTE_SCOPE_ID};
use crate::lint::context::{LintClock, LintContext, PopulationBasis};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintCoverage, LintEvidenceRef,
    LintMetric, LintMetricCode, LintMetricValue, LintOpaqueId, LintOutcome, LintPrecondition,
    LintRecommendationCode, LintSeverity, LintSummaryCode, LintValidationMethod,
    LINT_MAX_EVIDENCE_PER_CHECK,
};

mod fact;
mod inventory;
pub(crate) use fact::fact_probe;
pub(crate) use inventory::inventory;

pub(super) fn channel(
    context: &LintContext<'_, '_>,
    id: &'static str,
    assessment: ChannelAssessment,
) -> LintCheckResult {
    let (eligible, observed, finding, off) = parts(assessment);
    let _ = context.record_population(id, selected_basis(context), eligible);
    build(
        context.clock(),
        id,
        ResultSpec::new(eligible, observed, finding, off, false, finding),
    )
}

#[cfg(test)]
pub(super) fn channel_for_test(
    clock: &LintClock,
    id: &'static str,
    assessment: ChannelAssessment,
) -> LintCheckResult {
    let (eligible, observed, finding, off) = parts(assessment);
    build(
        clock,
        id,
        ResultSpec::new(eligible, observed, finding, off, false, finding),
    )
}

fn parts(assessment: ChannelAssessment) -> (u64, u64, bool, bool) {
    match assessment {
        ChannelAssessment::ExpectedEmpty { eligible } => (eligible, 0, false, true),
        ChannelAssessment::Finding { eligible } => (eligible, 0, true, false),
        ChannelAssessment::Live { eligible, observed } => (eligible, observed, false, false),
    }
}

pub(super) fn fixed(
    context: &LintContext<'_, '_>,
    id: &'static str,
    population: u64,
    finding: bool,
    inventory: bool,
) -> LintCheckResult {
    let basis = if matches!(id, ROUTE_SCOPE_ID | OBSERVABILITY_ID | RERANKER_ID) {
        PopulationBasis::Global
    } else {
        selected_basis(context)
    };
    let _ = context.record_population(id, basis, population);
    build(
        context.clock(),
        id,
        ResultSpec::new(
            population,
            if finding { 0 } else { population },
            finding,
            false,
            inventory,
            finding && !matches!(id, ROUTE_SCOPE_ID | OBSERVABILITY_ID | RERANKER_ID),
        ),
    )
}

fn selected_basis(context: &LintContext<'_, '_>) -> PopulationBasis {
    if context.scope().filter().is_selected() {
        PopulationBasis::SelectedScope
    } else {
        PopulationBasis::Global
    }
}

struct ResultSpec {
    eligible: u64,
    observed: u64,
    finding: bool,
    off: bool,
    inventory: bool,
    evidence_allowed: bool,
}

impl ResultSpec {
    const fn new(
        eligible: u64,
        observed: u64,
        finding: bool,
        off: bool,
        inventory: bool,
        evidence_allowed: bool,
    ) -> Self {
        Self {
            eligible,
            observed,
            finding,
            off,
            inventory,
            evidence_allowed,
        }
    }
}

fn build(clock: &LintClock, id: &'static str, spec: ResultSpec) -> LintCheckResult {
    let ResultSpec {
        eligible,
        observed,
        finding,
        off,
        inventory,
        evidence_allowed,
    } = spec;
    let evidence_cap = if evidence_allowed {
        eligible.min(u64::from(LINT_MAX_EVIDENCE_PER_CHECK))
    } else {
        0
    };
    let evidence = (0..evidence_cap)
        .filter_map(|i| {
            usize::try_from(i)
                .ok()
                .and_then(LintOpaqueId::from_sorted_position)
        })
        .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
        .collect();
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: id.to_string(),
        outcome: if finding {
            LintOutcome::Finding
        } else {
            LintOutcome::Pass
        },
        severity: if finding {
            LintSeverity::Error
        } else {
            LintSeverity::Info
        },
        applicability: if off {
            LintApplicability::ExpectedEmpty
        } else if inventory {
            LintApplicability::Inventory
        } else {
            LintApplicability::Applicable
        },
        precondition: if off {
            LintPrecondition::ConfiguredOff
        } else {
            LintPrecondition::Ready
        },
        coverage: LintCoverage::new(
            LintValidationMethod::ExactAggregate,
            eligible,
            eligible,
            LINT_MAX_EVIDENCE_PER_CHECK,
            evidence_allowed && eligible > u64::from(LINT_MAX_EVIDENCE_PER_CHECK),
            evidence_cap,
        )
        .unwrap(),
        metrics: vec![
            LintMetric::new(
                LintMetricCode::EligibleRecords,
                LintMetricValue::Count { value: eligible },
            ),
            LintMetric::new(
                LintMetricCode::ObservedRecords,
                LintMetricValue::Count { value: observed },
            ),
            LintMetric::new(
                LintMetricCode::AffectedRecords,
                LintMetricValue::Count {
                    value: eligible.saturating_sub(observed),
                },
            ),
        ],
        summary_code: if finding {
            LintSummaryCode::FindingDetected
        } else if off {
            LintSummaryCode::ExpectedEmpty
        } else {
            LintSummaryCode::CheckPassed
        },
        recommendation_code: finding.then_some(LintRecommendationCode::ReviewFinding),
        evidence,
        duration_ms: clock.duration_ms(),
    })
    .unwrap()
}
