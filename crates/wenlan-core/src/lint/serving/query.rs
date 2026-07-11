use crate::lint::context::{LintContext, ScopeFilter};

pub(super) struct Counts {
    pub(super) eligible: u64,
    pub(super) episode: u64,
    pub(super) fact: u64,
    pub(super) graph: u64,
    pub(super) page: u64,
    pub(super) summary: u64,
}

pub(super) async fn load(context: &LintContext<'_, '_>) -> Result<Counts, ()> {
    let (memory_filter, page_filter, params) = match context.scope().filter() {
        ScopeFilter::Global => ("", "", libsql::params::Params::None),
        ScopeFilter::Registered(scope) => (
            " AND m.space=?1",
            " WHERE p.workspace=?1",
            libsql::params::Params::Positional(vec![libsql::Value::Text(scope.clone())]),
        ),
        ScopeFilter::Uncategorized => (
            " AND m.space IS NULL",
            " WHERE p.workspace IS NULL",
            libsql::params::Params::None,
        ),
    };
    let sql = format!("SELECT
      (SELECT COUNT(DISTINCT m.source_id) FROM memories m WHERE m.source='memory'{memory_filter}),
      (SELECT COUNT(DISTINCT m.source_id) FROM memories m WHERE m.source='episode'{memory_filter}),
      (SELECT COUNT(DISTINCT m.source_id) FROM memories m JOIN child_vectors c ON c.parent_id=m.source_id AND c.parent_kind='memory' AND c.embedding IS NOT NULL WHERE m.source='memory'{memory_filter}),
      (SELECT COUNT(DISTINCT m.source_id) FROM memories m JOIN memory_entities e ON e.memory_id=m.source_id WHERE m.source='memory'{memory_filter}),
      (SELECT COUNT(*) FROM pages p{page_filter}),
      (SELECT COUNT(DISTINCT m.source_id) FROM memories m JOIN summary_node_sources s ON s.memory_source_id=m.source_id WHERE m.source='memory'{memory_filter})");
    let mut rows = context
        .snapshot()
        .query(&sql, params)
        .await
        .map_err(|_| ())?;
    let row = rows.next().await.map_err(|_| ())?.ok_or(())?;
    Ok(Counts {
        eligible: count(&row, 0)?,
        episode: count(&row, 1)?,
        fact: count(&row, 2)?,
        graph: count(&row, 3)?,
        page: count(&row, 4)?,
        summary: count(&row, 5)?,
    })
}

fn count(row: &libsql::Row, index: i32) -> Result<u64, ()> {
    u64::try_from(row.get::<i64>(index).map_err(|_| ())?).map_err(|_| ())
}
