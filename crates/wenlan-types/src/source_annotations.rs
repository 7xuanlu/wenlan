// SPDX-License-Identifier: Apache-2.0
//! User-authored annotations attached to a saved source.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateSourceAnnotationRequest {
    pub source: String,
    pub source_id: String,
    #[serde(default)]
    pub space: Option<String>,
    pub expected_version: u64,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub page_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceAnnotationPage {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceAnnotation {
    pub source: String,
    pub source_id: String,
    pub notes: String,
    pub page_ids: Vec<String>,
    pub version: u64,
    pub updated_at: i64,
    pub pages: Vec<SourceAnnotationPage>,
}
