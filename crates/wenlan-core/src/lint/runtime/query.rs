use super::{RuntimeRunConfig, INDEXES, PROVIDERS, SCHEMA, STATUS, WORKER};
use crate::lint::context::LintContext;
use std::collections::BTreeSet;

const TABLES: &[&str] = &[
    "activities",
    "agent_activity",
    "agent_connections",
    "app_metadata",
    "briefing_cache",
    "capture_refs",
    "child_vectors",
    "document_enrichment_queue",
    "document_tags",
    "entities",
    "memories",
    "narrative_cache",
    "pages",
    "profiles",
    "session_snapshots",
    "spaces",
    "summary_nodes",
];
const SEARCH_OBJECTS: &[&str] = &[
    "child_vectors_vec_idx",
    "entities_vec_idx",
    "idx_pages_embedding",
    "idx_summary_nodes_embedding",
    "memories_fts_delete",
    "memories_fts_insert",
    "memories_fts_update",
    "memories_vec_idx",
    "pages_fts_delete",
    "pages_fts_insert",
    "pages_fts_update",
    "summary_nodes_fts_delete",
    "summary_nodes_fts_insert",
    "summary_nodes_fts_update",
];

pub(super) struct RuntimeSnapshot {
    user_version: u64,
    missing_tables: u64,
    missing_search_objects: u64,
    files_indexed: u64,
}

impl RuntimeSnapshot {
    pub(super) fn assessments(&self, config: &RuntimeRunConfig) -> [super::result::Assessment; 5] {
        let schema_affected = self.missing_tables
            + u64::from(self.user_version != u64::from(crate::db::SCHEMA_VERSION));
        let provider_available = config.observation.provider_slots_available.unwrap_or(0);
        let reranker_available = config.observation.reranker_paths_available.unwrap_or(0);
        let provider_affected = config
            .snapshot
            .provider_slots_requested
            .saturating_sub(provider_available)
            + config
                .snapshot
                .reranker_paths_requested
                .saturating_sub(reranker_available);
        let status_population = u64::from(config.observation.status_files_indexed.is_some());
        let status_affected = u64::from(
            config
                .observation
                .status_files_indexed
                .is_some_and(|value| value != self.files_indexed),
        );
        let worker_population = u64::from(config.observation.ingest_worker_closed.is_some());
        let worker_affected = u64::from(config.observation.ingest_worker_closed == Some(true));
        [
            super::result::Assessment::new(
                SCHEMA,
                u64::try_from(TABLES.len() + 1).unwrap_or(u64::MAX),
                schema_affected,
            ),
            super::result::Assessment::new(
                INDEXES,
                u64::try_from(SEARCH_OBJECTS.len()).unwrap_or(u64::MAX),
                self.missing_search_objects,
            ),
            super::result::Assessment::new(
                PROVIDERS,
                config.snapshot.provider_slots_requested + config.snapshot.reranker_paths_requested,
                provider_affected,
            )
            .with_observed(provider_available + reranker_available),
            super::result::Assessment::new(STATUS, status_population, status_affected)
                .with_observed(self.files_indexed),
            super::result::Assessment::new(WORKER, worker_population, worker_affected)
                .with_metric(
                    wenlan_types::lint::LintMetricCode::WorkingMemoryTelemetryRows,
                    config.observation.working_memory_entries.unwrap_or(0),
                )
                .with_metric(
                    wenlan_types::lint::LintMetricCode::WorkingMemoryTelemetryUnavailable,
                    u64::from(config.observation.working_memory_entries.is_none()),
                ),
        ]
    }
}

pub(super) async fn load(
    context: &LintContext<'_, '_>,
    _config: &RuntimeRunConfig,
) -> Result<RuntimeSnapshot, ()> {
    #[cfg(test)]
    if _config.force_query_failure {
        scalar(
            context,
            "SELECT COUNT(*) FROM task15_missing_runtime_source",
        )
        .await?;
    }
    let user_version = scalar(context, "PRAGMA user_version").await?;
    let tables = object_names(context, "table").await?;
    let search_objects = all_object_names(context).await?;
    let files_indexed = scalar(
        context,
        "SELECT COUNT(*) FROM memories WHERE source != 'episode'",
    )
    .await?;
    Ok(RuntimeSnapshot {
        user_version,
        missing_tables: missing(TABLES, &tables),
        missing_search_objects: missing(SEARCH_OBJECTS, &search_objects),
        files_indexed,
    })
}

async fn scalar(context: &LintContext<'_, '_>, sql: &str) -> Result<u64, ()> {
    let mut rows = context
        .snapshot()
        .query(sql, libsql::params::Params::None)
        .await
        .map_err(|_| ())?;
    let value = rows
        .next()
        .await
        .map_err(|_| ())?
        .ok_or(())?
        .get::<i64>(0)
        .map_err(|_| ())?;
    u64::try_from(value).map_err(|_| ())
}

async fn object_names(
    context: &LintContext<'_, '_>,
    object_type: &str,
) -> Result<BTreeSet<String>, ()> {
    let mut rows = context
        .snapshot()
        .query(
            "SELECT name FROM sqlite_schema WHERE type=?1 ORDER BY name",
            libsql::params::Params::Positional(vec![libsql::Value::Text(object_type.to_string())]),
        )
        .await
        .map_err(|_| ())?;
    collect_names(&mut rows).await
}

async fn all_object_names(context: &LintContext<'_, '_>) -> Result<BTreeSet<String>, ()> {
    let mut rows = context
        .snapshot()
        .query(
            "SELECT name FROM sqlite_schema WHERE type IN ('index','trigger') ORDER BY name",
            libsql::params::Params::None,
        )
        .await
        .map_err(|_| ())?;
    collect_names(&mut rows).await
}

async fn collect_names(
    rows: &mut crate::lint::snapshot::LintRows<'_>,
) -> Result<BTreeSet<String>, ()> {
    let mut names = BTreeSet::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        names.insert(row.get::<String>(0).map_err(|_| ())?);
    }
    Ok(names)
}

fn missing(expected: &[&str], observed: &BTreeSet<String>) -> u64 {
    u64::try_from(
        expected
            .iter()
            .filter(|name| !observed.contains(**name))
            .count(),
    )
    .unwrap_or(u64::MAX)
}
