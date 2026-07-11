use super::MemoryRecord;
use crate::lint::context::{LintContext, ScopeFilter};

pub(super) async fn load_records(context: &LintContext<'_, '_>) -> Result<Vec<MemoryRecord>, ()> {
    let (where_scope, params) = match context.scope().filter() {
        ScopeFilter::Global => ("", libsql::params::Params::None),
        ScopeFilter::Registered(space) => (
            " AND m.space = ?1",
            libsql::params::Params::Positional(vec![libsql::Value::Text(space.clone())]),
        ),
        ScopeFilter::Uncategorized => (" AND m.space IS NULL", libsql::params::Params::None),
    };
    let sql = format!(
        "WITH grouped AS (
           SELECT m.source_id,
             CASE WHEN COUNT(*) = COUNT(DISTINCT m.chunk_index)
                    AND COUNT(DISTINCT COALESCE(m.supersedes,'')) = 1
                    AND MIN(m.pending_revision) IN (0,1)
                    AND MAX(m.pending_revision) IN (0,1)
                    AND MIN(COALESCE(m.is_recap,0)) IN (0,1)
                    AND MAX(COALESCE(m.is_recap,0)) IN (0,1)
                    AND MIN(m.stability) IN ('new','learned','confirmed')
                    AND MAX(m.stability) IN ('new','learned','confirmed')
                    AND MIN(m.supersede_mode) IN ('hide','archive','evicted')
                    AND MAX(m.supersede_mode) IN ('hide','archive','evicted')
                   THEN 1 ELSE 0 END AS lifecycle_valid,
             MIN(m.supersedes) AS supersedes,
             MAX(m.pending_revision) AS pending_revision,
             MAX(COALESCE(m.is_recap,0)) AS is_recap,
             MIN(CASE WHEN m.embedding IS NULL THEN 0 ELSE 1 END) AS embedding_complete,
             MAX(COALESCE(m.needs_reembed,0)) AS needs_reembed,
             (SELECT COUNT(*) FROM enrichment_steps es
               WHERE es.source_id=m.source_id
                 AND es.status IN ('failed','abandoned')) AS failed_steps,
             MAX(CASE WHEN m.memory_type IS NOT NULL AND m.memory_type != ''
                      THEN 1 ELSE 0 END) AS classified,
             MAX(CASE WHEN m.event_date IS NOT NULL THEN 1 ELSE 0 END) AS event_dated,
             EXISTS(SELECT 1 FROM memories e
                     WHERE e.source='episode' AND e.episode_of=m.source_id) AS episode,
             EXISTS(SELECT 1 FROM child_vectors c
                     WHERE c.parent_kind='memory' AND c.parent_id=m.source_id) AS fact,
             EXISTS(SELECT 1 FROM page_sources p WHERE p.memory_source_id=m.source_id)
               OR EXISTS(SELECT 1 FROM page_evidence pe
                          WHERE pe.source_kind='memory' AND pe.locator=m.source_id) AS page_link,
             EXISTS(SELECT 1 FROM summary_node_sources s
                     WHERE s.memory_source_id=m.source_id) AS summary
           FROM memories m
          WHERE m.source='memory'{where_scope}
          GROUP BY m.source_id
        )
        SELECT grouped.source_id, grouped.lifecycle_valid, grouped.supersedes,
               CASE WHEN grouped.supersedes IS NULL OR EXISTS(
                   SELECT 1 FROM memories t
                    WHERE t.source='memory' AND t.source_id=grouped.supersedes
               ) THEN 1 ELSE 0 END,
               EXISTS(SELECT 1 FROM memories r
                       WHERE r.source='memory' AND r.pending_revision=0
                         AND r.supersedes=grouped.source_id),
               grouped.pending_revision, grouped.is_recap,
               grouped.embedding_complete, grouped.needs_reembed,
               grouped.failed_steps, grouped.classified, grouped.event_dated,
               grouped.episode, grouped.fact, grouped.page_link, grouped.summary
          FROM grouped ORDER BY grouped.source_id"
    );
    let mut rows = context
        .snapshot()
        .query(&sql, params)
        .await
        .map_err(|_| ())?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        records.push(MemoryRecord {
            source_id: row.get(0).map_err(|_| ())?,
            lifecycle_valid: row.get::<i64>(1).map_err(|_| ())? != 0,
            supersedes: row.get(2).ok(),
            target_exists: row.get::<i64>(3).map_err(|_| ())? != 0,
            replaced_by_active: row.get::<i64>(4).map_err(|_| ())? != 0,
            pending_revision: row.get::<i64>(5).map_err(|_| ())? != 0,
            recap: row.get::<i64>(6).map_err(|_| ())? != 0,
            embedding_complete: row.get::<i64>(7).map_err(|_| ())? != 0,
            needs_reembed: row.get::<i64>(8).map_err(|_| ())? != 0,
            failed_steps: u64::try_from(row.get::<i64>(9).map_err(|_| ())?).map_err(|_| ())?,
            classified: row.get::<i64>(10).map_err(|_| ())? != 0,
            event_dated: row.get::<i64>(11).map_err(|_| ())? != 0,
            episode: row.get::<i64>(12).map_err(|_| ())? != 0,
            fact: row.get::<i64>(13).map_err(|_| ())? != 0,
            page_link: row.get::<i64>(14).map_err(|_| ())? != 0,
            summary: row.get::<i64>(15).map_err(|_| ())? != 0,
            head: false,
        });
    }
    Ok(records)
}
