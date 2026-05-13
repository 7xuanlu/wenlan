// SPDX-License-Identifier: Apache-2.0
//! Library target for integration-testing the HTTP router.
//!
//! Only the modules needed by `tests/` are re-exported here. The binary
//! entry-point (`main.rs`) continues to own the daemon lifecycle.

pub mod config_routes;
pub mod error;
pub mod import_routes;
pub mod ingest_batcher;
pub mod ingest_routes;
pub mod knowledge_routes;
pub mod memory_routes;
pub mod onboarding_routes;
pub mod refinery_routes;
pub mod router;
pub mod routes;
pub mod scheduler;
pub mod source_routes;
pub mod state;
pub mod websocket;

// cmd_* modules contain main-only CLI logic; not re-exported.
