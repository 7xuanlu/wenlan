// SPDX-License-Identifier: AGPL-3.0-only
//! Generic concept export framework.

pub mod knowledge;
pub mod obsidian;

use crate::error::OriginError;
use crate::pages::Page;

// ExportStats moved to origin-types in Phase 5-D PR2 so the Tauri app can
// deserialize it without pulling in the full origin-core dep.
pub use origin_types::ExportStats;

#[derive(Debug)]
pub struct ExportResult {
    pub concept_id: String,
    pub path: String,
}

/// Trait for exporting pages to external formats/systems.
pub trait PageExporter {
    fn export(&self, page: &Page) -> Result<ExportResult, OriginError>;
    fn export_all(&self, pages: &[Page]) -> Result<ExportStats, OriginError>;
}
