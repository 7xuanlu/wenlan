use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintCoverage, LintEvidenceRef,
    LintOpaqueId, LintOutcome, LintPrecondition, LintRecommendationCode, LintSeverity,
    LintSummaryCode, LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Level {
    Pass,
    Warning,
    Error,
    Prerequisite,
}

#[derive(Debug, Clone, Copy)]
struct RowAssessment {
    level: Level,
    inventory: bool,
}

#[derive(Debug, Default)]
pub(super) struct Assessment {
    rows: Vec<RowAssessment>,
}

impl Assessment {
    pub(super) fn push(&mut self, level: Level, inventory: bool) {
        self.rows.push(RowAssessment { level, inventory });
    }

    pub(super) fn population(&self) -> u64 {
        u64::try_from(self.rows.len()).unwrap_or(u64::MAX)
    }

    pub(super) fn result(
        self,
        check_id: &str,
        duration_ms: u64,
    ) -> Result<LintCheckResult, wenlan_types::lint::LintContractError> {
        let level = self
            .rows
            .iter()
            .map(|row| row.level)
            .max()
            .unwrap_or(Level::Pass);
        let inventory = self.rows.is_empty() || self.rows.iter().any(|row| row.inventory);
        let (outcome, severity, applicability, precondition, summary, recommendation) =
            result_state(level, inventory);
        let defect_ordinals = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.level != Level::Pass)
            .collect::<Vec<_>>();
        let evidence = defect_ordinals
            .iter()
            .take(usize::from(LINT_MAX_EVIDENCE_PER_CHECK))
            .filter_map(|(position, _)| {
                LintOpaqueId::from_sorted_position(*position)
                    .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
            })
            .collect::<Vec<_>>();
        let population = self.population();
        LintCheckResult::try_new(LintCheckResultInput {
            check_id: check_id.to_string(),
            outcome,
            severity,
            applicability,
            precondition,
            coverage: LintCoverage::new(
                LintValidationMethod::FullEnumeration,
                population,
                population,
                LINT_MAX_EVIDENCE_PER_CHECK,
                defect_ordinals.len() > usize::from(LINT_MAX_EVIDENCE_PER_CHECK),
                u64::try_from(evidence.len()).unwrap_or(u64::MAX),
            )?,
            metrics: Vec::new(),
            summary_code: summary,
            recommendation_code: recommendation,
            evidence,
            duration_ms,
        })
    }
}

fn result_state(
    level: Level,
    inventory: bool,
) -> (
    LintOutcome,
    LintSeverity,
    LintApplicability,
    LintPrecondition,
    LintSummaryCode,
    Option<LintRecommendationCode>,
) {
    match level {
        Level::Pass => (
            LintOutcome::Pass,
            LintSeverity::Info,
            if inventory {
                LintApplicability::Inventory
            } else {
                LintApplicability::Applicable
            },
            LintPrecondition::Ready,
            LintSummaryCode::CheckPassed,
            None,
        ),
        Level::Warning => (
            LintOutcome::Finding,
            LintSeverity::Warning,
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::FindingDetected,
            Some(LintRecommendationCode::ReviewFinding),
        ),
        Level::Error => (
            LintOutcome::Finding,
            LintSeverity::Error,
            LintApplicability::Applicable,
            LintPrecondition::Ready,
            LintSummaryCode::FindingDetected,
            Some(LintRecommendationCode::ReviewFinding),
        ),
        Level::Prerequisite => (
            LintOutcome::NotRunPrerequisite,
            LintSeverity::Error,
            LintApplicability::NotApplicable,
            LintPrecondition::MissingPrerequisite,
            LintSummaryCode::PrerequisiteUnavailable,
            Some(LintRecommendationCode::RestorePrerequisite),
        ),
    }
}
