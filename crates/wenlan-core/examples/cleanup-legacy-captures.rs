// SPDX-License-Identifier: Apache-2.0
use anyhow::{bail, Context, Result};
use libsql::{Connection, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .context("expected database path")?;
    let expected = match (args.next(), args.next()) {
        (None, None) => None,
        (Some(flag), Some(value)) if flag == "--expect" => Some(
            value
                .into_string()
                .map_err(|_| anyhow::anyhow!("plan token must be valid UTF-8"))?,
        ),
        _ => bail!("usage: cleanup-legacy-captures DB [--expect PLAN_TOKEN]"),
    };
    if !path.is_file() {
        bail!("database path does not exist");
    }

    let database = libsql::Builder::new_local(&path)
        .build()
        .await
        .context("open local libSQL database")?;
    let connection = database.connect().context("connect to local database")?;
    if expected.is_none() {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::ReadOnly)
            .await
            .context("begin cleanup plan snapshot")?;
        let token = database_fingerprint(&transaction).await?;
        transaction
            .rollback()
            .await
            .context("close cleanup plan snapshot")?;
        println!("{token}");
        return Ok(());
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .context("begin cleanup transaction")?;

    let result = async {
        let actual = database_fingerprint(&transaction).await?;
        if expected.as_deref() != Some(actual.as_str()) {
            bail!("plan token mismatch; database changed after preview");
        }
        cleanup(&transaction).await
    }
    .await;
    if let Err(error) = result {
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

async fn database_fingerprint(connection: &Connection) -> Result<String> {
    let mut digest = Sha256::new();
    digest_value(&mut digest, b"wenlan-legacy-cleanup-plan-v2");
    fingerprint_query(
        connection,
        &mut digest,
        "sqlite_schema",
        "SELECT quote(type), quote(name), quote(tbl_name), quote(rootpage), quote(sql)
           FROM sqlite_schema
          ORDER BY type, name, tbl_name, rootpage, sql",
        5,
    )
    .await?;

    // Hash only tables that can change target selection, blockers, or cleanup
    // effects. Legacy external-content FTS and vector shadow tables are not
    // cleanup inputs and may be unreadable without their original extension.
    for table in [
        "access_log",
        "activities",
        "agent_activity",
        "capture_refs",
        "derived_artifact_sweeps",
        "document_enrichment_queue",
        "document_tags",
        "enrichment_steps",
        "eval_judgments",
        "eval_signals",
        "memories",
        "memory_entities",
        "page_evidence",
        "page_sources",
        "pages",
        "refinement_queue",
        "rejected_memories",
        "relations",
        "source_sync_state",
        "summary_node_sources",
    ] {
        let columns = column_names(connection, table).await?;
        digest_value(&mut digest, table.as_bytes());
        for column in &columns {
            digest_value(&mut digest, column.as_bytes());
        }
        if columns.is_empty() {
            continue;
        }
        let quoted = columns
            .iter()
            .map(|column| format!("quote({})", quote_identifier(column)))
            .collect::<Vec<_>>();
        let order = (1..=columns.len())
            .map(|index| index.to_string())
            .collect::<Vec<_>>();
        let sql = format!(
            "SELECT {} FROM {} ORDER BY {}",
            quoted.join(", "),
            quote_identifier(table),
            order.join(", ")
        );
        fingerprint_query(connection, &mut digest, table, &sql, columns.len()).await?;
    }
    Ok(hex::encode(digest.finalize()))
}

async fn column_names(connection: &Connection, table: &str) -> Result<Vec<String>> {
    let sql = format!("PRAGMA table_info({})", quote_literal(table));
    let mut rows = connection.query(&sql, ()).await?;
    let mut columns = Vec::new();
    while let Some(row) = rows.next().await? {
        columns.push(row.get::<String>(1)?);
    }
    Ok(columns)
}

async fn fingerprint_query(
    connection: &Connection,
    digest: &mut Sha256,
    label: &str,
    sql: &str,
    column_count: usize,
) -> Result<()> {
    digest_value(digest, label.as_bytes());
    let mut rows = connection
        .query(sql, ())
        .await
        .with_context(|| format!("fingerprint cleanup population {label}"))?;
    while let Some(row) = rows.next().await? {
        digest_value(digest, b"row");
        for index in 0..column_count {
            let index = i32::try_from(index).context("cleanup fingerprint column count")?;
            digest_value(digest, row.get::<String>(index)?.as_bytes());
        }
    }
    Ok(())
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn digest_value(digest: &mut Sha256, value: &[u8]) {
    digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    digest.update(value);
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
               WHERE instr(',' || COALESCE(agent_activity.memory_ids,'') || ',',
                           ',' || t.source_id || ',') > 0
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
