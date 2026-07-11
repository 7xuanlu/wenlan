mod db_checks;
mod frontmatter;
mod link_checks;
mod path;
mod provenance_checks;
mod state;
mod state_checks;
mod traversal;

pub mod fs;

#[cfg(test)]
mod integration_tests;

use super::catalog::{catalog, ScopePolicy};
use super::context::{LintContext, PopulationBasis};
use super::runner::{configured_off_results, failed_results, prerequisite_results};
use wenlan_types::lint::LintCheckResult;

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    page_projection_enabled: bool,
) -> Vec<LintCheckResult> {
    if !page_projection_enabled {
        if record_placeholder_populations(context, |_| true).is_err() {
            return failed_results(context.clock());
        }
        return configured_off_results(context.clock().clone());
    }
    if context.gate().check(std::time::Duration::ZERO).is_err() {
        if record_placeholder_populations(context, |_| true).is_err() {
            return failed_results(context.clock());
        }
        return failed_results(context.clock());
    }
    if context.page_scan().is_none() {
        if record_placeholder_populations(context, |_| true).is_err() {
            return failed_results(context.clock());
        }
        return prerequisite_results(context.clock());
    }

    let mut results = state_checks::run(context).await;
    results.extend(provenance_checks::run(context).await);
    results.extend(db_checks::run(context).await);
    results.extend(link_checks::run(context).await);
    let implemented_ids = [
        state_checks::STATE_CONTRACT_ID,
        state_checks::IDENTITY_ID,
        state_checks::VERSION_ALIGNMENT_ID,
        provenance_checks::SOURCE_COVERAGE_ID,
        provenance_checks::CITATION_PARTITIONS_ID,
        db_checks::PARTITIONS_ID,
        db_checks::DUPLICATE_TITLES_ID,
        db_checks::ARCHIVE_ID,
        db_checks::REVIEW_ID,
        link_checks::ORPHAN_LABELS_ID,
        link_checks::MANIFEST_ID,
        link_checks::ARTIFACT_ID,
    ];
    if record_placeholder_populations(context, |id| !implemented_ids.contains(&id)).is_err() {
        return failed_results(context.clock());
    }
    results.extend(
        prerequisite_results(context.clock())
            .into_iter()
            .filter(|result| !implemented_ids.contains(&result.check_id())),
    );
    results
}

fn record_placeholder_populations(
    context: &LintContext<'_, '_>,
    include: impl Fn(&str) -> bool,
) -> Result<(), ()> {
    let selected = context.scope().filter().is_selected();
    for entry in catalog() {
        if !include(entry.id) {
            continue;
        }
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
            return Err(());
        }
    }
    Ok(())
}

pub(crate) fn uses_cross_store(check_id: &str) -> bool {
    matches!(
        check_id,
        state_checks::IDENTITY_ID | state_checks::VERSION_ALIGNMENT_ID | link_checks::ARTIFACT_ID
    )
}

pub(crate) fn uses_filesystem(check_id: &str) -> bool {
    uses_cross_store(check_id)
        || matches!(
            check_id,
            state_checks::STATE_CONTRACT_ID | link_checks::MANIFEST_ID
        )
}
