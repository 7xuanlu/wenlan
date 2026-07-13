use super::super::fs::{
    normalize_target_path, EntryKind, EntryScope, ManifestProjection, PageScan,
};
use super::super::provenance_checks::result::Assessment;
use crate::export::provenance::stub_filename;
use crate::lint::context::LintContext;
use std::collections::BTreeSet;
use wenlan_types::lint::{LintMetric, LintMetricCode, LintMetricValue};

pub(super) fn load(
    context: &LintContext<'_, '_>,
    suppress_evidence: bool,
) -> Result<Assessment, ()> {
    context
        .page_scan()
        .map(|scan| assess(scan, suppress_evidence))
        .ok_or(())
}

pub(super) fn assess(scan: &PageScan, suppress_evidence: bool) -> Assessment {
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
            let mut reference_count = 0_u64;
            let mut expected = BTreeSet::new();
            for source_ids in pages.values() {
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
            (
                u64::try_from(pages.len()).unwrap_or(u64::MAX),
                reference_count,
                expected,
                false,
            )
        }
    };
    let observed_stubs = source_entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File && entry.path.ends_with(".md"))
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    let structural_errors = u64::try_from(
        source_entries
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::Symlink | EntryKind::Other))
            .count(),
    )
    .unwrap_or(u64::MAX);
    let divergence_count = expected_stubs.symmetric_difference(&observed_stubs).count();
    let error_count = structural_errors
        .saturating_add(generated_path_errors)
        .saturating_add(u64::from(parse_error));
    let population = page_count
        .saturating_add(reference_count)
        .saturating_add(u64::try_from(source_entries.len()).unwrap_or(u64::MAX))
        .saturating_add(u64::from(parse_error));
    let evidence_positions = if suppress_evidence {
        Vec::new()
    } else {
        (0..usize::try_from(error_count.min(100)).unwrap_or(100)).collect()
    };
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

fn metric(code: LintMetricCode, value: u64) -> LintMetric {
    LintMetric::new(code, LintMetricValue::Count { value })
}
