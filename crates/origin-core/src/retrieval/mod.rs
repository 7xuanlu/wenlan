// SPDX-License-Identifier: Apache-2.0
//! Retrieval module: pool builders, hard filters, and per-signal helpers
//! used by `db::search_memory*` paths.
//!
//! Moved out of `composite/` in PR-A so the legacy retrieval path can use
//! these helpers without importing the composite scoring orchestrator.

pub(crate) mod hard_filters;
pub(crate) mod signals;
