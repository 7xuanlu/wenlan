// SPDX-License-Identifier: Apache-2.0
//! Library surface for the Origin CLI.
//!
//! Re-exposes the internal modules so integration tests can call into them.

pub mod client;
pub mod commands;
pub mod output;
