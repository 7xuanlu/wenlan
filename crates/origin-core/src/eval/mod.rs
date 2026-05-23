// SPDX-License-Identifier: Apache-2.0
//! Memory eval system — quality measurement and feedback capture.

pub mod anthropic;
pub mod cli_batch;
pub mod judge;
pub mod shared;

pub mod answer_quality;
pub mod context_path;
pub mod fixtures;
pub mod gen;
pub mod kg_faithfulness;
pub mod latency;
pub mod lifecycle;
pub mod locomo;
pub mod longmemeval;
pub mod metrics;
pub mod pipeline;
pub mod report;
pub mod retrieval;
pub mod runner;
pub mod signals;
/// Backward-compat alias: old code using `eval::token_efficiency::*` still works.
pub use retrieval as token_efficiency;
