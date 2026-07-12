// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintCheckGroup {
    Identity,
    KnowledgeGraph,
    Memories,
    Operations,
    Pages,
    Runtime,
    Serving,
}

impl LintCheckGroup {
    pub const ALL: [Self; 7] = [
        Self::Identity,
        Self::KnowledgeGraph,
        Self::Memories,
        Self::Operations,
        Self::Pages,
        Self::Runtime,
        Self::Serving,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::KnowledgeGraph => "knowledge_graph",
            Self::Memories => "memories",
            Self::Operations => "operations",
            Self::Pages => "pages",
            Self::Runtime => "runtime",
            Self::Serving => "serving",
        }
    }

    pub fn for_check_id(check_id: &str) -> Option<Self> {
        let owner = check_id
            .split_once('.')
            .map_or(check_id, |(owner, _)| owner);
        match owner {
            "identity" => Some(Self::Identity),
            "entities" | "kg" | "memory_entities" | "observations" | "relations" => {
                Some(Self::KnowledgeGraph)
            }
            "memories" => Some(Self::Memories),
            "operations" => Some(Self::Operations),
            "pages" => Some(Self::Pages),
            "runtime" => Some(Self::Runtime),
            "serving" => Some(Self::Serving),
            _ => None,
        }
    }
}
