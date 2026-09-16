// SPDX-License-Identifier: Apache-2.0
//! Pure-OKF v0.2 export bundle: `wenlan export okf <dir>`.
//!
//! The md projection under `~/.wenlan/pages` already conforms to OKF, but it
//! keeps `[[wikilinks]]`, the `<!-- origin:sources:* -->` delimiters and
//! `_sources/` stubs because Obsidian and the page watcher depend on them.
//! This module writes the pure-OKF twin into a caller-named directory: the
//! same frontmatter (minus `related:`, whose edges the converted body links
//! already carry), standard markdown links, and a fixed
//! `index.md` / `pages/` / `sources/` layout with no Obsidian-only syntax.

use crate::error::WenlanError;
use crate::export::ExportStats;
use crate::pages::Page;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

pub const OKF_VERSION: &str = "0.2";
const MARKER_FILE: &str = ".wenlan-okf-export.json";
const INDEX_FILE: &str = "index.md";
const PAGES_DIR: &str = "pages";
const SOURCES_DIR: &str = "sources";

/// Root marker proving a directory holds a Wenlan OKF export (and listing
/// exactly the bundle files the last export wrote, so a re-export can remove
/// stale ones without touching user files beside them).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ExportMarker {
    okf_version: String,
    generated_by: String,
    files: Vec<String>,
}

fn generated_by() -> String {
    format!("wenlan/{}", crate::version())
}

fn marker_for(files: &HashSet<String>) -> ExportMarker {
    let mut files: Vec<String> = files.iter().cloned().collect();
    files.sort();
    ExportMarker {
        okf_version: OKF_VERSION.to_string(),
        generated_by: generated_by(),
        files,
    }
}

// Matches `obsidian::convert_links_to_wikilinks`' shape: `[Title](concept_id)`,
// plus a capture around the id so the replacer need not re-parse the match.
static CONCEPT_LINK_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\[([^\]]+)\]\((concept_[a-zA-Z0-9\-]+)\)").unwrap());

// Any other `[text](target)` markdown link (URLs, relative paths, ...).
static MD_LINK_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"\[([^\]]+)\]\(([^)\s]+)\)").unwrap());

// Mirrors `sources::obsidian::WIKILINK_RE` (kept local: that static is
// private, and this module only needs the match shape, not the extractor).
static WIKILINK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(!?)\[\[([^\]|#]+)(?:#([^\]|]+))?(?:\|([^\]]+))?\]\]").unwrap()
});

/// True when `entry` names a file this exporter owns: `index.md`,
/// `pages/<name>.md` or `sources/<name>.md` where `<name>` carries no `/`,
/// `\` or `..` and has no leading dot. Anything else in a marker (a forged
/// `../x.md`, an absolute path, a future key) is ignored, never deleted.
pub(crate) fn marker_entry_is_safe(entry: &str) -> bool {
    if entry == INDEX_FILE {
        return true;
    }
    for dir in [PAGES_DIR, SOURCES_DIR] {
        let prefix = format!("{dir}/");
        if let Some(name) = entry.strip_prefix(&prefix) {
            if !name.ends_with(".md") {
                return false;
            }
            let stem = &name[..name.len() - ".md".len()];
            if stem.is_empty() || stem.starts_with('.') {
                return false;
            }
            if stem.contains('/') || stem.contains('\\') || stem.contains("..") {
                return false;
            }
            return true;
        }
    }
    false
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Write `bytes` to `path` without following a pre-existing symlink: a
/// `pages/x.md` that is a symlink is unlinked first so the write cannot
/// clobber whatever the link points at.
fn write_file_nofollow(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if is_symlink(path) {
        std::fs::remove_file(path)?;
    }
    std::fs::write(path, bytes)
}

/// Plan stable bundle filenames: pages sorted by `(created_at, id)`, slug
/// via `obsidian::slugify`, empty slug falling back to
/// `sanitize_stub_id(page.id)`, collisions suffixed `-2`, `-3`, ...
/// Returns `(page_id, file)` pairs in export order.
pub(crate) fn plan_page_filenames(pages: &[Page]) -> Vec<(String, String)> {
    let mut order: Vec<usize> = (0..pages.len()).collect();
    order.sort_by(|&a, &b| {
        pages[a]
            .created_at
            .cmp(&pages[b].created_at)
            .then_with(|| pages[a].id.cmp(&pages[b].id))
    });
    let mut used: HashSet<String> = HashSet::new();
    let mut planned = Vec::with_capacity(pages.len());
    for i in order {
        let page = &pages[i];
        let mut base = crate::export::obsidian::slugify(&page.title);
        if base.is_empty() {
            base = crate::export::provenance::sanitize_stub_id(&page.id);
        }
        let mut candidate = format!("{base}.md");
        let mut n = 2;
        while used.contains(&candidate) {
            candidate = format!("{base}-{n}.md");
            n += 1;
        }
        used.insert(candidate.clone());
        planned.push((page.id.clone(), candidate));
    }
    planned
}

/// Fold every exported page title through `MemoryDB::page_title_key` to its
/// owning page id — `None` when two exported pages fold to the same key
/// (the ambiguous-title contract: such targets stay unresolved).
pub(crate) fn title_owner_map(pages: &[Page]) -> HashMap<String, Option<String>> {
    let mut map: HashMap<String, Option<String>> = HashMap::new();
    for page in pages {
        let key = crate::db::MemoryDB::page_title_key(&page.title);
        match map.get_mut(&key) {
            None => {
                map.insert(key, Some(page.id.clone()));
            }
            Some(slot) => {
                *slot = None;
            }
        }
    }
    map
}

/// Replace regex matches outside fenced code blocks, splicing `replace` over
/// each accepted match. Ranges come from
/// `sources::obsidian::code_block_ranges`, recomputed on the current text per
/// pass (a previous pass shifts byte offsets).
fn replace_outside_code(
    content: &str,
    re: &regex::Regex,
    mut replace: impl FnMut(&regex::Captures<'_>) -> String,
) -> String {
    let ranges = crate::sources::obsidian::code_block_ranges(content);
    let in_code = |offset: usize| ranges.iter().any(|r| r.contains(&offset));
    let mut out = String::with_capacity(content.len());
    let mut last = 0;
    for cap in re.captures_iter(content) {
        let m = cap.get(0).expect("capture group 0 always matches");
        if in_code(m.start()) {
            continue;
        }
        out.push_str(&content[last..m.start()]);
        out.push_str(&replace(&cap));
        last = m.end();
    }
    out.push_str(&content[last..]);
    out
}

/// Convert one page body to bundle links. `id_to_file` maps exported page
/// ids to bundle files, `title_owner` resolves folded titles (see
/// [`title_owner_map`]), and `source_ids` is this page's cited memory ids.
///
/// - `[Text](concept_<id>)`: exported page -> `[Text](/pages/<file>)`,
///   unknown concept id -> plain `Text`. Non-concept `[t](target)` links are
///   already standard markdown: kept when `target` is not an exported page
///   id, rewritten when it is.
/// - `[[Target]]`, `[[Target|Display]]`, `[[Target#Heading]]`, `![[Target]]`:
///   resolved titles -> `[Display or Target](/pages/<file>)` with
///   `#<slugify(heading)>` when a heading is present; unresolved -> plain
///   `Display or Target`. Embeds become ordinary links.
/// - `[[<id>]]` where `<id>` is one of this page's `source_memory_ids` ->
///   `[<id>](/sources/<stub>)`.
/// - Anything inside fenced code blocks is untouched.
pub(crate) fn convert_body(
    content: &str,
    source_ids: &[String],
    id_to_file: &HashMap<String, String>,
    title_owner: &HashMap<String, Option<String>>,
) -> String {
    let cited: HashSet<&str> = source_ids.iter().map(String::as_str).collect();
    // Pass 1: concept links. The dedicated shape first (unknown concept ids
    // collapse to plain text), then any remaining `[t](target)` whose target
    // is an exported page id.
    let pass = replace_outside_code(content, &CONCEPT_LINK_RE, |cap| {
        let (text, id) = (&cap[1], &cap[2]);
        match id_to_file.get(id) {
            Some(file) => format!("[{text}](/pages/{file})"),
            None => text.to_string(),
        }
    });
    let pass = replace_outside_code(&pass, &MD_LINK_RE, |cap| {
        let (text, target) = (&cap[1], &cap[2]);
        // Never rewrite absolute bundle links (including the ones pass 1 just
        // wrote): only bare page ids resolve here.
        if target.starts_with('/') {
            return cap[0].to_string();
        }
        match id_to_file.get(target) {
            Some(file) => format!("[{text}](/pages/{file})"),
            None => cap[0].to_string(),
        }
    });
    // Pass 2: wikilinks (code ranges recomputed on the rewritten text).
    replace_outside_code(&pass, &WIKILINK_RE, |cap| {
        let target = &cap[2];
        let heading = cap.get(3).map(|m| m.as_str());
        let display = cap.get(4).map(|m| m.as_str());
        let is_source_cite =
            heading.is_none() && display.is_none() && cap[1].is_empty() && cited.contains(target);
        if is_source_cite {
            let stub = crate::export::provenance::stub_filename(target);
            return format!("[{target}](/sources/{stub})");
        }
        let key = crate::db::MemoryDB::page_title_key(target);
        let owner = title_owner.get(&key).and_then(|o| o.as_ref());
        match owner.and_then(|id| id_to_file.get(id)) {
            Some(file) => {
                let label = display.unwrap_or(target);
                match heading {
                    Some(h) => {
                        let frag = crate::export::obsidian::slugify(h);
                        format!("[{label}](/pages/{file}#{frag})")
                    }
                    None => format!("[{label}](/pages/{file})"),
                }
            }
            None => display.unwrap_or(target).to_string(),
        }
    })
}

/// One bundle page file: projection frontmatter minus `related:`, plus the
/// converted body and a delimiter-free `## Sources` section (omitted when
/// the page cites nothing).
pub(crate) fn render_page_file(
    page: &Page,
    id_to_file: &HashMap<String, String>,
    title_owner: &HashMap<String, Option<String>>,
) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&crate::export::knowledge::page_frontmatter(page, false));
    out.push_str("---\n\n");
    out.push_str(&convert_body(
        &page.content,
        &page.source_memory_ids,
        id_to_file,
        title_owner,
    ));
    if page.source_memory_ids.is_empty() {
        out.push('\n');
    } else {
        out.push_str("\n\n## Sources\n");
        for id in &page.source_memory_ids {
            let stub = crate::export::provenance::stub_filename(id);
            out.push_str(&format!("- [{id}](/sources/{stub})\n"));
        }
    }
    out
}

/// One bundle source stub: `type: source` frontmatter pointing at the memory,
/// plus a one-sentence body. No memory content, no `origin_stub:` marker.
pub(crate) fn render_source_stub(memory_id: &str) -> String {
    let title = crate::export::provenance::yaml_quoted(memory_id);
    let resource = crate::export::provenance::yaml_quoted(&format!("wenlan://memory/{memory_id}"));
    format!(
        "---\ntype: source\ntitle: {title}\nresource: {resource}\n---\n\n\
         This is a read-only reference to Wenlan memory `{memory_id}`.\n"
    )
}

/// Bundle `index.md`: the same renderer as the projection index, with bundle
/// links (`/pages/<file>`). `files` maps page ids to bundle filenames.
pub(crate) fn render_bundle_index(pages: &[Page], files: &HashMap<String, String>) -> String {
    let rows: Vec<(String, String, String, Option<String>)> = pages
        .iter()
        .filter_map(|page| {
            files.get(&page.id).map(|file| {
                (
                    page.title.clone(),
                    file.clone(),
                    page.summary.clone().unwrap_or_default(),
                    page.space.clone(),
                )
            })
        })
        .collect();
    crate::export::knowledge::render_index_for_entries(rows, "/pages/")
}

/// Read the previous marker's file list. `Ok(None)` means no marker (fresh
/// directory rules apply); an unreadable or unparseable marker is a
/// [`WenlanError::Conflict`] — never guess which files are ours.
fn read_old_marker(root: &Path) -> Result<Option<Vec<String>>, WenlanError> {
    let path = root.join(MARKER_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(WenlanError::Io(e)),
    };
    let marker: ExportMarker = serde_json::from_slice(&bytes).map_err(|_| {
        WenlanError::Conflict(format!(
            "{} exists but is not a Wenlan OKF export marker; refusing to touch {}",
            MARKER_FILE,
            root.display()
        ))
    })?;
    if marker.okf_version != OKF_VERSION {
        return Err(WenlanError::Conflict(format!(
            "{} declares okf_version {:?}; refusing to touch {}",
            MARKER_FILE,
            marker.okf_version,
            root.display()
        )));
    }
    Ok(Some(marker.files))
}

/// Write the OKF bundle for `pages` into `target`.
///
/// The directory must not exist (it is created), be empty, or hold a
/// previous Wenlan OKF export (root marker); anything else is a
/// [`WenlanError::Conflict`]. The root, `pages/` and `sources/` must not be
/// symlinks. Re-export deletes only marker-listed bundle files that are no
/// longer written (validated names only); user files beside them (`.git/`,
/// README, notes) are never touched. The marker is first rewritten with the
/// union of old and planned files, then — after a successful write — with
/// only the new set.
pub fn export_okf(pages: &[Page], target: &Path) -> Result<ExportStats, WenlanError> {
    let mut stats = ExportStats::default();
    for dir in [target, &target.join(PAGES_DIR), &target.join(SOURCES_DIR)] {
        if is_symlink(dir) {
            return Err(WenlanError::Conflict(format!(
                "refusing OKF export through symlink {}",
                dir.display()
            )));
        }
    }
    if target.is_file() {
        return Err(WenlanError::Conflict(format!(
            "OKF export target {} is a file, not a directory",
            target.display()
        )));
    }
    let old_files = read_old_marker(target)?;
    if old_files.is_none() && target.exists() {
        // Fail closed: a directory we cannot inspect is not "empty", and
        // writing into it would risk mixing the bundle with unknown files.
        let mut entries = std::fs::read_dir(target).map_err(WenlanError::Io)?;
        if entries.next().is_some() {
            return Err(WenlanError::Conflict(
                "directory is not empty and is not a Wenlan OKF export".to_string(),
            ));
        }
    }
    let old_files: HashSet<String> = old_files.unwrap_or_default().into_iter().collect();

    let planned_pairs = plan_page_filenames(pages);
    let id_to_file: HashMap<String, String> = planned_pairs.iter().cloned().collect();
    let title_owner = title_owner_map(pages);
    let by_id: HashMap<&str, &Page> = pages.iter().map(|p| (p.id.as_str(), p)).collect();

    let mut planned: HashSet<String> = HashSet::new();
    planned.insert(INDEX_FILE.to_string());
    for (_, file) in &planned_pairs {
        planned.insert(format!("{PAGES_DIR}/{file}"));
    }
    let mut stub_ids: Vec<&String> = pages
        .iter()
        .flat_map(|p| p.source_memory_ids.iter())
        .collect();
    stub_ids.sort();
    stub_ids.dedup();
    for id in &stub_ids {
        planned.insert(format!(
            "{SOURCES_DIR}/{}",
            crate::export::provenance::stub_filename(id)
        ));
    }

    std::fs::create_dir_all(target.join(PAGES_DIR))?;
    std::fs::create_dir_all(target.join(SOURCES_DIR))?;
    // Crash-safety: the union marker owns every file either generation may
    // have left behind, so a failed run stays re-collectable.
    let union: HashSet<String> = old_files.union(&planned).cloned().collect();
    write_file_nofollow(
        &target.join(MARKER_FILE),
        serde_json::to_vec_pretty(&marker_for(&union))
            .expect("export marker serializes")
            .as_slice(),
    )?;

    // Stale files: marker-listed, validated, and no longer planned.
    let mut stale: Vec<&String> = old_files.difference(&planned).collect();
    stale.sort();
    for entry in stale {
        if !marker_entry_is_safe(entry) {
            continue;
        }
        let path = target.join(entry);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(WenlanError::Io(e)),
        }
    }

    for (id, file) in &planned_pairs {
        let page = match by_id.get(id.as_str()) {
            Some(page) => page,
            None => continue,
        };
        let body = render_page_file(page, &id_to_file, &title_owner);
        match write_file_nofollow(&target.join(PAGES_DIR).join(file), body.as_bytes()) {
            Ok(()) => stats.exported += 1,
            Err(e) => {
                log::warn!("[okf] export failed for '{}': {}", page.title, e);
                stats.failed += 1;
            }
        }
    }
    for id in &stub_ids {
        let path = target
            .join(SOURCES_DIR)
            .join(crate::export::provenance::stub_filename(id));
        if let Err(e) = write_file_nofollow(path.as_path(), render_source_stub(id).as_bytes()) {
            log::warn!("[okf] source stub failed for '{id}': {e}");
            stats.failed += 1;
        }
    }
    let index = render_bundle_index(pages, &id_to_file);
    write_file_nofollow(&target.join(INDEX_FILE), index.as_bytes())?;
    write_file_nofollow(
        &target.join(MARKER_FILE),
        serde_json::to_vec_pretty(&marker_for(&planned))
            .expect("export marker serializes")
            .as_slice(),
    )?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_page(id: &str, title: &str, content: &str) -> Page {
        Page {
            id: id.to_string(),
            title: title.to_string(),
            summary: Some(format!("{title} summary")),
            content: content.to_string(),
            entity_id: None,
            space: Some("test-space".to_string()),
            source_memory_ids: vec![],
            version: 1,
            status: "active".to_string(),
            created_at: "2026-09-01T00:00:00+00:00".to_string(),
            last_compiled: "2026-09-02T00:00:00+00:00".to_string(),
            last_modified: "2026-09-02T00:00:00+00:00".to_string(),
            sources_updated_count: 0,
            stale_reason: None,
            pending_rebuild: None,
            refresh_blocked_reason: None,
            user_edited: false,
            relevance_score: 0.0,
            last_edited_by: None,
            last_edited_at: None,
            last_delta_summary: None,
            changelog: None,
            workspace: None,
            creation_kind: "distilled".to_string(),
            review_status: "confirmed".to_string(),
            citations: Vec::new(),
            kind: "concept".to_string(),
            truth: None,
        }
    }

    fn link_context(pages: &[Page]) -> (HashMap<String, String>, HashMap<String, Option<String>>) {
        let id_to_file: HashMap<String, String> = plan_page_filenames(pages).into_iter().collect();
        let title_owner = title_owner_map(pages);
        (id_to_file, title_owner)
    }

    fn convert(pages: &[Page], page: &Page) -> String {
        let (id_to_file, title_owner) = link_context(pages);
        convert_body(
            &page.content,
            &page.source_memory_ids,
            &id_to_file,
            &title_owner,
        )
    }

    #[test]
    fn concept_link_resolved_to_bundle_path_and_unknown_collapses() {
        let pages = vec![
            test_page("concept_a", "Alpha", "see [Beta](concept_b)"),
            test_page("concept_b", "Beta", "body"),
        ];
        let body = convert(&pages, &pages[0]);
        assert!(body.contains("[Beta](/pages/beta.md)"), "{body}");
        let ghost = test_page("concept_g", "Ghost", "see [Nobody](concept_nobody9)");
        let pages_and_ghost: Vec<Page> = pages
            .iter()
            .cloned()
            .chain(std::iter::once(ghost.clone()))
            .collect();
        let body = convert(&pages_and_ghost, &ghost);
        assert!(body.contains("see Nobody"), "{body}");
        assert!(!body.contains("concept_nobody9"), "{body}");
    }

    #[test]
    fn wikilink_display_heading_and_embed_forms() {
        let pages = [
            test_page("concept_a", "Alpha", "x"),
            test_page("concept_b", "Target Page", "body"),
        ];
        let probe = test_page(
            "concept_probe",
            "Probe",
            "a [[Target Page]] b [[Target Page|Shown]] c [[Target Page#My Heading]] d ![[Target Page]]",
        );
        let all: Vec<Page> = pages
            .iter()
            .cloned()
            .chain(std::iter::once(probe.clone()))
            .collect();
        let body = convert(&all, &probe);
        assert!(
            body.contains("[Target Page](/pages/target-page.md)"),
            "{body}"
        );
        assert!(body.contains("[Shown](/pages/target-page.md)"), "{body}");
        assert!(
            body.contains("[Target Page](/pages/target-page.md#my-heading)"),
            "{body}"
        );
        // Embeds become ordinary links; no `![[` survives.
        assert!(!body.contains("![["), "{body}");
        assert!(!body.contains("[["), "{body}");
    }

    #[test]
    fn ambiguous_title_stays_plain_text() {
        let pages = [
            test_page("concept_a", "Shared Name", "x"),
            test_page("concept_b", "shared name", "y"),
        ];
        let probe = test_page(
            "concept_probe",
            "Probe",
            "see [[Shared Name|here]] and [[Shared Name]]",
        );
        let all: Vec<Page> = pages
            .iter()
            .cloned()
            .chain(std::iter::once(probe.clone()))
            .collect();
        let body = convert(&all, &probe);
        assert!(body.contains("see here and Shared Name"), "{body}");
        assert!(!body.contains("/pages/"), "{body}");
    }

    #[test]
    fn unresolved_wikilink_keeps_display_only() {
        let pages = [test_page("concept_a", "Alpha", "x")];
        let probe = test_page("concept_probe", "Probe", "[[Nobody]] and [[Nobody|Nick]]");
        let all: Vec<Page> = pages
            .iter()
            .cloned()
            .chain(std::iter::once(probe.clone()))
            .collect();
        let body = convert(&all, &probe);
        assert_eq!(body, "Nobody and Nick");
    }

    #[test]
    fn source_cite_links_to_stub_and_sources_section_has_no_delimiters() {
        let mut page = test_page("concept_a", "Alpha", "grounded in [[mem_1]] and [[Beta]]");
        page.source_memory_ids = vec!["mem_1".to_string()];
        let other = test_page("concept_b", "Beta", "body");
        let pages = vec![page.clone(), other];
        let (id_to_file, title_owner) = link_context(&pages);
        let file = render_page_file(&page, &id_to_file, &title_owner);
        assert!(file.contains("[mem_1](/sources/mem_1.md)"), "{file}");
        assert!(file.contains("[Beta](/pages/beta.md)"), "{file}");
        assert!(
            file.contains("## Sources\n- [mem_1](/sources/mem_1.md)\n"),
            "{file}"
        );
        assert!(!file.contains("origin:sources"), "{file}");
        assert!(!file.contains("[["), "{file}");
    }

    #[test]
    fn source_less_page_has_no_sources_section() {
        let pages = vec![test_page("concept_a", "Alpha", "plain body")];
        let (id_to_file, title_owner) = link_context(&pages);
        let file = render_page_file(&pages[0], &id_to_file, &title_owner);
        assert!(!file.contains("## Sources"), "{file}");
        assert!(file.ends_with("plain body\n"), "{file}");
    }

    #[test]
    fn fenced_code_blocks_are_untouched() {
        let pages = [
            test_page("concept_a", "Alpha", "x"),
            test_page("concept_b", "Target Page", "body"),
        ];
        let fence = "```md\n[Beta](concept_b) and [[Target Page]]\n```";
        let probe = test_page(
            "concept_probe",
            "Probe",
            &format!("before\n{fence}\nafter [[Target Page]]"),
        );
        let all: Vec<Page> = pages
            .iter()
            .cloned()
            .chain(std::iter::once(probe.clone()))
            .collect();
        let body = convert(&all, &probe);
        assert!(body.contains(fence), "{body}");
        assert!(
            body.contains("after [Target Page](/pages/target-page.md)"),
            "{body}"
        );
    }

    #[test]
    fn slug_collision_gets_numeric_suffix() {
        let pages = vec![
            test_page("concept_a", "Same Name", "x"),
            test_page("concept_b", "Same Name", "y"),
            test_page("concept_c", "Same Name", "z"),
        ];
        let planned = plan_page_filenames(&pages);
        let mut files: Vec<&str> = planned.iter().map(|(_, f)| f.as_str()).collect();
        files.sort();
        assert_eq!(
            files,
            vec!["same-name-2.md", "same-name-3.md", "same-name.md"]
        );
    }

    #[test]
    fn empty_slug_falls_back_to_sanitized_id() {
        let pages = vec![test_page("concept_a!!", "!!!", "x")];
        let planned = plan_page_filenames(&pages);
        assert_eq!(planned.len(), 1);
        let expected = format!(
            "{}.md",
            crate::export::provenance::sanitize_stub_id("concept_a!!")
        );
        assert_eq!(planned[0].1, expected);
    }

    #[test]
    fn page_frontmatter_matches_projection_minus_related() {
        let mut page = test_page("concept_a", "Alpha", "see [[Beta]]");
        page.source_memory_ids = vec!["mem_1".to_string()];
        let projection = crate::export::knowledge::page_frontmatter(&page, true);
        let bundle = crate::export::knowledge::page_frontmatter(&page, false);
        assert!(projection.contains("related:"), "{projection}");
        assert!(!bundle.contains("related:"), "{bundle}");
        let projection_without_related: String = projection
            .lines()
            .filter(|line| !line.starts_with("related:"))
            .map(|line| format!("{line}\n"))
            .collect();
        assert_eq!(projection_without_related, bundle);
    }

    #[test]
    fn rendered_page_file_opens_with_shared_frontmatter() {
        let pages = vec![test_page("concept_a", "Alpha", "body")];
        let (id_to_file, title_owner) = link_context(&pages);
        let file = render_page_file(&pages[0], &id_to_file, &title_owner);
        let expected_head = format!(
            "---\n{}---\n\n",
            crate::export::knowledge::page_frontmatter(&pages[0], false)
        );
        assert!(file.starts_with(&expected_head), "{file}");
    }

    #[test]
    fn source_stub_frontmatter_points_at_memory() {
        let stub = render_source_stub("mem_1");
        assert!(stub.contains("type: source"), "{stub}");
        assert!(stub.contains("title: \"mem_1\""), "{stub}");
        assert!(
            stub.contains("resource: \"wenlan://memory/mem_1\""),
            "{stub}"
        );
        assert!(!stub.contains("origin_stub"), "{stub}");
        // One sentence, no memory content beyond the id.
        assert!(stub.matches('\n').count() >= 6, "{stub}");
        assert!(!stub.contains("origin:"), "{stub}");
    }

    #[test]
    fn bundle_index_uses_pages_prefix_and_groups_unfiled_last() {
        let mut filed = test_page("concept_a", "Zulu", "x");
        filed.space = Some("b-space".to_string());
        let mut unfiled = test_page("concept_b", "Alpha", "y");
        unfiled.space = None;
        let pages = vec![filed, unfiled];
        let id_to_file: HashMap<String, String> = plan_page_filenames(&pages).into_iter().collect();
        let index = render_bundle_index(&pages, &id_to_file);
        assert!(
            index.starts_with("---\nokf_version: \"0.2\"\n---\n\n"),
            "{index}"
        );
        assert!(index.contains("[Zulu](/pages/zulu.md)"), "{index}");
        assert!(index.contains("[Alpha](/pages/alpha.md)"), "{index}");
        assert!(!index.contains("](/zulu.md)"), "{index}");
        let unfiled_pos = index.find("## Unfiled").expect("Unfiled section");
        let space_pos = index.find("## b-space").expect("space section");
        assert!(space_pos < unfiled_pos, "{index}");
    }

    #[test]
    fn refuses_foreign_nonempty_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("README.md"), b"someone else").unwrap();
        let pages = vec![test_page("concept_a", "Alpha", "x")];
        let err = export_okf(&pages, dir.path()).unwrap_err();
        assert!(
            err.to_string()
                .contains("directory is not empty and is not a Wenlan OKF export"),
            "{err}"
        );
    }

    #[test]
    fn corrupt_marker_is_a_conflict() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(MARKER_FILE), b"{not json").unwrap();
        let pages = vec![test_page("concept_a", "Alpha", "x")];
        let err = export_okf(&pages, dir.path()).unwrap_err();
        assert!(matches!(err, WenlanError::Conflict(_)), "{err}");
    }

    #[test]
    fn reexport_keeps_user_files_and_removes_stale_pages() {
        let dir = tempfile::TempDir::new().unwrap();
        let both = vec![
            test_page("concept_a", "Alpha", "x"),
            test_page("concept_b", "Beta", "y"),
        ];
        let stats = export_okf(&both, dir.path()).unwrap();
        assert_eq!(stats.exported, 2);
        // User files beside the bundle (never in the marker).
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("README.md"), b"mine").unwrap();
        assert!(dir.path().join("pages/beta.md").exists());

        let just_a = vec![test_page("concept_a", "Alpha", "x")];
        let stats = export_okf(&just_a, dir.path()).unwrap();
        assert_eq!(stats.exported, 1);
        assert!(
            !dir.path().join("pages/beta.md").exists(),
            "stale page removed"
        );
        assert!(dir.path().join("pages/alpha.md").exists());
        assert!(dir.path().join("README.md").exists(), "user file kept");
        assert!(dir.path().join(".git").is_dir(), ".git kept");
        let marker: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join(MARKER_FILE)).unwrap()).unwrap();
        let files: HashSet<String> = marker["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(!files.iter().any(|f| f.contains("beta")), "{files:?}");
        assert!(files.contains("index.md"), "{files:?}");
        assert!(files.contains("pages/alpha.md"), "{files:?}");
    }

    #[test]
    fn malicious_marker_entries_delete_nothing_outside() {
        let outer = tempfile::TempDir::new().unwrap();
        let root = outer.path().join("bundle");
        std::fs::create_dir_all(root.join("pages")).unwrap();
        std::fs::write(outer.path().join("evil.md"), b"precious").unwrap();
        std::fs::write(root.join("pages").join("keep.md"), b"user note").unwrap();
        let marker = serde_json::json!({
            "okf_version": "0.2",
            "generated_by": "wenlan/test",
            "files": ["../evil.md", "pages/../../evil.md", "pages/.hidden.md", "index.md", "/abs.md"],
        });
        std::fs::write(root.join(MARKER_FILE), serde_json::to_vec(&marker).unwrap()).unwrap();
        let stats = export_okf(&[], root.as_path()).unwrap();
        assert_eq!(stats.exported, 0);
        assert!(
            outer.path().join("evil.md").exists(),
            "outside file untouched"
        );
        assert!(
            root.join("pages").join("keep.md").exists(),
            "unlisted file kept"
        );
    }

    #[test]
    fn marker_entry_validation_rejects_escapes() {
        assert!(marker_entry_is_safe("index.md"));
        assert!(marker_entry_is_safe("pages/alpha.md"));
        assert!(marker_entry_is_safe("sources/mem_1.md"));
        for evil in [
            "../x.md",
            "pages/../../x.md",
            "pages/.hidden.md",
            ".hidden.md",
            "pages/a/b.md",
            "pages/a\\b.md",
            "pages/a..b.md",
            "pages/.md",
            "pages/x.txt",
            "other/x.md",
            "",
        ] {
            assert!(!marker_entry_is_safe(evil), "{evil} must be rejected");
        }
    }

    #[test]
    fn bundle_conforms_okf_shape() {
        let mut a = test_page(
            "concept_a",
            "Alpha",
            "see [[Beta]]\n\n```md\n[[untouched]]\n```",
        );
        a.source_memory_ids = vec!["mem_1".to_string()];
        let mut b = test_page("concept_b", "Beta", "plain");
        b.space = None;
        b.summary = None;
        let pages = vec![a, b];
        let dir = tempfile::TempDir::new().unwrap();
        export_okf(&pages, dir.path()).unwrap();

        let mut md_files = Vec::new();
        collect_md(dir.path(), &mut md_files);
        assert!(md_files.len() >= 4, "{md_files:?}");
        for path in &md_files {
            let rel = path.strip_prefix(dir.path()).unwrap();
            let content = std::fs::read_to_string(path).unwrap();
            let (fm, _body) = crate::sources::obsidian::extract_frontmatter(&content);
            if rel == Path::new("index.md") {
                let keys: HashSet<String> = fm.fields.keys().cloned().collect();
                assert_eq!(
                    keys,
                    HashSet::from(["okf_version".to_string()]),
                    "{content}"
                );
                assert_eq!(fm.get_str("okf_version"), Some("0.2"));
            } else {
                let ty = fm.get_str("type").unwrap_or("");
                assert!(!ty.is_empty(), "{rel:?} needs a non-empty type:\n{content}");
            }
            assert!(
                !outside_code_contains(&content, "[["),
                "{rel:?} leaks wikilinks"
            );
            assert!(
                !outside_code_contains(&content, "<!-- origin:"),
                "{rel:?} leaks delimiters"
            );
        }
    }

    fn collect_md(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_md(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                out.push(path);
            }
        }
    }

    fn outside_code_contains(content: &str, needle: &str) -> bool {
        let ranges = crate::sources::obsidian::code_block_ranges(content);
        let mut visible = String::new();
        let mut last = 0;
        for r in &ranges {
            visible.push_str(&content[last..r.start]);
            last = r.end;
        }
        visible.push_str(&content[last..]);
        visible.contains(needle)
    }
}
