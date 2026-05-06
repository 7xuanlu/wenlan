// SPDX-License-Identifier: Apache-2.0
//! Event emission trait — re-exported from origin-types (Phase 5 PR2).
//!
//! All code that previously imported from `origin_core::events` continues
//! to work unchanged via this re-export.

pub use origin_types::events::{EventEmitter, NoopEmitter};
