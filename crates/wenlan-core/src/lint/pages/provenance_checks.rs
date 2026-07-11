mod citations;
pub(super) mod result;
mod source;

use self::citations::load_and_assess_citations;
use self::result::failed_result;
use self::source::load_and_assess_sources;
use crate::lint::context::{LintContext, PopulationBasis};
use wenlan_types::lint::LintCheckResult;

pub(crate) const SOURCE_COVERAGE_ID: &str = "pages.provenance.source_evidence_coverage";
pub(crate) const CITATION_PARTITIONS_ID: &str = "pages.citations.partitions";

pub(crate) async fn run(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    let basis = if context.scope().filter().is_selected() {
        PopulationBasis::SelectedScope
    } else {
        PopulationBasis::Global
    };
    let source = load_and_assess_sources(context).await;
    let citations = load_and_assess_citations(context).await;

    [
        finish(context, SOURCE_COVERAGE_ID, basis, source),
        finish(context, CITATION_PARTITIONS_ID, basis, citations),
    ]
    .into_iter()
    .collect()
}

fn finish(
    context: &LintContext<'_, '_>,
    check_id: &'static str,
    basis: PopulationBasis,
    assessment: Result<result::Assessment, ()>,
) -> LintCheckResult {
    let Ok(assessment) = assessment else {
        let _ = context.record_population(check_id, basis, 0);
        return failed_result(check_id, context.clock().duration_ms());
    };
    let population = assessment.population();
    if context
        .record_population(check_id, basis, population)
        .is_err()
    {
        return failed_result(check_id, context.clock().duration_ms());
    }
    assessment
        .result(check_id, context.clock().duration_ms())
        .unwrap_or_else(|_| failed_result(check_id, context.clock().duration_ms()))
}

#[cfg(test)]
#[path = "provenance_checks_test.rs"]
mod tests;
