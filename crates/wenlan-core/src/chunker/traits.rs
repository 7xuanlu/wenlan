// SPDX-License-Identifier: Apache-2.0
use std::collections::HashMap;

/// Context passed to chunking strategies
#[allow(dead_code)]
pub struct ChunkContext<'a> {
    pub content: &'a str,
    pub title: &'a str,
    pub file_path: &'a str,
    pub metadata: &'a HashMap<String, String>,
}

/// Information about a single chunk produced by a strategy
#[derive(Debug, Clone)]
pub struct ChunkInfo {
    pub content: String,
    pub chunk_type: String,       // "markdown", "code", "text"
    pub language: Option<String>, // "rust", "python", etc.
    pub byte_range: Option<(usize, usize)>,
    pub semantic_unit: Option<String>, // "section_h1", "function", "paragraph"
}

/// Trait for implementing different chunking strategies
pub trait ChunkingStrategy: Send + Sync {
    /// Chunks the content according to this strategy
    fn chunk(&self, context: ChunkContext) -> Vec<ChunkInfo>;
}
