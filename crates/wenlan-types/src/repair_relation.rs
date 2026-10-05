// SPDX-License-Identifier: Apache-2.0
//! Intent contract for an entity-relation repair selection.
//!
//! This is a frozen wire shape only: it validates and roundtrips a
//! reviewer's chosen repair action for an entity-relation lint finding.
//! Canonical relation-type vocabulary, snapshot resolution, and the actual
//! write/apply path belong to wenlan-core; this type never claims apply
//! authorization or completion.

use serde::{Deserialize, Serialize};

const INVALID_SELECTION: &str = "invalid_entity_relation_repair_selection";

/// A reviewer's chosen repair action for an entity-relation lint finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntityRelationRepairChoice {
    Add {
        from_entity: String,
        to_entity: String,
        relation_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_memory_id: Option<String>,
    },
    Retire {
        relation_id: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum EntityRelationRepairChoiceWire {
    Add {
        from_entity: String,
        to_entity: String,
        relation_type: String,
        #[serde(default)]
        source_memory_id: Option<String>,
    },
    Retire {
        relation_id: String,
    },
}

impl From<EntityRelationRepairChoiceWire> for EntityRelationRepairChoice {
    fn from(wire: EntityRelationRepairChoiceWire) -> Self {
        match wire {
            EntityRelationRepairChoiceWire::Add {
                from_entity,
                to_entity,
                relation_type,
                source_memory_id,
            } => Self::Add {
                from_entity,
                to_entity,
                relation_type,
                source_memory_id,
            },
            EntityRelationRepairChoiceWire::Retire { relation_id } => Self::Retire { relation_id },
        }
    }
}

/// Nonempty, trim-stable, and free of control characters. No length limit is
/// imposed here; canonical length/format constraints belong to the caller.
fn is_valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.trim() == value && !value.chars().any(|c| c.is_control())
}

/// `^[a-z][a-z0-9_]*$`. Canonical vocabulary lookup (which relation types are
/// actually known) is wenlan-core's job, not this wire contract's.
fn is_valid_relation_type(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl EntityRelationRepairChoice {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Add {
                from_entity,
                to_entity,
                relation_type,
                source_memory_id,
            } => {
                if !is_valid_identifier(from_entity) || !is_valid_identifier(to_entity) {
                    return Err(INVALID_SELECTION.to_string());
                }
                if from_entity == to_entity {
                    return Err(INVALID_SELECTION.to_string());
                }
                if !is_valid_relation_type(relation_type) {
                    return Err(INVALID_SELECTION.to_string());
                }
                if let Some(source) = source_memory_id {
                    if !is_valid_identifier(source) {
                        return Err(INVALID_SELECTION.to_string());
                    }
                }
                Ok(())
            }
            Self::Retire { relation_id } => {
                if !is_valid_identifier(relation_id) {
                    return Err(INVALID_SELECTION.to_string());
                }
                Ok(())
            }
        }
    }
}

impl<'de> Deserialize<'de> for EntityRelationRepairChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let choice = EntityRelationRepairChoiceWire::deserialize(deserializer)?.into();
        Self::validate(&choice).map_err(serde::de::Error::custom)?;
        Ok(choice)
    }
}

/// A review-scoped entity-relation repair selection: which review this
/// answers, and the reviewer's chosen action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityRelationRepairSelection {
    pub review_id: String,
    pub choice: EntityRelationRepairChoice,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntityRelationRepairSelectionWire {
    review_id: String,
    choice: EntityRelationRepairChoiceWire,
}

impl EntityRelationRepairSelection {
    pub fn validate(&self) -> Result<(), String> {
        if !is_valid_identifier(&self.review_id) {
            return Err(INVALID_SELECTION.to_string());
        }
        self.choice.validate()
    }
}

impl<'de> Deserialize<'de> for EntityRelationRepairSelection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = EntityRelationRepairSelectionWire::deserialize(deserializer)?;
        let selection = Self {
            review_id: wire.review_id,
            choice: wire.choice.into(),
        };
        selection.validate().map_err(serde::de::Error::custom)?;
        Ok(selection)
    }
}

const INVALID_MUTATION: &str = "invalid_entity_relation_repair_mutation";
const INVALID_SQL_VALUE: &str = "invalid_entity_relation_repair_sql_value";
const INVALID_SNAPSHOT: &str = "invalid_entity_relation_repair_snapshot";

/// Canonical fallback relation type: a requested type promoted into the
/// shared vocabulary normalizes to this type.
const CANONICAL_FALLBACK_RELATION_TYPE: &str = "related_to";

/// Maximum confidence for a relation mutation, in basis points (100.00%).
const MAX_CONFIDENCE_BASIS_POINTS: u16 = 10_000;

/// The manifest-level mutation for an entity-relation repair: what the
/// writer must add (normalizing the requested type to its canonical form)
/// or which relation it must retire. Vocabulary lookup and the actual
/// write/apply path belong to wenlan-core; this type only validates and
/// roundtrips the requested change. Direct Rust construction must be
/// checked again with [`RepairRelationMutation::validate`] at manifest
/// validation time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairRelationMutation {
    Add {
        requested_relation_type: String,
        canonical_relation_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_memory_id: Option<String>,
        confidence_basis_points: u16,
        retire_relation_ids: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        vocabulary_promotion: Option<String>,
    },
    Retire,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RepairRelationMutationWire {
    Add {
        requested_relation_type: String,
        canonical_relation_type: String,
        #[serde(default)]
        source_memory_id: Option<String>,
        confidence_basis_points: u16,
        retire_relation_ids: Vec<String>,
        #[serde(default)]
        vocabulary_promotion: Option<String>,
    },
    Retire {},
}

impl From<RepairRelationMutationWire> for RepairRelationMutation {
    fn from(wire: RepairRelationMutationWire) -> Self {
        match wire {
            RepairRelationMutationWire::Add {
                requested_relation_type,
                canonical_relation_type,
                source_memory_id,
                confidence_basis_points,
                retire_relation_ids,
                vocabulary_promotion,
            } => Self::Add {
                requested_relation_type,
                canonical_relation_type,
                source_memory_id,
                confidence_basis_points,
                retire_relation_ids,
                vocabulary_promotion,
            },
            RepairRelationMutationWire::Retire {} => Self::Retire,
        }
    }
}

/// Sorted, unique, and each a valid identifier. Empty is allowed (a fresh
/// add retires nothing).
fn is_sorted_unique_identifiers(values: &[String]) -> bool {
    values.iter().all(|value| is_valid_identifier(value))
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

impl RepairRelationMutation {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Add {
                requested_relation_type,
                canonical_relation_type,
                source_memory_id,
                confidence_basis_points,
                retire_relation_ids,
                vocabulary_promotion,
            } => {
                if !is_valid_relation_type(requested_relation_type)
                    || !is_valid_relation_type(canonical_relation_type)
                {
                    return Err(INVALID_MUTATION.to_string());
                }
                if let Some(source) = source_memory_id {
                    if !is_valid_identifier(source) {
                        return Err(INVALID_MUTATION.to_string());
                    }
                }
                if *confidence_basis_points > MAX_CONFIDENCE_BASIS_POINTS {
                    return Err(INVALID_MUTATION.to_string());
                }
                if !is_sorted_unique_identifiers(retire_relation_ids) {
                    return Err(INVALID_MUTATION.to_string());
                }
                // Without a promotion the requested type may be an alias;
                // core resolves it against the captured vocabulary.
                if vocabulary_promotion.as_ref().is_some_and(|promotion| {
                    promotion != requested_relation_type
                        || canonical_relation_type != CANONICAL_FALLBACK_RELATION_TYPE
                }) {
                    return Err(INVALID_MUTATION.to_string());
                }
                Ok(())
            }
            Self::Retire => Ok(()),
        }
    }
}

impl<'de> Deserialize<'de> for RepairRelationMutation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mutation = RepairRelationMutationWire::deserialize(deserializer)?.into();
        Self::validate(&mutation).map_err(serde::de::Error::custom)?;
        Ok(mutation)
    }
}

/// Captured input/output tables for an entity-relation rollback snapshot.
/// Fixed set only: no arbitrary table names. This is lossless row capture
/// for core's future V3 rollback, not SQL to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairRelationTable {
    Edges,
    EntityPageMap,
    Pages,
    Memories,
    SpaceGraphState,
    RelationTypeVocabulary,
    RefinementQueue,
    AgentActivity,
}

impl RepairRelationTable {
    /// All captured tables, in canonical snapshot order.
    pub const ALL_TABLES: [Self; 8] = [
        Self::Edges,
        Self::EntityPageMap,
        Self::Pages,
        Self::Memories,
        Self::SpaceGraphState,
        Self::RelationTypeVocabulary,
        Self::RefinementQueue,
        Self::AgentActivity,
    ];
}

fn is_lower_hex(value: &str) -> bool {
    value
        .chars()
        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// A single lossless cell value in a captured rollback row. `Real` carries
/// the raw IEEE-754 bits as exactly 16 lowercase hex characters (not a
/// lossy decimal rendering); `Text` carries arbitrary data faithfully.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairRelationSqlValue {
    Null,
    Integer { value: i64 },
    Real { bits: String },
    Text { value: String },
    Blob { hex: String },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RepairRelationSqlValueWire {
    Null {},
    Integer { value: i64 },
    Real { bits: String },
    Text { value: String },
    Blob { hex: String },
}

impl From<RepairRelationSqlValueWire> for RepairRelationSqlValue {
    fn from(wire: RepairRelationSqlValueWire) -> Self {
        match wire {
            RepairRelationSqlValueWire::Null {} => Self::Null,
            RepairRelationSqlValueWire::Integer { value } => Self::Integer { value },
            RepairRelationSqlValueWire::Real { bits } => Self::Real { bits },
            RepairRelationSqlValueWire::Text { value } => Self::Text { value },
            RepairRelationSqlValueWire::Blob { hex } => Self::Blob { hex },
        }
    }
}

impl RepairRelationSqlValue {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Null | Self::Integer { .. } | Self::Text { .. } => Ok(()),
            Self::Real { bits } => {
                if bits.len() == 16 && is_lower_hex(bits) {
                    Ok(())
                } else {
                    Err(INVALID_SQL_VALUE.to_string())
                }
            }
            Self::Blob { hex } => {
                if hex.len().is_multiple_of(2) && is_lower_hex(hex) {
                    Ok(())
                } else {
                    Err(INVALID_SQL_VALUE.to_string())
                }
            }
        }
    }
}

impl<'de> Deserialize<'de> for RepairRelationSqlValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = RepairRelationSqlValueWire::deserialize(deserializer)?.into();
        Self::validate(&value).map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

/// `[A-Za-z_][A-Za-z0-9_]*` (ASCII only). No SQL is executed here.
fn is_valid_sql_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Lossless capture of one table's rows. Row width must exactly match the
/// column list. Empty row sets are allowed (a fresh add needs the target
/// absent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairRelationTableSnapshot {
    pub table: RepairRelationTable,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<RepairRelationSqlValue>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairRelationTableSnapshotWire {
    table: RepairRelationTable,
    columns: Vec<String>,
    rows: Vec<Vec<RepairRelationSqlValue>>,
}

impl RepairRelationTableSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.columns.is_empty()
            || !self
                .columns
                .iter()
                .all(|column| is_valid_sql_identifier(column))
        {
            return Err(INVALID_SNAPSHOT.to_string());
        }
        let mut sorted = self.columns.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != self.columns.len() {
            return Err(INVALID_SNAPSHOT.to_string());
        }
        for row in &self.rows {
            if row.len() != self.columns.len() {
                return Err(INVALID_SNAPSHOT.to_string());
            }
            for value in row {
                value.validate().map_err(|_| INVALID_SNAPSHOT.to_string())?;
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for RepairRelationTableSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = RepairRelationTableSnapshotWire::deserialize(deserializer)?;
        let snapshot = Self {
            table: wire.table,
            columns: wire.columns,
            rows: wire.rows,
        };
        snapshot.validate().map_err(serde::de::Error::custom)?;
        Ok(snapshot)
    }
}

/// Lossless capture of all input/output tables for an entity-relation
/// rollback: exactly the eight [`RepairRelationTable`] tables, in enum
/// order, no duplicates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairRelationSnapshot {
    pub tables: Vec<RepairRelationTableSnapshot>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairRelationSnapshotWire {
    tables: Vec<RepairRelationTableSnapshot>,
}

impl RepairRelationSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.tables.len() != RepairRelationTable::ALL_TABLES.len() {
            return Err(INVALID_SNAPSHOT.to_string());
        }
        for (snapshot, expected) in self
            .tables
            .iter()
            .zip(RepairRelationTable::ALL_TABLES.iter())
        {
            if snapshot.table != *expected {
                return Err(INVALID_SNAPSHOT.to_string());
            }
            snapshot.validate()?;
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for RepairRelationSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = RepairRelationSnapshotWire::deserialize(deserializer)?;
        let snapshot = Self {
            tables: wire.tables,
        };
        snapshot.validate().map_err(serde::de::Error::custom)?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_retire_selections_round_trip_as_typed_wire_contracts() {
        let selections = [
            EntityRelationRepairSelection {
                review_id: "review-1".to_string(),
                choice: EntityRelationRepairChoice::Add {
                    from_entity: "entity-a".to_string(),
                    to_entity: "entity-b".to_string(),
                    relation_type: "reports_to".to_string(),
                    source_memory_id: Some("memory-1".to_string()),
                },
            },
            EntityRelationRepairSelection {
                review_id: "review-2".to_string(),
                choice: EntityRelationRepairChoice::Add {
                    from_entity: "entity-a".to_string(),
                    to_entity: "entity-b".to_string(),
                    relation_type: "reports_to".to_string(),
                    source_memory_id: None,
                },
            },
            EntityRelationRepairSelection {
                review_id: "review-3".to_string(),
                choice: EntityRelationRepairChoice::Retire {
                    relation_id: "relation-1".to_string(),
                },
            },
        ];

        for selection in selections {
            let encoded = serde_json::to_value(&selection).unwrap();
            let decoded: EntityRelationRepairSelection = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded, selection);
        }

        let no_source = serde_json::to_string(&EntityRelationRepairSelection {
            review_id: "review-2".to_string(),
            choice: EntityRelationRepairChoice::Add {
                from_entity: "entity-a".to_string(),
                to_entity: "entity-b".to_string(),
                relation_type: "reports_to".to_string(),
                source_memory_id: None,
            },
        })
        .unwrap();
        assert!(
            !no_source.contains("source_memory_id"),
            "expected None source_memory_id to be omitted on the wire, got: {no_source}",
        );
    }

    #[test]
    fn rejects_unknown_fields_self_loop_malformed_identifiers_predicate_and_empty_source() {
        let valid = serde_json::json!({
            "review_id": "review-1",
            "choice": {
                "kind": "add",
                "from_entity": "entity-a",
                "to_entity": "entity-b",
                "relation_type": "reports_to",
            }
        });
        assert!(serde_json::from_value::<EntityRelationRepairSelection>(valid.clone()).is_ok());

        let mut unknown_top_level = valid.clone();
        unknown_top_level
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_string(), serde_json::json!(true));
        assert!(
            serde_json::from_value::<EntityRelationRepairSelection>(unknown_top_level).is_err()
        );

        let mut unknown_choice_level = valid.clone();
        unknown_choice_level["choice"]["unexpected"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<EntityRelationRepairSelection>(unknown_choice_level).is_err()
        );

        let mut self_loop = valid.clone();
        self_loop["choice"]["to_entity"] = serde_json::json!("entity-a");
        assert!(serde_json::from_value::<EntityRelationRepairSelection>(self_loop).is_err());

        let mut untrimmed_identifier = valid.clone();
        untrimmed_identifier["choice"]["from_entity"] = serde_json::json!(" entity-a");
        assert!(
            serde_json::from_value::<EntityRelationRepairSelection>(untrimmed_identifier).is_err()
        );

        let mut empty_review_id = valid.clone();
        empty_review_id["review_id"] = serde_json::json!("");
        assert!(serde_json::from_value::<EntityRelationRepairSelection>(empty_review_id).is_err());

        let mut bad_relation_type = valid.clone();
        bad_relation_type["choice"]["relation_type"] = serde_json::json!("Reports_To");
        assert!(
            serde_json::from_value::<EntityRelationRepairSelection>(bad_relation_type).is_err()
        );

        let mut empty_source = valid.clone();
        empty_source["choice"]["source_memory_id"] = serde_json::json!("");
        assert!(serde_json::from_value::<EntityRelationRepairSelection>(empty_source).is_err());

        let mut retire_unknown = serde_json::json!({
            "review_id": "review-1",
            "choice": {
                "kind": "retire",
                "relation_id": "relation-1",
            }
        });
        assert!(
            serde_json::from_value::<EntityRelationRepairSelection>(retire_unknown.clone()).is_ok()
        );
        retire_unknown["choice"]["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<EntityRelationRepairSelection>(retire_unknown).is_err());
    }

    #[test]
    fn direct_rust_construction_still_requires_explicit_validate_call() {
        let self_loop = EntityRelationRepairChoice::Add {
            from_entity: "entity-a".to_string(),
            to_entity: "entity-a".to_string(),
            relation_type: "reports_to".to_string(),
            source_memory_id: None,
        };
        assert_eq!(self_loop.validate(), Err(INVALID_SELECTION.to_string()));

        let valid = EntityRelationRepairChoice::Retire {
            relation_id: "relation-1".to_string(),
        };
        assert_eq!(valid.validate(), Ok(()));

        let selection = EntityRelationRepairSelection {
            review_id: String::new(),
            choice: valid,
        };
        assert_eq!(selection.validate(), Err(INVALID_SELECTION.to_string()));
    }

    fn valid_add_mutation() -> RepairRelationMutation {
        RepairRelationMutation::Add {
            requested_relation_type: "reports_to".to_string(),
            canonical_relation_type: "reports_to".to_string(),
            source_memory_id: Some("memory-1".to_string()),
            confidence_basis_points: 7500,
            retire_relation_ids: vec!["relation-1".to_string(), "relation-2".to_string()],
            vocabulary_promotion: None,
        }
    }

    fn empty_snapshot() -> RepairRelationSnapshot {
        RepairRelationSnapshot {
            tables: RepairRelationTable::ALL_TABLES
                .iter()
                .map(|table| RepairRelationTableSnapshot {
                    table: *table,
                    columns: vec!["id".to_string()],
                    rows: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn mutation_add_and_retire_round_trip_as_typed_wire_contracts() {
        for mutation in [valid_add_mutation(), RepairRelationMutation::Retire] {
            assert_eq!(mutation.validate(), Ok(()));
            let encoded = serde_json::to_value(&mutation).unwrap();
            let decoded: RepairRelationMutation = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded, mutation);
        }

        let omitted = serde_json::to_string(&RepairRelationMutation::Add {
            requested_relation_type: "reports_to".to_string(),
            canonical_relation_type: "reports_to".to_string(),
            source_memory_id: None,
            confidence_basis_points: 0,
            retire_relation_ids: Vec::new(),
            vocabulary_promotion: None,
        })
        .unwrap();
        assert!(
            !omitted.contains("source_memory_id") && !omitted.contains("vocabulary_promotion"),
            "expected None optionals to be omitted on the wire, got: {omitted}",
        );
    }

    #[test]
    fn mutation_rejects_bad_confidence_retire_ids_predicates_and_promotion() {
        // Confidence above 100.00% is rejected.
        let mut over_confident = valid_add_mutation();
        if let RepairRelationMutation::Add {
            confidence_basis_points,
            ..
        } = &mut over_confident
        {
            *confidence_basis_points = 10_001;
        }
        assert_eq!(over_confident.validate(), Err(INVALID_MUTATION.to_string()));

        // Retire ids must be sorted, unique, and valid identifiers.
        for retire_ids in [
            vec!["relation-2".to_string(), "relation-1".to_string()],
            vec!["relation-1".to_string(), "relation-1".to_string()],
            vec![" relation-1".to_string()],
        ] {
            let mut mutation = valid_add_mutation();
            if let RepairRelationMutation::Add {
                retire_relation_ids,
                ..
            } = &mut mutation
            {
                *retire_relation_ids = retire_ids;
            }
            assert_eq!(mutation.validate(), Err(INVALID_MUTATION.to_string()));
        }

        // Relation types use the same snake_case predicate as the intent.
        let mut bad_type = valid_add_mutation();
        if let RepairRelationMutation::Add {
            requested_relation_type,
            ..
        } = &mut bad_type
        {
            *requested_relation_type = "Reports_To".to_string();
        }
        assert_eq!(bad_type.validate(), Err(INVALID_MUTATION.to_string()));

        // Empty optional source is rejected.
        let mut empty_source = valid_add_mutation();
        if let RepairRelationMutation::Add {
            source_memory_id, ..
        } = &mut empty_source
        {
            *source_memory_id = Some(String::new());
        }
        assert_eq!(empty_source.validate(), Err(INVALID_MUTATION.to_string()));

        // Promotion must equal the requested type with canonical related_to.
        let promoted = RepairRelationMutation::Add {
            requested_relation_type: "manages".to_string(),
            canonical_relation_type: "related_to".to_string(),
            source_memory_id: None,
            confidence_basis_points: 9000,
            retire_relation_ids: Vec::new(),
            vocabulary_promotion: Some("manages".to_string()),
        };
        assert_eq!(promoted.validate(), Ok(()));

        let mismatched_promotion = RepairRelationMutation::Add {
            requested_relation_type: "manages".to_string(),
            canonical_relation_type: "related_to".to_string(),
            source_memory_id: None,
            confidence_basis_points: 9000,
            retire_relation_ids: Vec::new(),
            vocabulary_promotion: Some("reports_to".to_string()),
        };
        assert_eq!(
            mismatched_promotion.validate(),
            Err(INVALID_MUTATION.to_string())
        );

        let promotion_without_fallback = RepairRelationMutation::Add {
            requested_relation_type: "manages".to_string(),
            canonical_relation_type: "manages".to_string(),
            source_memory_id: None,
            confidence_basis_points: 9000,
            retire_relation_ids: Vec::new(),
            vocabulary_promotion: Some("manages".to_string()),
        };
        assert_eq!(
            promotion_without_fallback.validate(),
            Err(INVALID_MUTATION.to_string())
        );

        // Without a promotion a normalization alias may differ from canonical.
        let alias = RepairRelationMutation::Add {
            requested_relation_type: "manages".to_string(),
            canonical_relation_type: "reports_to".to_string(),
            source_memory_id: None,
            confidence_basis_points: 9000,
            retire_relation_ids: Vec::new(),
            vocabulary_promotion: None,
        };
        assert_eq!(alias.validate(), Ok(()));

        // Unknown wire fields and invalid wire values are rejected.
        let valid = serde_json::json!({
            "kind": "retire",
        });
        let mut unknown = valid.clone();
        unknown["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RepairRelationMutation>(unknown).is_err());

        let bad_confidence = serde_json::json!({
            "kind": "add",
            "requested_relation_type": "reports_to",
            "canonical_relation_type": "reports_to",
            "confidence_basis_points": 10001,
            "retire_relation_ids": [],
        });
        assert!(serde_json::from_value::<RepairRelationMutation>(bad_confidence).is_err());

        // Direct construction is re-checked by validate, as manifest
        // validation will do again.
        assert_eq!(RepairRelationMutation::Retire.validate(), Ok(()));
    }

    #[test]
    fn sql_values_round_trip_and_reject_malformed_bits() {
        let values = [
            RepairRelationSqlValue::Null,
            RepairRelationSqlValue::Integer { value: i64::MIN },
            RepairRelationSqlValue::Real {
                bits: "3ff8000000000000".to_string(),
            },
            RepairRelationSqlValue::Text {
                value: "arbitrary\0data\nwith \"quotes\" and \u{1f600}".to_string(),
            },
            RepairRelationSqlValue::Blob { hex: String::new() },
            RepairRelationSqlValue::Blob {
                hex: "00ff".to_string(),
            },
        ];
        for value in values {
            assert_eq!(value.validate(), Ok(()));
            let encoded = serde_json::to_value(&value).unwrap();
            let decoded: RepairRelationSqlValue = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded, value);
        }

        for bad in [
            serde_json::json!({"kind": "real", "bits": "3ff800000000000"}),
            serde_json::json!({"kind": "real", "bits": "3FF8000000000000"}),
            serde_json::json!({"kind": "real", "bits": "3ff800000000000g"}),
            serde_json::json!({"kind": "blob", "hex": "abc"}),
            serde_json::json!({"kind": "blob", "hex": "AB"}),
            serde_json::json!({"kind": "null", "unexpected": true}),
        ] {
            assert!(
                serde_json::from_value::<RepairRelationSqlValue>(bad.clone()).is_err(),
                "expected rejection, got ok for: {bad}",
            );
        }
    }

    #[test]
    fn snapshots_require_exact_width_unique_columns_and_all_eight_tables_in_order() {
        let snapshot = empty_snapshot();
        assert_eq!(snapshot.validate(), Ok(()));
        let encoded = serde_json::to_value(&snapshot).unwrap();
        let decoded: RepairRelationSnapshot = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, snapshot);

        // A populated table with exact-width rows roundtrips.
        let mut populated = empty_snapshot();
        populated.tables[0] = RepairRelationTableSnapshot {
            table: RepairRelationTable::Edges,
            columns: vec!["id".to_string(), "weight".to_string(), "score".to_string()],
            rows: vec![vec![
                RepairRelationSqlValue::Text {
                    value: "edge-1".to_string(),
                },
                RepairRelationSqlValue::Integer { value: 3 },
                RepairRelationSqlValue::Real {
                    bits: "4008000000000000".to_string(),
                },
            ]],
        };
        assert_eq!(populated.validate(), Ok(()));
        let encoded = serde_json::to_value(&populated).unwrap();
        let decoded: RepairRelationSnapshot = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, populated);

        // Row width mismatch.
        let mut wide = empty_snapshot();
        wide.tables[0].rows = vec![vec![RepairRelationSqlValue::Null]];
        wide.tables[0].columns = vec!["a".to_string(), "b".to_string()];
        assert_eq!(wide.validate(), Err(INVALID_SNAPSHOT.to_string()));
        assert!(serde_json::from_value::<RepairRelationSnapshot>(
            serde_json::to_value(&wide).unwrap()
        )
        .is_err());

        // Duplicate, empty, and non-identifier columns.
        for columns in [
            vec!["id".to_string(), "id".to_string()],
            Vec::new(),
            vec!["1id".to_string()],
            vec!["has space".to_string()],
        ] {
            let mut bad = empty_snapshot();
            bad.tables[0].columns = columns;
            assert_eq!(bad.validate(), Err(INVALID_SNAPSHOT.to_string()));
        }

        // Swapped order, missing table, and duplicate table.
        let mut swapped = empty_snapshot();
        swapped.tables.swap(0, 1);
        assert_eq!(swapped.validate(), Err(INVALID_SNAPSHOT.to_string()));

        let mut missing = empty_snapshot();
        missing.tables.pop();
        assert_eq!(missing.validate(), Err(INVALID_SNAPSHOT.to_string()));

        let mut duplicated = empty_snapshot();
        duplicated.tables[1] = duplicated.tables[0].clone();
        assert_eq!(duplicated.validate(), Err(INVALID_SNAPSHOT.to_string()));
    }
}
