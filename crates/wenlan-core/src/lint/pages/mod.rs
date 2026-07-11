mod frontmatter;
mod path;
mod state;
mod traversal;

pub mod fs;

use super::catalog::{catalog, ScopePolicy};
use super::context::{LintContext, PopulationBasis};
use super::runner::{configured_off_results, failed_results, prerequisite_results};
use wenlan_types::lint::LintCheckResult;

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    page_projection_enabled: bool,
) -> Vec<LintCheckResult> {
    let selected = context.scope().filter().is_selected();
    for entry in catalog() {
        let basis = if selected
            && matches!(
                entry.scope_policy,
                ScopePolicy::ScopedRows | ScopePolicy::DbAnchoredProjection
            ) {
            PopulationBasis::SelectedScope
        } else {
            PopulationBasis::Global
        };
        if context.record_population(entry.id, basis, 0).is_err() {
            return failed_results(context.clock());
        }
    }
    let _shared_snapshot = context.snapshot();
    let _bounded_page_scan = context.page_scan();
    if context.gate().check(std::time::Duration::ZERO).is_err() {
        return failed_results(context.clock());
    }
    if page_projection_enabled {
        prerequisite_results(context.clock())
    } else {
        configured_off_results(context.clock().clone())
    }
}
