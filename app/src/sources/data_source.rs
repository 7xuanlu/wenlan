// SPDX-License-Identifier: AGPL-3.0-only
//! DataSource trait — app-local definition.
//! Moved from origin-core::sources; the trait and its impls are app-only
//! (origin-server never references DataSource directly).
use crate::error::AppError;
use async_trait::async_trait;
use origin_types::sources::{RawDocument, SourceStatus};
use std::any::Any;

/// Trait that all data source connectors must implement.
#[async_trait]
pub trait DataSource: Send + Sync {
    /// Unique name for this source ("gmail", "notion", etc.)
    fn name(&self) -> &str;

    /// Whether this source requires OAuth authentication
    fn requires_auth(&self) -> bool;

    /// Check if the source is currently connected/authenticated
    async fn is_connected(&self) -> bool;

    /// Connect/authenticate the source (triggers OAuth if needed)
    async fn connect(&mut self) -> Result<(), AppError>;

    /// Disconnect the source (revoke tokens, cleanup)
    async fn disconnect(&mut self) -> Result<(), AppError>;

    /// Fetch new/updated content since last sync
    async fn fetch_updates(&mut self) -> Result<Vec<RawDocument>, AppError>;

    /// Initial full sync - fetches all available content
    async fn full_sync(&mut self) -> Result<Vec<RawDocument>, AppError>;

    /// Get the current status of this source
    async fn status(&self) -> SourceStatus;

    /// Downcast to concrete type
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
