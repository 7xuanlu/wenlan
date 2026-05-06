// SPDX-License-Identifier: Apache-2.0
//! One-shot migration phases run by the refinery scheduler.
//!
//! Migrations differ from synthesis: they bridge schema or model changes
//! (e.g. swapping the embedding model means re-embedding existing chunks).
//! Each migration is idempotent — running it on already-migrated data is a
//! no-op.

pub mod reembed;
