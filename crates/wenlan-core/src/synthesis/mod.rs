// SPDX-License-Identifier: Apache-2.0
//! Synthesis phases — derived views over raw memories.
//!
//! These phases generate compact summaries from clusters of memories: recaps
//! (chronological digests) and decision_logs (decision tracebacks). They run
//! periodically rather than on each ingest.

pub mod decision_logs;
pub mod detect;
pub mod distill;
pub mod emergence;
pub mod overview;
pub mod recaps;
pub mod refinement_queue;
pub mod wikilinks;
