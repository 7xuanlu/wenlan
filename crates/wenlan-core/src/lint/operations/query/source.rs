use super::{metric, AgeBuckets};
use crate::lint::operations::config::OperationsRunConfig;
use crate::lint::operations::read_context::OperationsReadContext;
use crate::lint::operations::result::Assessment;
use crate::lint::operations::SOURCE_CONFIGURATION;
use std::collections::BTreeSet;
use wenlan_types::lint::{
    LintMetric, LintMetricCode, LintMetricStringCode, LintMetricValue, LintReasonCode,
    LintSeverity, LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) async fn load(
    context: &OperationsReadContext<'_, '_>,
    config: OperationsRunConfig,
) -> Result<Assessment, ()> {
    let mut affected_positions = config
        .invalid_positions
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    affected_positions.extend(config.terminal_positions.iter().copied());
    let mut rows = context
        .snapshot()
        .query(
            "SELECT source_id,file_path,mtime_ns,content_hash,last_synced_at
           FROM source_sync_state ORDER BY source_id,file_path",
            libsql::params::Params::None,
        )
        .await
        .map_err(|_| ())?;
    let mut sync_count = 0_u64;
    let mut sync_invalid = 0_u64;
    let mut ages = AgeBuckets::default();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        let source_id = row.get::<String>(0).map_err(|_| ())?;
        let file_path = row.get::<String>(1).map_err(|_| ())?;
        let mtime = row.get::<i64>(2).map_err(|_| ())?;
        let hash = row.get::<String>(3).map_err(|_| ())?;
        let last_sync = row.get::<i64>(4).map_err(|_| ())?;
        let valid = !source_id.trim().is_empty()
            && !file_path.trim().is_empty()
            && mtime >= 0
            && !hash.trim().is_empty()
            && ages.observe(last_sync, context.clock().epoch_seconds())
            && (!config.captured || config.configured_ids.contains(&source_id));
        if !valid {
            sync_invalid = sync_invalid.saturating_add(1);
            if let Ok(offset) = usize::try_from(sync_count) {
                let base = usize::try_from(config.source_count).unwrap_or(usize::MAX);
                affected_positions.insert(base.saturating_add(offset));
            }
        }
        sync_count = sync_count.saturating_add(1);
    }
    let invalid = u64::try_from(config.invalid_positions.len())
        .unwrap_or(u64::MAX)
        .saturating_add(sync_invalid);
    let terminal = u64::try_from(config.terminal_positions.len()).unwrap_or(u64::MAX);
    let population = config.source_count.saturating_add(sync_count);
    let affected = u64::try_from(affected_positions.len()).unwrap_or(u64::MAX);
    let mut metrics = vec![
        metric(LintMetricCode::SourceConfigurations, config.source_count),
        metric(LintMetricCode::SourceInvalidConfigurations, invalid),
        metric(LintMetricCode::SourceTerminalFailures, terminal),
        metric(LintMetricCode::SourceSyncCheckpoints, sync_count),
        metric(LintMetricCode::AffectedRecords, affected),
        status_metric(config.captured),
    ];
    metrics.extend(ages.metrics());
    let mut reasons = Vec::new();
    if invalid > 0 {
        reasons.push(LintReasonCode::InvalidSourceConfiguration);
    }
    if terminal > 0 {
        reasons.push(LintReasonCode::TerminalOperationFailure);
    }
    Ok(Assessment::inventory(
        SOURCE_CONFIGURATION,
        population,
        affected,
        LintSeverity::Error,
        metrics,
        affected_positions
            .into_iter()
            .take(usize::from(LINT_MAX_EVIDENCE_PER_CHECK))
            .collect(),
        reasons,
    ))
}

fn status_metric(captured: bool) -> LintMetric {
    LintMetric::new(
        LintMetricCode::SourceConfigurationStatus,
        LintMetricValue::CatalogCode {
            code: if captured {
                LintMetricStringCode::Present
            } else {
                LintMetricStringCode::Missing
            },
        },
    )
}
