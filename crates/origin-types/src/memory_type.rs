// SPDX-License-Identifier: Apache-2.0
//! Canonical memory type registry. Single source of truth for the set of
//! `memory_type` values the daemon accepts and emits.
//!
//! Shared by:
//! - `origin-core::classify` — classifier enforces this set when parsing LLM output
//! - `origin-core::eval` — eval harness uses this set when generating synthetic data
//! - `origin-mcp::tools` — MCP tool schema descriptions reference this set so
//!   the JSON Schema advertised to clients matches what the daemon accepts
//!
//! When adding a type, update `VALID_MEMORY_TYPES` below. The drift test in
//! this module asserts that every type appears in the schema descriptions.

/// The canonical set of memory types accepted by the daemon classifier and
/// storage layer. Legacy values (e.g. `"goal"`) are folded by callers via
/// stability tier mapping but are NOT listed here.
pub const VALID_MEMORY_TYPES: &[&str] = &[
    "identity",
    "preference",
    "decision",
    "lesson",
    "gotcha",
    "fact",
];

/// JSON Schema `description` for the `memory_type` parameter on memory-write
/// tools (e.g. MCP `capture`). Lists the two-level filter values (profile /
/// knowledge) and the precise types, plus the "auto-classified if omitted"
/// hint that steers agents away from guessing.
pub const MEMORY_TYPE_CAPTURE_DESCRIPTION: &str =
    "\"profile\" (about the user) or \"knowledge\" (about the world) — or precise: \
     \"identity\", \"preference\", \"decision\", \"lesson\", \"gotcha\", \"fact\" — \
     auto-classified if omitted";

/// JSON Schema `description` for the `memory_type` parameter on memory-read
/// filter tools (e.g. MCP `recall`, `list_pending`). Same vocabulary as
/// capture, framed as a filter.
pub const MEMORY_TYPE_FILTER_DESCRIPTION: &str =
    "Filter by type. Two-level filter: \"profile\" (user-facing) or \
     \"knowledge\" (world-facing), or precise: \"identity\", \"preference\", \
     \"decision\", \"lesson\", \"gotcha\", \"fact\".";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_description_lists_every_valid_type() {
        for ty in VALID_MEMORY_TYPES {
            let needle = format!("\"{ty}\"");
            assert!(
                MEMORY_TYPE_CAPTURE_DESCRIPTION.contains(&needle),
                "MEMORY_TYPE_CAPTURE_DESCRIPTION missing \"{ty}\""
            );
        }
    }

    #[test]
    fn filter_description_lists_every_valid_type() {
        for ty in VALID_MEMORY_TYPES {
            let needle = format!("\"{ty}\"");
            assert!(
                MEMORY_TYPE_FILTER_DESCRIPTION.contains(&needle),
                "MEMORY_TYPE_FILTER_DESCRIPTION missing \"{ty}\""
            );
        }
    }

    #[test]
    fn valid_set_has_no_legacy_goal() {
        assert!(
            !VALID_MEMORY_TYPES.contains(&"goal"),
            "\"goal\" is a legacy value folded via stability tier mapping; it must not appear in VALID_MEMORY_TYPES"
        );
    }
}
