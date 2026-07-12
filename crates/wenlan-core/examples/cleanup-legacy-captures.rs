// SPDX-License-Identifier: Apache-2.0
use anyhow::{bail, Context, Result};
use libsql::{Connection, TransactionBehavior};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("expected database path")?;
    if !path.is_file() {
        bail!("database path does not exist");
    }

    let database = libsql::Builder::new_local(&path)
        .build()
        .await
        .context("open local libSQL database")?;
    let connection = database.connect().context("connect to local database")?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .context("begin cleanup transaction")?;

    if let Err(error) = cleanup(&transaction).await {
        transaction
            .rollback()
            .await
            .context("rollback failed cleanup transaction")?;
        return Err(error);
    }
    transaction
        .commit()
        .await
        .context("commit cleanup transaction")?;
    Ok(())
}

async fn cleanup(connection: &Connection) -> Result<()> {
    for statement in [
        "CREATE TEMP TABLE lint_legacy_capture_targets (source_id TEXT PRIMARY KEY)",
        "INSERT OR IGNORE INTO lint_legacy_capture_targets
         SELECT DISTINCT source_id FROM memories WHERE source='hotkey_capture'",
        "INSERT OR IGNORE INTO lint_legacy_capture_targets
         SELECT DISTINCT source_id FROM capture_refs WHERE source='hotkey'",
        "CREATE TEMP TABLE lint_legacy_capture_activities (activity_id TEXT PRIMARY KEY)",
        "INSERT OR IGNORE INTO lint_legacy_capture_activities
         SELECT DISTINCT activity_id FROM capture_refs WHERE source='hotkey'",
        "DELETE FROM agent_activity
          WHERE EXISTS (
              SELECT 1 FROM lint_legacy_capture_targets t
               WHERE instr(COALESCE(agent_activity.memory_ids,''),t.source_id)>0
          )",
        "DELETE FROM access_log
          WHERE source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM memory_entities
          WHERE memory_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM relations
          WHERE source_memory_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM eval_signals
          WHERE memory_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM eval_judgments
          WHERE memory_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "UPDATE rejected_memories SET similar_to_source_id=NULL
          WHERE similar_to_source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM source_sync_state
          WHERE source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM enrichment_steps
          WHERE source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM summary_node_sources
          WHERE memory_source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM document_enrichment_queue
          WHERE source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM derived_artifact_sweeps
          WHERE source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM document_tags
          WHERE source IN ('focus_capture','ambient','hotkey_capture')
             OR source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM capture_refs
          WHERE source='hotkey'
             OR source_id IN (SELECT source_id FROM lint_legacy_capture_targets)",
        "DELETE FROM activities
          WHERE id IN (SELECT activity_id FROM lint_legacy_capture_activities)
            AND NOT EXISTS (
                SELECT 1 FROM capture_refs WHERE capture_refs.activity_id=activities.id
            )",
        "DELETE FROM memories WHERE source='hotkey_capture'",
    ] {
        connection
            .execute(statement, ())
            .await
            .with_context(|| format!("legacy cleanup statement failed: {statement}"))?;
    }
    Ok(())
}
