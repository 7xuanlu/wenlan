use super::{metric, AgeBuckets};
use crate::lint::context::LintContext;
use crate::lint::operations::result::Assessment;
use crate::lint::operations::MAINTENANCE_BACKLOGS;
use crate::reconcile::FrontierState;
use std::collections::BTreeMap;
use wenlan_types::lint::{
    LintMetricCode, LintReasonCode, LintSeverity, LINT_MAX_EVIDENCE_PER_CHECK,
};

const FRONTIER_DOCS: &str = "reconcile_frontier_docs";
const FRONTIER_CAPTURES: &str = "reconcile_frontier_captures";
const KEYS: [&str; 6] = [
    "compile_queue_depth_v1",
    "maintenance_retro_sweep_v1_complete",
    "maintenance_retro_sweep_v1_pause",
    "last_daily_steep_ts",
    FRONTIER_DOCS,
    FRONTIER_CAPTURES,
];

pub(super) async fn load(context: &LintContext<'_, '_>) -> Result<Assessment, ()> {
    let mut rows = context
        .snapshot()
        .query(
            "SELECT key,value FROM app_metadata
          WHERE key IN ('compile_queue_depth_v1','maintenance_retro_sweep_v1_complete',
                        'maintenance_retro_sweep_v1_pause','last_daily_steep_ts',
                        'reconcile_frontier_docs','reconcile_frontier_captures') ORDER BY key",
            libsql::params::Params::None,
        )
        .await
        .map_err(|_| ())?;
    let mut values = BTreeMap::new();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        values.insert(
            row.get::<String>(0).map_err(|_| ())?,
            row.get::<String>(1).map_err(|_| ())?,
        );
    }
    let mut invalid_positions = Vec::new();
    let mut pending = 0_u64;
    let mut missing_oracles = 0_u64;
    let mut no_progress = 0_u64;
    let mut ages = AgeBuckets::default();
    if let Some(value) = values.get(KEYS[0]) {
        match value.parse::<u64>() {
            Ok(value) => pending = value,
            Err(_) => invalid_positions.push(0),
        }
    }
    let complete = parse_flag(values.get(KEYS[1]), 1, &mut invalid_positions);
    let paused = parse_flag(values.get(KEYS[2]), 2, &mut invalid_positions);
    if complete == Some(true) && paused == Some(true) {
        invalid_positions.push(2);
    }
    if let Some(value) = values.get(KEYS[3]) {
        if value
            .parse::<i64>()
            .ok()
            .is_none_or(|timestamp| !ages.observe(timestamp, context.clock().epoch_seconds()))
        {
            invalid_positions.push(3);
        }
    }
    for (position, key) in [(4, FRONTIER_DOCS), (5, FRONTIER_CAPTURES)] {
        match values.get(key) {
            None => missing_oracles = missing_oracles.saturating_add(1),
            Some(value) => match serde_json::from_str::<FrontierState>(value) {
                Ok(frontier) if frontier.failures > 0 && frontier.stuck_id.is_some() => {
                    no_progress = no_progress.saturating_add(1);
                    invalid_positions.push(position);
                }
                Ok(frontier) if frontier.failures == 0 && frontier.stuck_id.is_none() => {}
                Ok(_) | Err(_) => invalid_positions.push(position),
            },
        }
    }
    invalid_positions.sort_unstable();
    invalid_positions.dedup();
    let invalid = u64::try_from(invalid_positions.len())
        .unwrap_or(u64::MAX)
        .saturating_sub(no_progress);
    let affected = invalid.saturating_add(no_progress);
    let mut metrics = vec![
        metric(LintMetricCode::AffectedRecords, affected),
        metric(LintMetricCode::PendingRecords, pending),
        metric(LintMetricCode::OperationInvalidStates, invalid),
        metric(LintMetricCode::OperationDurableNoProgress, no_progress),
        metric(
            LintMetricCode::OperationMissingProgressOracles,
            missing_oracles,
        ),
    ];
    metrics.extend(ages.metrics());
    let mut reasons = Vec::new();
    if invalid > 0 {
        reasons.push(LintReasonCode::InvalidOperationState);
    }
    if no_progress > 0 {
        reasons.push(LintReasonCode::DurableNoProgress);
    }
    Ok(Assessment::inventory(
        MAINTENANCE_BACKLOGS,
        u64::try_from(KEYS.len()).unwrap_or(u64::MAX),
        affected,
        if invalid > 0 {
            LintSeverity::Error
        } else {
            LintSeverity::Warning
        },
        metrics,
        invalid_positions
            .into_iter()
            .take(usize::from(LINT_MAX_EVIDENCE_PER_CHECK))
            .collect(),
        reasons,
    ))
}

fn parse_flag(value: Option<&String>, position: usize, invalid: &mut Vec<usize>) -> Option<bool> {
    match value.map(String::as_str) {
        None => None,
        Some("0") => Some(false),
        Some("1") => Some(true),
        Some(_) => {
            invalid.push(position);
            None
        }
    }
}
