// SPDX-License-Identifier: AGPL-3.0-only
//! App-level sources -- re-exports origin-types wire types + origin-core trait/impls.
pub mod sync;

// Wire types (Source, SourceStatus, RawDocument, MemoryType, etc.) from the
// shared types crate -- no heavy deps, safe for downstream consumers.
pub use origin_types::sources::*;

// Trait and connector impls that depend on origin-core internals.
pub use origin_core::sources::{
    base_confidence, compute_effective_confidence, decay_rate, local_files, obsidian, trust_weight,
    DataSource,
};
