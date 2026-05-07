// SPDX-License-Identifier: Apache-2.0
//! Onboarding milestone wire types. Shared with origin-mcp + origin-app.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Canonical identifier for each onboarding milestone. The string form
/// (produced by `as_str`, `FromStr`, and serde's kebab-case rename) is
/// the single source of truth for (a) the DB primary key, (b) the JSON
/// wire format, (c) `MilestoneRecord.id`. All four forms must stay in sync --
/// the round-trip test in this module enforces that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MilestoneId {
    IntelligenceReady,
    FirstMemory,
    FirstRecall,
    /// Wire format preserved as "first-concept" until 0c.2/0c.3 DB/API rename pass.
    #[serde(rename = "first-concept")]
    FirstPage,
    GraphAlive,
    SecondAgent,
}

impl MilestoneId {
    pub fn as_str(&self) -> &'static str {
        match self {
            MilestoneId::IntelligenceReady => "intelligence-ready",
            MilestoneId::FirstMemory => "first-memory",
            MilestoneId::FirstRecall => "first-recall",
            MilestoneId::FirstPage => "first-concept",
            MilestoneId::GraphAlive => "graph-alive",
            MilestoneId::SecondAgent => "second-agent",
        }
    }
}

impl FromStr for MilestoneId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "intelligence-ready" => Ok(MilestoneId::IntelligenceReady),
            "first-memory" => Ok(MilestoneId::FirstMemory),
            "first-recall" => Ok(MilestoneId::FirstRecall),
            "first-concept" => Ok(MilestoneId::FirstPage),
            "graph-alive" => Ok(MilestoneId::GraphAlive),
            "second-agent" => Ok(MilestoneId::SecondAgent),
            _ => Err(format!("unknown milestone id: {}", s)),
        }
    }
}

/// A recorded milestone, returned by DB queries and API endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneRecord {
    pub id: MilestoneId,
    pub first_triggered_at: i64,
    pub acknowledged_at: Option<i64>,
    /// Optional JSON payload (e.g. concept_id, agent_name).
    pub payload: Option<serde_json::Value>,
}
