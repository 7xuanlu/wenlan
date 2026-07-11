use super::{metric, AgeBuckets};
use crate::lint::context::LintContext;
use crate::lint::operations::result::Assessment;
use crate::lint::operations::{REFINEMENT_INVENTORY, REJECTION_INVENTORY};
use wenlan_types::lint::{
    LintMetricCode, LintReasonCode, LintSeverity, LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) async fn load_refinements(context: &LintContext<'_, '_>) -> Result<Assessment, ()> {
    let mut rows = context
        .snapshot()
        .query(
            "SELECT action,source_ids,status,created_at FROM refinement_queue ORDER BY id",
            libsql::params::Params::None,
        )
        .await
        .map_err(|_| ())?;
    let mut counts = ReviewCounts::default();
    let mut positions = Vec::new();
    let mut ages = AgeBuckets::default();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        let action = row.get::<String>(0).map_err(|_| ())?;
        let source_ids = row.get::<String>(1).map_err(|_| ())?;
        let status = row.get::<String>(2).map_err(|_| ())?;
        let created = row.get::<String>(3).map_err(|_| ())?;
        let timestamp = parse_timestamp(&created);
        let known_status = match status.as_str() {
            "pending" => {
                counts.pending = counts.pending.saturating_add(1);
                true
            }
            "awaiting_review" => {
                counts.awaiting = counts.awaiting.saturating_add(1);
                true
            }
            "auto_applied" | "resolved" | "dismissed" => {
                counts.terminal = counts.terminal.saturating_add(1);
                true
            }
            _ => false,
        };
        let source_ids_valid = serde_json::from_str::<Vec<String>>(&source_ids).is_ok();
        let valid = !action.trim().is_empty()
            && source_ids_valid
            && known_status
            && timestamp.is_some_and(|value| ages.observe(value, context.clock().epoch_seconds()));
        if !valid {
            counts.invalid = counts.invalid.saturating_add(1);
            push_position(&mut positions, counts.population);
        }
        counts.population = counts.population.saturating_add(1);
    }
    let mut metrics = vec![
        metric(LintMetricCode::ObservedRecords, counts.population),
        metric(LintMetricCode::AffectedRecords, counts.invalid),
        metric(LintMetricCode::OperationPending, counts.pending),
        metric(LintMetricCode::OperationAwaitingReview, counts.awaiting),
        metric(LintMetricCode::OperationTerminal, counts.terminal),
        metric(LintMetricCode::OperationInvalidStates, counts.invalid),
    ];
    metrics.extend(ages.metrics());
    Ok(Assessment::inventory(
        REFINEMENT_INVENTORY,
        counts.population,
        counts.invalid,
        LintSeverity::Error,
        metrics,
        positions,
        (counts.invalid > 0)
            .then_some(LintReasonCode::InvalidOperationState)
            .into_iter()
            .collect(),
    ))
}

pub(super) async fn load_rejections(context: &LintContext<'_, '_>) -> Result<Assessment, ()> {
    let mut rows = context
        .snapshot()
        .query(
            "SELECT rejection_reason,created_at FROM rejected_memories ORDER BY id",
            libsql::params::Params::None,
        )
        .await
        .map_err(|_| ())?;
    let mut population = 0_u64;
    let mut invalid = 0_u64;
    let mut positions = Vec::new();
    let mut ages = AgeBuckets::default();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        let reason = row.get::<String>(0).map_err(|_| ())?;
        let created = row.get::<i64>(1).map_err(|_| ())?;
        if reason.trim().is_empty() || !ages.observe(created, context.clock().epoch_seconds()) {
            invalid = invalid.saturating_add(1);
            push_position(&mut positions, population);
        }
        population = population.saturating_add(1);
    }
    let mut metrics = vec![
        metric(LintMetricCode::ObservedRecords, population),
        metric(LintMetricCode::AffectedRecords, invalid),
        metric(LintMetricCode::OperationInvalidStates, invalid),
    ];
    metrics.extend(ages.metrics());
    Ok(Assessment::inventory(
        REJECTION_INVENTORY,
        population,
        invalid,
        LintSeverity::Error,
        metrics,
        positions,
        (invalid > 0)
            .then_some(LintReasonCode::InvalidOperationState)
            .into_iter()
            .collect(),
    ))
}

#[derive(Default)]
struct ReviewCounts {
    population: u64,
    pending: u64,
    awaiting: u64,
    terminal: u64,
    invalid: u64,
}

fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp())
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .map(|value| value.and_utc().timestamp())
        })
        .ok()
}

fn push_position(positions: &mut Vec<usize>, position: u64) {
    if positions.len() < usize::from(LINT_MAX_EVIDENCE_PER_CHECK) {
        if let Ok(position) = usize::try_from(position) {
            positions.push(position);
        }
    }
}
