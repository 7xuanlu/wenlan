// SPDX-License-Identifier: Apache-2.0
//! Poll-based filesystem watcher for the page projection at
//! `~/.origin/pages/*.md`.
//!
//! md is canonical. When the user opens a page in Obsidian / VS Code / etc.
//! and saves a change, the daemon's DB row goes stale. This module walks the
//! projection on a scheduler tick, compares each file against its DB
//! counterpart, and applies the edit back through `update_page_content`
//! with `link_reason = "fs_edit"`. That flips `user_edited` (same effect
//! as a `manual_edit` write via POST /api/pages/{id}) so the refinery's
//! re-distill escalation branch fires instead of clobbering the user's
//! prose.
//!
//! Conflict direction: if the md's `origin_version` is less than the DB's,
//! the daemon wrote last and the md on disk is stale (probably from a
//! refinery cycle that hasn't re-projected yet). We skip rather than
//! roll the DB back. The reverse (md ahead, DB behind) doesn't happen in
//! practice — the daemon is the only writer that bumps the version
//! column.
//!
//! v1 is poll-only, runs on the same cadence as the refinery scheduler
//! tick. An event-driven `notify` watcher is a follow-up if 30-second
//! latency proves too slow.

use crate::db::MemoryDB;
use crate::error::OriginError;
use crate::sources::obsidian;
use std::path::{Path, PathBuf};

/// Counts the watcher reports back to the scheduler tick. Useful for the
/// daily-status feed once the UI surface lands; for now the daemon just
/// logs `applied` so an operator can see edits flowing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WatcherStats {
    /// Total .md files inspected this pass.
    pub scanned: usize,
    /// Edits successfully written back to the DB.
    pub applied: usize,
    /// File had no `origin_id` in the frontmatter — most likely a user-
    /// added md the daemon hasn't claimed. Skipped, not auto-promoted.
    pub skipped_no_origin_id: usize,
    /// Frontmatter named a page_id the DB doesn't know. Dangling md from
    /// an archived/deleted page or a manually-pasted file.
    pub skipped_unknown_page: usize,
    /// md version stamp is behind the DB. Daemon wrote last; md will get
    /// re-projected on the next daemon write, no need to reflect it back.
    pub skipped_daemon_ahead: usize,
    /// md body matches DB content. No-op.
    pub skipped_unchanged: usize,
    /// IO or parse failures — surfaced via `log::warn!` per file.
    pub errors: usize,
}

/// Scan `knowledge_path` for page md files, reflect external edits back
/// into the DB. Idempotent and safe to call on every scheduler tick.
pub async fn sync_filesystem_edits(
    db: &MemoryDB,
    knowledge_path: &Path,
) -> Result<WatcherStats, OriginError> {
    let mut stats = WatcherStats::default();
    if !knowledge_path.exists() {
        return Ok(stats);
    }
    let entries = match std::fs::read_dir(knowledge_path) {
        Ok(e) => e,
        Err(e) => {
            log::warn!(
                "[page-watcher] read_dir({}) failed: {e}",
                knowledge_path.display()
            );
            return Ok(stats);
        }
    };
    let knowledge_path_buf: PathBuf = knowledge_path.to_path_buf();
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        // Pages live flat under knowledge_path. Subdirectories (e.g. the
        // `.origin/` state dir) aren't recursed.
        if !path.is_file() || ext != "md" {
            continue;
        }
        stats.scanned += 1;
        match sync_one_file(db, &path, &knowledge_path_buf).await {
            Ok(Outcome::Applied { page_id }) => {
                stats.applied += 1;
                log::info!(
                    "[page-watcher] applied fs_edit for {page_id} from {}",
                    path.display()
                );
            }
            Ok(Outcome::SkippedNoOriginId) => stats.skipped_no_origin_id += 1,
            Ok(Outcome::SkippedUnknownPage { page_id }) => {
                stats.skipped_unknown_page += 1;
                log::debug!(
                    "[page-watcher] {} references unknown page {page_id}",
                    path.display()
                );
            }
            Ok(Outcome::SkippedDaemonAhead { page_id }) => {
                stats.skipped_daemon_ahead += 1;
                log::debug!("[page-watcher] {page_id}: md trails DB version, skipping");
            }
            Ok(Outcome::Unchanged) => stats.skipped_unchanged += 1,
            Err(e) => {
                stats.errors += 1;
                log::warn!("[page-watcher] {}: {e}", path.display());
            }
        }
    }
    Ok(stats)
}

enum Outcome {
    Applied { page_id: String },
    SkippedNoOriginId,
    SkippedUnknownPage { page_id: String },
    SkippedDaemonAhead { page_id: String },
    Unchanged,
}

async fn sync_one_file(
    db: &MemoryDB,
    path: &Path,
    knowledge_path: &Path,
) -> Result<Outcome, OriginError> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| OriginError::VectorDb(format!("read {}: {e}", path.display())))?;
    let (fm, body) = obsidian::extract_frontmatter(&raw);
    let page_id = match fm.get_str("origin_id") {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return Ok(Outcome::SkippedNoOriginId),
    };

    let existing = match db.get_page(&page_id).await? {
        Some(p) => p,
        None => return Ok(Outcome::SkippedUnknownPage { page_id }),
    };

    // Skip when md is stale relative to DB. `origin_version` in the
    // frontmatter is written by KnowledgeWriter::render_markdown and
    // tracks the DB's version column at projection time. If the daemon
    // bumped version without re-projecting (e.g. refinery write without
    // KnowledgeWriter call), the md is behind — we'd otherwise roll the
    // DB back to a stale body.
    // Accept `origin_version: 3`, `origin_version: 3.0`, or
    // `origin_version: "3"` — YAML parses each into a different serde_yaml
    // shape and a hand-edited frontmatter shouldn't lock the user out of
    // future fs_edits just because they retyped the field as a float.
    let md_version: i64 = fm
        .fields
        .get("origin_version")
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_f64().map(|f| f as i64))
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
        })
        .unwrap_or(0);
    if md_version < existing.version {
        return Ok(Outcome::SkippedDaemonAhead { page_id });
    }

    // Trim trailing newlines on both sides — render_markdown emits a
    // trailing `\n` and editors tend to add their own. Otherwise a no-op
    // save would look like a real edit forever.
    let body_norm = body.trim_end_matches('\n');
    let db_norm = existing.content.trim_end_matches('\n');
    if body_norm == db_norm {
        return Ok(Outcome::Unchanged);
    }

    // Preserve existing source list — the user edited prose, not the
    // memory provenance. Sources change only via /distill refresh or
    // explicit POST.
    let req = origin_types::requests::UpdatePageRequest {
        content: body_norm.to_string(),
        source_memory_ids: existing.source_memory_ids.clone(),
    };
    // Pass knowledge_path so update_page re-projects the md with the new
    // version stamp; without that the next tick would see origin_version
    // trailing the DB and skip as SkippedDaemonAhead.
    // require_stale=false: user edits are unconditional.
    // knowledge_path=Some: page_watcher IS the fs writer; update_page
    //   re-projects rather than skipping the write.
    crate::post_write::update_page(db, &page_id, req, "fs_edit", false, Some(knowledge_path))
        .await?;

    Ok(Outcome::Applied { page_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::events::NoopEmitter;
    use crate::export::knowledge::KnowledgeWriter;
    use crate::pages::Page;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn fresh_db() -> (MemoryDB, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let emitter: Arc<dyn crate::events::EventEmitter> = Arc::new(NoopEmitter);
        let db = MemoryDB::new(&path, emitter).await.unwrap();
        (db, dir)
    }

    fn write_page_md(dir: &std::path::Path, page: &Page, body_override: Option<&str>) {
        let body = body_override.unwrap_or(&page.content);
        let domain_line = match &page.domain {
            Some(d) => format!("domain: {d}\n"),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\n{}origin_id: {}\norigin_version: {}\ncreated: {}\nmodified: {}\n---\n\n{}\n",
            page.title,
            domain_line,
            page.id,
            page.version,
            page.created_at.chars().take(10).collect::<String>(),
            page.last_modified.chars().take(10).collect::<String>(),
            body
        );
        std::fs::write(dir.join(format!("{}.md", slug(&page.title))), content).unwrap();
    }

    fn slug(s: &str) -> String {
        s.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect()
    }

    fn sample_page(id: &str, title: &str, content: &str) -> Page {
        let now = chrono::Utc::now().to_rfc3339();
        Page {
            id: id.to_string(),
            title: title.to_string(),
            summary: None,
            content: content.to_string(),
            entity_id: None,
            domain: None,
            source_memory_ids: vec!["mem_seed".to_string()],
            version: 1,
            status: "active".to_string(),
            created_at: now.clone(),
            last_compiled: now.clone(),
            last_modified: now,
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

    #[tokio::test]
    async fn applies_user_edit_when_md_body_differs() {
        let (db, _ddir) = fresh_db().await;
        let knowledge_dir = TempDir::new().unwrap();
        let page = sample_page("page_a", "Topic A", "original body line");
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page(
            &page.id,
            &page.title,
            None,
            &page.content,
            None,
            None,
            &["mem_seed"],
            &now,
        )
        .await
        .unwrap();

        write_page_md(knowledge_dir.path(), &page, Some("user-edited body line"));
        let stats = sync_filesystem_edits(&db, knowledge_dir.path())
            .await
            .unwrap();
        assert_eq!(stats.applied, 1);

        let p = db.get_page("page_a").await.unwrap().unwrap();
        assert_eq!(p.content, "user-edited body line");
        // fs_edit must flip user_edited so refinery escalates instead of
        // overwriting on the next re-distill.
        assert!(p.user_edited);
    }

    #[tokio::test]
    async fn no_op_when_md_matches_db() {
        let (db, _ddir) = fresh_db().await;
        let knowledge_dir = TempDir::new().unwrap();
        let page = sample_page("page_b", "Topic B", "matching content");
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page(
            &page.id,
            &page.title,
            None,
            &page.content,
            None,
            None,
            &["mem_seed"],
            &now,
        )
        .await
        .unwrap();
        write_page_md(knowledge_dir.path(), &page, None);

        let stats = sync_filesystem_edits(&db, knowledge_dir.path())
            .await
            .unwrap();
        assert_eq!(stats.applied, 0);
        assert_eq!(stats.skipped_unchanged, 1);
    }

    #[tokio::test]
    async fn skips_md_with_no_origin_id() {
        let (db, _ddir) = fresh_db().await;
        let knowledge_dir = TempDir::new().unwrap();
        // User dropped a plain md in the page dir with no origin_id.
        std::fs::write(
            knowledge_dir.path().join("freeform.md"),
            "---\ntitle: \"Random\"\n---\n\njust a note",
        )
        .unwrap();

        let stats = sync_filesystem_edits(&db, knowledge_dir.path())
            .await
            .unwrap();
        assert_eq!(stats.applied, 0);
        assert_eq!(stats.skipped_no_origin_id, 1);
    }

    #[tokio::test]
    async fn skips_md_pointing_at_unknown_page_id() {
        let (db, _ddir) = fresh_db().await;
        let knowledge_dir = TempDir::new().unwrap();
        let ghost = sample_page("page_ghost", "Ghost", "ghost body");
        // DB never inserted page_ghost — md is dangling.
        write_page_md(knowledge_dir.path(), &ghost, None);

        let stats = sync_filesystem_edits(&db, knowledge_dir.path())
            .await
            .unwrap();
        assert_eq!(stats.applied, 0);
        assert_eq!(stats.skipped_unknown_page, 1);
    }

    #[tokio::test]
    async fn skips_when_md_version_trails_db() {
        let (db, _ddir) = fresh_db().await;
        let knowledge_dir = TempDir::new().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page(
            "page_c",
            "Topic C",
            None,
            "daemon body v3",
            None,
            None,
            &["mem_seed"],
            &now,
        )
        .await
        .unwrap();
        // Daemon bumped version to 3 via two updates without re-projecting md.
        db.update_page_content("page_c", "daemon body v2", &["mem_seed"], "re_distill")
            .await
            .unwrap();
        db.update_page_content("page_c", "daemon body v3", &["mem_seed"], "re_distill")
            .await
            .unwrap();

        // md frontmatter stamps origin_version: 1 — the projection from
        // the original insert. md body differs from DB.
        let mut stale = sample_page("page_c", "Topic C", "old body from disk");
        stale.version = 1;
        write_page_md(knowledge_dir.path(), &stale, None);

        let stats = sync_filesystem_edits(&db, knowledge_dir.path())
            .await
            .unwrap();
        // Daemon ahead — don't roll DB back to the disk's old body.
        assert_eq!(stats.applied, 0);
        assert_eq!(stats.skipped_daemon_ahead, 1);
        let p = db.get_page("page_c").await.unwrap().unwrap();
        assert_eq!(p.content, "daemon body v3");
        assert!(!p.user_edited);
    }

    #[tokio::test]
    async fn applies_consecutive_user_edits() {
        // Regression: after apply, md must be re-projected so the frontmatter
        // origin_version stays in sync with the DB. Otherwise the second
        // edit lands as SkippedDaemonAhead.
        //
        // Seed the page via KnowledgeWriter so state.json + filename + DB
        // all line up the way they would in production. Subsequent edits
        // rewrite that same file in place.
        let (db, _ddir) = fresh_db().await;
        let knowledge_dir = TempDir::new().unwrap();
        let page = sample_page("page_x", "Topic X", "original body");
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page(
            &page.id,
            &page.title,
            None,
            &page.content,
            None,
            None,
            &["mem_seed"],
            &now,
        )
        .await
        .unwrap();
        let writer = KnowledgeWriter::new(knowledge_dir.path().to_path_buf());
        writer.write_page(&page).unwrap();
        let md_path = knowledge_dir
            .path()
            .join(writer.page_filename(&page.id).expect("state has file"));

        // Edit #1 — replace body.
        let projected = std::fs::read_to_string(&md_path).unwrap();
        let header_end = projected.find("\n---\n\n").expect("frontmatter present");
        let head = &projected[..header_end + "\n---\n\n".len()];
        std::fs::write(&md_path, format!("{head}edit one\n")).unwrap();
        let s1 = sync_filesystem_edits(&db, knowledge_dir.path())
            .await
            .unwrap();
        assert_eq!(s1.applied, 1, "edit one must apply; got {:?}", s1);

        // Edit #2 — same path, new body. The watcher's re-projection
        // bumped origin_version, so a fresh read picks up the new header.
        let projected = std::fs::read_to_string(&md_path).unwrap();
        let header_end = projected.find("\n---\n\n").expect("frontmatter present");
        let head = &projected[..header_end + "\n---\n\n".len()];
        std::fs::write(&md_path, format!("{head}edit two — second pass\n")).unwrap();
        let s2 = sync_filesystem_edits(&db, knowledge_dir.path())
            .await
            .unwrap();
        assert_eq!(s2.applied, 1, "edit two must apply; got {:?}", s2);
        let p = db.get_page("page_x").await.unwrap().unwrap();
        assert_eq!(p.content, "edit two — second pass");
    }

    #[tokio::test]
    async fn nonexistent_knowledge_path_is_a_noop() {
        let (db, _ddir) = fresh_db().await;
        let missing = std::path::PathBuf::from("/tmp/origin-page-watcher-nonexistent");
        let stats = sync_filesystem_edits(&db, &missing).await.unwrap();
        assert_eq!(stats, WatcherStats::default());
    }
}
