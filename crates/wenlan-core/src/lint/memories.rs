use super::catalog::{catalog_group, LintCheckGroup};
use super::context::{LintContext, PopulationBasis};
use super::runner::failed_results_for_group;
use wenlan_types::lint::{
    LintApplicability, LintCheckResult, LintMetric, LintPrecondition, LintSeverity,
};

mod query;
mod result;
use query::load_records;
use result::{base_metrics, finish, partition_assessment};

const LIFECYCLE_ID: &str = "memories.lifecycle_integrity";
const SUPERSESSION_ID: &str = "memories.supersession_integrity";
const EMBEDDING_ID: &str = "memories.embedding_integrity";
const ENRICHMENT_ID: &str = "memories.enrichment_failures";
const PARTITIONS_ID: &str = "memories.partition_inventory";
const EPISODE_ID: &str = "memories.derived.episode";
const FACT_ID: &str = "memories.derived.fact";
const PAGE_ID: &str = "memories.derived.page_links";
const SUMMARY_ID: &str = "memories.derived.summary";
const TEMPORAL_ID: &str = "memories.derived.temporal";

#[derive(Debug, Clone)]
struct MemoryRecord {
    source_id: String,
    lifecycle_valid: bool,
    supersedes: Option<String>,
    target_exists: bool,
    replaced_by_active: bool,
    pending_revision: bool,
    recap: bool,
    embedding_complete: bool,
    needs_reembed: bool,
    failed_steps: u64,
    classified: bool,
    event_dated: bool,
    episode: bool,
    fact: bool,
    page_link: bool,
    summary: bool,
    head: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MemoryFeatureConfig {
    pub(crate) episode: bool,
    pub(crate) fact: bool,
    pub(crate) page: bool,
    pub(crate) summary: bool,
    pub(crate) temporal: bool,
    readiness: Option<DerivedReadiness>,
}

impl MemoryFeatureConfig {
    pub(crate) fn capture(page: bool) -> Self {
        Self {
            episode: crate::db::episode_channel_enabled(),
            fact: crate::retrieval::fact_channel::fact_channel_enabled(),
            page,
            summary: crate::db::global_prelude_enabled(),
            temporal: crate::db::temporal_grounding_enabled(),
            readiness: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DerivedReadiness {
    provider_ready: bool,
    active_backfill: bool,
    completed_sweeps: u8,
    missing_age_seconds: u64,
}

impl DerivedReadiness {
    fn overdue(self) -> bool {
        self.provider_ready
            && !self.active_backfill
            && self.completed_sweeps >= 2
            && self.missing_age_seconds >= 3_600
    }
}

pub(crate) async fn run(
    context: &LintContext<'_, '_>,
    config: MemoryFeatureConfig,
) -> Vec<LintCheckResult> {
    let mut records = match load_records(context).await {
        Ok(records) => records,
        Err(()) => return failed_memory_results(context),
    };
    mark_heads_and_supersession(&mut records);
    assessments(&records, config)
        .into_iter()
        .map(|assessment| finish(context, assessment))
        .collect()
}

fn failed_memory_results(context: &LintContext<'_, '_>) -> Vec<LintCheckResult> {
    let basis = if context.scope().filter().is_selected() {
        PopulationBasis::SelectedScope
    } else {
        PopulationBasis::Global
    };
    for entry in catalog_group(LintCheckGroup::Memories) {
        let _ = context.record_population(entry.id, basis, 0);
    }
    failed_results_for_group(context.clock(), LintCheckGroup::Memories)
}

struct CheckAssessment {
    id: &'static str,
    levels: Vec<LintSeverity>,
    applicability: LintApplicability,
    precondition: LintPrecondition,
    metrics: Vec<LintMetric>,
}

fn assessments(records: &[MemoryRecord], config: MemoryFeatureConfig) -> Vec<CheckAssessment> {
    let heads = records
        .iter()
        .filter(|record| record.head)
        .collect::<Vec<_>>();
    let lifecycle = structural(LIFECYCLE_ID, records, |record| record.lifecycle_valid);
    let supersession = structural(SUPERSESSION_ID, records, |record| {
        record.supersedes.as_deref() != Some(record.source_id.as_str())
            && record.target_exists
            && (!record.pending_revision || record.supersedes.is_some())
    });
    let embedding = structural(EMBEDDING_ID, &heads, |record| {
        record.embedding_complete || record.needs_reembed
    });
    let enrichment = structural(ENRICHMENT_ID, records, |record| record.failed_steps == 0);
    let mut checks = vec![lifecycle, supersession, embedding, enrichment];
    checks.push(partition_assessment(&heads, records));
    checks.extend([
        derived(EPISODE_ID, &heads, config.episode, config.readiness, |r| {
            r.episode
        }),
        derived(FACT_ID, &heads, config.fact, config.readiness, |r| r.fact),
        derived(PAGE_ID, &heads, config.page, None, |r| r.page_link),
        derived(SUMMARY_ID, &heads, config.summary, config.readiness, |r| {
            r.summary
        }),
        derived(TEMPORAL_ID, &heads, config.temporal, None, |r| {
            r.event_dated
        }),
    ]);
    checks
}

fn structural<T>(id: &'static str, records: &[T], valid: impl Fn(&T) -> bool) -> CheckAssessment {
    CheckAssessment {
        id,
        levels: records
            .iter()
            .map(|record| {
                if valid(record) {
                    LintSeverity::Info
                } else {
                    LintSeverity::Error
                }
            })
            .collect(),
        applicability: if records.is_empty() {
            LintApplicability::Inventory
        } else {
            LintApplicability::Applicable
        },
        precondition: LintPrecondition::Ready,
        metrics: base_metrics(records.len(), records.iter().filter(|r| !valid(r)).count()),
    }
}

fn derived(
    id: &'static str,
    heads: &[&MemoryRecord],
    enabled: bool,
    readiness: Option<DerivedReadiness>,
    present: impl Fn(&MemoryRecord) -> bool,
) -> CheckAssessment {
    let missing = heads.iter().filter(|record| !present(record)).count();
    let actionable = enabled && readiness.is_some_and(DerivedReadiness::overdue);
    CheckAssessment {
        id,
        levels: heads
            .iter()
            .map(|record| {
                if present(record) || !actionable {
                    LintSeverity::Info
                } else {
                    LintSeverity::Warning
                }
            })
            .collect(),
        applicability: if !enabled {
            LintApplicability::ExpectedEmpty
        } else if actionable {
            LintApplicability::Applicable
        } else {
            LintApplicability::Inventory
        },
        precondition: if enabled {
            LintPrecondition::Ready
        } else {
            LintPrecondition::ConfiguredOff
        },
        metrics: base_metrics(heads.len(), missing),
    }
}

fn mark_heads_and_supersession(records: &mut [MemoryRecord]) {
    for record in records {
        record.head = !record.pending_revision && !record.recap && !record.replaced_by_active;
    }
}

#[cfg(test)]
#[path = "memories_test.rs"]
mod tests;
