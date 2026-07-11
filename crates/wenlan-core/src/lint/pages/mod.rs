mod frontmatter;
mod path;
mod state;
mod traversal;

pub mod fs;

use super::context::LintContext;
use super::runner::{configured_off_results, prerequisite_results};
use wenlan_types::lint::LintCheckResult;

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    page_projection_enabled: bool,
) -> Vec<LintCheckResult> {
    let _ = (
        context.snapshot(),
        context.scope().filter(),
        context.page_scan(),
        context.gate(),
    );
    if page_projection_enabled {
        prerequisite_results(context.clock())
    } else {
        configured_off_results(context.clock().clone())
    }
}
