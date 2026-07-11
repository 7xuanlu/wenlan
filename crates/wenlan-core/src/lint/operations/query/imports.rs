use super::{metric, AgeBuckets};
use crate::lint::context::LintContext;
use crate::lint::operations::result::Assessment;
use crate::lint::operations::IMPORT_CHECKPOINTS;
use wenlan_types::lint::{
    LintMetricCode, LintReasonCode, LintSeverity, LINT_MAX_EVIDENCE_PER_CHECK,
};

pub(super) async fn load(context: &LintContext<'_, '_>) -> Result<Assessment, ()> {
    let mut rows = context.snapshot().query(
        "SELECT vendor,total_conversations,processed_conversations,stage,error_message,updated_at
           FROM import_state ORDER BY id",
        libsql::params::Params::None,
    ).await.map_err(|_| ())?;
    let mut population = 0_u64;
    let mut invalid = 0_u64;
    let mut terminal = 0_u64;
    let mut positions = Vec::new();
    let mut ages = AgeBuckets::default();
    while let Some(row) = rows.next().await.map_err(|_| ())? {
        let vendor = row.get::<String>(0).map_err(|_| ())?;
        let total = row.get::<Option<i64>>(1).map_err(|_| ())?;
        let processed = row.get::<i64>(2).map_err(|_| ())?;
        let stage = row.get::<String>(3).map_err(|_| ())?;
        let error = row.get::<Option<String>>(4).map_err(|_| ())?;
        let updated = row.get::<String>(5).map_err(|_| ())?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&updated)
            .map(|value| value.timestamp())
            .ok();
        let terminal_failure = stage == "error";
        let structurally_valid = matches!(vendor.as_str(), "chatgpt" | "claude")
            && matches!(
                stage.as_str(),
                "parsing" | "stage_a" | "stage_b" | "done" | "error"
            )
            && processed >= 0
            && total.is_none_or(|total| total >= 0 && processed <= total)
            && timestamp
                .is_some_and(|timestamp| ages.observe(timestamp, context.clock().epoch_seconds()))
            && (terminal_failure || error.is_none());
        if terminal_failure {
            terminal = terminal.saturating_add(1);
        }
        if !structurally_valid {
            invalid = invalid.saturating_add(1);
        }
        if terminal_failure || !structurally_valid {
            push_position(&mut positions, population);
        }
        population = population.saturating_add(1);
    }
    let affected = invalid.saturating_add(terminal);
    let mut metrics = vec![
        metric(LintMetricCode::ObservedRecords, population),
        metric(LintMetricCode::AffectedRecords, affected),
        metric(LintMetricCode::OperationTerminalFailures, terminal),
        metric(LintMetricCode::OperationInvalidStates, invalid),
    ];
    metrics.extend(ages.metrics());
    let mut reasons = Vec::new();
    if terminal > 0 {
        reasons.push(LintReasonCode::TerminalOperationFailure);
    }
    if invalid > 0 {
        reasons.push(LintReasonCode::InvalidOperationState);
    }
    Ok(Assessment::inventory(
        IMPORT_CHECKPOINTS,
        population,
        affected,
        LintSeverity::Error,
        metrics,
        positions,
        reasons,
    ))
}

fn push_position(positions: &mut Vec<usize>, position: u64) {
    if positions.len() < usize::from(LINT_MAX_EVIDENCE_PER_CHECK) {
        if let Ok(position) = usize::try_from(position) {
            positions.push(position);
        }
    }
}
