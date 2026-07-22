// SPDX-License-Identifier: Apache-2.0
//! Provenance projection helpers: delimiter-owned Sources block, canonical
//! storage validation, and `_sources/` stub projection + GC.

use crate::error::WenlanError;

/// Opening delimiter for the export-only `## Sources` block. The block is
/// generated from DB truth at projection time and stripped at ingress; it is
/// NEVER part of canonical `Page.content`.
pub const SOURCES_BLOCK_START: &str = "<!-- origin:sources:start -->";
/// Closing delimiter for the export-only Sources block.
pub const SOURCES_BLOCK_END: &str = "<!-- origin:sources:end -->";

/// Strip ONLY the delimiter-owned Sources block from a page body. A user may
/// legitimately type a `## Sources` heading or a `[[mem_123]]` wikilink in
/// prose; neither is touched. Removes the first `START..END` span (inclusive
/// of both delimiters and any trailing newline before START) and returns the
/// remainder trimmed of trailing whitespace. If the delimiters are absent or
/// malformed (END before START, or START with no END), the body is returned
/// trimmed but otherwise untouched.
pub fn canonicalize_page_body(body: &str) -> String {
    let start = match body.find(SOURCES_BLOCK_START) {
        Some(i) => i,
        None => return body.trim_end().to_string(),
    };
    let after_start = start + SOURCES_BLOCK_START.len();
    let end_rel = match body[after_start..].find(SOURCES_BLOCK_END) {
        Some(i) => i,
        None => return body.trim_end().to_string(),
    };
    let end = after_start + end_rel + SOURCES_BLOCK_END.len();
    // Drop whitespace/newlines immediately preceding the block so a fresh
    // projection (body + "\n\n" + block) canonicalizes back to bare body.
    let head = body[..start].trim_end();
    let tail = body[end..].trim_start();
    let mut out = String::from(head);
    if !tail.is_empty() {
        out.push_str("\n\n");
        out.push_str(tail);
    }
    out.trim_end().to_string()
}

/// Validate CLIENT-supplied canonical Page source without rewriting it.
///
/// The exporter owns both Sources delimiters. Persisting either token would
/// make vault projection decoding ambiguous, so canonical ingress rejects the
/// whole write instead of silently deleting user bytes. Delimiter-free source
/// is returned as the same borrowed string so every later storage consumer
/// sees the exact representation the caller supplied.
pub fn validate_canonical_page_content(content: &str) -> Result<&str, WenlanError> {
    if content.contains(SOURCES_BLOCK_START) || content.contains(SOURCES_BLOCK_END) {
        return Err(WenlanError::Validation(
            "Page content contains a daemon-reserved Sources delimiter".to_string(),
        ));
    }
    Ok(content)
}

/// Compatibility projection decoder retained for downstream callers that used
/// the pre-0.14.1 ingress helper directly.
///
/// Canonical storage paths must use [`validate_canonical_page_content`].
/// This function intentionally preserves its historical lossy behavior for
/// callers decoding daemon-owned projection text.
pub fn sanitize_ingress_content(content: &str) -> String {
    let stripped = canonicalize_page_body(content);
    let cleaned = stripped
        .replace(SOURCES_BLOCK_START, "")
        .replace(SOURCES_BLOCK_END, "");
    cleaned.trim_end().to_string()
}

/// Encode `s` as a YAML-safe double-quoted scalar. A JSON string literal is
/// a valid YAML flow scalar, so serde_json handles quotes/backslashes/control
/// chars correctly. Falls back to a naive quote only if JSON encoding fails
/// (it never does for a String).
pub(crate) fn yaml_quoted(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s.replace('"', "'")))
}

/// Render the export-only `## Sources` block from a page's cited memory ids.
/// Returns the empty string when there are no sources (source-less pages get
/// no block). The block is wrapped in the delimiters so the ingress
/// canonicalizer can strip it exactly.
pub fn render_sources_block(source_memory_ids: &[String]) -> String {
    if source_memory_ids.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(SOURCES_BLOCK_START);
    out.push_str("\n## Sources\n");
    for id in source_memory_ids {
        out.push_str(&format!("- [[{id}]]\n"));
    }
    out.push_str(SOURCES_BLOCK_END);
    out.push('\n');
    out
}

/// Render the read-only `sources:` frontmatter line (quoted wikilinks, which
/// Obsidian requires for list properties). Empty string when no sources.
/// PROJECTION-OUT ONLY — the watcher never reads this back.
// ids are `mem_*`-shaped (no YAML-specials), but they still route through
// `yaml_quoted` for uniform safety — unlike `related_frontmatter`, which takes
// untrusted free-text titles where escaping is load-bearing.
pub fn sources_frontmatter(source_memory_ids: &[String]) -> String {
    if source_memory_ids.is_empty() {
        return String::new();
    }
    let quoted: Vec<String> = source_memory_ids
        .iter()
        .map(|id| yaml_quoted(&format!("[[{id}]]")))
        .collect();
    format!("sources: [{}]\n", quoted.join(", "))
}

/// Render the read-only `related:` frontmatter line from page→page wikilink
/// targets. Empty string when there are none.
pub fn related_frontmatter(related_titles: &[String]) -> String {
    if related_titles.is_empty() {
        return String::new();
    }
    let quoted: Vec<String> = related_titles
        .iter()
        .map(|t| yaml_quoted(&format!("[[{t}]]")))
        .collect();
    format!("related: [{}]\n", quoted.join(", "))
}

use cap_fs_ext::{DirExt as _, FollowSymlinks, OpenOptionsFollowExt as _};
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{Read as _, Write as _};
use std::path::Path;

/// Subdir under `knowledge_path` for read-only source stubs. The page_watcher
/// scans only the top-level `.md` files, so this subdir is never synced back.
pub const SOURCES_STUB_DIR: &str = "_sources";

/// Maps page_id → the memory ids that page currently cites. Persisted at
/// `_sources/.manifest.json` so GC knows which stubs are still referenced
/// across daemon restarts. GC reaps by the `origin_stub:` marker (see
/// `gc_orphan_stubs`), so daemon stubs of any id shape (`mem_*`, `import_*`)
/// are reaped when orphaned while unmarked user notes are spared.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StubManifest {
    pages: HashMap<String, Vec<String>>,
}

impl StubManifest {
    pub fn record(&mut self, page_id: &str, source_memory_ids: &[String]) {
        if source_memory_ids.is_empty() {
            self.pages.remove(page_id);
        } else {
            self.pages
                .insert(page_id.to_string(), source_memory_ids.to_vec());
        }
    }

    pub fn forget_page(&mut self, page_id: &str) {
        self.pages.remove(page_id);
    }

    fn cited_ids(&self) -> HashSet<String> {
        self.pages.values().flatten().cloned().collect()
    }

    pub fn load(knowledge_path: &Path) -> Self {
        let p = knowledge_path.join(SOURCES_STUB_DIR).join(".manifest.json");
        std::fs::read_to_string(&p)
            .ok()
            .and_then(|d| serde_json::from_str(&d).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, knowledge_path: &Path) -> std::io::Result<()> {
        let dir = knowledge_path.join(SOURCES_STUB_DIR);
        std::fs::create_dir_all(&dir)?;
        let data = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into());
        std::fs::write(dir.join(".manifest.json"), data)
    }

    pub(crate) fn load_from(root: &Dir) -> Self {
        let Ok(dir) = open_or_create_sources_dir(root, false) else {
            return Self::default();
        };
        let Some(dir) = dir else {
            return Self::default();
        };
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let mut file = match dir.open_with(".manifest.json", &options) {
            Ok(file) => file,
            Err(_) => return Self::default(),
        };
        let mut data = String::new();
        if (&mut file)
            .take(4 * 1024 * 1024 + 1)
            .read_to_string(&mut data)
            .is_err()
            || data.len() > 4 * 1024 * 1024
        {
            return Self::default();
        }
        serde_json::from_str(&data).unwrap_or_default()
    }

    pub(crate) fn save_to(&self, root: &Dir) -> std::io::Result<()> {
        let dir = open_or_create_sources_dir(root, true)?
            .expect("create=true always returns the sources directory");
        let data = serde_json::to_vec_pretty(self).unwrap_or_else(|_| b"{}".to_vec());
        write_regular_nofollow(&dir, ".manifest.json", &data)
    }
}

/// Deletes orphan daemon-written stub files (those no longer cited by any page
/// in `manifest`). Scope is the `origin_stub:` MARKER, not the filename: a
/// `.md` file under `_sources/` is reaped iff it carries the marker (so it is a
/// daemon projection) AND its filename is not in the cited set. Files without
/// the marker — user notes of ANY name, including one named `mem_*.md` — are
/// never touched. Reaping by marker reaps `import_*` stubs too (whose names
/// don't start with `mem_`), closing the leak the old name-prefix scope had.
pub fn gc_orphan_stubs(knowledge_path: &Path, manifest: &StubManifest) -> std::io::Result<()> {
    let dir = knowledge_path.join(SOURCES_STUB_DIR);
    if !dir.exists() {
        return Ok(());
    }
    let cited: HashSet<String> = manifest
        .cited_ids()
        .iter()
        .map(|id| stub_filename(id))
        .collect();
    for entry in std::fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        // Skip non-`.md` files (e.g. the `.manifest.json` projection index).
        if !name.ends_with(".md") {
            continue;
        }
        // Still cited → keep.
        if cited.contains(&name) {
            continue;
        }
        // Only reap DAEMON-written stubs (carry the `origin_stub:` marker
        // written by `project_stubs_for_page`). User notes under `_sources/`
        // (any name) lack the marker and are spared.
        let is_daemon_stub = std::fs::read_to_string(&path)
            .map(|c| c.contains("origin_stub:"))
            .unwrap_or(false);
        if is_daemon_stub {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

pub(crate) fn gc_orphan_stubs_in(root: &Dir, manifest: &StubManifest) -> std::io::Result<()> {
    let Some(dir) = open_or_create_sources_dir(root, false)? else {
        return Ok(());
    };
    let cited: HashSet<String> = manifest
        .cited_ids()
        .iter()
        .map(|id| stub_filename(id))
        .collect();
    for entry in dir.entries()? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name_text) = name.to_str() else {
            continue;
        };
        if !name_text.ends_with(".md") || cited.contains(name_text) {
            continue;
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        let mut file = match dir.open_with(Path::new(&name), &options) {
            Ok(file) => file,
            Err(_) => continue,
        };
        let mut content = String::new();
        let daemon_stub = (&mut file)
            .take(1024 * 1024 + 1)
            .read_to_string(&mut content)
            .is_ok()
            && content.len() <= 1024 * 1024
            && content.contains("origin_stub:");
        if daemon_stub {
            let _ = dir.remove_file(Path::new(&name));
        }
    }
    Ok(())
}

/// Map a memory id to a filesystem/wikilink-safe token. Ids already matching
/// `[A-Za-z0-9_-]+` (all stored Wenlan `mem_<uuid>` ids) pass through
/// unchanged. Other chars are hex-escaped as `_XX`.
/// NOTE: not collision-free for arbitrary input — because `_` is both the
/// escape introducer and a passthrough char, `"mem_a/"` and a literal
/// `"mem_a_2f"` both map to `"mem_a_2f"`. Safe for P1 (cited ids are always
/// UUID-shaped `mem_*`). When imported ids (which may carry `/`, spaces, etc.)
/// can become page sources in P2, make this injective then — e.g. append a
/// deterministic hash for ids failing the safe-charset check — with a
/// collision regression test. Tracked as a P2 follow-up.
pub fn sanitize_stub_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for c in id.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('_');
            for b in c.to_string().as_bytes() {
                out.push_str(&format!("{b:02x}"));
            }
        }
    }
    out
}

/// Stub filename for a memory id, e.g. `mem_1.md`.
pub fn stub_filename(id: &str) -> String {
    format!("{}.md", sanitize_stub_id(id))
}

/// Project a read-only stub note for each cited memory id under
/// `<knowledge_path>/_sources/`. Idempotent: rewrites the stub each call.
pub fn project_stubs_for_page(
    knowledge_path: &Path,
    _page_id: &str,
    source_memory_ids: &[String],
) -> std::io::Result<()> {
    if source_memory_ids.is_empty() {
        return Ok(());
    }
    let dir = knowledge_path.join(SOURCES_STUB_DIR);
    std::fs::create_dir_all(&dir)?;
    for id in source_memory_ids {
        let path = dir.join(stub_filename(id));
        let quoted = yaml_quoted(id);
        let body = format!(
            "---\ntitle: {quoted}\norigin_stub: {quoted}\n---\n\n\
             This is a read-only source projection for memory `{id}`. \
             Edit the memory in Wenlan, not this file.\n"
        );
        std::fs::write(&path, body)?;
    }
    Ok(())
}

pub(crate) fn project_stubs_for_page_in(
    root: &Dir,
    _page_id: &str,
    source_memory_ids: &[String],
) -> std::io::Result<()> {
    if source_memory_ids.is_empty() {
        return Ok(());
    }
    let dir = open_or_create_sources_dir(root, true)?
        .expect("create=true always returns the sources directory");
    for id in source_memory_ids {
        let quoted = yaml_quoted(id);
        let body = format!(
            "---\ntitle: {quoted}\norigin_stub: {quoted}\n---\n\n\
             This is a read-only source projection for memory `{id}`. \
             Edit the memory in Wenlan, not this file.\n"
        );
        write_regular_nofollow(&dir, &stub_filename(id), body.as_bytes())?;
    }
    Ok(())
}

fn open_or_create_sources_dir(root: &Dir, create: bool) -> std::io::Result<Option<Dir>> {
    match root.symlink_metadata(SOURCES_STUB_DIR) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            root.open_dir_nofollow(SOURCES_STUB_DIR).map(Some)
        }
        Ok(_) => Err(std::io::Error::other(
            "source projection directory is not a plain directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
            root.create_dir(SOURCES_STUB_DIR)?;
            root.open_dir_nofollow(SOURCES_STUB_DIR).map(Some)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn write_regular_nofollow(directory: &Dir, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create(true)
        .truncate(true)
        .follow(FollowSymlinks::No);
    let mut file = directory.open_with(name, &options)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_the_delimiter_block() {
        let body = format!(
            "## Overview\nReal prose here.\n\n{SOURCES_BLOCK_START}\n## Sources\n- [[mem_1]]\n{SOURCES_BLOCK_END}\n"
        );
        let canon = canonicalize_page_body(&body);
        assert_eq!(canon, "## Overview\nReal prose here.");
        assert!(!canon.contains(SOURCES_BLOCK_START));
        assert!(!canon.contains("[[mem_1]]"));
    }

    #[test]
    fn user_typed_mem_wikilink_in_prose_survives() {
        // No delimiter block at all — a bare `[[mem_123]]` the user wrote.
        let body = "I cited [[mem_123]] in my own note.\n\n## Sources\nhand-written, not the daemon block.";
        let canon = canonicalize_page_body(body);
        assert_eq!(canon, body.trim_end());
        assert!(canon.contains("[[mem_123]]"));
        assert!(canon.contains("## Sources"));
    }

    #[test]
    fn missing_end_delimiter_leaves_body_untouched() {
        let body = format!("prose\n\n{SOURCES_BLOCK_START}\n## Sources\n- [[mem_1]]\n");
        let canon = canonicalize_page_body(&body);
        // No END → no strip; only trailing whitespace trimmed.
        assert!(canon.contains(SOURCES_BLOCK_START));
        assert!(canon.contains("[[mem_1]]"));
    }

    #[test]
    fn preserves_prose_after_the_block() {
        let body =
            format!("head prose\n\n{SOURCES_BLOCK_START}\nx\n{SOURCES_BLOCK_END}\n\ntail prose");
        let canon = canonicalize_page_body(&body);
        assert_eq!(canon, "head prose\n\ntail prose");
    }

    #[test]
    fn canonical_content_validation_preserves_delimiter_free_source_exactly() {
        let content = "\u{feff}\r\n  ## Overview  \r\nNormal prose.\t \r\n\r\n";
        assert_ne!(
            content.trim_end(),
            content,
            "positive control: trimming must change this fixture"
        );
        let validated = validate_canonical_page_content(content).unwrap();
        assert_eq!(validated, content);
        assert_eq!(validated.as_ptr(), content.as_ptr());
    }

    #[test]
    fn canonical_content_validation_rejects_reserved_delimiters_without_rewriting() {
        let cases = [
            format!("before {SOURCES_BLOCK_START} after"),
            format!("before {SOURCES_BLOCK_END} after"),
            format!("{SOURCES_BLOCK_START}\nowned\n{SOURCES_BLOCK_END}"),
            format!("```md\n{SOURCES_BLOCK_START}\n```\nkept prose"),
        ];
        for content in cases {
            let error = validate_canonical_page_content(&content).unwrap_err();
            assert!(matches!(error, WenlanError::Validation(_)));
            assert!(
                content.contains("before")
                    || content.contains("owned")
                    || content.contains("kept prose")
            );
        }
    }

    #[test]
    fn compatibility_ingress_sanitizer_keeps_its_projection_decoding_behavior() {
        let content = format!(
            "head\n\n{SOURCES_BLOCK_START}\n## Sources\n- [[mem_1]]\n{SOURCES_BLOCK_END}\n\ntail"
        );
        assert_eq!(sanitize_ingress_content(&content), "head\n\ntail");
    }

    #[test]
    fn render_sources_block_is_delimiter_wrapped_and_canonicalizes_to_empty() {
        let ids = ["mem_1".to_string(), "mem_2".to_string()];
        let block = render_sources_block(&ids);
        assert!(block.starts_with(SOURCES_BLOCK_START));
        assert!(block.trim_end().ends_with(SOURCES_BLOCK_END));
        assert!(block.contains("## Sources"));
        assert!(block.contains("[[mem_1]]"));
        assert!(block.contains("[[mem_2]]"));
        // A body that is exactly the block canonicalizes to empty.
        assert_eq!(canonicalize_page_body(&block), "");
    }

    #[test]
    fn render_sources_block_empty_for_no_sources() {
        let ids: [String; 0] = [];
        assert_eq!(render_sources_block(&ids), String::new());
    }

    #[test]
    fn sources_frontmatter_quotes_wikilinks() {
        let ids = ["mem_1".to_string(), "mem_2".to_string()];
        let fm = sources_frontmatter(&ids);
        assert_eq!(fm, "sources: [\"[[mem_1]]\", \"[[mem_2]]\"]\n");
    }

    #[test]
    fn sources_frontmatter_empty_emits_nothing() {
        let ids: [String; 0] = [];
        assert_eq!(sources_frontmatter(&ids), String::new());
    }

    #[test]
    fn related_frontmatter_quotes_page_titles() {
        let titles = ["Other Page".to_string()];
        let fm = related_frontmatter(&titles);
        assert_eq!(fm, "related: [\"[[Other Page]]\"]\n");
    }

    #[test]
    fn related_frontmatter_escapes_titles_to_valid_yaml() {
        let titles = ["My \"Quoted\" Page".to_string()];
        let fm = related_frontmatter(&titles);
        // The emitted frontmatter block must parse as valid YAML (no map collapse).
        let yaml = format!("title: x\n{fm}");
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml)
            .expect("frontmatter with a quote-bearing title must be valid YAML");
        // And the related entry must round-trip to the original wikilink target.
        let related = parsed
            .get("related")
            .and_then(|v| v.as_sequence())
            .expect("related seq");
        assert_eq!(related[0].as_str().unwrap(), "[[My \"Quoted\" Page]]");
    }

    #[test]
    fn sanitize_stub_id_passes_safe_mem_ids() {
        assert_eq!(sanitize_stub_id("mem_abc123"), "mem_abc123");
        assert_eq!(sanitize_stub_id("mem_1"), "mem_1");
    }

    #[test]
    fn sanitize_stub_id_escapes_unsafe_chars() {
        // Imported ids may carry slashes/spaces/dots that break filenames.
        let unsafe_id = "mem_a/b c.d";
        let safe = sanitize_stub_id(unsafe_id);
        assert!(!safe.contains('/'));
        assert!(!safe.contains(' '));
        // Distinct safe-charset ids map distinctly.
        assert_ne!(sanitize_stub_id("mem_a/b"), sanitize_stub_id("mem_a-b"));
    }

    #[test]
    fn project_stubs_writes_read_only_notes_under_sources_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let ids = ["mem_1".to_string(), "mem_2".to_string()];
        project_stubs_for_page(dir.path(), "page_a", &ids).unwrap();
        let p1 = dir.path().join("_sources").join("mem_1.md");
        let p2 = dir.path().join("_sources").join("mem_2.md");
        assert!(p1.exists());
        assert!(p2.exists());
        let body = std::fs::read_to_string(&p1).unwrap();
        // Stub identifies the memory and is marked a read-only projection.
        assert!(body.contains("mem_1"));
        assert!(body.contains("read-only"));
    }

    #[test]
    fn shared_stub_bytes_do_not_depend_on_citing_page() {
        let dir = tempfile::TempDir::new().unwrap();
        let source = ["mem_shared".to_string()];
        project_stubs_for_page(dir.path(), "page_a", &source).unwrap();
        let path = dir.path().join("_sources").join("mem_shared.md");
        let from_page_a = std::fs::read(&path).unwrap();

        project_stubs_for_page(dir.path(), "page_b", &source).unwrap();

        assert_eq!(std::fs::read(path).unwrap(), from_page_a);
    }

    #[test]
    fn gc_removes_orphan_mem_stubs_keeps_still_cited() {
        let dir = tempfile::TempDir::new().unwrap();
        // page_a cites mem_1, mem_2; page_b cites mem_2.
        let mut manifest = StubManifest::default();
        manifest.record("page_a", &["mem_1".to_string(), "mem_2".to_string()]);
        manifest.record("page_b", &["mem_2".to_string()]);
        project_stubs_for_page(
            dir.path(),
            "page_a",
            &["mem_1".to_string(), "mem_2".to_string()],
        )
        .unwrap();
        project_stubs_for_page(dir.path(), "page_b", &["mem_2".to_string()]).unwrap();

        // page_a re-projected, now citing only mem_1 → mem_2 still cited by page_b.
        manifest.record("page_a", &["mem_1".to_string()]);
        gc_orphan_stubs(dir.path(), &manifest).unwrap();
        assert!(dir.path().join("_sources").join("mem_1.md").exists());
        assert!(dir.path().join("_sources").join("mem_2.md").exists());

        // page_b drops mem_2 entirely → mem_2 now orphan → GC removes it.
        manifest.record("page_b", &[]);
        gc_orphan_stubs(dir.path(), &manifest).unwrap();
        assert!(dir.path().join("_sources").join("mem_1.md").exists());
        assert!(!dir.path().join("_sources").join("mem_2.md").exists());
    }

    #[test]
    fn gc_reaps_daemon_stubs_spares_user_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let sources = dir.path().join("_sources");
        // Daemon-written stubs (carry the origin_stub: marker) for a mem_ id
        // AND a non-mem_ import id — both must be reaped when orphaned.
        project_stubs_for_page(dir.path(), "page_a", &["mem_orphan".to_string()]).unwrap();
        project_stubs_for_page(dir.path(), "page_b", &["import_42_3".to_string()]).unwrap();
        // User notes under _sources/ (no marker) — any name, including one that
        // looks like a daemon stub (`mem_decoy.md`). Both must survive.
        std::fs::write(sources.join("my-research.md"), "my own note").unwrap();
        std::fs::write(sources.join("mem_decoy.md"), "user note, not a stub").unwrap();

        let manifest = StubManifest::default(); // nothing cited → all stubs orphan
        gc_orphan_stubs(dir.path(), &manifest).unwrap();

        assert!(
            !sources.join("mem_orphan.md").exists(),
            "orphan daemon mem_ stub reaped"
        );
        assert!(
            !sources.join("import_42_3.md").exists(),
            "orphan daemon import_ stub reaped (no longer leaks)"
        );
        assert!(
            sources.join("my-research.md").exists(),
            "user file (no marker) must survive"
        );
        assert!(
            sources.join("mem_decoy.md").exists(),
            "user file named mem_* (no marker) must survive — marker-based, not name-based"
        );
    }

    #[test]
    fn gc_reaps_orphan_import_stub() {
        let dir = tempfile::TempDir::new().unwrap();
        let sources = dir.path().join("_sources");
        // An imported memory cited by a page → daemon stub projected.
        let mut manifest = StubManifest::default();
        manifest.record("page_x", &["import_9_9".to_string()]);
        project_stubs_for_page(dir.path(), "page_x", &["import_9_9".to_string()]).unwrap();
        assert!(sources.join("import_9_9.md").exists());

        // Page drops the source → import stub is now orphan → GC reaps it.
        manifest.record("page_x", &[]);
        gc_orphan_stubs(dir.path(), &manifest).unwrap();
        assert!(
            !sources.join("import_9_9.md").exists(),
            "orphan import_ stub must be reaped (the leak this fix closes)"
        );
    }
}
