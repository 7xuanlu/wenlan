// SPDX-License-Identifier: Apache-2.0
//! Pure post-write validation for the entity-relation repair writer.
//!
//! This module deliberately receives only the lossless before/after capture
//! and the writer's returned effects.  It does not read or mutate the
//! database.  The caller's whole-database digest protects rows outside the
//! declared exclusions; this validator checks every cell inside those
//! exclusions and models the graph-state triggers that are not represented by
//! the writer's returned graph updates.

use crate::{
    db::{self, repair_relation::RelationWriteEffects},
    error::WenlanError,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use wenlan_types::{
    repair::{RepairManifest, RepairMemoryField, RepairMutation, RepairTarget},
    repair_relation::{
        RepairRelationSnapshot, RepairRelationSqlValue, RepairRelationTable,
        RepairRelationTableSnapshot,
    },
};

const RELATION_CHECK_ID: &str = "kg.semantic.entity_relations";
const SOURCE_REPAIR_AGENT: &str = "source-repair";
const UNFILED_SPACE_ID: &str = "00000000-0000-4000-8000-000000000001";

fn invalid(detail: impl Into<String>) -> WenlanError {
    WenlanError::Validation(format!("repair_relation_effects_{}", detail.into()))
}

fn require(condition: bool, detail: impl Into<String>) -> Result<(), WenlanError> {
    condition.then_some(()).ok_or_else(|| invalid(detail))
}

fn table(
    snapshot: &RepairRelationSnapshot,
    expected: RepairRelationTable,
) -> Result<&RepairRelationTableSnapshot, WenlanError> {
    snapshot
        .tables
        .iter()
        .find(|table| table.table == expected)
        .ok_or_else(|| invalid(format!("missing_{}", table_name(expected))))
}

fn table_name(table: RepairRelationTable) -> &'static str {
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

fn column(table: &RepairRelationTableSnapshot, name: &str) -> Result<usize, WenlanError> {
    table
        .columns
        .iter()
        .position(|column| column == name)
        .ok_or_else(|| {
            invalid(format!(
                "missing_column_{}_{}",
                table_name(table.table),
                name
            ))
        })
}

fn required_columns(
    table: &RepairRelationTableSnapshot,
    names: &[&str],
) -> Result<Vec<usize>, WenlanError> {
    names.iter().map(|name| column(table, name)).collect()
}

fn cell<'a>(
    table: &'a RepairRelationTableSnapshot,
    row: &'a [RepairRelationSqlValue],
    name: &str,
) -> Result<&'a RepairRelationSqlValue, WenlanError> {
    let index = column(table, name)?;
    row.get(index)
        .ok_or_else(|| invalid(format!("short_row_{}_{}", table_name(table.table), name)))
}

fn text<'a>(value: &'a RepairRelationSqlValue, field: &str) -> Result<&'a str, WenlanError> {
    match value {
        RepairRelationSqlValue::Text { value } => Ok(value),
        _ => Err(invalid(format!("unsupported_text_{field}"))),
    }
}

fn optional_text<'a>(
    value: &'a RepairRelationSqlValue,
    field: &str,
) -> Result<Option<&'a str>, WenlanError> {
    match value {
        RepairRelationSqlValue::Null => Ok(None),
        RepairRelationSqlValue::Text { value } => Ok(Some(value)),
        _ => Err(invalid(format!("unsupported_optional_text_{field}"))),
    }
}

fn integer(value: &RepairRelationSqlValue, field: &str) -> Result<i64, WenlanError> {
    match value {
        RepairRelationSqlValue::Integer { value } => Ok(*value),
        _ => Err(invalid(format!("unsupported_integer_{field}"))),
    }
}

fn optional_integer(
    value: &RepairRelationSqlValue,
    field: &str,
) -> Result<Option<i64>, WenlanError> {
    match value {
        RepairRelationSqlValue::Null => Ok(None),
        RepairRelationSqlValue::Integer { value } => Ok(Some(*value)),
        _ => Err(invalid(format!("unsupported_optional_integer_{field}"))),
    }
}

fn real(value: &RepairRelationSqlValue, field: &str) -> Result<f64, WenlanError> {
    match value {
        RepairRelationSqlValue::Real { bits } => {
            let bits = u64::from_str_radix(bits, 16)
                .map_err(|_| invalid(format!("malformed_real_{field}")))?;
            let value = f64::from_bits(bits);
            value
                .is_finite()
                .then_some(value)
                .ok_or_else(|| invalid(format!("nonfinite_real_{field}")))
        }
        RepairRelationSqlValue::Integer { value } => Ok(*value as f64),
        _ => Err(invalid(format!("unsupported_real_{field}"))),
    }
}

fn json(value: &RepairRelationSqlValue, field: &str) -> Result<Option<Value>, WenlanError> {
    match value {
        RepairRelationSqlValue::Null => Ok(None),
        RepairRelationSqlValue::Text { value } => serde_json::from_str(value)
            .map(Some)
            .map_err(|_| invalid(format!("malformed_json_{field}"))),
        _ => Err(invalid(format!("unsupported_json_{field}"))),
    }
}

fn json_object(
    value: &RepairRelationSqlValue,
    field: &str,
) -> Result<Map<String, Value>, WenlanError> {
    match json(value, field)? {
        None => Ok(Map::new()),
        Some(Value::Object(object)) => Ok(object),
        Some(_) => Err(invalid(format!("json_not_object_{field}"))),
    }
}

fn same_json_number(left: &Value, expected: f64, field: &str) -> Result<(), WenlanError> {
    let actual = left
        .as_f64()
        .ok_or_else(|| invalid(format!("json_number_{field}")))?;
    require(
        actual.to_bits() == expected.to_bits(),
        format!("json_value_{field}"),
    )
}

fn in_window(value: i64, started_at: i64, finished_at: i64) -> bool {
    value > 0 && started_at <= value && value <= finished_at
}

fn validate_window(started_at: i64, finished_at: i64) -> Result<(), WenlanError> {
    require(started_at > 0, "invalid_started_at")?;
    require(finished_at >= started_at, "invalid_finished_at")
}

fn validate_shapes(
    before: &RepairRelationSnapshot,
    after: &RepairRelationSnapshot,
) -> Result<(), WenlanError> {
    before
        .validate()
        .map_err(|error| invalid(format!("before_{error}")))?;
    after
        .validate()
        .map_err(|error| invalid(format!("after_{error}")))?;
    require(
        before.tables.len() == RepairRelationTable::ALL_TABLES.len()
            && after.tables.len() == RepairRelationTable::ALL_TABLES.len(),
        "table_count",
    )?;
    for expected in RepairRelationTable::ALL_TABLES {
        let before_table = table(before, expected)?;
        let after_table = table(after, expected)?;
        require(
            before_table.columns == after_table.columns,
            format!("columns_changed_{}", table_name(expected)),
        )?;
        for row in before_table.rows.iter().chain(after_table.rows.iter()) {
            require(
                row.len() == before_table.columns.len(),
                format!("row_width_{}", table_name(expected)),
            )?;
        }
    }
    // These rows are excluded from the outer digest. Every column must have
    // a defined postcondition; a new schema must be handled before applying.
    for (kind, width) in [
        (RepairRelationTable::Edges, 18),
        (RepairRelationTable::SpaceGraphState, 5),
        (RepairRelationTable::RelationTypeVocabulary, 4),
        (RepairRelationTable::RefinementQueue, 8),
        (RepairRelationTable::AgentActivity, 7),
    ] {
        require(
            table(before, kind)?.columns.len() == width,
            "unsupported_mutable_table_schema",
        )?;
    }
    // These are the columns consumed below.  Checking them explicitly keeps a
    // future or hand-built snapshot with an incomplete schema fail-closed.
    required_columns(
        table(before, RepairRelationTable::Edges)?,
        &[
            "edge_id",
            "src_id",
            "src_kind",
            "dst_id",
            "dst_kind",
            "edge_type",
            "lineage",
            "grounded",
            "root_id",
            "space",
            "weight",
            "payload",
            "provenance",
            "operation_id",
            "created_at",
            "superseded_by",
            "valid_until",
            "semantic_type",
        ],
    )?;
    required_columns(
        table(before, RepairRelationTable::EntityPageMap)?,
        &["entity_id", "page_id", "created_at"],
    )?;
    required_columns(table(before, RepairRelationTable::Pages)?, &["id", "space"])?;
    required_columns(
        table(before, RepairRelationTable::Memories)?,
        &["id", "source_id", "source"],
    )?;
    required_columns(
        table(before, RepairRelationTable::SpaceGraphState)?,
        &[
            "space",
            "graph_generation",
            "grouping_generation",
            "published_generation",
            "dirty",
        ],
    )?;
    required_columns(
        table(before, RepairRelationTable::RelationTypeVocabulary)?,
        &["canonical", "aliases", "category", "count"],
    )?;
    required_columns(
        table(before, RepairRelationTable::RefinementQueue)?,
        &[
            "id",
            "action",
            "source_ids",
            "payload",
            "confidence",
            "status",
            "created_at",
            "resolved_at",
        ],
    )?;
    required_columns(
        table(before, RepairRelationTable::AgentActivity)?,
        &[
            "id",
            "timestamp",
            "agent_name",
            "action",
            "memory_ids",
            "query",
            "detail",
        ],
    )?;
    Ok(())
}

fn rows_by_key<'a>(
    table: &'a RepairRelationTableSnapshot,
    key: &str,
) -> Result<BTreeMap<String, &'a Vec<RepairRelationSqlValue>>, WenlanError> {
    let index = column(table, key)?;
    let mut rows = BTreeMap::new();
    for row in &table.rows {
        let key = text(
            row.get(index)
                .ok_or_else(|| invalid(format!("short_key_{}", table_name(table.table))))?,
            key,
        )?;
        require(
            rows.insert(key.to_string(), row).is_none(),
            format!("duplicate_{}_key", table_name(table.table)),
        )?;
    }
    Ok(rows)
}

fn rows_by_integer_key<'a>(
    table: &'a RepairRelationTableSnapshot,
    key: &str,
) -> Result<BTreeMap<i64, &'a Vec<RepairRelationSqlValue>>, WenlanError> {
    let index = column(table, key)?;
    let mut rows = BTreeMap::new();
    for row in &table.rows {
        let key = integer(
            row.get(index)
                .ok_or_else(|| invalid(format!("short_key_{}", table_name(table.table))))?,
            key,
        )?;
        require(
            rows.insert(key, row).is_none(),
            format!("duplicate_{}_key", table_name(table.table)),
        )?;
    }
    Ok(rows)
}

fn validate_manifest(
    manifest: &RepairManifest,
) -> Result<
    (
        &str,
        &str,
        &str,
        &wenlan_types::repair_relation::RepairRelationMutation,
    ),
    WenlanError,
> {
    require(
        manifest.writer() == wenlan_types::repair::RepairWriter::EntityRelation,
        "writer",
    )?;
    let (relation_id, from, to) = match manifest.target() {
        RepairTarget::EntityRelation {
            relation_id,
            from_entity,
            to_entity,
            ..
        } => (
            relation_id.as_str(),
            from_entity.as_str(),
            to_entity.as_str(),
        ),
        _ => return Err(invalid("target")),
    };
    let RepairMutation::EntityRelation { change } = manifest.mutation() else {
        return Err(invalid("mutation"));
    };
    change
        .validate()
        .map_err(|error| invalid(format!("mutation_{error}")))?;
    let allowed = manifest.allowed_effects();
    require(allowed.owner() == manifest.target(), "allowed_owner")?;
    require(
        allowed.fields()
            == [
                RepairMemoryField::RelationEdges,
                RepairMemoryField::CommunityGraphState,
                RepairMemoryField::RelationVocabulary,
                RepairMemoryField::RelationActivity,
                RepairMemoryField::RelationReviewQueue,
            ],
        "allowed_fields",
    )?;
    require(
        manifest.source().check_id() == RELATION_CHECK_ID,
        "source_check",
    )?;
    require(
        manifest.source().review_binding().is_some(),
        "review_binding",
    )?;
    Ok((relation_id, from, to, change))
}

#[derive(Clone)]
struct Edge {
    id: String,
    src: String,
    src_kind: String,
    dst: String,
    dst_kind: String,
    edge_type: String,
    lineage: String,
    grounded: i64,
    root_id: RepairRelationSqlValue,
    space: String,
    weight: RepairRelationSqlValue,
    payload: RepairRelationSqlValue,
    provenance: RepairRelationSqlValue,
    operation_id: RepairRelationSqlValue,
    created_at: i64,
    superseded_by: RepairRelationSqlValue,
    valid_until: Option<i64>,
    semantic_type: Option<String>,
}

impl Edge {
    fn from_row(
        table: &RepairRelationTableSnapshot,
        row: &[RepairRelationSqlValue],
    ) -> Result<Self, WenlanError> {
        let id = text(cell(table, row, "edge_id")?, "edge_id")?.to_string();
        let src = text(cell(table, row, "src_id")?, "src_id")?.to_string();
        let src_kind = text(cell(table, row, "src_kind")?, "src_kind")?.to_string();
        let dst = text(cell(table, row, "dst_id")?, "dst_id")?.to_string();
        let dst_kind = text(cell(table, row, "dst_kind")?, "dst_kind")?.to_string();
        let edge_type = text(cell(table, row, "edge_type")?, "edge_type")?.to_string();
        require(edge_type == "relates", "unsupported_edge_type")?;
        let lineage = text(cell(table, row, "lineage")?, "lineage")?.to_string();
        let grounded = integer(cell(table, row, "grounded")?, "grounded")?;
        require(grounded == 0 || grounded == 1, "grounded_vocabulary")?;
        let space = text(cell(table, row, "space")?, "space")?.to_string();
        let created_at = integer(cell(table, row, "created_at")?, "created_at")?;
        let superseded_by = cell(table, row, "superseded_by")?.clone();
        let valid_until = optional_integer(cell(table, row, "valid_until")?, "valid_until")?;
        let semantic_type =
            optional_text(cell(table, row, "semantic_type")?, "semantic_type")?.map(str::to_string);
        Ok(Self {
            id,
            src,
            src_kind,
            dst,
            dst_kind,
            edge_type,
            lineage,
            grounded,
            root_id: cell(table, row, "root_id")?.clone(),
            space,
            weight: cell(table, row, "weight")?.clone(),
            payload: cell(table, row, "payload")?.clone(),
            provenance: cell(table, row, "provenance")?.clone(),
            operation_id: cell(table, row, "operation_id")?.clone(),
            created_at,
            superseded_by,
            valid_until,
            semantic_type,
        })
    }

    fn active_assertion(&self) -> bool {
        self.edge_type == "relates"
            && self.valid_until.is_none()
            && self.src_kind == "entity"
            && self.dst_kind == "entity"
            && self.lineage == "assertion"
    }
}

fn edge_map(table: &RepairRelationTableSnapshot) -> Result<BTreeMap<String, Edge>, WenlanError> {
    let mut rows = BTreeMap::new();
    for row in &table.rows {
        let edge = Edge::from_row(table, row)?;
        require(
            rows.insert(edge.id.clone(), edge).is_none(),
            "duplicate_edge_id",
        )?;
    }
    Ok(rows)
}

type EdgeMap = BTreeMap<String, Edge>;

fn parse_edges(
    before: &RepairRelationSnapshot,
    after: &RepairRelationSnapshot,
) -> Result<(EdgeMap, EdgeMap), WenlanError> {
    Ok((
        edge_map(table(before, RepairRelationTable::Edges)?)?,
        edge_map(table(after, RepairRelationTable::Edges)?)?,
    ))
}

fn endpoint_spaces(
    before: &RepairRelationSnapshot,
    from: &str,
    to: &str,
) -> Result<(Option<String>, Option<String>), WenlanError> {
    let map_table = table(before, RepairRelationTable::EntityPageMap)?;
    let page_table = table(before, RepairRelationTable::Pages)?;
    let page_by_entity = rows_by_key(map_table, "entity_id")?;
    let pages = rows_by_key(page_table, "id")?;
    let page_space = |entity: &str| -> Result<Option<String>, WenlanError> {
        let Some(row) = page_by_entity.get(entity) else {
            return Ok(None);
        };
        let page_id = text(cell(map_table, row, "page_id")?, "page_id")?;
        let Some(page) = pages.get(page_id) else {
            return Err(invalid("entity_page_missing"));
        };
        Ok(Some(
            text(cell(page_table, page, "space")?, "space")?.to_string(),
        ))
    };
    Ok((page_space(from)?, page_space(to)?))
}

fn canonical_space_lineage(
    before: &RepairRelationSnapshot,
    from: &str,
    to: &str,
) -> Result<(String, String), WenlanError> {
    let (from_space, to_space) = endpoint_spaces(before, from, to)?;
    match (&from_space, &to_space) {
        (Some(from), Some(to)) if from == to => Ok((from.clone(), "assertion".to_string())),
        _ => Ok((
            from_space
                .or(to_space)
                .unwrap_or_else(|| UNFILED_SPACE_ID.to_string()),
            "legacy".to_string(),
        )),
    }
}

fn lineage_rank(lineage: &str) -> u8 {
    match lineage {
        "evidence" => 3,
        "synthesis" => 2,
        "assertion" => 1,
        _ => 0,
    }
}

fn expected_target_lineage(prior: Option<&Edge>, fresh_space: &str, fresh_lineage: &str) -> String {
    let Some(prior) = prior else {
        return fresh_lineage.to_string();
    };
    if prior.valid_until.is_some()
        || prior.space != fresh_space
        || (fresh_lineage == "legacy" && prior.valid_until.is_none())
        || lineage_rank(fresh_lineage) > lineage_rank(&prior.lineage)
    {
        fresh_lineage.to_string()
    } else {
        prior.lineage.clone()
    }
}

fn expected_payload(
    before: Option<&Edge>,
    source_memory_id: Option<&str>,
    confidence: f64,
    now: i64,
) -> Result<RepairRelationSqlValue, WenlanError> {
    let mut object = if let Some(edge) = before {
        json_object(&edge.payload, "edge_payload")?
    } else {
        Map::new()
    };
    for (key, value) in &object {
        match key.as_str() {
            "source_memory_id" | "source_agent" | "explanation"
                if !value.is_null() && !value.is_string() =>
            {
                return Err(invalid(format!("{key}_shape")));
            }
            "confidence"
                if !value.is_null()
                    && value
                        .as_f64()
                        .is_none_or(|confidence| !confidence.is_finite()) =>
            {
                return Err(invalid("confidence_shape"));
            }
            "asserted_at" if !value.is_null() && value.as_i64().is_none() => {
                return Err(invalid("asserted_at_shape"));
            }
            _ => {}
        }
    }
    let prior_source = match object.get("source_memory_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.as_str()),
        Some(_) => return Err(invalid("source_memory_id_shape")),
    };
    if let Some(source) = source_memory_id {
        if let Some(prior) = prior_source {
            require(prior == source, "source_binding_conflict")?;
        } else {
            object.insert(
                "source_memory_id".to_string(),
                Value::String(source.to_string()),
            );
        }
    }

    let prior_confidence = match object.get("confidence") {
        None | Some(Value::Null) => None,
        Some(value) => Some(value.as_f64().ok_or_else(|| invalid("confidence_shape"))?),
    };
    let stronger = before.is_none() || prior_confidence.is_none_or(|prior| confidence > prior);
    let merged_confidence = if stronger {
        Some(confidence)
    } else {
        prior_confidence
    };
    if let Some(confidence) = merged_confidence {
        object.insert("confidence".to_string(), serde_json::json!(confidence));
    }
    if before.is_none() {
        object.insert(
            "source_agent".to_string(),
            Value::String(SOURCE_REPAIR_AGENT.to_string()),
        );
        object.insert("asserted_at".to_string(), Value::Number(now.into()));
    } else {
        let asserted = match object.get("asserted_at") {
            Some(Value::Number(value)) => {
                value.as_i64().ok_or_else(|| invalid("asserted_at_shape"))?
            }
            None | Some(Value::Null) => now,
            Some(_) => return Err(invalid("asserted_at_shape")),
        };
        object.insert("asserted_at".to_string(), Value::Number(asserted.into()));
    }
    let encoded = Value::Object(object).to_string();
    Ok(RepairRelationSqlValue::Text { value: encoded })
}

fn validate_payload_value(
    actual: &RepairRelationSqlValue,
    expected: &RepairRelationSqlValue,
) -> Result<(), WenlanError> {
    let actual = json_object(actual, "after_edge_payload")?;
    let expected = json_object(expected, "expected_edge_payload")?;
    require(actual == expected, "edge_payload")
}

fn validate_edge_immutable(before: &Edge, after: &Edge) -> Result<(), WenlanError> {
    require(after.id == before.id, "edge_id_changed")?;
    require(
        after.src == before.src && after.dst == before.dst,
        "edge_endpoints_changed",
    )?;
    require(
        after.src_kind == before.src_kind
            && after.dst_kind == before.dst_kind
            && after.edge_type == before.edge_type,
        "edge_identity_columns_changed",
    )?;
    require(after.grounded == before.grounded, "edge_grounding_changed")?;
    require(after.root_id == before.root_id, "edge_root_changed")?;
    require(after.weight == before.weight, "edge_weight_changed")?;
    require(
        after.provenance == before.provenance,
        "edge_provenance_changed",
    )?;
    require(
        after.operation_id == before.operation_id,
        "edge_operation_changed",
    )?;
    require(
        after.created_at == before.created_at,
        "edge_created_at_changed",
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_target(
    before: &BTreeMap<String, Edge>,
    after: &BTreeMap<String, Edge>,
    before_snapshot: &RepairRelationSnapshot,
    relation_id: &str,
    from: &str,
    to: &str,
    canonical: &str,
    source_memory_id: Option<&str>,
    confidence_basis_points: u16,
    has_retirements: bool,
    started_at: i64,
    finished_at: i64,
) -> Result<(Edge, Option<Edge>, String), WenlanError> {
    let target = after
        .get(relation_id)
        .ok_or_else(|| invalid("target_missing_after"))?
        .clone();
    let expected_id =
        crate::provenance::compute_edge_id("relates", "entity", from, "entity", to, canonical);
    require(expected_id == relation_id, "target_identity")?;
    require(
        target.src == from
            && target.dst == to
            && target.src_kind == "entity"
            && target.dst_kind == "entity"
            && target.edge_type == "relates"
            && target.semantic_type.as_deref() == Some(canonical),
        "target_canonical_columns",
    )?;
    require(
        target.valid_until.is_none()
            && matches!(&target.superseded_by, RepairRelationSqlValue::Null),
        "target_active",
    )?;
    let prior = before.get(relation_id).cloned();
    let (space, fresh_lineage) = canonical_space_lineage(before_snapshot, from, to)?;
    let lineage = expected_target_lineage(prior.as_ref(), &space, &fresh_lineage);
    require(
        target.space == space && target.lineage == lineage,
        "target_space_lineage",
    )?;
    let confidence = f64::from(confidence_basis_points) / 10_000.0;
    let payload = expected_payload(prior.as_ref(), source_memory_id, confidence, started_at)?;
    validate_payload_value(&target.payload, &payload)?;

    if let Some(prior) = &prior {
        validate_edge_immutable(prior, &target)?;
        if prior.valid_until.is_none() {
            let before_payload = json_object(&prior.payload, "prior_edge_payload")?;
            let after_payload = json_object(&target.payload, "after_edge_payload")?;
            let mut meaningful = prior.space != target.space
                || prior.lineage != target.lineage
                || prior.semantic_type != target.semantic_type
                || prior.valid_until != target.valid_until
                || prior.superseded_by != target.superseded_by;
            if source_memory_id.is_some()
                && !matches!(
                    before_payload.get("source_memory_id"),
                    Some(Value::String(_))
                )
            {
                meaningful = true;
            }
            if prior_payload_changed(&before_payload, &after_payload)? {
                meaningful = true;
            }
            require(meaningful || has_retirements, "target_noop")?;
        }
    } else {
        require(target.grounded == 0, "new_edge_grounding")?;
        require(
            matches!(&target.root_id, RepairRelationSqlValue::Null),
            "new_edge_root",
        )?;
        require(
            matches!(&target.weight, RepairRelationSqlValue::Null),
            "new_edge_weight",
        )?;
        require(
            matches!(&target.provenance, RepairRelationSqlValue::Null),
            "new_edge_provenance",
        )?;
        require(
            matches!(&target.operation_id, RepairRelationSqlValue::Null),
            "new_edge_operation",
        )?;
        require(
            in_window(target.created_at, started_at, finished_at),
            "new_edge_created_at",
        )?;
    }
    Ok((target, prior, fresh_lineage))
}

fn prior_payload_changed(
    before: &Map<String, Value>,
    after: &Map<String, Value>,
) -> Result<bool, WenlanError> {
    // A semantic patch can only alter these canonical keys.  The full object
    // equality is still the strongest check; this helper merely rejects a
    // key removed or altered outside that owned set with a precise error.
    for key in before.keys().chain(after.keys()) {
        let changed = before.get(key) != after.get(key);
        if changed
            && !matches!(
                key.as_str(),
                "confidence" | "explanation" | "source_agent" | "asserted_at" | "source_memory_id"
            )
        {
            return Err(invalid(format!("payload_key_{key}")));
        }
    }
    Ok(before != after)
}

fn validate_retired(
    before: &BTreeMap<String, Edge>,
    after: &BTreeMap<String, Edge>,
    retire_ids: &[String],
    started_at: i64,
    finished_at: i64,
) -> Result<Vec<(Edge, Edge)>, WenlanError> {
    let mut pairs = Vec::with_capacity(retire_ids.len());
    for id in retire_ids {
        let prior = before
            .get(id)
            .ok_or_else(|| invalid(format!("retire_missing_before_{id}")))?
            .clone();
        let current = after
            .get(id)
            .ok_or_else(|| invalid(format!("retire_missing_after_{id}")))?
            .clone();
        require(
            prior.valid_until.is_none(),
            format!("retire_not_active_{id}"),
        )?;
        validate_edge_immutable(&prior, &current)?;
        let valid_until = current
            .valid_until
            .ok_or_else(|| invalid(format!("retire_valid_until_{id}")))?;
        require(
            in_window(valid_until, started_at, finished_at),
            format!("retire_timestamp_{id}"),
        )?;
        require(
            matches!(&current.superseded_by, RepairRelationSqlValue::Null),
            format!("retire_superseded_by_{id}"),
        )?;
        pairs.push((prior, current));
    }
    Ok(pairs)
}

fn validate_declared_retirements(
    before: &BTreeMap<String, Edge>,
    relation_id: &str,
    requested: &str,
    declared: &[String],
) -> Result<(), WenlanError> {
    let expected = before
        .values()
        .filter(|edge| {
            edge.id != relation_id
                && edge.valid_until.is_none()
                && edge
                    .semantic_type
                    .as_deref()
                    .is_some_and(|value| value != requested)
        })
        .map(|edge| edge.id.clone())
        .collect::<Vec<_>>();
    require(expected == declared, "retire_set")
}

fn compare_edge_sets(
    before: &BTreeMap<String, Edge>,
    after: &BTreeMap<String, Edge>,
    relation_id: &str,
    target_was_present: bool,
    retire_ids: &[String],
) -> Result<(), WenlanError> {
    let allowed_new = !target_was_present;
    for id in before.keys() {
        require(after.contains_key(id), format!("edge_deleted_{id}"))?;
    }
    for id in after.keys() {
        require(
            before.contains_key(id) || (allowed_new && id == relation_id),
            format!("edge_extra_{id}"),
        )?;
    }
    for id in before.keys() {
        if id != relation_id && !retire_ids.iter().any(|retire| retire == id) {
            require(
                before[id].same_shape(&after[id]),
                format!("edge_unowned_changed_{id}"),
            )?;
        }
    }
    Ok(())
}

impl Edge {
    fn same_shape(&self, other: &Self) -> bool {
        self.id == other.id
            && self.src == other.src
            && self.src_kind == other.src_kind
            && self.dst == other.dst
            && self.dst_kind == other.dst_kind
            && self.edge_type == other.edge_type
            && self.lineage == other.lineage
            && self.grounded == other.grounded
            && self.root_id == other.root_id
            && self.space == other.space
            && self.weight == other.weight
            && self.payload == other.payload
            && self.provenance == other.provenance
            && self.operation_id == other.operation_id
            && self.created_at == other.created_at
            && self.superseded_by == other.superseded_by
            && self.valid_until == other.valid_until
            && self.semantic_type == other.semantic_type
    }
}

#[derive(Clone)]
struct GraphState {
    graph_generation: i64,
    grouping_generation: i64,
    published_generation: Option<i64>,
    dirty: i64,
}

fn parse_graph_states(
    snapshot: &RepairRelationSnapshot,
) -> Result<BTreeMap<String, GraphState>, WenlanError> {
    let table = table(snapshot, RepairRelationTable::SpaceGraphState)?;
    let mut result = BTreeMap::new();
    for row in &table.rows {
        let space = text(cell(table, row, "space")?, "graph_space")?.to_string();
        let state = GraphState {
            graph_generation: integer(cell(table, row, "graph_generation")?, "graph_generation")?,
            grouping_generation: integer(
                cell(table, row, "grouping_generation")?,
                "grouping_generation",
            )?,
            published_generation: optional_integer(
                cell(table, row, "published_generation")?,
                "published_generation",
            )?,
            dirty: integer(cell(table, row, "dirty")?, "dirty")?,
        };
        require(
            state.graph_generation >= 0 && state.grouping_generation >= 0,
            "negative_generation",
        )?;
        require(
            state.published_generation.is_none_or(|value| value >= 0),
            "negative_published",
        )?;
        require(state.dirty == 0 || state.dirty == 1, "dirty_vocabulary")?;
        require(
            result.insert(space, state).is_none(),
            "duplicate_graph_space",
        )?;
    }
    Ok(result)
}

fn increment(value: &mut i64, field: &str) -> Result<(), WenlanError> {
    *value = value
        .checked_add(1)
        .ok_or_else(|| invalid(format!("{field}_overflow")))?;
    Ok(())
}

fn state_insert_or_update(
    states: &mut BTreeMap<String, GraphState>,
    space: &str,
    parity: &mut u64,
) -> Result<(), WenlanError> {
    if let Some(state) = states.get_mut(space) {
        increment(&mut state.graph_generation, "graph_generation")?;
        increment(&mut state.grouping_generation, "grouping_generation")?;
        state.dirty = 1;
    } else {
        states.insert(
            space.to_string(),
            GraphState {
                graph_generation: 1,
                grouping_generation: 1,
                published_generation: None,
                dirty: 1,
            },
        );
    }
    *parity = parity
        .checked_add(1)
        .ok_or_else(|| invalid("parity_overflow"))?;
    Ok(())
}

fn state_grouping_update(
    states: &mut BTreeMap<String, GraphState>,
    space: &str,
    parity: &mut u64,
) -> Result<(), WenlanError> {
    if let Some(state) = states.get_mut(space) {
        increment(&mut state.grouping_generation, "grouping_generation")?;
        state.dirty = 1;
        *parity = parity
            .checked_add(1)
            .ok_or_else(|| invalid("parity_overflow"))?;
    }
    Ok(())
}

fn edge_trigger(
    old: Option<&Edge>,
    new: &Edge,
    states: &mut BTreeMap<String, GraphState>,
    parity: &mut u64,
) -> Result<(), WenlanError> {
    let Some(old) = old else {
        if new.active_assertion() {
            if new.grounded == 1 {
                state_insert_or_update(states, &new.space, parity)?;
            } else {
                state_grouping_update(states, &new.space, parity)?;
            }
        }
        return Ok(());
    };
    let old_active = old.active_assertion();
    let new_active = new.active_assertion();
    if !old_active && !new_active {
        return Ok(());
    }
    if old.grounded == 1 && (!new_active || old.space != new.space) {
        state_insert_or_update(states, &old.space, parity)?;
    }
    if new.grounded == 1 && (!old_active || old.space != new.space) {
        state_insert_or_update(states, &new.space, parity)?;
    }
    if old.space == new.space
        && (old.grounded == 1 || new.grounded == 1)
        && old_active
        && new_active
        && (old.id != new.id
            || old.src != new.src
            || old.dst != new.dst
            || old.grounded != new.grounded)
    {
        state_insert_or_update(states, &old.space, parity)?;
    }
    if old_active && (!new_active || old.space != new.space) && old.grounded == 0 {
        state_grouping_update(states, &old.space, parity)?;
    }
    if new_active && (!old_active || old.space != new.space) && new.grounded == 0 {
        state_grouping_update(states, &new.space, parity)?;
    }
    if old.space == new.space
        && old.grounded == 0
        && new.grounded == 0
        && old_active
        && new_active
        && (old.id != new.id
            || old.src != new.src
            || old.dst != new.dst
            || old.grounded != new.grounded)
    {
        state_grouping_update(states, &old.space, parity)?;
    }
    Ok(())
}

fn graph_changes(
    old: Option<&Edge>,
    new: &Edge,
    retire: bool,
    fresh_lineage: Option<&str>,
) -> Vec<(String, String, String)> {
    if retire {
        return old
            .filter(|edge| edge.active_assertion() && edge.grounded == 1)
            .map(|edge| (edge.space.clone(), edge.src.clone(), edge.dst.clone()))
            .into_iter()
            .collect();
    }
    let Some(old) = old else {
        return Vec::new();
    };
    let prior_grounded = old.grounded == 1;
    let prior_participates = prior_grounded && old.active_assertion();
    let current_participates =
        prior_grounded && fresh_lineage.unwrap_or(new.lineage.as_str()) == "assertion";
    let moved = old.space != new.space;
    let mut changes = Vec::new();
    if prior_participates && current_participates && moved {
        changes.push((old.space.clone(), old.src.clone(), old.dst.clone()));
    }
    if current_participates && (!prior_participates || moved) {
        changes.push((new.space.clone(), new.src.clone(), new.dst.clone()));
    }
    changes
}

fn bump_graph_generations(
    states: &mut BTreeMap<String, GraphState>,
    changes: &[(String, String, String)],
    parity: &mut u64,
) -> Result<Vec<(String, i64, BTreeSet<String>)>, WenlanError> {
    let mut by_space: BTreeMap<String, (i64, BTreeSet<String>)> = BTreeMap::new();
    for (space, src, dst) in changes {
        let (count, nodes) = by_space.entry(space.clone()).or_default();
        increment(count, "graph_trigger_count")?;
        nodes.insert(src.clone());
        nodes.insert(dst.clone());
    }
    let mut updates = Vec::with_capacity(by_space.len());
    for (space, (count, nodes)) in by_space {
        if let Some(state) = states.get_mut(&space) {
            state.graph_generation = if state.graph_generation < count {
                1
            } else {
                state
                    .graph_generation
                    .checked_sub(count)
                    .and_then(|value| value.checked_add(1))
                    .ok_or_else(|| invalid("graph_generation_overflow"))?
                    .max(1)
            };
            state.dirty = 1;
        } else {
            states.insert(
                space.clone(),
                GraphState {
                    graph_generation: 1,
                    grouping_generation: 1,
                    published_generation: None,
                    dirty: 1,
                },
            );
        }
        *parity = parity
            .checked_add(1)
            .ok_or_else(|| invalid("parity_overflow"))?;
        let generation = states
            .get(&space)
            .ok_or_else(|| invalid("graph_state_missing_after_bump"))?
            .grouping_generation;
        updates.push((space, generation, nodes));
    }
    Ok(updates)
}

fn apply_edge_operation(
    states: &mut BTreeMap<String, GraphState>,
    effects_updates: &mut Vec<(String, i64, BTreeSet<String>)>,
    parity: &mut u64,
    old: Option<&Edge>,
    new: &Edge,
    retire: bool,
    fresh_lineage: Option<&str>,
) -> Result<(), WenlanError> {
    edge_trigger(old, new, states, parity)?;
    let changes = graph_changes(old, new, retire, fresh_lineage);
    let updates = bump_graph_generations(states, &changes, parity)?;
    effects_updates.extend(updates);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_graph_and_parity(
    before: &RepairRelationSnapshot,
    after: &RepairRelationSnapshot,
    before_edges: &BTreeMap<String, Edge>,
    after_edges: &BTreeMap<String, Edge>,
    relation_id: &str,
    retire_ids: &[String],
    is_add: bool,
    fresh_lineage: Option<&str>,
    effects: &RelationWriteEffects,
) -> Result<u64, WenlanError> {
    let mut states = parse_graph_states(before)?;
    let after_states = parse_graph_states(after)?;
    let mut expected_updates = Vec::new();
    let mut parity = 0;
    if is_add {
        let target = after_edges
            .get(relation_id)
            .ok_or_else(|| invalid("graph_target_missing"))?;
        apply_edge_operation(
            &mut states,
            &mut expected_updates,
            &mut parity,
            before_edges.get(relation_id),
            target,
            false,
            fresh_lineage,
        )?;
        for id in retire_ids {
            let old = before_edges
                .get(id)
                .ok_or_else(|| invalid(format!("graph_retire_missing_{id}")))?;
            let new = after_edges
                .get(id)
                .ok_or_else(|| invalid(format!("graph_retire_after_missing_{id}")))?;
            apply_edge_operation(
                &mut states,
                &mut expected_updates,
                &mut parity,
                Some(old),
                new,
                true,
                None,
            )?;
        }
    } else {
        let old = before_edges
            .get(relation_id)
            .ok_or_else(|| invalid("graph_retire_target_missing"))?;
        let new = after_edges
            .get(relation_id)
            .ok_or_else(|| invalid("graph_retire_target_after_missing"))?;
        apply_edge_operation(
            &mut states,
            &mut expected_updates,
            &mut parity,
            Some(old),
            new,
            true,
            None,
        )?;
    }
    require(states.keys().eq(after_states.keys()), "graph_state_rows")?;
    for (space, expected) in &states {
        let actual = after_states
            .get(space)
            .ok_or_else(|| invalid(format!("graph_state_missing_{space}")))?;
        require(
            expected.graph_generation == actual.graph_generation
                && expected.grouping_generation == actual.grouping_generation
                && expected.published_generation == actual.published_generation
                && expected.dirty == actual.dirty,
            format!("graph_state_changed_{space}"),
        )?;
    }
    require(effects.graph_updates == expected_updates, "graph_updates")?;
    Ok(parity)
}

fn validate_vocabulary(
    before: &RepairRelationSnapshot,
    after: &RepairRelationSnapshot,
    canonical: &str,
    target_was_present: bool,
    incremented: bool,
) -> Result<(), WenlanError> {
    let before_table = table(before, RepairRelationTable::RelationTypeVocabulary)?;
    let after_table = table(after, RepairRelationTable::RelationTypeVocabulary)?;
    let before_rows = rows_by_key(before_table, "canonical")?;
    let after_rows = rows_by_key(after_table, "canonical")?;
    require(before_rows.keys().eq(after_rows.keys()), "vocabulary_rows")?;
    if target_was_present {
        require(!incremented, "vocabulary_incremented_reassert")?;
        require(before_table.rows == after_table.rows, "vocabulary_changed")?;
    } else {
        require(incremented, "vocabulary_not_incremented")?;
        let before_canonical = before_rows.get(canonical);
        let after_canonical = after_rows.get(canonical);
        require(after_canonical.is_some(), "vocabulary_canonical_missing")?;
        let old = before_canonical.ok_or_else(|| invalid("vocabulary_before_missing"))?;
        let new = after_canonical.ok_or_else(|| invalid("vocabulary_after_missing"))?;
        for field in ["canonical", "aliases", "category"] {
            require(
                cell(before_table, old, field)? == cell(after_table, new, field)?,
                format!("vocabulary_{field}"),
            )?;
        }
        let old_count = integer(cell(before_table, old, "count")?, "vocabulary_count")?;
        let new_count = integer(cell(after_table, new, "count")?, "vocabulary_count")?;
        require(old_count >= 0, "vocabulary_negative_count")?;
        require(
            new_count
                == old_count
                    .checked_add(1)
                    .ok_or_else(|| invalid("vocabulary_count_overflow"))?,
            "vocabulary_count",
        )?;
        for key in before_rows.keys() {
            if key != canonical {
                require(
                    before_rows.get(key) == after_rows.get(key),
                    format!("vocabulary_row_changed_{key}"),
                )?;
            }
        }
    }
    Ok(())
}

fn expected_archived(edge: &Edge) -> Result<Value, WenlanError> {
    let payload = json_object(&edge.payload, "archived_payload")?;
    Ok(serde_json::json!({
        "id": edge.id,
        "from_entity": edge.src,
        "to_entity": edge.dst,
        "relation_type": edge.semantic_type,
        "source_agent": payload.get("source_agent").cloned().unwrap_or(Value::Null),
        "confidence": payload.get("confidence").cloned().unwrap_or(Value::Null),
        "explanation": payload.get("explanation").cloned().unwrap_or(Value::Null),
        "source_memory_id": payload.get("source_memory_id").cloned().unwrap_or(Value::Null),
        "created_at": edge.created_at,
    }))
}

#[allow(clippy::too_many_arguments)]
fn validate_activity(
    before: &RepairRelationSnapshot,
    after: &RepairRelationSnapshot,
    manifest: &RepairManifest,
    effects: &RelationWriteEffects,
    relation_id: &str,
    from: &str,
    to: &str,
    requested: Option<&str>,
    retire_ids: &[String],
    before_edges: &BTreeMap<String, Edge>,
    started_at: i64,
    finished_at: i64,
) -> Result<(), WenlanError> {
    let before_table = table(before, RepairRelationTable::AgentActivity)?;
    let after_table = table(after, RepairRelationTable::AgentActivity)?;
    let before_rows = rows_by_integer_key(before_table, "id")?;
    let after_rows = rows_by_integer_key(after_table, "id")?;
    require(
        after_rows.len() == before_rows.len() + effects.activity.len(),
        "activity_count",
    )?;
    let before_ids = before_rows.keys().copied().collect::<Vec<_>>();
    for id in before_ids {
        require(
            after_rows.contains_key(&id),
            format!("activity_overwritten_{id}"),
        )?;
        require(
            before_rows[&id] == after_rows[&id],
            format!("activity_changed_{id}"),
        )?;
    }
    let mut new_ids = after_rows
        .keys()
        .filter(|id| !before_rows.contains_key(id))
        .copied()
        .collect::<Vec<_>>();
    new_ids.sort_unstable();
    require(new_ids.len() == effects.activity.len(), "activity_new_ids")?;
    require(
        new_ids.windows(2).all(|pair| pair[0] < pair[1]),
        "activity_id_unique",
    )?;
    let expected_actions = if requested.is_some() {
        let mut actions = retire_ids
            .iter()
            .map(|_| "relation_supersede_auto")
            .collect::<Vec<_>>();
        actions.push("relation_create");
        actions
    } else {
        vec!["relation_retire"]
    };
    require(
        effects.activity.len() == expected_actions.len()
            && effects
                .activity
                .iter()
                .zip(expected_actions)
                .all(|(effect, action)| effect.action == action),
        "activity_actions",
    )?;
    let review_id = manifest
        .source()
        .review_binding()
        .ok_or_else(|| invalid("review_binding"))?
        .review_id();
    for (id, effect) in new_ids.iter().zip(effects.activity.iter()) {
        let row = after_rows[id];
        let timestamp = integer(cell(after_table, row, "timestamp")?, "activity_timestamp")?;
        require(
            in_window(timestamp, started_at, finished_at),
            "activity_timestamp_window",
        )?;
        require(
            text(cell(after_table, row, "agent_name")?, "activity_agent")? == SOURCE_REPAIR_AGENT,
            "activity_agent",
        )?;
        require(
            text(cell(after_table, row, "query")?, "activity_query")? == manifest.manifest_id(),
            "activity_query",
        )?;
        let ids = if effect.memory_ids.is_empty() {
            "".to_string()
        } else {
            effect.memory_ids.join(",")
        };
        require(
            text(cell(after_table, row, "memory_ids")?, "activity_memory_ids")? == ids,
            "activity_memory_ids",
        )?;
        require(
            text(cell(after_table, row, "action")?, "activity_action")? == effect.action,
            "activity_action",
        )?;
        require(
            text(cell(after_table, row, "detail")?, "activity_detail")? == effect.detail,
            "activity_detail",
        )?;
        require(*id > 0, "activity_id_positive")?;
    }
    let review_before = rows_by_key(table(before, RepairRelationTable::RefinementQueue)?, "id")?;
    let review_after = rows_by_key(table(after, RepairRelationTable::RefinementQueue)?, "id")?;
    let review_before = review_before
        .get(review_id)
        .ok_or_else(|| invalid("review_row_missing"))?;
    let review_after = review_after
        .get(review_id)
        .ok_or_else(|| invalid("review_row_missing_after"))?;
    require(*review_before == *review_after, "review_row_changed")?;

    if let Some(requested) = requested {
        // Check the canonical details independently of the effects object so a
        // forged effect cannot make an unrelated auto-retirement look valid.
        for (index, old_id) in retire_ids.iter().enumerate() {
            let old = before_edges
                .get(old_id)
                .ok_or_else(|| invalid(format!("activity_old_edge_{old_id}")))?;
            require(
                effects.activity[index].memory_ids == vec![relation_id.to_string(), old_id.clone()],
                "activity_auto_ids",
            )?;
            let detail: Value = serde_json::from_str(&effects.activity[index].detail)
                .map_err(|_| invalid("activity_auto_detail_json"))?;
            let expected = serde_json::json!({
                "existing_id": old_id,
                "new_id": relation_id,
                "from": from,
                "to": to,
                "old_type": old.semantic_type,
                "new_type": requested,
                "archived": expected_archived(old)?,
            });
            require(detail == expected, "activity_auto_detail")?;
        }
        let create = effects
            .activity
            .last()
            .ok_or_else(|| invalid("activity_create_missing"))?;
        require(
            create.memory_ids == vec![relation_id.to_string()],
            "activity_create_ids",
        )?;
        require(
            create.detail == format!("from={from}, to={to}, type={requested}"),
            "activity_create_detail",
        )?;
    } else {
        let effect = effects
            .activity
            .first()
            .ok_or_else(|| invalid("activity_retire_missing"))?;
        let old = before_edges
            .get(relation_id)
            .ok_or_else(|| invalid("activity_retire_edge"))?;
        let detail: Value = serde_json::from_str(&effect.detail)
            .map_err(|_| invalid("activity_retire_detail_json"))?;
        let archived = expected_archived(old)?;
        require(
            detail
                == serde_json::json!({"archived": archived, "repair_manifest_id": manifest.manifest_id()}),
            "activity_retire_detail",
        )?;
        require(
            effect.memory_ids == vec![relation_id.to_string()],
            "activity_retire_ids",
        )?;
    }
    Ok(())
}

fn validate_promotion(
    before: &RepairRelationSnapshot,
    after: &RepairRelationSnapshot,
    manifest: &RepairManifest,
    promotion: Option<&str>,
    proposed: bool,
    started_at: i64,
    finished_at: i64,
) -> Result<(), WenlanError> {
    let before_table = table(before, RepairRelationTable::RefinementQueue)?;
    let after_table = table(after, RepairRelationTable::RefinementQueue)?;
    let before_rows = rows_by_key(before_table, "id")?;
    let after_rows = rows_by_key(after_table, "id")?;
    let review_id = manifest
        .source()
        .review_binding()
        .ok_or_else(|| invalid("review_binding"))?
        .review_id();
    let Some(promotion) = promotion else {
        require(!proposed, "unexpected_promotion")?;
        require(before_table.rows == after_table.rows, "queue_changed")?;
        return Ok(());
    };
    let promotion_id = db::MemoryDB::vocab_proposal_fingerprint("relation", promotion);
    require(promotion_id != review_id, "promotion_review_collision")?;
    for (id, row) in &before_rows {
        if id != &promotion_id {
            require(
                after_rows.get(id) == Some(row),
                format!("queue_row_changed_{id}"),
            )?;
        }
    }
    let before_promotion = before_rows.get(&promotion_id);
    let after_promotion = after_rows.get(&promotion_id);
    match (before_promotion, after_promotion, proposed) {
        (Some(before), Some(after), false) => {
            require(*before == *after, "promotion_existing_changed")?
        }
        (None, Some(after), true) => {
            require(
                text(cell(after_table, after, "action")?, "promotion_action")? == "vocab_promote",
                "promotion_action",
            )?;
            require(
                text(cell(after_table, after, "source_ids")?, "promotion_sources")? == "[]",
                "promotion_sources",
            )?;
            let payload: Value = text(cell(after_table, after, "payload")?, "promotion_payload")?
                .parse()
                .map_err(|_| invalid("promotion_payload_json"))?;
            require(
                payload
                    == serde_json::json!({"action":"vocab_promote","kind":"relation","old_value":promotion,"category":null}),
                "promotion_payload",
            )?;
            same_json_number(
                &Value::Number(
                    serde_json::Number::from_f64(real(
                        cell(after_table, after, "confidence")?,
                        "promotion_confidence",
                    )?)
                    .ok_or_else(|| invalid("promotion_confidence"))?,
                ),
                1.0,
                "promotion_confidence",
            )?;
            require(
                text(cell(after_table, after, "status")?, "promotion_status")? == "awaiting_review",
                "promotion_status",
            )?;
            let created = text(
                cell(after_table, after, "created_at")?,
                "promotion_created_at",
            )?;
            let timestamp = chrono::NaiveDateTime::parse_from_str(created, "%Y-%m-%d %H:%M:%S")
                .map(|value| value.and_utc().timestamp())
                .map_err(|_| invalid("promotion_created_at"))?;
            require(
                in_window(timestamp, started_at, finished_at),
                "promotion_created_at_window",
            )?;
            require(
                matches!(
                    cell(after_table, after, "resolved_at")?,
                    RepairRelationSqlValue::Null
                ),
                "promotion_resolved_at",
            )?;
        }
        (None, None, false) => return Err(invalid("promotion_missing_after")),
        (None, Some(_), false) => return Err(invalid("promotion_unreported_insert")),
        (Some(_), Some(_), true) => return Err(invalid("promotion_existing_marked_new")),
        (Some(_), None, _) => return Err(invalid("promotion_deleted")),
        (None, None, true) => return Err(invalid("promotion_not_inserted")),
    }
    require(
        after_rows.len() == before_rows.len() + usize::from(before_promotion.is_none()),
        "promotion_extra_rows",
    )?;
    Ok(())
}

fn validate_memories_source(
    before: &RepairRelationSnapshot,
    source_memory_id: Option<&str>,
) -> Result<(), WenlanError> {
    let Some(source_memory_id) = source_memory_id else {
        return Ok(());
    };
    let table = table(before, RepairRelationTable::Memories)?;
    let mut found = false;
    for row in &table.rows {
        let source = text(cell(table, row, "source")?, "memory_source")?;
        let source_id = text(cell(table, row, "source_id")?, "memory_source_id")?;
        if source == "memory" && source_id == source_memory_id {
            found = true;
        }
    }
    require(found, "source_memory_missing")
}

/// Validate the exact owned effects of one entity-relation repair.
///
/// The returned value is the expected increment of the singleton community
/// parity generation: every state-row write performed by the canonical edge
/// triggers and the graph-generation normalization helper contributes one.
pub(crate) fn validate(
    before: &RepairRelationSnapshot,
    after: &RepairRelationSnapshot,
    manifest: &RepairManifest,
    effects: &RelationWriteEffects,
    started_at: i64,
    finished_at: i64,
) -> Result<u64, WenlanError> {
    validate_window(started_at, finished_at)?;
    validate_shapes(before, after)?;
    let (relation_id, from, to, change) = validate_manifest(manifest)?;
    let (before_edges, after_edges) = parse_edges(before, after)?;
    let retire_target_ids = vec![relation_id.to_string()];
    let (is_add, requested, canonical, source_memory_id, confidence, retire_ids, promotion) =
        match change {
            wenlan_types::repair_relation::RepairRelationMutation::Add {
                requested_relation_type,
                canonical_relation_type,
                source_memory_id,
                confidence_basis_points,
                retire_relation_ids,
                vocabulary_promotion,
            } => (
                true,
                Some(requested_relation_type.as_str()),
                canonical_relation_type.as_str(),
                source_memory_id.as_deref(),
                *confidence_basis_points,
                retire_relation_ids.as_slice(),
                vocabulary_promotion.as_deref(),
            ),
            wenlan_types::repair_relation::RepairRelationMutation::Retire => {
                (false, None, "", None, 0, retire_target_ids.as_slice(), None)
            }
        };
    for table_kind in [
        RepairRelationTable::EntityPageMap,
        RepairRelationTable::Pages,
        RepairRelationTable::Memories,
    ] {
        require(
            table(before, table_kind)?.rows == table(after, table_kind)?.rows,
            format!("{}_changed", table_name(table_kind)),
        )?;
    }
    require(
        !retire_ids.iter().any(|id| id == relation_id) || !is_add,
        "target_in_retire_ids",
    )?;
    if is_add {
        validate_memories_source(before, source_memory_id)?;
        let (_, prior, fresh_lineage) = validate_target(
            &before_edges,
            &after_edges,
            before,
            relation_id,
            from,
            to,
            canonical,
            source_memory_id,
            confidence,
            !retire_ids.is_empty(),
            started_at,
            finished_at,
        )?;
        validate_declared_retirements(&before_edges, relation_id, requested.unwrap(), retire_ids)?;
        validate_retired(
            &before_edges,
            &after_edges,
            retire_ids,
            started_at,
            finished_at,
        )?;
        compare_edge_sets(
            &before_edges,
            &after_edges,
            relation_id,
            prior.is_some(),
            retire_ids,
        )?;
        validate_vocabulary(
            before,
            after,
            canonical,
            prior.is_some(),
            effects.vocabulary_incremented,
        )?;
        validate_promotion(
            before,
            after,
            manifest,
            promotion,
            effects.vocabulary_proposed,
            started_at,
            finished_at,
        )?;
        validate_activity(
            before,
            after,
            manifest,
            effects,
            relation_id,
            from,
            to,
            requested,
            retire_ids,
            &before_edges,
            started_at,
            finished_at,
        )?;
        validate_graph_and_parity(
            before,
            after,
            &before_edges,
            &after_edges,
            relation_id,
            retire_ids,
            true,
            Some(&fresh_lineage),
            effects,
        )
    } else {
        require(
            !effects.vocabulary_incremented && !effects.vocabulary_proposed,
            "retire_side_effects",
        )?;
        validate_retired(
            &before_edges,
            &after_edges,
            retire_ids,
            started_at,
            finished_at,
        )?;
        compare_edge_sets(&before_edges, &after_edges, relation_id, true, retire_ids)?;
        validate_vocabulary(before, after, "", true, false)?;
        validate_promotion(
            before,
            after,
            manifest,
            None,
            false,
            started_at,
            finished_at,
        )?;
        validate_activity(
            before,
            after,
            manifest,
            effects,
            relation_id,
            from,
            to,
            None,
            retire_ids,
            &before_edges,
            started_at,
            finished_at,
        )?;
        validate_graph_and_parity(
            before,
            after,
            &before_edges,
            &after_edges,
            relation_id,
            retire_ids,
            false,
            None,
            effects,
        )
    }
}
