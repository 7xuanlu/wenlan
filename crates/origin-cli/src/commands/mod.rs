// SPDX-License-Identifier: Apache-2.0
//! Subcommand implementations for the origin CLI.

pub mod agents;
pub mod list;
pub mod mcp;
pub mod recall;
pub mod search;
pub mod service;
pub mod setup;
pub mod status;
pub mod store;

#[cfg(not(target_os = "windows"))]
pub use service::service_unit_path;
pub use service::{install, is_installed, uninstall, SERVICE_LABEL};
