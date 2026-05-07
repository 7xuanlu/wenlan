// SPDX-License-Identifier: AGPL-3.0-only
//! System information detection for the Tauri app.
//!
//! Mirrors `origin_core::system_info::detect_system_info` so that once the
//! app drops its origin-core dependency (PR2) this module can be updated to
//! use a standalone implementation without touching the call site.

use origin_types::system_info::SystemInfo;

/// Detect system capabilities and hardware information.
pub fn detect_system_info() -> SystemInfo {
    origin_core::system_info::detect_system_info()
}
