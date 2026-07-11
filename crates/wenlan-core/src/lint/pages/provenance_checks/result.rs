use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintCoverage, LintEvidenceRef,
    LintMetric, LintOpaqueId, LintOutcome, LintPrecondition, LintRecommendationCode, LintSeverity,
    LintSummaryCode, LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Level {
    Pass,
    Warning,
    Error,
}

#[derive(Debug, Default)]
pub(super) struct Assessment {
    rows: Vec<Level>,
    inventory: bool,
    metrics: Vec<LintMetric>,
}

impl Assessment {
    pub(super) fn push(&mut self, level: Level) {
        self.rows.push(level);
    }

    pub(super) fn mark_inventory(&mut self) {
        self.inventory = true;
    }

    pub(super) fn set_metrics(&mut self, metrics: Vec<LintMetric>) {
        self.metrics = metrics;
    }

    pub(super) fn population(&self) -> u64 {
        u64::try_from(self.rows.len()).unwrap_or(u64::MAX)
    }

    pub(super) fn result(
        self,
        check_id: &str,
        duration_ms: u64,
    ) -> Result<LintCheckResult, wenlan_types::lint::LintContractError> {
        let level = self.rows.iter().copied().max().unwrap_or(Level::Pass);
        let defect_positions = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, level)| **level != Level::Pass)
            .collect::<Vec<_>>();
        let evidence = defect_positions
            .iter()
            .take(usize::from(LINT_MAX_EVIDENCE_PER_CHECK))
            .filter_map(|(position, _)| {
                LintOpaqueId::from_sorted_position(*position)
                    .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
            })
            .collect::<Vec<_>>();
        let population = self.population();
        let (outcome, severity, applicability, summary_code, recommendation_code) = match level {
            Level::Pass => (
                LintOutcome::Pass,
                LintSeverity::Info,
                if self.inventory || self.rows.is_empty() {
                    LintApplicability::Inventory
                } else {
                    LintApplicability::Applicable
                },
                LintSummaryCode::CheckPassed,
                None,
            ),
            Level::Warning => (
                LintOutcome::Finding,
                LintSeverity::Warning,
                LintApplicability::Applicable,
                LintSummaryCode::FindingDetected,
                Some(LintRecommendationCode::ReviewFinding),
            ),
            Level::Error => (
                LintOutcome::Finding,
                LintSeverity::Error,
                LintApplicability::Applicable,
                LintSummaryCode::FindingDetected,
                Some(LintRecommendationCode::ReviewFinding),
            ),
        };
        LintCheckResult::try_new(LintCheckResultInput {
            check_id: check_id.to_string(),
            outcome,
            severity,
            applicability,
            precondition: LintPrecondition::Ready,
            coverage: LintCoverage::new(
                LintValidationMethod::FullEnumeration,
                population,
                population,
                LINT_MAX_EVIDENCE_PER_CHECK,
                defect_positions.len() > usize::from(LINT_MAX_EVIDENCE_PER_CHECK),
                u64::try_from(evidence.len()).unwrap_or(u64::MAX),
            )?,
            metrics: self.metrics,
            summary_code,
            recommendation_code,
            evidence,
            duration_ms,
        })
    }
}

pub(super) fn failed_result(check_id: &str, duration_ms: u64) -> LintCheckResult {
    LintCheckResult::try_new(LintCheckResultInput {
        check_id: check_id.to_string(),
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
        )
        .expect("static failed coverage is valid"),
        metrics: Vec::new(),
        summary_code: LintSummaryCode::ExecutionFailed,
        recommendation_code: Some(LintRecommendationCode::InspectRuntime),
        evidence: Vec::new(),
        duration_ms,
    })
    .expect("static failed result is valid")
}
