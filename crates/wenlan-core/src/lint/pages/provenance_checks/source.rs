use super::result::{Assessment, Level};
use crate::citations::resolve_page_evidence_source_kind;
use crate::lint::context::{LintContext, ScopeFilter};
use std::collections::BTreeMap;
use wenlan_types::lint::{LintMetric, LintMetricCode, LintMetricValue};

const VALID_KINDS: [&str; 4] = ["authored", "external_file", "external_url", "memory"];

#[derive(Debug, Clone)]
pub(super) struct SourceRecord {
    #[cfg(test)]
    pub(super) locator: String,
    pub(super) expected_kind: String,
    pub(super) evidence_kinds: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ExtraEvidence {
    pub(super) source_kind: String,
    pub(super) locator_present: bool,
}

#[derive(Debug)]
struct SourceBuilder {
    source: String,
    source_agent: Option<String>,
    resolved_source_id: String,
    evidence_kinds: Vec<String>,
}

pub(super) async fn load_and_assess_sources(
    context: &LintContext<'_, '_>,
) -> Result<Assessment, ()> {
    let records = load_sources(context).await?;
    let extras = load_extra_evidence(context).await?;
    Ok(assess_sources(&records, &extras))
}

pub(super) fn assess_sources(records: &[SourceRecord], extras: &[ExtraEvidence]) -> Assessment {
    let mut assessment = Assessment::default();
    let mut affected = 0_u64;
    for record in records {
        let unknown = record
            .evidence_kinds
            .iter()
            .any(|kind| !VALID_KINDS.contains(&kind.as_str()));
        let level = if unknown || record.evidence_kinds.is_empty() {
            Level::Error
        } else if record.evidence_kinds.len() == 1
            && record.evidence_kinds[0] == record.expected_kind
        {
            Level::Pass
        } else {
            Level::Warning
        };
        if level != Level::Pass {
            affected = affected.saturating_add(1);
        }
        assessment.push(level);
    }
    for extra in extras {
        assessment.mark_inventory();
        if !VALID_KINDS.contains(&extra.source_kind.as_str()) {
            assessment.push(Level::Error);
            affected = affected.saturating_add(1);
        }
        if !extra.locator_present {
            assessment.mark_inventory();
        }
    }
    assessment.set_metrics(vec![
        count_metric(LintMetricCode::EligibleRecords, records.len()),
        count_metric(LintMetricCode::ObservedRecords, extras.len()),
        LintMetric::new(
            LintMetricCode::AffectedRecords,
            LintMetricValue::Count { value: affected },
        ),
    ]);
    assessment
}

pub(super) async fn load_sources(context: &LintContext<'_, '_>) -> Result<Vec<SourceRecord>, ()> {
    let (scope_sql, params) = scoped_pages(context.scope().filter());
    let sql = format!(
        "WITH scoped_pages AS ({scope_sql}),
         ranked_sources AS (
           SELECT ps.page_id, ps.memory_source_id, m.source, m.source_agent,
                  COALESCE(m.source_id, ps.memory_source_id) AS resolved_source_id,
                  ROW_NUMBER() OVER (
                    PARTITION BY ps.page_id, ps.memory_source_id
                    ORDER BY CASE
                      WHEN m.source_id = ps.memory_source_id THEN 0
                      WHEN m.id = ps.memory_source_id THEN 1
                      ELSE 2 END,
                      m.id
                  ) AS source_rank
             FROM page_sources ps
             JOIN scoped_pages sp ON sp.id = ps.page_id
             LEFT JOIN memories m
               ON m.source_id = ps.memory_source_id OR m.id = ps.memory_source_id
         )
         SELECT rs.page_id, rs.memory_source_id, COALESCE(rs.source, ''),
                rs.source_agent, rs.resolved_source_id, pe.source_kind
           FROM ranked_sources rs
           LEFT JOIN page_evidence pe
             ON pe.page_id = rs.page_id AND pe.locator = rs.memory_source_id
          WHERE rs.source_rank = 1
          ORDER BY rs.page_id, rs.memory_source_id, pe.source_kind"
    );
    let mut rows = context
        .snapshot()
        .query(&sql, params)
        .await
        .map_err(|_| ())?;
    let mut grouped = BTreeMap::<(String, String), SourceBuilder>::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        let page_id = row.get::<String>(0).map_err(|_| ())?;
        let locator = row.get::<String>(1).map_err(|_| ())?;
        let builder = grouped
            .entry((page_id, locator))
            .or_insert_with(|| SourceBuilder {
                source: row.get::<String>(2).unwrap_or_default(),
                source_agent: row.get::<Option<String>>(3).unwrap_or(None),
                resolved_source_id: row.get::<String>(4).unwrap_or_default(),
                evidence_kinds: Vec::new(),
            });
        if let Ok(Some(kind)) = row.get::<Option<String>>(5) {
            builder.evidence_kinds.push(kind);
        }
    }
    Ok(grouped
        .into_iter()
        .map(|((_, _locator), builder)| SourceRecord {
            #[cfg(test)]
            locator: _locator,
            expected_kind: resolve_page_evidence_source_kind(
                &builder.source,
                builder.source_agent.as_deref(),
                &builder.resolved_source_id,
            )
            .to_string(),
            evidence_kinds: builder.evidence_kinds,
        })
        .collect())
}

async fn load_extra_evidence(context: &LintContext<'_, '_>) -> Result<Vec<ExtraEvidence>, ()> {
    let (scope_sql, params) = scoped_pages(context.scope().filter());
    let sql = format!(
        "WITH scoped_pages AS ({scope_sql})
         SELECT pe.source_kind, pe.locator
           FROM page_evidence pe
           JOIN scoped_pages sp ON sp.id = pe.page_id
           LEFT JOIN page_sources ps
             ON ps.page_id = pe.page_id AND ps.memory_source_id = pe.locator
          WHERE ps.page_id IS NULL
          ORDER BY pe.page_id, pe.source_kind, COALESCE(pe.locator, '')"
    );
    let mut rows = context
        .snapshot()
        .query(&sql, params)
        .await
        .map_err(|_| ())?;
    let mut extras = Vec::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        extras.push(ExtraEvidence {
            source_kind: row.get(0).map_err(|_| ())?,
            locator_present: row.get::<Option<String>>(1).map_err(|_| ())?.is_some(),
        });
    }
    Ok(extras)
}

fn scoped_pages(filter: &ScopeFilter) -> (&'static str, libsql::params::Params) {
    match filter {
        ScopeFilter::Global => ("SELECT id FROM pages", libsql::params::Params::None),
        ScopeFilter::Registered(workspace) => (
            "SELECT id FROM pages WHERE workspace = ?1",
            libsql::params::Params::Positional(vec![libsql::Value::Text(workspace.clone())]),
        ),
        ScopeFilter::Uncategorized => (
            "SELECT id FROM pages WHERE workspace IS NULL",
            libsql::params::Params::None,
        ),
    }
}

fn count_metric(code: LintMetricCode, value: usize) -> LintMetric {
    LintMetric::new(
        code,
        LintMetricValue::Count {
            value: u64::try_from(value).unwrap_or(u64::MAX),
        },
    )
}
