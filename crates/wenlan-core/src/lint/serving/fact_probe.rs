use crate::lint::context::{LintContext, ScopeFilter};
use crate::retrieval::fact_channel::{max_pool_by_parent, ChildHit};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub(super) struct FactProbe {
    pub(super) eligible: u64,
    pub(super) affected_positions: Vec<usize>,
}

pub(super) async fn run(
    context: &LintContext<'_, '_>,
    parent_limit: usize,
) -> Result<FactProbe, ()> {
    run_with_ann(context, parent_limit, &SnapshotAnn).await
}

pub(super) async fn run_with_ann<Q: AnnTopK>(
    context: &LintContext<'_, '_>,
    parent_limit: usize,
    ann: &Q,
) -> Result<FactProbe, ()> {
    let scope = match context.scope().filter() {
        ScopeFilter::Global => return Ok(FactProbe::default()),
        ScopeFilter::Registered(scope) => ProbeScope::Registered(scope),
        ScopeFilter::Uncategorized => ProbeScope::Uncategorized,
    };
    let Some(embedding) = probe_embedding(context, &scope).await? else {
        return Ok(FactProbe::default());
    };
    if parent_limit == 0 {
        return Ok(FactProbe::default());
    }
    let fetch_limit = parent_limit.checked_mul(3).ok_or(())?;
    let ranked = ann.query(context, embedding.clone(), fetch_limit).await?;
    let global_hits = ranked
        .iter()
        .enumerate()
        .map(|(rank, child)| ChildHit {
            parent_id: child.parent_id.clone(),
            rank,
        })
        .collect::<Vec<_>>();
    let global_parents = max_pool_by_parent(&global_hits)
        .into_iter()
        .take(parent_limit)
        .collect::<BTreeSet<_>>();
    let scoped_ranked = ranked_scoped_children(context, &scope, embedding, fetch_limit).await?;
    let scoped_hits = scoped_ranked
        .iter()
        .enumerate()
        .map(|(rank, child)| ChildHit {
            parent_id: child.parent_id.clone(),
            rank,
        })
        .collect::<Vec<_>>();
    let scoped_parents = max_pool_by_parent(&scoped_hits)
        .into_iter()
        .take(parent_limit)
        .collect::<Vec<_>>();
    let affected_positions = scoped_parents
        .iter()
        .enumerate()
        .filter_map(|(position, parent)| (!global_parents.contains(parent)).then_some(position))
        .collect();
    Ok(FactProbe {
        eligible: u64::try_from(scoped_parents.len()).map_err(|_| ())?,
        affected_positions,
    })
}

#[derive(Debug)]
pub(super) struct RankedChild {
    parent_id: String,
}

pub(super) trait AnnTopK {
    async fn query(
        &self,
        context: &LintContext<'_, '_>,
        embedding: Vec<u8>,
        k: usize,
    ) -> Result<Vec<RankedChild>, ()>;
}

struct SnapshotAnn;

impl AnnTopK for SnapshotAnn {
    async fn query(
        &self,
        context: &LintContext<'_, '_>,
        embedding: Vec<u8>,
        k: usize,
    ) -> Result<Vec<RankedChild>, ()> {
        let k = i64::try_from(k).map_err(|_| ())?;
        let mut rows = context
            .snapshot()
            .query(
                "SELECT cv.parent_id, vector_distance_cos(cv.embedding, ?1) AS dist FROM vector_top_k('child_vectors_vec_idx', ?1, ?2) vt JOIN child_vectors cv ON cv.rowid=vt.id JOIN memories m ON m.source='memory' AND m.chunk_index=0 AND m.source_id=cv.parent_id WHERE cv.parent_kind='memory' ORDER BY dist, cv.id",
                libsql::params::Params::Positional(vec![
                    libsql::Value::Blob(embedding),
                    libsql::Value::Integer(k),
                ]),
            )
            .await
            .map_err(|_| ())?;
        let mut ranked = Vec::new();
        while let Some(row) = rows.next().await.map_err(|_| ())? {
            ranked.push(RankedChild {
                parent_id: row.get::<String>(0).map_err(|_| ())?,
            });
        }
        Ok(ranked)
    }
}

enum ProbeScope<'a> {
    Registered(&'a str),
    Uncategorized,
}

async fn probe_embedding(
    context: &LintContext<'_, '_>,
    scope: &ProbeScope<'_>,
) -> Result<Option<Vec<u8>>, ()> {
    let (predicate, params) = match scope {
        ProbeScope::Registered(space) => (
            "m.space IS NULL OR m.space != ?1",
            libsql::params::Params::Positional(vec![libsql::Value::Text((*space).to_string())]),
        ),
        ProbeScope::Uncategorized => ("m.space IS NOT NULL", libsql::params::Params::None),
    };
    let sql = format!(
        "SELECT cv.embedding FROM child_vectors cv JOIN memories m ON m.source='memory' AND m.chunk_index=0 AND m.source_id=cv.parent_id WHERE cv.parent_kind='memory' AND cv.embedding IS NOT NULL AND ({predicate}) ORDER BY cv.id LIMIT 1"
    );
    let mut rows = context
        .snapshot()
        .query(&sql, params)
        .await
        .map_err(|_| ())?;
    if let Some(row) = rows.next().await.map_err(|_| ())? {
        return row.get::<Vec<u8>>(0).map(Some).map_err(|_| ());
    }
    drop(rows);
    let mut rows = context
        .snapshot()
        .query(
            "SELECT embedding FROM child_vectors WHERE parent_kind='memory' AND embedding IS NOT NULL ORDER BY id LIMIT 1",
            libsql::params::Params::None,
        )
        .await
        .map_err(|_| ())?;
    rows.next()
        .await
        .map_err(|_| ())?
        .map(|row| row.get::<Vec<u8>>(0).map_err(|_| ()))
        .transpose()
}

async fn ranked_scoped_children(
    context: &LintContext<'_, '_>,
    scope: &ProbeScope<'_>,
    embedding: Vec<u8>,
    fetch_limit: usize,
) -> Result<Vec<RankedChild>, ()> {
    let fetch_limit = i64::try_from(fetch_limit).map_err(|_| ())?;
    let (scope_expr, params) = match scope {
        ProbeScope::Registered(space) => (
            "m.space=?3",
            vec![
                libsql::Value::Blob(embedding),
                libsql::Value::Integer(fetch_limit),
                libsql::Value::Text((*space).to_string()),
            ],
        ),
        ProbeScope::Uncategorized => (
            "m.space IS NULL",
            vec![
                libsql::Value::Blob(embedding),
                libsql::Value::Integer(fetch_limit),
            ],
        ),
    };
    let sql = format!(
        "SELECT cv.parent_id, vector_distance_cos(cv.embedding, ?1) AS dist FROM child_vectors cv JOIN memories m ON m.source='memory' AND m.chunk_index=0 AND m.source_id=cv.parent_id WHERE cv.parent_kind='memory' AND cv.embedding IS NOT NULL AND {scope_expr} ORDER BY dist, cv.id LIMIT ?2"
    );
    let mut rows = context
        .snapshot()
        .query(&sql, libsql::params::Params::Positional(params))
        .await
        .map_err(|_| ())?;
    let mut ranked = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        ranked.push(RankedChild {
            parent_id: row.get::<String>(0).map_err(|_| ())?,
        });
    }
    Ok(ranked)
}
