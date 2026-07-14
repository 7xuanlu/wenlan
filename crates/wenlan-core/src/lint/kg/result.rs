use super::config::KgRunConfig;
use super::query::RowCheck;
use crate::lint::context::{LintContext, PopulationBasis, PopulationLedgerError};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintContractError, LintCoverage,
    LintEvidenceRef, LintMetric, LintMetricCode, LintMetricStringCode, LintMetricValue,
    LintOpaqueId, LintOutcome, LintPrecondition, LintRecommendationCode, LintSeverity,
    LintSummaryCode, LintValidationMethod, LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) struct Assessment {
    id: &'static str,
    population: u64,
    affected: u64,
    severity: LintSeverity,
    applicability: LintApplicability,
    precondition: LintPrecondition,
    metrics: Vec<LintMetric>,
    evidence_positions: Vec<usize>,
    method: LintValidationMethod,
    basis: PopulationBasis,
}

impl Assessment {
    pub(super) fn structural(id: &'static str, rows: RowCheck) -> Self {
        Self {
            id,
            population: rows.population,
            affected: rows.affected,
            severity: LintSeverity::Error,
            applicability: LintApplicability::Applicable,
            precondition: LintPrecondition::Ready,
            metrics: base_metrics(rows.population, rows.affected),
            evidence_positions: rows.evidence_positions,
            method: LintValidationMethod::FullEnumeration,
            basis: PopulationBasis::SelectedScope,
        }
    }

    pub(super) fn inventory(id: &'static str, population: u64, metrics: Vec<LintMetric>) -> Self {
        Self::inventory_with_basis(id, population, metrics, PopulationBasis::SelectedScope)
    }

    pub(super) fn global_inventory(
        id: &'static str,
        population: u64,
        metrics: Vec<LintMetric>,
    ) -> Self {
        Self::inventory_with_basis(id, population, metrics, PopulationBasis::Global)
    }

    fn inventory_with_basis(
        id: &'static str,
        population: u64,
        metrics: Vec<LintMetric>,
        basis: PopulationBasis,
    ) -> Self {
        Self {
            id,
            population,
            affected: 0,
            severity: LintSeverity::Info,
            applicability: LintApplicability::Inventory,
            precondition: LintPrecondition::Ready,
            metrics,
            evidence_positions: Vec::new(),
            method: LintValidationMethod::ExactAggregate,
            basis,
        }
    }

    pub(super) fn liveness(config: KgRunConfig, eligible: u64, linked: u64) -> Self {
        let affected = if config.serving_enabled && eligible > 0 && linked == 0 {
            eligible
        } else {
            0
        };
        Self {
            id: super::LIVENESS,
            population: eligible,
            affected,
            severity: LintSeverity::Warning,
            applicability: if config.serving_enabled {
                LintApplicability::Applicable
            } else {
                LintApplicability::ExpectedEmpty
            },
            precondition: if config.serving_enabled {
                LintPrecondition::Ready
            } else {
                LintPrecondition::ConfiguredOff
            },
            metrics: vec![
                metric(LintMetricCode::EligibleRecords, eligible),
                metric(LintMetricCode::ObservedRecords, linked),
                metric(LintMetricCode::AffectedRecords, affected),
                status_metric(LintMetricCode::KgServingStatus, config.serving_enabled),
                status_metric(LintMetricCode::KgSweepStatus, config.sweep_enabled),
                LintMetric::new(
                    LintMetricCode::KgProviderReadiness,
                    LintMetricValue::CatalogCode {
                        code: if config.provider_ready {
                            LintMetricStringCode::Ready
                        } else {
                            LintMetricStringCode::Missing
                        },
                    },
                ),
            ],
            evidence_positions: Vec::new(),
            method: LintValidationMethod::ExactAggregate,
            basis: PopulationBasis::SelectedScope,
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
    let basis = if context.scope().filter().is_selected() {
        assessment.basis
    } else {
        PopulationBasis::Global
    };
    let evidence = assessment
        .evidence_positions
        .iter()
        .filter_map(|position| LintOpaqueId::from_sorted_position(*position))
        .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
        .collect::<Vec<_>>();
    let finding = assessment.affected > 0;
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
        applicability: assessment.applicability,
        precondition: assessment.precondition,
        coverage: LintCoverage::new(
            assessment.method,
            assessment.population,
            assessment.population,
            LINT_MAX_EVIDENCE_PER_CHECK,
            assessment.affected > u64::try_from(evidence.len()).unwrap_or(u64::MAX),
            u64::try_from(evidence.len()).unwrap_or(u64::MAX),
        )?,
        metrics: assessment.metrics,
        summary_code: if finding {
            LintSummaryCode::FindingDetected
        } else if assessment.precondition == LintPrecondition::ConfiguredOff {
            LintSummaryCode::ExpectedEmpty
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

fn base_metrics(eligible: u64, affected: u64) -> Vec<LintMetric> {
    vec![
        metric(LintMetricCode::EligibleRecords, eligible),
        metric(LintMetricCode::AffectedRecords, affected),
    ]
}

fn metric(code: LintMetricCode, value: u64) -> LintMetric {
    LintMetric::new(code, LintMetricValue::Count { value })
}

pub(super) fn status_metric(code: LintMetricCode, enabled: bool) -> LintMetric {
    LintMetric::new(
        code,
        LintMetricValue::CatalogCode {
            code: if enabled {
                LintMetricStringCode::Enabled
            } else {
                LintMetricStringCode::Disabled
            },
        },
    )
}
