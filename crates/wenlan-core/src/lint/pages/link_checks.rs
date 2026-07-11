use super::fs::{normalize_target_path, EntryKind, EntryScope, ManifestProjection, PageScan};
use super::provenance_checks::result::{failed_result, Assessment, Level};
use crate::export::provenance::stub_filename;
use crate::lint::context::{LintContext, PopulationBasis, ScopeFilter};
use std::collections::BTreeSet;
use wenlan_types::lint::{LintCheckResult, LintMetric, LintMetricCode, LintMetricValue};

pub(crate) const ORPHAN_LABELS_ID: &str = "pages.links.orphan_labels";
pub(crate) const MANIFEST_ID: &str = "pages.projection.manifest_inventory";
pub(crate) const ARTIFACT_ID: &str = "pages.project.artifact_inventory";

pub(crate) async fn run(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    let orphan = load_orphans(context).await;
    let manifest = load_manifest(context).await;
    let artifacts = load_artifacts(context).await;
    [
        finish(context, ORPHAN_LABELS_ID, scoped_basis(context), orphan),
        finish(context, MANIFEST_ID, scoped_basis(context), manifest),
        finish(context, ARTIFACT_ID, PopulationBasis::Global, artifacts),
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

async fn load_orphans(context: &LintContext<'_, '_>) -> Result<Assessment, ()> {
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
    assessment.set_metrics(vec![metric(LintMetricCode::PageOrphanLabels, count)]);
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

async fn load_manifest(context: &LintContext<'_, '_>) -> Result<Assessment, ()> {
    let scan = context.page_scan().ok_or(())?;
    let selected_page_ids = load_selected_page_ids(context).await?;
    Ok(assess_manifest(scan, selected_page_ids.as_ref()))
}

async fn load_selected_page_ids(
    context: &LintContext<'_, '_>,
) -> Result<Option<BTreeSet<String>>, ()> {
    let (sql, params) = match context.scope().filter() {
        ScopeFilter::Global => return Ok(None),
        ScopeFilter::Registered(workspace) => (
            "SELECT id FROM pages WHERE workspace = ?1 ORDER BY id",
            libsql::params::Params::Positional(vec![libsql::Value::Text(workspace.clone())]),
        ),
        ScopeFilter::Uncategorized => (
            "SELECT id FROM pages WHERE workspace IS NULL ORDER BY id",
            libsql::params::Params::None,
        ),
    };
    let mut rows = context
        .snapshot()
        .query(sql, params)
        .await
        .map_err(|_| ())?;
    let mut ids = BTreeSet::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        ids.insert(row.get(0).map_err(|_| ())?);
    }
    Ok(Some(ids))
}

fn assess_manifest(scan: &PageScan, selected_page_ids: Option<&BTreeSet<String>>) -> Assessment {
    let source_entries = scan
        .entries
        .iter()
        .filter(|entry| entry.scope == EntryScope::SourceInventory)
        .collect::<Vec<_>>();
    let mut generated_path_errors = 0_u64;
    let (page_count, reference_count, expected_stubs, parse_error) = match &scan.manifest {
        ManifestProjection::Missing => (0, 0, BTreeSet::new(), false),
        ManifestProjection::Invalid => (0, 0, BTreeSet::new(), true),
        ManifestProjection::Parsed(pages) => {
            let scoped = pages.iter().filter(|(page_id, _)| {
                selected_page_ids.is_none_or(|selected| selected.contains(*page_id))
            });
            let mut page_count = 0_u64;
            let mut reference_count = 0_u64;
            let mut expected = BTreeSet::new();
            for (_, source_ids) in scoped {
                page_count = page_count.saturating_add(1);
                reference_count = reference_count
                    .saturating_add(u64::try_from(source_ids.len()).unwrap_or(u64::MAX));
                for source_id in source_ids {
                    let candidate = format!("_sources/{}", stub_filename(source_id));
                    match normalize_target_path(&candidate) {
                        Ok(path) if path.as_str().starts_with("_sources/") => {
                            expected.insert(path.as_str().to_string());
                        }
                        _ => generated_path_errors = generated_path_errors.saturating_add(1),
                    }
                }
            }
            (page_count, reference_count, expected, false)
        }
    };
    let all_observed_stubs = source_entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File && entry.path.ends_with(".md"))
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    let observed_stubs = if selected_page_ids.is_some() {
        all_observed_stubs
            .intersection(&expected_stubs)
            .cloned()
            .collect::<BTreeSet<_>>()
    } else {
        all_observed_stubs
    };
    let structural_errors = if selected_page_ids.is_some() {
        0
    } else {
        u64::try_from(
            source_entries
                .iter()
                .filter(|entry| matches!(entry.kind, EntryKind::Symlink | EntryKind::Other))
                .count(),
        )
        .unwrap_or(u64::MAX)
    };
    let divergence_count = expected_stubs.symmetric_difference(&observed_stubs).count();
    let error_count = structural_errors
        .saturating_add(generated_path_errors)
        .saturating_add(u64::from(parse_error));
    let source_population = if selected_page_ids.is_some() {
        u64::try_from(observed_stubs.len()).unwrap_or(u64::MAX)
    } else {
        u64::try_from(source_entries.len()).unwrap_or(u64::MAX)
    };
    let population = page_count
        .saturating_add(reference_count)
        .saturating_add(source_population)
        .saturating_add(u64::from(parse_error));
    let evidence_positions = (0..usize::try_from(error_count.min(100)).unwrap_or(100)).collect();
    let mut assessment =
        Assessment::from_aggregate(population, 0, error_count, true, evidence_positions);
    assessment.set_metrics(vec![
        metric(LintMetricCode::PageManifestPages, page_count),
        metric(LintMetricCode::PageManifestReferences, reference_count),
        metric(
            LintMetricCode::PageSourceStubs,
            u64::try_from(observed_stubs.len()).unwrap_or(u64::MAX),
        ),
        metric(
            LintMetricCode::PageManifestDivergences,
            u64::try_from(divergence_count).unwrap_or(u64::MAX),
        ),
    ]);
    assessment
}

async fn load_artifacts(context: &LintContext<'_, '_>) -> Result<Assessment, ()> {
    let scan = context.page_scan().ok_or(())?;
    let counts = ArtifactCounts::from_scan(scan);
    let links = load_link_counts(context).await?;
    let mut assessment = Assessment::default();
    assessment.mark_inventory();
    for _ in 0..10 {
        assessment.push(Level::Pass);
    }
    assessment.set_metrics(vec![
        metric(LintMetricCode::ProjectPurposeArtifacts, counts.purpose),
        metric(LintMetricCode::ProjectSchemaArtifacts, counts.schema),
        metric(LintMetricCode::ProjectIndexArtifacts, counts.index),
        metric(LintMetricCode::ProjectLogArtifacts, counts.log),
        metric(LintMetricCode::ProjectOverviewArtifacts, counts.overview),
        metric(LintMetricCode::PageSourceStubs, counts.source_stubs),
        metric(LintMetricCode::ProjectArchiveRecords, links.archived),
        metric(LintMetricCode::ProjectOutboundLinks, links.outbound),
        metric(LintMetricCode::ProjectInboundLinks, links.inbound),
        metric(LintMetricCode::ProjectBrokenLinks, links.broken),
    ]);
    Ok(assessment)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ArtifactCounts {
    purpose: u64,
    schema: u64,
    index: u64,
    log: u64,
    overview: u64,
    source_stubs: u64,
}

impl ArtifactCounts {
    fn from_scan(scan: &PageScan) -> Self {
        let mut counts = Self::default();
        for entry in scan
            .entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::File)
        {
            let basename = entry
                .path
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            match basename.as_str() {
                "purpose.md" => counts.purpose += 1,
                "schema.md" => counts.schema += 1,
                "index.md" | "_index.md" => counts.index += 1,
                "log.md" => counts.log += 1,
                "overview.md" => counts.overview += 1,
                _ => {}
            }
            if entry.scope == EntryScope::SourceInventory && basename.ends_with(".md") {
                counts.source_stubs += 1;
            }
        }
        counts
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct LinkCounts {
    archived: u64,
    outbound: u64,
    inbound: u64,
    broken: u64,
}

async fn load_link_counts(context: &LintContext<'_, '_>) -> Result<LinkCounts, ()> {
    let mut rows = context
        .snapshot()
        .query(
            "SELECT \
               (SELECT COUNT(*) FROM pages WHERE status = 'archived'), \
               COUNT(*), \
               COALESCE(SUM(CASE WHEN pl.target_page_id IS NOT NULL THEN 1 ELSE 0 END), 0), \
               COALESCE(SUM(CASE WHEN pl.target_page_id IS NULL THEN 1 ELSE 0 END), 0) \
             FROM page_links pl",
            libsql::params::Params::None,
        )
        .await
        .map_err(|_| ())?;
    let row = rows.next().await.map_err(|_| ())?.ok_or(())?;
    Ok(LinkCounts {
        archived: count_column(&row, 0)?,
        outbound: count_column(&row, 1)?,
        inbound: count_column(&row, 2)?,
        broken: count_column(&row, 3)?,
    })
}

fn count_column(row: &libsql::Row, index: i32) -> Result<u64, ()> {
    u64::try_from(row.get::<i64>(index).map_err(|_| ())?).map_err(|_| ())
}

fn metric(code: LintMetricCode, value: u64) -> LintMetric {
    LintMetric::new(code, LintMetricValue::Count { value })
}

#[cfg(test)]
#[path = "link_checks_test.rs"]
mod tests;
