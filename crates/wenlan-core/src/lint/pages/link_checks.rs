mod artifacts;
mod manifest;
mod orphans;

use super::provenance_checks::result::{failed_result, Assessment};
use crate::lint::context::{LintContext, PopulationBasis};
use wenlan_types::lint::LintCheckResult;

pub(crate) const ORPHAN_LABELS_ID: &str = "pages.links.orphan_labels";
pub(crate) const MANIFEST_ID: &str = "pages.projection.manifest_inventory";
pub(crate) const ARTIFACT_ID: &str = "pages.project.artifact_inventory";

pub(crate) async fn run(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    let suppress_global_evidence = context.scope().filter().is_selected();
    [
        finish(
            context,
            ORPHAN_LABELS_ID,
            scoped_basis(context),
            orphans::load(context).await,
        ),
        finish(
            context,
            MANIFEST_ID,
            PopulationBasis::Global,
            manifest::load(context, suppress_global_evidence),
        ),
        finish(
            context,
            ARTIFACT_ID,
            PopulationBasis::Global,
            artifacts::load(context).await,
        ),
    ]
    .into_iter()
    .collect()
}

fn finish(
    context: &LintContext<'_, '_>,
    check_id: &'static str,
    basis: PopulationBasis,
    assessment: Result<Assessment, ()>,
) -> LintCheckResult {
    let Ok(assessment) = assessment else {
        let _ = context.record_population(check_id, basis, 0);
        return failed_result(check_id, context.clock().duration_ms());
    };
    if context
        .record_population(check_id, basis, assessment.population())
        .is_err()
    {
        return failed_result(check_id, context.clock().duration_ms());
    }
    assessment
        .result(check_id, context.clock().duration_ms())
        .unwrap_or_else(|_| failed_result(check_id, context.clock().duration_ms()))
}

fn scoped_basis(context: &LintContext<'_, '_>) -> PopulationBasis {
    if context.scope().filter().is_selected() {
        PopulationBasis::SelectedScope
    } else {
        PopulationBasis::Global
    }
}

#[cfg(test)]
use artifacts::{load as load_artifacts, ArtifactCounts};
#[cfg(test)]
use manifest::assess as assess_manifest;
#[cfg(test)]
use orphans::load as load_orphans;

#[cfg(test)]
#[path = "link_checks_test.rs"]
mod tests;
