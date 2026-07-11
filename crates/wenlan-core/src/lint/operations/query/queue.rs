use super::{metric, AgeBuckets};
use crate::lint::operations::read_context::OperationsReadContext;
use crate::lint::operations::result::Assessment;
use crate::lint::operations::DOCUMENT_QUEUE;
use wenlan_types::lint::{
    LintMetricCode, LintReasonCode, LintSeverity, LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) async fn load(context: &OperationsReadContext<'_, '_>) -> Result<Assessment, ()> {
    let mut rows = context.snapshot().query(
        "SELECT status,last_completed_chunk,attempt_count,next_retry_at,error_detail,enqueued_at,updated_at
           FROM document_enrichment_queue ORDER BY source_id,file_path",
        libsql::params::Params::None,
    ).await.map_err(|_| ())?;
    let mut counts = Counts::default();
    let mut positions = Vec::new();
    let mut ages = AgeBuckets::default();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        let status = row.get::<String>(0).map_err(|_| ())?;
        let checkpoint = row.get::<i64>(1).map_err(|_| ())?;
        let attempts = row.get::<i64>(2).map_err(|_| ())?;
        let retry_at = row.get::<Option<i64>>(3).map_err(|_| ())?;
        let error = row.get::<Option<String>>(4).map_err(|_| ())?;
        let enqueued_at = row.get::<i64>(5).map_err(|_| ())?;
        let updated_at = row.get::<i64>(6).map_err(|_| ())?;
        let common_valid = checkpoint >= -1
            && attempts >= 0
            && updated_at >= enqueued_at
            && ages.observe(enqueued_at, context.clock().epoch_seconds());
        let state_valid = match status.as_str() {
            "pending" => retry_at.is_none() && error.is_none(),
            "in_progress" => match (retry_at, error.as_deref()) {
                (None, None) => true,
                (Some(_), Some(value)) => attempts > 0 && !value.is_empty(),
                _ => false,
            },
            "paused" => {
                attempts > 0
                    && retry_at.is_some()
                    && error.as_deref().is_some_and(|v| !v.is_empty())
            }
            "done" => retry_at.is_none() && error.is_none(),
            _ => false,
        };
        let expired = status == "paused"
            && retry_at.is_some_and(|retry_at| retry_at <= context.clock().epoch_seconds());
        let active = status == "paused"
            && retry_at.is_some_and(|retry_at| retry_at > context.clock().epoch_seconds());
        counts.population = counts.population.saturating_add(1);
        counts.active_retries = counts.active_retries.saturating_add(u64::from(active));
        counts.expired_retries = counts.expired_retries.saturating_add(u64::from(expired));
        counts.pending = counts.pending.saturating_add(u64::from(status != "done"));
        if !common_valid || !state_valid {
            counts.invalid = counts.invalid.saturating_add(1);
        }
        if expired || !common_valid || !state_valid {
            counts.affected = counts.affected.saturating_add(1);
            push_position(&mut positions, counts.population.saturating_sub(1));
        }
    }
    let mut metrics = vec![
        metric(LintMetricCode::ObservedRecords, counts.population),
        metric(LintMetricCode::AffectedRecords, counts.affected),
        metric(LintMetricCode::PendingRecords, counts.pending),
        metric(
            LintMetricCode::OperationActiveRetries,
            counts.active_retries,
        ),
        metric(
            LintMetricCode::OperationExpiredRetries,
            counts.expired_retries,
        ),
        metric(LintMetricCode::OperationInvalidStates, counts.invalid),
    ];
    metrics.extend(ages.metrics());
    let mut reasons = Vec::new();
    if counts.expired_retries > 0 {
        reasons.push(LintReasonCode::ExpiredRetry);
    }
    if counts.invalid > 0 {
        reasons.push(LintReasonCode::InvalidOperationState);
    }
    Ok(Assessment::inventory(
        DOCUMENT_QUEUE,
        counts.population,
        counts.affected,
        if counts.invalid > 0 {
            LintSeverity::Error
        } else {
            LintSeverity::Warning
        },
        metrics,
        positions,
        reasons,
    ))
}

#[derive(Default)]
struct Counts {
    population: u64,
    pending: u64,
    active_retries: u64,
    expired_retries: u64,
    invalid: u64,
    affected: u64,
}

fn push_position(positions: &mut Vec<usize>, position: u64) {
    if positions.len() < usize::from(LINT_MAX_EVIDENCE_PER_CHECK) {
        if let Ok(position) = usize::try_from(position) {
            positions.push(position);
        }
    }
}
