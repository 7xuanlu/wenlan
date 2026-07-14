mod query;
mod result;
mod session;

use crate::lint::catalog::{catalog_group, LintCheckGroup, ScopePolicy};
use crate::lint::context::{LintContext, PopulationBasis};
use wenlan_types::lint::LintCheckResult;

pub(crate) const CACHES: &str = "identity.cache_inventory";
pub(crate) const MEMORY: &str = "identity.memory_state_integrity";
pub(crate) const REGISTRY: &str = "identity.registry_integrity";
pub(crate) const SESSIONS: &str = "identity.session_structure";
pub(crate) const TAGS: &str = "identity.tag_integrity";

pub(crate) async fn run(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    let snapshot = match query::load(context).await {
        Ok(snapshot) => snapshot,
        Err(()) => return failed_results(context),
    };
    snapshot
        .assessments()
        .into_iter()
        .map(|assessment| result::finish(context, assessment))
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_else(|_| failed_results(context))
}

fn failed_results(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    let selected = context.scope().filter().is_selected();
    for entry in catalog_group(LintCheckGroup::Identity) {
        let basis = if selected && entry.scope_policy == ScopePolicy::ScopedRows {
            PopulationBasis::SelectedScope
        } else {
            PopulationBasis::Global
        };
        let _ = context.record_population(entry.id, basis, 0);
    }
    crate::lint::runner::failed_results_for_group(context.clock(), LintCheckGroup::Identity)
}

#[cfg(test)]
#[path = "identity_test.rs"]
mod tests;
