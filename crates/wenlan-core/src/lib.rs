// SPDX-License-Identifier: Apache-2.0
//! Core business logic for the Wenlan memory system.
//!
//! This crate contains memory storage, retrieval, embeddings, LLM processing,
//! and all non-UI logic. It is being extracted from the Tauri app crate
//! incrementally; the current set of modules is the zero-dependency slice
//! moved in phase 3a.

pub use wenlan_types;

pub mod access_tracker;
pub mod activity;
pub mod briefing;
pub mod cache;
pub mod chat_import;
pub mod chunker;
pub mod citations;
pub mod classify;
pub mod config;
pub mod context_packager;
pub mod contradiction;
pub mod db;
pub mod decay;
pub mod document_enrichment;
#[cfg(test)]
mod drift_guard;
pub mod engine;
pub mod env_compat;
pub mod error;
pub mod eval;
pub mod events;
pub mod export;
pub mod extract;
pub mod faithfulness;
pub mod importer;
pub mod ingest;
pub mod kg;
pub mod kg_quality;
pub mod lint;
pub mod llm_classifier;
pub mod llm_provider;
pub mod maintenance;
pub mod memory_schema;
pub mod migrate_rename;
pub mod migrations;
pub mod narrative;
pub mod on_device_models;
pub mod onboarding;
pub mod page_projection_tracker;
pub mod pages;
pub mod post_ingest;
pub mod post_write;
pub mod privacy;
pub mod prompts;
pub mod quality_gate;
pub mod reconcile;
pub mod refinery;
pub mod reranker;
pub(crate) mod retrieval;
pub mod router;
pub mod schema;
pub mod sources;
pub mod spaces;
pub mod synthesis;
pub mod tags;
pub(crate) mod temporal_query;
pub mod topic_match;
pub mod tuning;

// Re-exports for convenience.
pub use error::WenlanError;
pub use events::{EventEmitter, NoopEmitter};

/// Crate version.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!version().is_empty());
    }
}
