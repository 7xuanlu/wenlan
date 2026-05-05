// SPDX-License-Identifier: AGPL-3.0-only
//! Knowledge compilation: pages are synthesized wiki entries distilled from memory clusters.

use serde::{Deserialize, Serialize};

/// A compiled knowledge page — structured, cross-referenced, backed by source memories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub content: String,
    pub entity_id: Option<String>,
    pub domain: Option<String>,
    /// Kept for dual-write transition; prefer concept_sources join table for new reads.
    pub source_memory_ids: Vec<String>,
    pub version: i64,
    pub status: String,
    pub created_at: String,
    pub last_compiled: String,
    pub last_modified: String,
    /// How many source memories were updated since last distillation.
    pub sources_updated_count: i64,
    /// Why this page is stale: "source_updated" | "source_conflict" | None.
    pub stale_reason: Option<String>,
    /// True if a human has edited this page's content directly.
    pub user_edited: bool,
    /// Relevance score from search (0.0-1.0). Only populated by `search_pages`;
    /// zero for persisted/non-search contexts.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub relevance_score: f32,
}

fn is_zero_f32(v: &f32) -> bool {
    *v == 0.0
}

impl Page {
    pub fn new_id() -> String {
        format!("concept_{}", uuid::Uuid::new_v4())
    }
}

/// Filter pages by source overlap with search results.
///
/// A page is contextually relevant if the memories it was compiled from
/// overlap with the memories that search_memory returned for this query.
/// This is the strongest relevance signal: it answers "is this page about
/// the thing I'm searching for?" rather than relying on embedding similarity
/// (which we proved doesn't discriminate between good and garbage pages).
///
/// `min_overlap`: minimum number of search result source_ids that must appear
/// in the page's `source_memory_ids`. Recommended: 2 (filters noise while
/// keeping pages with genuine topical overlap).
pub fn filter_pages_by_source_overlap(
    pages: &[Page],
    search_result_source_ids: &std::collections::HashSet<String>,
    min_overlap: usize,
) -> Vec<Page> {
    pages
        .iter()
        .filter(|c| {
            let overlap = c
                .source_memory_ids
                .iter()
                .filter(|sid| search_result_source_ids.contains(sid.as_str()))
                .count();
            overlap >= min_overlap
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn make_page(id: &str, source_ids: &[&str]) -> Page {
        Page {
            id: id.to_string(),
            title: id.to_string(),
            summary: None,
            content: String::new(),
            entity_id: None,
            domain: None,
            source_memory_ids: source_ids.iter().map(|s| s.to_string()).collect(),
            version: 1,
            status: "active".to_string(),
            created_at: String::new(),
            last_compiled: String::new(),
            last_modified: String::new(),
            sources_updated_count: 0,
            stale_reason: None,
            user_edited: false,
            relevance_score: 0.5,
        }
    }

    #[test]
    fn test_overlap_keeps_matching_concept() {
        let pages = vec![make_page("c1", &["m1", "m2", "m3"])];
        let search_ids: HashSet<String> = ["m1", "m2"].iter().map(|s| s.to_string()).collect();
        let kept = filter_pages_by_source_overlap(&pages, &search_ids, 2);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn test_overlap_filters_low_overlap() {
        let pages = vec![make_page("c1", &["m1", "m2", "m3"])];
        let search_ids: HashSet<String> = ["m1", "m99"].iter().map(|s| s.to_string()).collect();
        let kept = filter_pages_by_source_overlap(&pages, &search_ids, 2);
        assert_eq!(kept.len(), 0); // only 1 overlap, need 2
    }

    #[test]
    fn test_overlap_empty_concept_sources() {
        let pages = vec![make_page("c1", &[])];
        let search_ids: HashSet<String> = ["m1"].iter().map(|s| s.to_string()).collect();
        let kept = filter_pages_by_source_overlap(&pages, &search_ids, 1);
        assert_eq!(kept.len(), 0);
    }

    #[test]
    fn test_overlap_empty_search_results() {
        let pages = vec![make_page("c1", &["m1", "m2"])];
        let search_ids: HashSet<String> = HashSet::new();
        let kept = filter_pages_by_source_overlap(&pages, &search_ids, 1);
        assert_eq!(kept.len(), 0);
    }

    #[test]
    fn test_overlap_zero_threshold_keeps_all() {
        let pages = vec![make_page("c1", &["m1"]), make_page("c2", &["m99"])];
        let search_ids: HashSet<String> = ["m1"].iter().map(|s| s.to_string()).collect();
        let kept = filter_pages_by_source_overlap(&pages, &search_ids, 0);
        assert_eq!(kept.len(), 2); // min_overlap=0 keeps everything
    }

    #[test]
    fn test_overlap_mixed_keeps_and_filters() {
        let pages = vec![
            make_page("good", &["m1", "m2", "m3", "m4", "m5"]),
            make_page("noise", &["m90", "m91", "m92"]),
            make_page("edge", &["m1", "m90"]),
        ];
        let search_ids: HashSet<String> =
            ["m1", "m2", "m3"].iter().map(|s| s.to_string()).collect();
        let kept = filter_pages_by_source_overlap(&pages, &search_ids, 2);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "good"); // 3 overlap
                                        // "noise" has 0 overlap, "edge" has 1 overlap — both filtered at min_overlap=2
    }
}
