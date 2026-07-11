use super::super::provenance_checks::result::Assessment;
use crate::lint::context::{LintContext, ScopeFilter};
use wenlan_types::lint::{LintMetric, LintMetricCode, LintMetricValue};

pub(super) async fn load(context: &LintContext<'_, '_>) -> Result<Assessment, ()> {
    let (where_sql, params) = scoped_active_source_filter(context.scope().filter());
    let count_sql = format!(
        "SELECT COUNT(DISTINCT pl.label_key) FROM page_links pl \
         INNER JOIN pages p ON p.id = pl.source_page_id \
         WHERE pl.target_page_id IS NULL AND p.status = 'active'{where_sql}"
    );
    let mut count_rows = context
        .snapshot()
        .query(&count_sql, params.clone())
        .await
        .map_err(|_| ())?;
    let count = count_rows
        .next()
        .await
        .map_err(|_| ())?
        .ok_or(())?
        .get::<i64>(0)
        .map_err(|_| ())?;
    let count = u64::try_from(count).map_err(|_| ())?;

    let sample_sql = format!(
        "SELECT pl.label_key FROM page_links pl \
         INNER JOIN pages p ON p.id = pl.source_page_id \
         WHERE pl.target_page_id IS NULL AND p.status = 'active'{where_sql} \
         GROUP BY pl.label_key ORDER BY pl.label_key LIMIT 100"
    );
    let mut rows = context
        .snapshot()
        .query(&sample_sql, params)
        .await
        .map_err(|_| ())?;
    let mut positions = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        let _: String = row.get(0).map_err(|_| ())?;
        positions.push(positions.len());
    }
    if positions.len() != usize::try_from(count.min(100)).map_err(|_| ())? {
        return Err(());
    }
    let mut assessment = Assessment::from_aggregate(count, count, 0, count == 0, positions);
    assessment.set_metrics(vec![LintMetric::new(
        LintMetricCode::PageOrphanLabels,
        LintMetricValue::Count { value: count },
    )]);
    Ok(assessment)
}

fn scoped_active_source_filter(filter: &ScopeFilter) -> (&'static str, libsql::params::Params) {
    match filter {
        ScopeFilter::Global => ("", libsql::params::Params::None),
        ScopeFilter::Registered(workspace) => (
            " AND p.workspace = ?1",
            libsql::params::Params::Positional(vec![libsql::Value::Text(workspace.clone())]),
        ),
        ScopeFilter::Uncategorized => (" AND p.workspace IS NULL", libsql::params::Params::None),
    }
}

#[cfg(test)]
mod architecture_tests {
    #[test]
    fn lint_orphan_module_has_no_mutating_resolver_dependency() {
        let source = include_str!("orphans.rs");
        assert!(!source.contains("resolve_orphan_page_links"));
    }
}
