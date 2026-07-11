mod config;
mod query;
mod result;

use crate::lint::catalog::LintCheckGroup;
use crate::lint::context::{LintContext, PopulationBasis};
pub use config::RuntimeObservation;
pub(crate) use config::RuntimeRunConfig;
use wenlan_types::lint::LintCheckResult;

pub(crate) const SCHEMA: &str = "runtime.schema_contract";
pub(crate) const INDEXES: &str = "runtime.search_index_contract";
pub(crate) const PROVIDERS: &str = "runtime.provider_inventory";
pub(crate) const STATUS: &str = "runtime.status_parity";
pub(crate) const WORKER: &str = "runtime.ingest_worker_liveness";

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    config: &RuntimeRunConfig,
) -> Vec<LintCheckResult> {
    let snapshot = match query::load(context, config).await {
        Ok(snapshot) => snapshot,
        Err(()) => return failed_results(context),
    };
    snapshot
        .assessments(config)
        .into_iter()
        .map(|assessment| result::finish(context, assessment))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|_| failed_results(context))
}

fn failed_results(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    for entry in crate::lint::catalog::catalog_group(LintCheckGroup::Runtime) {
        let _ = context.record_population(entry.id, PopulationBasis::Global, 0);
    }
    crate::lint::runner::failed_results_for_group(context.clock(), LintCheckGroup::Runtime)
}

#[cfg(test)]
#[path = "runtime_test.rs"]
mod tests;
