mod query;

use crate::lint::catalog::{catalog_group, LintCheckGroup};
use crate::lint::context::{LintContext, PopulationBasis};
use query::{load, KgSnapshot, RowCheck};
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintCheckResultInput, LintCoverage, LintEvidenceRef,
    LintMetric, LintMetricCode, LintMetricValue, LintOpaqueId, LintOutcome, LintPrecondition,
    LintRecommendationCode, LintSeverity, LintSummaryCode, LintValidationMethod,
    LINT_MAX_EVIDENCE_PER_CHECK,
};

const ENTITY_INTEGRITY: &str = "entities.structural_integrity";
const ENTITY_PARTITIONS: &str = "entities.partition_inventory";
const ADVISORY: &str = "kg.advisory_inventory";
const AGGREGATES: &str = "kg.aggregate_inventory";
const LIVENESS: &str = "kg.substrate_liveness";
const LINKS: &str = "memory_entities.integrity";
const OBSERVATIONS: &str = "observations.integrity";
const RELATIONS: &str = "relations.integrity";

#[derive(Debug, Clone, Copy)]
pub(crate) struct KgFeatureConfig {
    pub(crate) enabled: bool,
}

impl KgFeatureConfig {
    pub(crate) fn capture() -> Self {
        Self {
            enabled: crate::db::entity_sweep_enabled(),
        }
    }

    #[cfg(test)]
    pub(crate) const fn for_test(enabled: bool) -> Self {
        Self { enabled }
    }
}

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    config: KgFeatureConfig,
) -> Vec<LintCheckResult> {
    match load(context).await {
        Ok(snapshot) => assessments(snapshot, config)
            .into_iter()
            .map(|assessment| finish(context, assessment))
            .collect(),
        Err(()) => failed_results(context),
    }
}

fn assessments(snapshot: KgSnapshot, config: KgFeatureConfig) -> Vec<Assessment> {
    let KgSnapshot {
        entities,
        observations,
        relations,
        links,
        partitions,
        aggregates,
        advisory,
        eligible_memories,
        linked_memories,
    } = snapshot;
    vec![
        Assessment::inventory(ENTITY_PARTITIONS, entities.population, partitions),
        Assessment::structural(ENTITY_INTEGRITY, entities),
        Assessment::global_inventory(ADVISORY, aggregates.entities, advisory),
        Assessment::global_inventory(AGGREGATES, aggregates.sum(), aggregates.metrics()),
        Assessment::liveness(config.enabled, eligible_memories, linked_memories),
        Assessment::structural(LINKS, links),
        Assessment::structural(OBSERVATIONS, observations),
        Assessment::structural(RELATIONS, relations),
    ]
}

struct Assessment {
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
    fn structural(id: &'static str, rows: RowCheck) -> Self {
        let metrics = base_metrics(rows.population, rows.affected);
        Self {
            id,
            population: rows.population,
            affected: rows.affected,
            severity: LintSeverity::Error,
            applicability: LintApplicability::Applicable,
            precondition: LintPrecondition::Ready,
            metrics,
            evidence_positions: rows.evidence_positions,
            method: LintValidationMethod::FullEnumeration,
            basis: PopulationBasis::SelectedScope,
        }
    }

    fn inventory(id: &'static str, population: u64, metrics: Vec<LintMetric>) -> Self {
        Self::inventory_with_basis(id, population, metrics, PopulationBasis::SelectedScope)
    }

    fn global_inventory(id: &'static str, population: u64, metrics: Vec<LintMetric>) -> Self {
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

    fn liveness(enabled: bool, eligible: u64, linked: u64) -> Self {
        let affected = u64::from(enabled && eligible > 0 && linked == 0);
        Self {
            id: LIVENESS,
            population: eligible,
            affected,
            severity: LintSeverity::Warning,
            applicability: if enabled {
                LintApplicability::Applicable
            } else {
                LintApplicability::ExpectedEmpty
            },
            precondition: if enabled {
                LintPrecondition::Ready
            } else {
                LintPrecondition::ConfiguredOff
            },
            metrics: vec![
                metric(LintMetricCode::EligibleRecords, eligible),
                metric(LintMetricCode::ObservedRecords, linked),
                metric(LintMetricCode::AffectedRecords, affected),
            ],
            evidence_positions: Vec::new(),
            method: LintValidationMethod::ExactAggregate,
            basis: PopulationBasis::SelectedScope,
        }
    }
}

fn finish(context: &LintContext<'_, '_>, assessment: Assessment) -> LintCheckResult {
    let basis = if context.scope().filter().is_selected() {
        assessment.basis
    } else {
        PopulationBasis::Global
    };
    let _ = context.record_population(assessment.id, basis, assessment.population);
    let evidence = assessment
        .evidence_positions
        .iter()
        .filter_map(|position| LintOpaqueId::from_sorted_position(*position))
        .map(|opaque_id| LintEvidenceRef::OpaqueId { opaque_id })
        .collect::<Vec<_>>();
    let finding = assessment.affected > 0;
    LintCheckResult::try_new(LintCheckResultInput {
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
        )
        .unwrap(),
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
    })
    .unwrap()
}

fn failed_results(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    let basis = if context.scope().filter().is_selected() {
        PopulationBasis::SelectedScope
    } else {
        PopulationBasis::Global
    };
    for entry in catalog_group(LintCheckGroup::KnowledgeGraph) {
        let actual_basis = if matches!(
            entry.scope_policy,
            crate::lint::catalog::ScopePolicy::GlobalAggregateOnly
        ) {
            PopulationBasis::Global
        } else {
            basis
        };
        let _ = context.record_population(entry.id, actual_basis, 0);
    }
    crate::lint::runner::failed_results_for_group(context.clock(), LintCheckGroup::KnowledgeGraph)
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

#[cfg(test)]
#[path = "kg_test.rs"]
mod tests;
