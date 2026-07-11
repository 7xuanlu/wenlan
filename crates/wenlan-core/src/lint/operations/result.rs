use crate::lint::context::{LintContext, PopulationBasis, PopulationLedgerError};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintContractError, LintCoverage,
    LintEvidenceRef, LintMetric, LintOpaqueId, LintOutcome, LintPrecondition, LintReasonCode,
    LintRecommendationCode, LintSeverity, LintSummaryCode, LintValidationMethod,
    LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) struct Assessment {
    pub(super) id: &'static str,
    pub(super) population: u64,
    pub(super) affected: u64,
    pub(super) severity: LintSeverity,
    pub(super) metrics: Vec<LintMetric>,
    pub(super) evidence_positions: Vec<usize>,
    pub(super) reason_codes: Vec<LintReasonCode>,
}

impl Assessment {
    pub(super) fn inventory(
        id: &'static str,
        population: u64,
        affected: u64,
        severity: LintSeverity,
        metrics: Vec<LintMetric>,
        evidence_positions: Vec<usize>,
        reason_codes: Vec<LintReasonCode>,
    ) -> Self {
        Self {
            id,
            population,
            affected,
            severity,
            metrics,
            evidence_positions,
            reason_codes,
        }
    }
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
    let evidence = if context.scope().filter().is_selected() {
        Vec::new()
    } else {
        evidence(&assessment)
    };
    let opaque_count = evidence
        .iter()
        .filter(|item| matches!(item, LintEvidenceRef::OpaqueId { .. }))
        .count();
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
            usize::try_from(assessment.affected).unwrap_or(usize::MAX) > opaque_count,
            u64::try_from(evidence.len()).unwrap_or(u64::MAX),
        )?,
        metrics: assessment.metrics,
        summary_code: if finding {
            LintSummaryCode::FindingDetected
        } else {
            LintSummaryCode::CheckPassed
        },
        recommendation_code: finding.then_some(LintRecommendationCode::ReviewFinding),
        evidence,
        duration_ms: context.clock().duration_ms(),
    })?;
    context.record_population(
        assessment.id,
        PopulationBasis::Global,
        assessment.population,
    )?;
    Ok(result)
}

fn evidence(assessment: &Assessment) -> Vec<LintEvidenceRef> {
    let cap = usize::from(LINT_MAX_EVIDENCE_PER_CHECK);
    let mut evidence = assessment
        .reason_codes
        .iter()
        .take(cap)
        .copied()
        .map(|reason_code| LintEvidenceRef::ReasonCode { reason_code })
        .collect::<Vec<_>>();
    let remaining = cap.saturating_sub(evidence.len());
    evidence.extend(
        assessment
            .evidence_positions
            .iter()
            .take(remaining)
            .filter_map(|position| LintOpaqueId::from_sorted_position(*position))
            .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id }),
    );
    evidence
}
