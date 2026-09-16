// SPDX-License-Identifier: Apache-2.0
//! Bounded, lossless input/effect capture shared by prepare and apply CAS.

use super::{repair_digest, RepairDigestExclusion, REPAIR_ROLLBACK_ARTIFACT_MAX_BYTES};
use crate::{
    db::MemoryDB,
    error::WenlanError,
    lint::snapshot::{LintReadSnapshot, LintRows},
};
use std::collections::BTreeMap;
use wenlan_types::{
    repair::RepairDigest,
    repair_relation::{
        RepairRelationSnapshot, RepairRelationSqlValue, RepairRelationTable,
        RepairRelationTableSnapshot,
    },
};

#[cfg(test)]
#[path = "relation_snapshot_tests.rs"]
mod tests;

pub(crate) struct RelationCaptureContext<'a> {
    pub manifest_id: &'a str,
    pub review_id: &'a str,
    pub from_entity: &'a str,
    pub to_entity: &'a str,
    pub owner_ids: &'a [String],
    pub canonical_relation_type: Option<&'a str>,
    pub vocabulary_promotion: Option<&'a str>,
}

pub(crate) enum RelationReader<'a, 'db> {
    Snapshot(&'a LintReadSnapshot<'db>),
    Connection(&'a libsql::Connection),
}

enum Rows<'a> {
    Snapshot(LintRows<'a>),
    Connection(libsql::Rows),
}
impl Rows<'_> {
    async fn next(&mut self) -> Result<Option<libsql::Row>, WenlanError> {
        match self {
            Self::Snapshot(rows) => rows.next().await.map_err(|e| database_error(e.to_string())),
            Self::Connection(rows) => rows.next().await.map_err(|e| database_error(e.to_string())),
        }
    }
}
impl RelationReader<'_, '_> {
    async fn query<'a>(
        &'a self,
        sql: &str,
        values: Vec<libsql::Value>,
    ) -> Result<Rows<'a>, WenlanError> {
        let parameters = libsql::params::Params::Positional(values);
        match self {
            Self::Snapshot(snapshot) => snapshot
                .query(sql, parameters)
                .await
                .map(Rows::Snapshot)
                .map_err(|e| database_error(e.to_string())),
            Self::Connection(connection) => connection
                .query(sql, parameters)
                .await
                .map(Rows::Connection)
                .map_err(|e| database_error(e.to_string())),
        }
    }
}

pub(crate) fn table_name(table: RepairRelationTable) -> &'static str {
    match table {
        RepairRelationTable::Edges => "edges",
        RepairRelationTable::EntityPageMap => "entity_page_map",
        RepairRelationTable::Pages => "pages",
        RepairRelationTable::Memories => "memories",
        RepairRelationTable::SpaceGraphState => "space_graph_state",
        RepairRelationTable::RelationTypeVocabulary => "relation_type_vocabulary",
        RepairRelationTable::RefinementQueue => "refinement_queue",
        RepairRelationTable::AgentActivity => "agent_activity",
    }
}

fn text(value: &str) -> libsql::Value {
    libsql::Value::Text(value.to_string())
}
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn database_error(detail: String) -> WenlanError {
    WenlanError::VectorDb(format!("repair relation capture: {detail}"))
}
fn too_large() -> WenlanError {
    WenlanError::Validation("repair_relation_snapshot_too_large".into())
}

pub(crate) async fn capture(
    reader: &RelationReader<'_, '_>,
    context: &RelationCaptureContext<'_>,
) -> Result<RepairRelationSnapshot, WenlanError> {
    let owners = serde_json::to_string(context.owner_ids)?;
    let promotion_id = context
        .vocabulary_promotion
        .map(|value| MemoryDB::vocab_proposal_fingerprint("relation", value));
    use RepairRelationTable::*;
    let queries = [
        (Edges, "edge_type='relates' AND src_id=?1 AND dst_id=?2", vec![text(context.from_entity), text(context.to_entity)]),
        (EntityPageMap, "entity_id IN (SELECT value FROM json_each(?1))", vec![text(&owners)]),
        (Pages, "id IN (SELECT page_id FROM entity_page_map WHERE entity_id IN (SELECT value FROM json_each(?1))) OR id IN (SELECT value FROM json_each(?1))", vec![text(&owners)]),
        (Memories, "source='memory' AND source_id IN (SELECT value FROM json_each(?1))", vec![text(&owners)]),
        (SpaceGraphState, "space IN (SELECT p.space FROM entity_page_map m JOIN pages p ON p.id=m.page_id WHERE m.entity_id IN (SELECT value FROM json_each(?1)) UNION SELECT space FROM edges WHERE edge_type='relates' AND src_id=?2 AND dst_id=?3)", vec![text(&owners), text(context.from_entity), text(context.to_entity)]),
        (RelationTypeVocabulary, "1=1", vec![]),
        (RefinementQueue, "id=?1 OR id=?2", vec![text(context.review_id), promotion_id.as_deref().map(text).unwrap_or(libsql::Value::Null)]),
        (AgentActivity, "query=?1", vec![text(context.manifest_id)]),
    ];
    let mut remaining = REPAIR_ROLLBACK_ARTIFACT_MAX_BYTES;
    let mut tables = Vec::with_capacity(queries.len());
    for (table, predicate, values) in queries {
        let name = table_name(table);
        let mut schema = reader
            .query(&format!("PRAGMA table_info({})", quoted(name)), vec![])
            .await?;
        let mut columns = Vec::new();
        let mut primary = Vec::new();
        while let Some(row) = schema.next().await? {
            let name = row
                .get::<String>(1)
                .map_err(|e| database_error(e.to_string()))?;
            let position = row
                .get::<i64>(5)
                .map_err(|e| database_error(e.to_string()))?;
            if position > 0 {
                primary.push((position, name.clone()));
            }
            columns.push(name);
        }
        drop(schema);
        if columns.is_empty() {
            return Err(database_error(format!("missing table {name}")));
        }
        primary.sort_by_key(|(position, _)| *position);
        let order = if primary.is_empty() {
            "_rowid_".to_string()
        } else {
            primary
                .iter()
                .map(|(_, name)| quoted(name))
                .collect::<Vec<_>>()
                .join(",")
        };
        // Preflight byte lengths before materializing a potentially huge text
        // or blob. JSON text escaping is at most six bytes per input byte;
        // hexadecimal blob capture doubles the payload.
        let lengths = columns.iter().map(|column| {
            let col = quoted(column);
            format!("(64 + CASE typeof({col}) WHEN 'blob' THEN length({col})*2 WHEN 'text' THEN length(CAST({col} AS BLOB))*6 ELSE 32 END)")
        }).collect::<Vec<_>>().join("+");
        let mut sizes = reader
            .query(
                &format!(
                    "SELECT COALESCE(SUM({lengths}),0) FROM {} WHERE {predicate}",
                    quoted(name)
                ),
                values.clone(),
            )
            .await?;
        let size = sizes
            .next()
            .await?
            .ok_or_else(|| database_error("missing size aggregate".into()))?
            .get::<i64>(0)
            .map_err(|e| database_error(e.to_string()))?;
        let estimated = u64::try_from(size).map_err(|_| too_large())?;
        if estimated > remaining {
            return Err(too_large());
        }
        drop(sizes);
        let query = format!(
            "SELECT {} FROM {} WHERE {predicate} ORDER BY {order}",
            columns
                .iter()
                .map(|c| quoted(c))
                .collect::<Vec<_>>()
                .join(","),
            quoted(name)
        );
        let mut result = reader.query(&query, values).await?;
        let mut rows = Vec::new();
        while let Some(row) = result.next().await? {
            let mut encoded = Vec::with_capacity(columns.len());
            for index in 0..columns.len() {
                let value = row
                    .get_value(i32::try_from(index).map_err(|_| too_large())?)
                    .map_err(|e| database_error(e.to_string()))?;
                encoded.push(match value {
                    libsql::Value::Null => RepairRelationSqlValue::Null,
                    libsql::Value::Integer(value) => RepairRelationSqlValue::Integer { value },
                    libsql::Value::Real(value) => RepairRelationSqlValue::Real {
                        bits: format!("{:016x}", value.to_bits()),
                    },
                    libsql::Value::Text(value) => RepairRelationSqlValue::Text { value },
                    libsql::Value::Blob(bytes) => RepairRelationSqlValue::Blob {
                        hex: hex::encode(bytes),
                    },
                });
            }
            rows.push(encoded);
        }
        let table = RepairRelationTableSnapshot {
            table,
            columns,
            rows,
        };
        let bytes = serde_json::to_vec(&table)?;
        remaining = remaining
            .checked_sub(bytes.len() as u64)
            .ok_or_else(too_large)?;
        tables.push(table);
    }
    let snapshot = RepairRelationSnapshot { tables };
    snapshot.validate().map_err(WenlanError::Validation)?;
    Ok(snapshot)
}

pub(crate) fn receipt(snapshot: &RepairRelationSnapshot) -> Result<RepairDigest, WenlanError> {
    Ok(repair_digest(&serde_json::to_vec(snapshot)?))
}

/// Bind the durable result, not the read dependencies or shared counters.
/// Prepare/apply still compare the full lossless snapshot and validate every
/// transaction effect. After commit, recall counters, page enrichment and graph
/// generations can advance independently while Deep verification runs. They
/// must not strand verification or pending-receipt recovery. Fresh lint reports
/// separately establish semantic correctness against the current source state.
/// Pair edges, endpoint identities, this manifest's activity and review payload
/// stay bound, including retired edges and their original provenance.
pub(crate) fn applied_receipt(
    snapshot: &RepairRelationSnapshot,
    context: &RelationCaptureContext<'_>,
) -> Result<RepairDigest, WenlanError> {
    snapshot.validate().map_err(WenlanError::Validation)?;
    let mut applied = snapshot.clone();
    applied.tables.retain(|table| {
        !matches!(
            table.table,
            RepairRelationTable::Pages
                | RepairRelationTable::Memories
                | RepairRelationTable::SpaceGraphState
        )
    });
    let vocabulary = applied
        .tables
        .iter_mut()
        .find(|table| table.table == RepairRelationTable::RelationTypeVocabulary)
        .ok_or_else(|| {
            WenlanError::Validation("repair_relation_snapshot_schema_mismatch".into())
        })?;
    let canonical = vocabulary
        .columns
        .iter()
        .position(|name| name == "canonical")
        .ok_or_else(|| {
            WenlanError::Validation("repair_relation_snapshot_schema_mismatch".into())
        })?;
    let count = vocabulary
        .columns
        .iter()
        .position(|name| name == "count")
        .ok_or_else(|| {
            WenlanError::Validation("repair_relation_snapshot_schema_mismatch".into())
        })?;
    vocabulary.rows.retain(|row| {
        matches!((&row[canonical], context.canonical_relation_type),
            (RepairRelationSqlValue::Text { value }, Some(expected)) if value == expected)
    });
    for row in &mut vocabulary.rows {
        row[count] = RepairRelationSqlValue::Null;
    }
    let queue = applied
        .tables
        .iter_mut()
        .find(|table| table.table == RepairRelationTable::RefinementQueue)
        .ok_or_else(|| {
            WenlanError::Validation("repair_relation_snapshot_schema_mismatch".into())
        })?;
    let id_index = queue
        .columns
        .iter()
        .position(|column| column == "id")
        .ok_or_else(|| {
            WenlanError::Validation("repair_relation_snapshot_schema_mismatch".into())
        })?;
    let completion_columns = ["status", "resolved_at"].map(|name| {
        queue
            .columns
            .iter()
            .position(|column| column == name)
            .ok_or_else(|| {
                WenlanError::Validation("repair_relation_snapshot_schema_mismatch".into())
            })
    });
    let [status, resolved_at] = completion_columns;
    let (status, resolved_at) = (status?, resolved_at?);
    for row in &mut queue.rows {
        if matches!(row.get(id_index), Some(RepairRelationSqlValue::Text { value }) if value == context.review_id)
        {
            row[status] = RepairRelationSqlValue::Null;
            row[resolved_at] = RepairRelationSqlValue::Null;
        }
    }
    Ok(repair_digest(&serde_json::to_vec(&serde_json::json!({
        "relation_applied_state_version": 2,
        "snapshot": applied,
    }))?))
}

/// Whole-database guard, excluding only declared write rows. Immutable
/// columns within those rows must additionally pass the writer's post-check.
/// Read-only inputs (source memory, endpoint pages, review row) stay covered.
pub(crate) fn effect_exclusions(
    edge_ids: &[String],
    spaces: &[String],
    canonical: Option<&str>,
    promotion: Option<&str>,
    manifest_id: &str,
) -> Result<BTreeMap<&'static str, RepairDigestExclusion>, WenlanError> {
    let mut exclusions = BTreeMap::from([
        (
            "edges",
            RepairDigestExclusion {
                predicate: "edge_id IN (SELECT value FROM json_each(?1))",
                parameters: vec![text(&serde_json::to_string(edge_ids)?)],
            },
        ),
        (
            "space_graph_state",
            RepairDigestExclusion {
                predicate: "space IN (SELECT value FROM json_each(?1))",
                parameters: vec![text(&serde_json::to_string(spaces)?)],
            },
        ),
        (
            "agent_activity",
            RepairDigestExclusion {
                predicate: "query=?1",
                parameters: vec![text(manifest_id)],
            },
        ),
        (
            "community_parity_input_state",
            RepairDigestExclusion {
                predicate: "singleton=1",
                parameters: vec![],
            },
        ),
    ]);
    if let Some(canonical) = canonical {
        exclusions.insert(
            "relation_type_vocabulary",
            RepairDigestExclusion {
                predicate: "canonical=?1",
                parameters: vec![text(canonical)],
            },
        );
    }
    if let Some(promotion) = promotion {
        exclusions.insert(
            "refinement_queue",
            RepairDigestExclusion {
                predicate: "id=?1",
                parameters: vec![text(&MemoryDB::vocab_proposal_fingerprint(
                    "relation", promotion,
                ))],
            },
        );
    }
    Ok(exclusions)
}
