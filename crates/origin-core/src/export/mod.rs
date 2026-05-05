// SPDX-License-Identifier: AGPL-3.0-only
//! Generic concept export framework.

pub mod knowledge;
pub mod obsidian;

use crate::error::OriginError;
use crate::pages::Page;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ExportResult {
    pub concept_id: String,
    pub path: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ExportStats {
    pub exported: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// Trait for exporting pages to external formats/systems.
pub trait PageExporter {
    fn export(&self, page: &Page) -> Result<ExportResult, OriginError>;
    fn export_all(&self, pages: &[Page]) -> Result<ExportStats, OriginError>;
}
