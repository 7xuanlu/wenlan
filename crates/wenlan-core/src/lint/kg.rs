mod config;
mod query;
mod result;

use crate::lint::catalog::{catalog_group, LintCheckGroup};
use crate::lint::context::{LintContext, PopulationBasis};
pub(crate) use config::KgRunConfig;
use query::{load, KgSnapshot};
use result::{finish, Assessment};
use wenlan_types::lint::LintCheckResult;

const ENTITY_INTEGRITY: &str = "entities.structural_integrity";
const ENTITY_PARTITIONS: &str = "entities.partition_inventory";
const ADVISORY: &str = "kg.advisory_inventory";
const AGGREGATES: &str = "kg.aggregate_inventory";
const LIVENESS: &str = "kg.substrate_liveness";
const LINKS: &str = "memory_entities.integrity";
const OBSERVATIONS: &str = "observations.integrity";
const RELATIONS: &str = "relations.integrity";

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    config: KgRunConfig,
) -> Vec<LintCheckResult> {
    let snapshot = match load(context, config.hub_cap).await {
        Ok(snapshot) => snapshot,
        Err(()) => return failed_results(context),
    };
    match assessments(snapshot, config)
        .into_iter()
        .map(|assessment| finish(context, assessment))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(results) => results,
        Err(_) => failed_results(context),
    }
}

fn assessments(snapshot: KgSnapshot, config: KgRunConfig) -> Vec<Assessment> {
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
        Assessment::liveness(config, eligible_memories, linked_memories),
        Assessment::structural(LINKS, links),
        Assessment::structural(OBSERVATIONS, observations),
        Assessment::structural(RELATIONS, relations),
    ]
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

#[cfg(test)]
#[path = "kg_test.rs"]
mod tests;

#[cfg(test)]
#[path = "kg_config_test.rs"]
mod config_tests;
