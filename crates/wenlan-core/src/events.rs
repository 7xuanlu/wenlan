// SPDX-License-Identifier: Apache-2.0
//! Event emission trait — re-exported from wenlan-types (Phase 5 PR2).
//!
//! All code that previously imported from `wenlan_core::events` continues
//! to work unchanged via this re-export.

pub use wenlan_types::events::{EventEmitter, NoopEmitter};
