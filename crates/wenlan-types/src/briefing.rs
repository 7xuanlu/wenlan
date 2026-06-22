// SPDX-License-Identifier: Apache-2.0
//! Wire types for daily briefing and contradiction responses.

use serde::{Deserialize, Serialize};

/// Response type for the daily briefing endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BriefingResponse {
    pub content: String,
    pub new_today: u64,
    pub primary_agent: Option<String>,
    pub generated_at: i64,
    pub is_stale: bool,
}

/// A pending contradiction surfaced by the refinement queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictionItem {
    pub id: String,
    pub new_content: String,
    pub existing_content: String,
    pub new_source_id: String,
    pub existing_source_id: String,
}
