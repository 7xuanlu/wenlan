// SPDX-License-Identifier: Apache-2.0
//! Knowledge writer — exports pages as `.md` files with state tracking.

use crate::error::WenlanError;
use crate::export::obsidian::{convert_links_to_wikilinks, slugify};
use crate::pages::Page;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const KNOWLEDGE_STATE_SCHEMA_V2: u32 = 2;

/// Process-local monotonic counter, combined with the pid, so concurrent
/// `write_page` calls for the same page never pick the same temp filename.
static TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize)]
struct KnowledgeState {
    #[serde(default = "default_schema_v2")]
    schema_version: u32,
    pages: HashMap<String, PageFileState>,
}

impl Default for KnowledgeState {
    fn default() -> Self {
        Self {
            schema_version: KNOWLEDGE_STATE_SCHEMA_V2,
            pages: HashMap::new(),
        }
    }
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

/// What one `reconcile` pass repaired. Logged as a single summary line at
/// startup — per-page detail would be thousands of lines on a real corpus.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ReconcileStats {
    /// Pages with a `state.json` entry, i.e. ones we could check at all.
    pub checked: usize,
    /// Projections found behind the DB and rewritten.
    pub rewritten: usize,
    /// `write_page` leftovers swept from the pages directory.
    pub temp_files_removed: usize,
    /// Repairs that failed; the page keeps its stale file until next boot.
    pub errors: usize,
}

/// The `origin_version` stamp `render_markdown` writes into every projected
/// file, or 0 when the file has no parseable frontmatter (empty, truncated, or
/// never ours). Shared by the page-watcher and the startup reconcile: they
/// make opposite decisions off this number, so reading it two different ways
/// would let reconcile overwrite exactly what the watcher protects.
///
/// Tolerant of `3`, `3.0` and `"3"` — YAML gives each a different serde shape,
/// and a hand-retyped frontmatter should not be mistaken for a stale one.
pub(crate) fn projected_origin_version(fm: &crate::sources::obsidian::NoteFrontmatter) -> i64 {
    fm.fields
        .get("origin_version")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_f64().map(|f| f as i64))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        })
        .unwrap_or(0)
}

/// True only for the `.{page_id}.{pid}.{seq}.tmp` names `write_page` creates.
/// Page ids carry no dots, so "exactly three components, the last two all
/// digits" is our writer's shape and nobody else's — an editor swap file or a
/// user's own `.tmp` scratch in the pages directory does not match.
fn is_write_page_temp_file(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".tmp")) else {
        return false;
    };
    let mut parts = rest.rsplitn(3, '.');
    let seq = parts.next().unwrap_or_default();
    let pid = parts.next().unwrap_or_default();
    let page_id = parts.next().unwrap_or_default();
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    !page_id.is_empty() && !page_id.contains('.') && all_digits(pid) && all_digits(seq)
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
    tracker: std::sync::Arc<crate::page_projection_tracker::PageProjectionTracker>,
}

impl KnowledgeWriter {
    pub fn new(path: PathBuf, database: &crate::db::MemoryDB) -> Self {
        Self {
            path,
            tracker: database.page_projection_tracker(),
        }
    }

    pub fn begin_projection_write(&self) -> KnowledgeProjectionWriteRef<'_> {
        KnowledgeProjectionWriteRef {
            writer: self,
            guard: self.tracker.begin_write(),
        }
    }

    #[cfg(test)]
    fn new_for_test(path: PathBuf) -> Self {
        Self {
            path,
            tracker: crate::page_projection_tracker::PageProjectionTracker::new(),
        }
    }

    #[cfg(test)]
    fn begin_test_write(&self) -> crate::page_projection_tracker::PageProjectionWriteGuard {
        self.tracker.begin_write()
    }

    #[cfg(test)]
    fn write_page_for_test(&self, page: &Page) -> Result<String, WenlanError> {
        let guard = self.begin_test_write();
        self.write_page(&guard, page)
    }

    #[cfg(test)]
    fn remove_page_for_test(&self, page_id: &str) -> Result<(), WenlanError> {
        let guard = self.begin_test_write();
        self.remove_page(&guard, page_id)
    }

    #[cfg(test)]
    fn reconcile_for_test(&self, pages: &[Page]) -> Result<ReconcileStats, WenlanError> {
        let guard = self.begin_test_write();
        self.reconcile(&guard, pages)
    }

    pub fn write_page(
        &self,
        guard: &crate::page_projection_tracker::PageProjectionWriteGuard,
        page: &Page,
    ) -> Result<String, WenlanError> {
        self.validate_guard(guard)?;
        let wenlan_dir = self.path.join(".wenlan");
        std::fs::create_dir_all(&wenlan_dir)?;

        let mut state = self.load_state();
        let filename = self.unique_filename(&page.id, &page.title, &state);
        let file_path = self.path.join(&filename);

        let content = render_markdown(page);
        // Write to a temp file in the same directory, then rename over the
        // target: rename within one directory is atomic, so a reader (Obsidian,
        // the fs watcher) never observes a half-written page file.
        //
        // ponytail: no fsync before the rename, so this buys atomicity for
        // readers, NOT crash durability — after a power loss the target may hold
        // either version, or a rename may be durable while its bytes are not.
        // That is the intended trade: an fsync per page would be paid on every
        // write of a bulk distill run, and markdown is a repairable projection
        // of the DB. `KnowledgeWriter::reconcile` is what makes that repairable
        // real — the daemon runs it at startup, and each of those outcomes
        // leaves the file's `origin_version` stamp behind the DB's, which is
        // exactly what it repairs. Add fsync (file, then the directory) only if
        // the projection ever becomes authoritative.
        let temp_filename = format!(
            ".{}.{}.{}.tmp",
            page.id,
            std::process::id(),
            TEMP_FILE_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let temp_path = self.path.join(&temp_filename);
        if let Err(e) = std::fs::write(&temp_path, &content) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e.into());
        }
        if let Err(e) = std::fs::rename(&temp_path, &file_path) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(e.into());
        }

        // Project read-only source stubs so [[mem_*]] resolves in Obsidian.
        if let Err(e) = crate::export::provenance::project_stubs_for_page(
            &self.path,
            &page.id,
            &page.source_memory_ids,
        ) {
            log::warn!("[knowledge] stub projection failed for {}: {e}", page.id);
        }

        // Manifest is a cosmetic projection index, updated load-modify-save
        // without a lock. HTTP page-write handlers can run concurrently with
        // scheduler distillation, so a lost update is possible — but the blast
        // radius is one stub and it self-heals: a spuriously-GC'd stub for a
        // live page is regenerated on that page's next write_page (project runs
        // before this update), and a leaked orphan is reaped on the next GC. A
        // real lock is a P2 concern, unjustified for a regenerable nav aid.
        let mut manifest = crate::export::provenance::StubManifest::load(&self.path);
        manifest.record(&page.id, &page.source_memory_ids);
        let _ = manifest.save(&self.path);
        let _ = crate::export::provenance::gc_orphan_stubs(&self.path, &manifest);

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

    /// Repair the markdown projection from the DB, which is authoritative.
    ///
    /// `write_page` deliberately skips fsync, so a crash can leave a page's
    /// file missing, holding the previous version's bytes (rename undone), or
    /// zero-length (rename durable, bytes not). This brings those forward
    /// through the same atomic `write_page` path, and sweeps `.tmp` leftovers
    /// from writes that died between write and rename.
    ///
    /// Staleness is judged by the `origin_version` stamp, NOT by comparing
    /// bytes against `render_markdown`. md is canonical for prose (see
    /// `sources::page_watcher`): a body that differs while the stamp is
    /// current is a user edit made while the daemon was down, and the watcher
    /// reflects it back into the DB on its next tick. Rewriting on
    /// content-inequality would delete that edit before the watcher ever saw
    /// it. A stale stamp, by contrast, can only mean the daemon wrote last and
    /// the file did not keep up.
    ///
    /// ponytail: two known ceilings, both cheap to live with. (1) A file
    /// corrupted in the BODY while its frontmatter still reads current is
    /// indistinguishable from a user edit without a per-page checksum, so it
    /// is left alone — add a content hash to `PageFileState` if that ever
    /// bites. (2) Pages with no `state.json` entry are skipped: we cannot tell
    /// which file on disk is theirs, and guessing forks a `<slug>-2.md`
    /// duplicate. The empty-directory case is already covered by the daemon's
    /// one-time backfill.
    pub fn reconcile(
        &self,
        guard: &crate::page_projection_tracker::PageProjectionWriteGuard,
        pages: &[Page],
    ) -> Result<ReconcileStats, WenlanError> {
        self.validate_guard(guard)?;
        let mut stats = ReconcileStats {
            temp_files_removed: self.sweep_temp_leftovers(),
            ..ReconcileStats::default()
        };

        let state = self.load_state();
        for page in pages {
            let Some(entry) = state.pages.get(&page.id) else {
                continue;
            };
            stats.checked += 1;
            if !self.projection_is_behind(&entry.file, page) {
                continue;
            }
            match self.write_page(guard, page) {
                Ok(_) => stats.rewritten += 1,
                Err(e) => {
                    log::warn!("[reconcile] repair failed for {}: {e}", page.id);
                    stats.errors += 1;
                }
            }
        }
        Ok(stats)
    }

    /// Whether `filename` is missing, unreadable, or stamped with a version
    /// older than the DB's. Unreadable counts as behind: the repair attempt
    /// will surface the real error rather than silently skipping the page.
    fn projection_is_behind(&self, filename: &str, page: &Page) -> bool {
        let Ok(raw) = std::fs::read_to_string(self.path.join(filename)) else {
            return true;
        };
        let (fm, _body) = crate::sources::obsidian::extract_frontmatter(&raw);
        projected_origin_version(&fm) < page.version
    }

    /// Remove `write_page` temp files orphaned by a crash between write and
    /// rename. Safe to run unconditionally at startup: the daemon holds the
    /// port by this point, so no live writer owns any of these.
    fn sweep_temp_leftovers(&self) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.path) else {
            return 0;
        };
        entries
            .flatten()
            .filter(|e| is_write_page_temp_file(&e.file_name().to_string_lossy()))
            .filter(|e| std::fs::remove_file(e.path()).is_ok())
            .count()
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

    pub fn remove_page(
        &self,
        guard: &crate::page_projection_tracker::PageProjectionWriteGuard,
        page_id: &str,
    ) -> Result<(), WenlanError> {
        self.validate_guard(guard)?;
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

            let mut manifest = crate::export::provenance::StubManifest::load(&self.path);
            manifest.forget_page(page_id);
            let _ = manifest.save(&self.path);
            let _ = crate::export::provenance::gc_orphan_stubs(&self.path, &manifest);
        }

        Ok(())
    }

    fn validate_guard(
        &self,
        guard: &crate::page_projection_tracker::PageProjectionWriteGuard,
    ) -> Result<(), WenlanError> {
        if guard.belongs_to(&self.tracker) {
            Ok(())
        } else {
            Err(WenlanError::VectorDb(
                "page projection write guard does not own this writer".to_string(),
            ))
        }
    }

    fn load_state(&self) -> KnowledgeState {
        let state_path = self.path.join(".wenlan/state.json");
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

        let mut state: KnowledgeState = serde_json::from_str(&data).unwrap_or_default();
        if state.schema_version == 0 {
            state.schema_version = KNOWLEDGE_STATE_SCHEMA_V2;
        }
        state
    }

    fn save_state(&self, state: &KnowledgeState) -> Result<(), WenlanError> {
        let state_path = self.path.join(".wenlan/state.json");
        let data = serde_json::to_string_pretty(state)?;
        std::fs::write(&state_path, data)?;
        Ok(())
    }
}

pub struct KnowledgeProjectionWriteRef<'writer> {
    writer: &'writer KnowledgeWriter,
    guard: crate::page_projection_tracker::PageProjectionWriteGuard,
}

impl KnowledgeProjectionWriteRef<'_> {
    pub fn write_page(&self, page: &Page) -> Result<String, WenlanError> {
        self.writer.write_page(&self.guard, page)
    }

    pub fn remove_page(&self, page_id: &str) -> Result<(), WenlanError> {
        self.writer.remove_page(&self.guard, page_id)
    }
}

pub struct KnowledgeProjectionWrite {
    writer: KnowledgeWriter,
    guard: crate::page_projection_tracker::PageProjectionWriteGuard,
}

impl KnowledgeProjectionWrite {
    pub fn new(path: PathBuf, database: &crate::db::MemoryDB) -> Self {
        Self {
            writer: KnowledgeWriter::new(path, database),
            guard: database.begin_page_projection_write(),
        }
    }

    pub fn write_page(&self, page: &Page) -> Result<String, WenlanError> {
        self.writer.write_page(&self.guard, page)
    }

    pub fn remove_page(&self, page_id: &str) -> Result<(), WenlanError> {
        self.writer.remove_page(&self.guard, page_id)
    }

    pub fn reconcile(&self, pages: &[Page]) -> Result<ReconcileStats, WenlanError> {
        self.writer.reconcile(&self.guard, pages)
    }
}

/// Public projection of a page's markdown (frontmatter + body + the delimiter
/// Sources block), so callers outside this module (the watcher's
/// protected-block re-projection check) can compute the canonical projection
/// without duplicating it.
pub fn render_markdown_for(page: &Page) -> String {
    render_markdown(page)
}

fn render_markdown(page: &Page) -> String {
    use crate::export::provenance::{
        related_frontmatter, render_sources_block, sources_frontmatter, yaml_quoted,
    };
    let mut out = String::new();

    // Frontmatter
    out.push_str("---\n");
    out.push_str(&format!("title: {}\n", yaml_quoted(&page.title)));
    if let Some(ref space) = page.space {
        out.push_str(&format!("space: {}\n", space));
    }
    out.push_str(&format!("origin_id: {}\n", page.id));
    out.push_str(&format!("origin_version: {}\n", page.version));
    let created_date: String = page.created_at.chars().take(10).collect();
    let modified_date: String = page.last_modified.chars().take(10).collect();
    out.push_str(&format!("created: {}\n", created_date));
    out.push_str(&format!("modified: {}\n", modified_date));
    // Read-only provenance projection (one-way; the watcher never reads it back).
    out.push_str(&sources_frontmatter(&page.source_memory_ids));
    let related = related_page_titles(&page.content);
    out.push_str(&related_frontmatter(&related));
    out.push_str("---\n\n");

    // Body with wikilinks
    out.push_str(&convert_links_to_wikilinks(&page.content));

    // Export-only delimiter-wrapped Sources block, generated from DB truth.
    let block = render_sources_block(&page.source_memory_ids);
    if !block.is_empty() {
        out.push_str("\n\n");
        out.push_str(&block);
    } else {
        out.push('\n');
    }

    out
}

/// Page-to-page wikilink targets in the body, for the read-only `related:`
/// frontmatter. `mem_*` targets are excluded — those are provenance, not
/// topic links.
fn related_page_titles(content: &str) -> Vec<String> {
    let wikified = convert_links_to_wikilinks(content);
    crate::sources::obsidian::extract_wikilinks(&wikified)
        .into_iter()
        .map(|w| w.target)
        .filter(|t| !t.starts_with("mem_"))
        .collect()
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
            pending_rebuild: None,
            user_edited: false,
            relevance_score: 0.0,
            last_edited_by: None,
            last_edited_at: None,
            last_delta_summary: None,
            changelog: None,
            creation_kind: "distilled".to_string(),
            review_status: "confirmed".to_string(),
            workspace: None,
            citations: Vec::new(),
        }
    }

    #[test]
    fn test_write_page_creates_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        let page = test_concept();

        let path = writer.write_page_for_test(&page).unwrap();
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
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());

        writer.write_page_for_test(&test_concept()).unwrap();

        let state = writer.load_state();
        assert_eq!(state.schema_version, KNOWLEDGE_STATE_SCHEMA_V2);
        assert!(state.pages.contains_key("concept_test123"));
        assert_eq!(state.pages["concept_test123"].file, "rust-ownership.md");
        assert_eq!(state.pages["concept_test123"].version, 2);
    }

    #[test]
    fn test_remove_page_deletes_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());

        let path = writer.write_page_for_test(&test_concept()).unwrap();
        assert!(std::path::Path::new(&path).exists());

        writer.remove_page_for_test("concept_test123").unwrap();
        assert!(!std::path::Path::new(&path).exists());

        let state = writer.load_state();
        assert!(!state.pages.contains_key("concept_test123"));
    }

    #[test]
    fn test_remove_nonexistent_page_noop() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        writer.remove_page_for_test("nonexistent").unwrap();
    }

    #[test]
    fn test_write_multiple_pages() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());

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

        writer.write_page_for_test(&c1).unwrap();
        writer.write_page_for_test(&c2).unwrap();

        assert!(dir.path().join("alpha.md").exists());
        assert!(dir.path().join("beta.md").exists());

        let state = writer.load_state();
        assert_eq!(state.pages.len(), 2);
    }

    #[test]
    fn test_knowledge_writer_overwrite_on_version_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());

        let mut page = test_concept();
        writer.write_page_for_test(&page).unwrap();

        let v1 = std::fs::read_to_string(dir.path().join("rust-ownership.md")).unwrap();
        assert!(v1.contains("origin_version: 2"));

        // Update version and content
        page.version = 3;
        page.content = "## Updated\nNew content.".to_string();
        writer.write_page_for_test(&page).unwrap();

        let v2 = std::fs::read_to_string(dir.path().join("rust-ownership.md")).unwrap();
        assert!(v2.contains("origin_version: 3"));
        assert!(v2.contains("## Updated"));
        assert!(!v2.contains("memory safety")); // old content replaced

        // State reflects new version
        let state = writer.load_state();
        assert_eq!(state.pages["concept_test123"].version, 3);
    }

    /// `write_page` must replace the target file via temp-file-then-rename,
    /// never via in-place truncate+write, so a concurrent reader can never
    /// observe a half-written page file. A plain `fs::write`
    /// truncates and rewrites the *same* inode, so its inode number is
    /// unchanged across writes; an atomic rename swaps in a new inode. That
    /// difference is deterministic (no thread timing needed) and is what
    /// actually distinguishes this from the old, non-atomic implementation —
    /// unlike the leftover-temp-file check below, which passes trivially on
    /// old code too (it never created temp files to begin with).
    #[cfg(unix)]
    #[test]
    fn write_page_replaces_file_via_atomic_rename_not_in_place_write() {
        use std::os::unix::fs::MetadataExt;

        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());

        let mut page = test_concept();
        let path = writer.write_page_for_test(&page).unwrap();
        let ino_before = std::fs::metadata(&path).unwrap().ino();

        page.version = 3;
        page.content = "## Updated\nNew content for atomicity check.".to_string();
        writer.write_page_for_test(&page).unwrap();

        let ino_after = std::fs::metadata(&path).unwrap().ino();
        assert_ne!(
            ino_before, ino_after,
            "rewrite kept the same inode — target was truncated in place, not atomically renamed"
        );

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, render_markdown_for(&page));

        let leftover_temp_files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(
            leftover_temp_files.is_empty(),
            "leftover temp file(s): {leftover_temp_files:?}"
        );
    }

    /// A projection file that vanished (rename never landed, user deleted it)
    /// is rewritten from DB truth.
    #[test]
    fn reconcile_rewrites_page_whose_projection_file_vanished() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        let page = test_concept();
        let path = writer.write_page_for_test(&page).unwrap();
        std::fs::remove_file(&path).unwrap();

        let stats = writer
            .reconcile_for_test(std::slice::from_ref(&page))
            .unwrap();

        assert_eq!(stats.checked, 1);
        assert_eq!(stats.rewritten, 1);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            render_markdown_for(&page),
            "vanished projection was not restored from the DB"
        );
    }

    /// The fsync-free write buys atomicity, not durability: after a power loss
    /// the rename may be undone, leaving the *previous* version's bytes under
    /// the target name. The `origin_version` stamp is what proves it — the DB
    /// moved on, the file did not.
    #[test]
    fn reconcile_rewrites_projection_left_behind_by_a_torn_rename() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());

        let mut page = test_concept();
        let path = writer.write_page_for_test(&page).unwrap();

        // DB advanced to v3; the file still holds the v2 projection.
        page.version = 3;
        page.content = "## Updated\nv3 body that never reached disk.".to_string();

        let stats = writer
            .reconcile_for_test(std::slice::from_ref(&page))
            .unwrap();

        assert_eq!(stats.rewritten, 1);
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            on_disk,
            render_markdown_for(&page),
            "stale projection was not brought forward to the DB version"
        );
    }

    /// The other power-loss shape: the rename is durable but the bytes are
    /// not, so the target is zero-length. No frontmatter parses, so the
    /// version stamp reads 0 — behind any real page (pages start at 1).
    #[test]
    fn reconcile_rewrites_projection_truncated_to_zero_bytes() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        let page = test_concept();
        let path = writer.write_page_for_test(&page).unwrap();
        std::fs::write(&path, "").unwrap();

        let stats = writer
            .reconcile_for_test(std::slice::from_ref(&page))
            .unwrap();

        assert_eq!(stats.rewritten, 1);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            render_markdown_for(&page)
        );
    }

    /// The safety boundary. md is canonical for prose (see `page_watcher`), so
    /// a body that differs from `render_markdown` while the version stamp is
    /// current is a USER EDIT made while the daemon was down — the watcher
    /// owns it, and reconcile must not clobber it on the way past. The temp
    /// sweep is likewise scoped to `write_page`'s own name shape, never to a
    /// file the user happens to have parked in the pages directory.
    #[test]
    fn reconcile_preserves_offline_user_edits_and_sweeps_only_our_temp_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        let page = test_concept();
        let path = writer.write_page_for_test(&page).unwrap();

        let edited = std::fs::read_to_string(&path)
            .unwrap()
            .replace("Rust uses ownership", "Hand-written prose the user typed");
        std::fs::write(&path, &edited).unwrap();

        let ours = dir.path().join(format!(".{}.4242.7.tmp", page.id));
        let user_scratch = dir.path().join("draft-notes.tmp");
        let editor_swap = dir.path().join(".obsidian-swap.tmp");
        for f in [&ours, &user_scratch, &editor_swap] {
            std::fs::write(f, "scratch").unwrap();
        }

        let stats = writer
            .reconcile_for_test(std::slice::from_ref(&page))
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            edited,
            "reconcile clobbered an offline user edit"
        );
        assert_eq!(stats.rewritten, 0);
        assert_eq!(stats.temp_files_removed, 1);
        assert!(!ours.exists(), "our own leftover temp file survived");
        assert!(
            user_scratch.exists() && editor_swap.exists(),
            "reconcile deleted a temp file that isn't ours"
        );
    }

    /// Without a `state.json` entry we cannot tell which file on disk is a
    /// page's projection, and guessing forks a `<slug>-2.md` duplicate against
    /// the real one (the hazard `page_watcher` calls out for vaults synced
    /// without `.wenlan/`). Reconcile skips those rather than fork.
    #[test]
    fn reconcile_skips_pages_with_no_state_entry_rather_than_forking_a_duplicate() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        let page = test_concept();
        // A projection exists on disk, but state.json does not know about it.
        std::fs::write(
            dir.path().join("rust-ownership.md"),
            render_markdown_for(&page),
        )
        .unwrap();

        let stats = writer
            .reconcile_for_test(std::slice::from_ref(&page))
            .unwrap();

        assert_eq!(stats.checked, 0);
        assert_eq!(stats.rewritten, 0);
        assert!(
            !dir.path().join("rust-ownership-2.md").exists(),
            "reconcile forked a duplicate projection"
        );
    }

    #[test]
    fn test_load_state_migrates_v1_concept_keys_to_page() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());

        // Write a legacy v1 state.json by hand.
        let v1_json = r#"{
            "concepts": {
                "concept_aaa": { "file": "a.md", "version": 1, "last_written": "2026-04-01T00:00:00+00:00" },
                "concept_bbb": { "file": "b.md", "version": 2, "last_written": "2026-04-02T00:00:00+00:00" }
            }
        }"#;
        std::fs::create_dir_all(dir.path().join(".wenlan")).unwrap();
        std::fs::write(dir.path().join(".wenlan/state.json"), v1_json).unwrap();

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
        let written = std::fs::read_to_string(dir.path().join(".wenlan/state.json")).unwrap();
        assert!(written.contains("\"pages\""));
        assert!(!written.contains("\"concepts\""));
        assert!(written.contains("\"schema_version\""));
    }

    #[test]
    fn test_load_state_upgrades_writer_default_v0() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        std::fs::create_dir_all(dir.path().join(".wenlan")).unwrap();
        std::fs::write(
            dir.path().join(".wenlan/state.json"),
            r#"{"schema_version":0,"pages":{}}"#,
        )
        .unwrap();

        assert_eq!(
            writer.load_state().schema_version,
            KNOWLEDGE_STATE_SCHEMA_V2
        );
    }

    #[test]
    fn test_knowledge_writer_no_domain() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());

        let page = Page {
            space: None,
            ..test_concept()
        };
        let path = writer.write_page_for_test(&page).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("space:"));
    }

    #[test]
    fn render_markdown_emits_sources_frontmatter_and_delimiter_block() {
        let mut page = test_concept();
        page.source_memory_ids = vec!["mem_1".to_string(), "mem_2".to_string()];
        let md = render_markdown(&page);
        // Read-only frontmatter property.
        assert!(md.contains("sources: [\"[[mem_1]]\", \"[[mem_2]]\"]"));
        // Delimiter-wrapped Sources block in the body.
        assert!(md.contains(crate::export::provenance::SOURCES_BLOCK_START));
        assert!(md.contains(crate::export::provenance::SOURCES_BLOCK_END));
        assert!(md.contains("## Sources"));
        // The projected body, run back through the canonicalizer, drops the
        // generated block — the round-trip invariant the watcher relies on.
        let (_, body) = crate::sources::obsidian::extract_frontmatter(&md);
        let canon = crate::export::provenance::canonicalize_page_body(body);
        assert!(!canon.contains(crate::export::provenance::SOURCES_BLOCK_START));
        assert!(!canon.contains("## Sources"));
        // Substring of test_concept()'s actual body text.
        assert!(canon.contains("Rust uses ownership"));
    }

    #[test]
    fn stubs_resolve_then_gc_when_page_removed() {
        let dir = tempfile::TempDir::new().unwrap();
        let writer = KnowledgeWriter::new_for_test(dir.path().to_path_buf());
        let mut page = test_concept();
        page.source_memory_ids = vec!["mem_x".to_string()];
        writer.write_page_for_test(&page).unwrap();
        let stub = dir.path().join("_sources").join("mem_x.md");
        assert!(stub.exists(), "stub projected on write");

        writer.remove_page_for_test(&page.id).unwrap();
        assert!(!stub.exists(), "orphan stub GC'd on page removal");
    }

    #[test]
    fn render_markdown_source_less_page_has_no_sources_artifacts() {
        let mut page = test_concept();
        page.source_memory_ids = vec![];
        let md = render_markdown(&page);
        assert!(!md.contains("sources:"));
        assert!(!md.contains(crate::export::provenance::SOURCES_BLOCK_START));
    }

    #[test]
    fn render_markdown_escapes_title_so_frontmatter_survives_quotes() {
        let mut page = test_concept();
        page.title = "The \"Real\" Architecture".to_string();
        let md = render_markdown(&page);
        let (fm, _body) = crate::sources::obsidian::extract_frontmatter(&md);
        // Frontmatter must NOT have collapsed to empty: origin_id still parses.
        assert_eq!(
            fm.get_str("origin_id"),
            Some(page.id.as_str()),
            "quote-bearing title collapsed the frontmatter map"
        );
        // And the title round-trips intact.
        assert_eq!(fm.get_str("title"), Some("The \"Real\" Architecture"));
    }
}
