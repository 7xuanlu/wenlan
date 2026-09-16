// SPDX-License-Identifier: Apache-2.0
//! Request contract for preparing a repair from daemon-fresh lint reports.

use crate::{repair::RepairLintScope, repair_relation::EntityRelationRepairSelection, MemoryType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CurrentRepairChoice {
    ReclassifyMemory {
        review_id: String,
        memory_id: String,
        after_memory_type: MemoryType,
    },
    RenamePageTitle {
        review_id: String,
        page_id: String,
        before_title: String,
        after_title: String,
    },
    CompleteEntityExtraction {
        review_id: String,
        memory_id: String,
        entity_ids: Vec<String>,
    },
    EntityRelation {
        selection: EntityRelationRepairSelection,
    },
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum CurrentRepairChoiceWire {
    ReclassifyMemory {
        review_id: String,
        memory_id: String,
        after_memory_type: MemoryType,
    },
    RenamePageTitle {
        review_id: String,
        page_id: String,
        before_title: String,
        after_title: String,
    },
    CompleteEntityExtraction {
        review_id: String,
        memory_id: String,
        entity_ids: Vec<String>,
    },
    EntityRelation {
        selection: EntityRelationRepairSelection,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrepareCurrentRepairRequest {
    pub lint_scope: RepairLintScope,
    pub choice: CurrentRepairChoice,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareCurrentRepairRequestWire {
    lint_scope: RepairLintScope,
    choice: CurrentRepairChoiceWire,
}

impl CurrentRepairChoice {
    fn nonempty(value: &str) -> bool {
        !value.is_empty() && value.trim() == value
    }

    pub fn reclassify_memory(
        review_id: String,
        memory_id: String,
        after_memory_type: MemoryType,
    ) -> Result<Self, String> {
        if !Self::nonempty(&review_id) || !Self::nonempty(&memory_id) {
            return Err("invalid_prepare_current_repair_request".to_string());
        }
        Ok(Self::ReclassifyMemory {
            review_id,
            memory_id,
            after_memory_type,
        })
    }

    pub fn rename_page_title(
        review_id: String,
        page_id: String,
        before_title: String,
        after_title: String,
    ) -> Result<Self, String> {
        if !Self::nonempty(&review_id)
            || !Self::nonempty(&page_id)
            || !Self::nonempty(&before_title)
            || !Self::nonempty(&after_title)
            || before_title == after_title
        {
            return Err("invalid_prepare_current_repair_request".to_string());
        }
        Ok(Self::RenamePageTitle {
            review_id,
            page_id,
            before_title,
            after_title,
        })
    }

    pub fn complete_entity_extraction(
        review_id: String,
        memory_id: String,
        entity_ids: Vec<String>,
    ) -> Result<Self, String> {
        if !Self::nonempty(&review_id) || !Self::nonempty(&memory_id) {
            return Err("invalid_prepare_current_repair_request".to_string());
        }
        Ok(Self::CompleteEntityExtraction {
            review_id,
            memory_id,
            entity_ids,
        })
    }

    pub fn entity_relation(selection: EntityRelationRepairSelection) -> Result<Self, String> {
        selection.validate()?;
        Ok(Self::EntityRelation { selection })
    }
}

impl<'de> Deserialize<'de> for CurrentRepairChoice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let choice = match CurrentRepairChoiceWire::deserialize(deserializer)? {
            CurrentRepairChoiceWire::ReclassifyMemory {
                review_id,
                memory_id,
                after_memory_type,
            } => Self::reclassify_memory(review_id, memory_id, after_memory_type),
            CurrentRepairChoiceWire::RenamePageTitle {
                review_id,
                page_id,
                before_title,
                after_title,
            } => Self::rename_page_title(review_id, page_id, before_title, after_title),
            CurrentRepairChoiceWire::CompleteEntityExtraction {
                review_id,
                memory_id,
                entity_ids,
            } => Self::complete_entity_extraction(review_id, memory_id, entity_ids),
            CurrentRepairChoiceWire::EntityRelation { selection } => {
                Self::entity_relation(selection)
            }
        };
        choice.map_err(serde::de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for PrepareCurrentRepairRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = PrepareCurrentRepairRequestWire::deserialize(deserializer)?;
        Ok(Self {
            lint_scope: wire.lint_scope,
            choice: match wire.choice {
                CurrentRepairChoiceWire::ReclassifyMemory {
                    review_id,
                    memory_id,
                    after_memory_type,
                } => {
                    CurrentRepairChoice::reclassify_memory(review_id, memory_id, after_memory_type)
                }
                CurrentRepairChoiceWire::RenamePageTitle {
                    review_id,
                    page_id,
                    before_title,
                    after_title,
                } => CurrentRepairChoice::rename_page_title(
                    review_id,
                    page_id,
                    before_title,
                    after_title,
                ),
                CurrentRepairChoiceWire::CompleteEntityExtraction {
                    review_id,
                    memory_id,
                    entity_ids,
                } => CurrentRepairChoice::complete_entity_extraction(
                    review_id, memory_id, entity_ids,
                ),
                CurrentRepairChoiceWire::EntityRelation { selection } => {
                    CurrentRepairChoice::entity_relation(selection)
                }
            }
            .map_err(serde::de::Error::custom)?,
        })
    }
}

impl PrepareCurrentRepairRequest {
    pub fn lint_scope(&self) -> &RepairLintScope {
        &self.lint_scope
    }

    pub fn choice(&self) -> &CurrentRepairChoice {
        &self.choice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_current_choices_round_trip_as_typed_wire_contracts() {
        let choices = [
            CurrentRepairChoice::reclassify_memory(
                "review-classification".to_string(),
                "memory-1".to_string(),
                MemoryType::Decision,
            )
            .unwrap(),
            CurrentRepairChoice::rename_page_title(
                "review-title".to_string(),
                "page-1".to_string(),
                "Before".to_string(),
                "After".to_string(),
            )
            .unwrap(),
            CurrentRepairChoice::complete_entity_extraction(
                "review-entity".to_string(),
                "memory-2".to_string(),
                vec!["entity-1".to_string()],
            )
            .unwrap(),
        ];

        for choice in choices {
            let request = PrepareCurrentRepairRequest {
                lint_scope: RepairLintScope::global(),
                choice,
            };
            let encoded = serde_json::to_value(&request).unwrap();
            let decoded: PrepareCurrentRepairRequest = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded, request);
        }
    }

    #[test]
    fn prepare_current_request_rejects_unknown_and_untrimmed_wire_fields() {
        let mut unknown = serde_json::json!({
            "lint_scope": {"kind": "global"},
            "choice": {
                "kind": "rename_page_title",
                "review_id": "review",
                "page_id": "page",
                "before_title": "Before",
                "after_title": "After"
            },
            "unexpected": true
        });
        assert!(serde_json::from_value::<PrepareCurrentRepairRequest>(unknown.clone()).is_err());

        unknown.as_object_mut().unwrap().remove("unexpected");
        assert!(serde_json::from_value::<PrepareCurrentRepairRequest>(unknown.clone()).is_ok());
        unknown["choice"]["review_id"] = serde_json::json!(" review");
        assert!(serde_json::from_value::<PrepareCurrentRepairRequest>(unknown).is_err());
    }
}
