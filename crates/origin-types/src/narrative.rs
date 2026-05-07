// SPDX-License-Identifier: Apache-2.0
//! Wire types for profile narrative responses.

use serde::{Deserialize, Serialize};

/// Response type for the profile narrative endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrativeResponse {
    pub content: String,
    pub generated_at: i64,
    pub is_stale: bool,
    pub memory_count: u64,
}
