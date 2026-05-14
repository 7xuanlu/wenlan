// SPDX-License-Identifier: Apache-2.0
//! Knowledge writer — exports pages as `.md` files with state tracking.

use crate::error::OriginError;
use crate::export::obsidian::{convert_links_to_wikilinks, slugify};
use crate::pages::Page;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

const KNOWLEDGE_STATE_SCHEMA_V2: u32 = 2;

#[derive(Debug, Default, Serialize, Deserialize)]
struct KnowledgeState {
    #[serde(default = "default_schema_v2")]
    schema_version: u32,
    pages: HashMap<String, PageFileState>,
}

fn default_schema_v2() -> u32 {
    KNOWLEDGE_STATE_SCHEMA_V2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PageFileState {
    file: String,
    version: i64,
    last_written: String,
}

/// Legacy v1 KnowledgeState. The Phase 0 (Page) taxonomy refactor renamed
/// `concepts` → `pages` and changed the page-id prefix `concept_<uuid>` →
/// `page_<uuid>`. v1 state.json files are auto-migrated on read. Drop in
/// next minor release.
#[derive(Debug, Default, Deserialize)]
struct LegacyKnowledgeStateV1 {
    concepts: HashMap<String, PageFileState>,
}

pub struct KnowledgeWriter {
    path: PathBuf,
}

impl KnowledgeWriter {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn write_page(&self, page: &Page) -> Result<String, OriginError> {
        let origin_dir = self.path.join(".origin");
        std::fs::create_dir_all(&origin_dir)?;

        let mut state = self.load_state();
        let filename = self.unique_filename(&page.id, &page.title, &state);
        let file_path = self.path.join(&filename);

        let content = render_markdown(page);
        std::fs::write(&file_path, &content)?;

        state.pages.insert(
            page.id.clone(),
            PageFileState {
                file: filename,
                version: page.version,
                last_written: page.last_modified.clone(),
            },
        );
        self.save_state(&state)?;

        Ok(file_path.to_string_lossy().to_string())
    }

    /// Resolve a slug-derived filename that does not collide with any other
    /// page's filename in `state`, AND does not collide with an existing
    /// file on disk that we have no state entry for (orphan from a manual
    /// drop, a failed previous rollback, etc.). The caller's own page id is
    /// allowed to keep its existing filename so version updates stay in
    /// place.
    fn unique_filename(&self, page_id: &str, title: &str, state: &KnowledgeState) -> String {
        // If this page already has a filename recorded, reuse it.
        if let Some(existing) = state.pages.get(page_id) {
            return existing.file.clone();
        }
        let base = slugify(title);
        let mut candidate = format!("{}.md", base);
        let mut n = 2;
        // Collect filenames belonging to *other* pages.
        let taken: std::collections::HashSet<&str> = state
            .pages
            .iter()
            .filter(|(id, _)| id.as_str() != page_id)
            .map(|(_, st)| st.file.as_str())
            .collect();
        loop {
            let collides_state = taken.contains(candidate.as_str());
            // Defence-in-depth: also check disk in case state.json missed
            // an orphan file (e.g. user dropped an .md in manually, or a
            // previous rollback couldn't persist state).
            let collides_disk = self.path.join(&candidate).exists();
            if !collides_state && !collides_disk {
                break;
            }
            candidate = format!("{}-{}.md", base, n);
            n += 1;
        }
        candidate
    }

    /// Resolve the filename currently recorded in `state.json` for a page,
    /// or `None` if the page has no projection yet. Used by the PUT route's
    /// rollback path so a failed DB update can restore the prior md bytes
    /// instead of orphaning the file.
    pub fn page_filename(&self, page_id: &str) -> Option<String> {
        self.load_state().pages.get(page_id).map(|s| s.file.clone())
    }

    pub fn remove_page(&self, page_id: &str) -> Result<(), OriginError> {
        let mut state = self.load_state();

        if let Some(entry) = state.pages.remove(page_id) {
            let file_path = self.path.join(&entry.file);
            // Delete the file *before* persisting state. If the file remove
            // fails we keep the state entry so the daemon can retry; if the
            // state save fails we have at most a stale empty entry pointing
            // at a missing file (detectable, recoverable) instead of an
            // orphan file with no DB or state reference.
            if file_path.exists() {
                std::fs::remove_file(&file_path)?;
            }
            self.save_state(&state)?;
        }

        Ok(())
    }

    fn load_state(&self) -> KnowledgeState {
        let state_path = self.path.join(".origin/state.json");
        let data = match std::fs::read_to_string(&state_path) {
            Ok(d) => d,
            Err(_) => return KnowledgeState::default(),
        };

        // v1 detection: has "concepts" key, no "pages" key. Migrate inline.
        // Heuristic on raw bytes is good enough — the legacy file has at most
        // a thousand small entries and we only enter this branch once per boot.
        if data.contains("\"concepts\"") && !data.contains("\"pages\"") {
            let v1: LegacyKnowledgeStateV1 = serde_json::from_str(&data).unwrap_or_default();
            log::info!(
                "[knowledge] migrating state.json v1 -> v2 ({} entries; rewriting concept_ -> page_ id prefix)",
                v1.concepts.len()
            );
            let pages: HashMap<String, PageFileState> = v1
                .concepts
                .into_iter()
                .map(|(id, st)| {
                    let new_id = if let Some(rest) = id.strip_prefix("concept_") {
                        format!("page_{rest}")
                    } else {
                        id
                    };
                    (new_id, st)
                })
                .collect();
            return KnowledgeState {
                schema_version: KNOWLEDGE_STATE_SCHEMA_V2,
                pages,
            };
        }

        serde_json::from_str(&data).unwrap_or_default()
    }

    fn save_state(&self, state: &KnowledgeState) -> Result<(), OriginError> {
        let state_path = self.path.join(".origin/state.json");
        let data = serde_json::to_string_pretty(state)?;
        std::fs::write(&state_path, data)?;
        Ok(())
    }
}

fn render_markdown(page: &Page) -> String {
    let mut out = String::new();

    // Frontmatter
    out.push_str("---\n");
    out.push_str(&format!("title: \"{}\"\n", page.title));
    if let Some(ref space) = page.space {
        out.push_str(&format!("space: {}\n", space));
    }
    out.push_str(&format!("origin_id: {}\n", page.id));
    out.push_str(&format!("origin_version: {}\n", page.version));
    let created_date: String = page.created_at.chars().take(10).collect();
    let modified_date: String = page.last_modified.chars().take(10).collect();
    out.push_str(&format!("created: {}\n", created_date));
    out.push_str(&format!("modified: {}\n", modified_date));
    out.push_str("---\n\n");

    // Body with wikilinks
    out.push_str(&convert_links_to_wikilinks(&page.content));
    out.push('\n');

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::Page;

    fn test_concept() -> Page {
        Page {
            id: "concept_test123".to_string(),
            title: "Rust Ownership".to_string(),
            summary: Some("Memory safety without GC".to_string()),
            content: "## Overview\nRust uses ownership for memory safety.\n\n## Related\n- [Borrowing](concept_borrow1)".to_string(),
            entity_id: None,
            space: Some("rust".to_string()),
            source_memory_ids: vec!["m1".to_string()],
            version: 2,
            status: "active".to_string(),
            created_at: "2026-04-01T00:00:00+00:00".to_string(),
            last_compiled: "2026-04-09T00:00:00+00:00".to_string(),
            last_modified: "2026-04-09T00:00:00+00:00".to_string(),
            sources_updated_count: 0,
            stale_reason: None,
            user_edited: false,
            relevance_score: 0.0,
            last_edited_by: None,
            last_edited_at: None,
            last_delta_summary: None,
            changelog: None,
        }
    }

    #[test]
    fn test_write_page_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new(dir.path().to_path_buf());
        let page = test_concept();

        let path = writer.write_page(&page).unwrap();
        assert!(path.ends_with("rust-ownership.md"));

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("title: \"Rust Ownership\""));
        assert!(content.contains("space: rust"));
        assert!(content.contains("origin_id: concept_test123"));
        assert!(content.contains("origin_version: 2"));
        // Wikilinks converted
        assert!(content.contains("[[Borrowing]]"));
        assert!(!content.contains("(concept_borrow1)"));
    }

    #[test]
    fn test_write_updates_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new(dir.path().to_path_buf());

        writer.write_page(&test_concept()).unwrap();

        let state = writer.load_state();
        assert!(state.pages.contains_key("concept_test123"));
        assert_eq!(state.pages["concept_test123"].file, "rust-ownership.md");
        assert_eq!(state.pages["concept_test123"].version, 2);
    }

    #[test]
    fn test_remove_page_deletes_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new(dir.path().to_path_buf());

        let path = writer.write_page(&test_concept()).unwrap();
        assert!(std::path::Path::new(&path).exists());

        writer.remove_page("concept_test123").unwrap();
        assert!(!std::path::Path::new(&path).exists());

        let state = writer.load_state();
        assert!(!state.pages.contains_key("concept_test123"));
    }

    #[test]
    fn test_remove_nonexistent_page_noop() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new(dir.path().to_path_buf());
        writer.remove_page("nonexistent").unwrap();
    }

    #[test]
    fn test_write_multiple_pages() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new(dir.path().to_path_buf());

        let c1 = Page {
            id: "concept_a".to_string(),
            title: "Alpha".to_string(),
            ..test_concept()
        };
        let c2 = Page {
            id: "concept_b".to_string(),
            title: "Beta".to_string(),
            ..test_concept()
        };

        writer.write_page(&c1).unwrap();
        writer.write_page(&c2).unwrap();

        assert!(dir.path().join("alpha.md").exists());
        assert!(dir.path().join("beta.md").exists());

        let state = writer.load_state();
        assert_eq!(state.pages.len(), 2);
    }

    #[test]
    fn test_knowledge_writer_overwrite_on_version_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new(dir.path().to_path_buf());

        let mut page = test_concept();
        writer.write_page(&page).unwrap();

        let v1 = std::fs::read_to_string(dir.path().join("rust-ownership.md")).unwrap();
        assert!(v1.contains("origin_version: 2"));

        // Update version and content
        page.version = 3;
        page.content = "## Updated\nNew content.".to_string();
        writer.write_page(&page).unwrap();

        let v2 = std::fs::read_to_string(dir.path().join("rust-ownership.md")).unwrap();
        assert!(v2.contains("origin_version: 3"));
        assert!(v2.contains("## Updated"));
        assert!(!v2.contains("memory safety")); // old content replaced

        // State reflects new version
        let state = writer.load_state();
        assert_eq!(state.pages["concept_test123"].version, 3);
    }

    #[test]
    fn test_load_state_migrates_v1_concept_keys_to_page() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new(dir.path().to_path_buf());

        // Write a legacy v1 state.json by hand.
        let v1_json = r#"{
            "concepts": {
                "concept_aaa": { "file": "a.md", "version": 1, "last_written": "2026-04-01T00:00:00+00:00" },
                "concept_bbb": { "file": "b.md", "version": 2, "last_written": "2026-04-02T00:00:00+00:00" }
            }
        }"#;
        std::fs::create_dir_all(dir.path().join(".origin")).unwrap();
        std::fs::write(dir.path().join(".origin/state.json"), v1_json).unwrap();

        let state = writer.load_state();
        // v1 keys are auto-rewritten to page_ prefix.
        assert!(state.pages.contains_key("page_aaa"));
        assert!(state.pages.contains_key("page_bbb"));
        assert!(!state.pages.contains_key("concept_aaa"));
        assert_eq!(state.pages["page_aaa"].file, "a.md");
        assert_eq!(state.pages["page_bbb"].version, 2);
        assert_eq!(state.schema_version, KNOWLEDGE_STATE_SCHEMA_V2);

        // After save_state, the file is rewritten in v2 form (no "concepts" key).
        writer.save_state(&state).unwrap();
        let written = std::fs::read_to_string(dir.path().join(".origin/state.json")).unwrap();
        assert!(written.contains("\"pages\""));
        assert!(!written.contains("\"concepts\""));
        assert!(written.contains("\"schema_version\""));
    }

    #[test]
    fn test_knowledge_writer_no_domain() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new(dir.path().to_path_buf());

        let page = Page {
            space: None,
            ..test_concept()
        };
        let path = writer.write_page(&page).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("space:"));
    }
}
