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
}
