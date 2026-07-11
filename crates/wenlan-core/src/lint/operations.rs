mod config;
mod query;
mod result;

use crate::lint::catalog::{catalog_group, LintCheckGroup};
use crate::lint::context::{LintContext, PopulationBasis};
pub(crate) use config::OperationsRunConfig;
use result::finish;
use wenlan_types::lint::LintCheckResult;

pub(super) const DOCUMENT_QUEUE: &str = "operations.document_queue";
pub(super) const IMPORT_CHECKPOINTS: &str = "operations.import_checkpoints";
pub(super) const MAINTENANCE_BACKLOGS: &str = "operations.maintenance_backlogs";
pub(super) const REFINEMENT_INVENTORY: &str = "operations.refinement_inventory";
pub(super) const REJECTION_INVENTORY: &str = "operations.rejection_inventory";
pub(super) const SOURCE_CONFIGURATION: &str = "operations.source_configuration";

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    config: OperationsRunConfig,
) -> Vec<LintCheckResult> {
    let assessments = match query::load(context, config).await {
        Ok(assessments) => assessments,
        Err(()) => return failed_results(context),
    };
    match assessments
        .into_iter()
        .map(|assessment| finish(context, assessment))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(results) => results,
        Err(_) => failed_results(context),
    }
}

fn failed_results(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    for entry in catalog_group(LintCheckGroup::Operations) {
        let _ = context.record_population(entry.id, PopulationBasis::Global, 0);
    }
    crate::lint::runner::failed_results_for_group(context.clock(), LintCheckGroup::Operations)
}

#[cfg(test)]
#[path = "operations_test.rs"]
mod tests;
