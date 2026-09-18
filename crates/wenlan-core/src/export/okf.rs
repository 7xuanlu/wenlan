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
use std::ops::Range;
use std::path::Path;
use std::sync::LazyLock;

pub const OKF_VERSION: &str = "0.2";
pub(crate) const MARKER_FILE: &str = ".wenlan-okf-export.json";
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

// Dedicated page-id links: `[Title](concept_<id>)` (the shape
// `obsidian::convert_links_to_wikilinks` reads) and `[Title](page_<id>)`
// (current page ids are minted as `page_<uuid>`), plus a capture around the
// id so the replacer need not re-parse the match.
static ID_LINK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\[([^\]]+)\]\(((?:concept|page)_[a-zA-Z0-9\-]+)\)").unwrap()
});

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
/// `index.md` and `log.md` are never a page's file: OKF reserves both at
/// every level of the hierarchy (spec 3.1), so a page titled "Index" is
/// `index-page.md` and one titled "Log" is `log-page.md`.
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
        let base = crate::export::knowledge::page_stem_clear_of_reserved(base);
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

/// `(space, folded title)` -> owning page id, or `None` when ambiguous.
pub(crate) type TitleOwners = HashMap<(Option<String>, String), Option<String>>;

/// Fold every exported page title through `MemoryDB::page_title_key` to its
/// owning page id, resolved per Space like
/// `MemoryDB::folded_title_owners_scoped` (`None` = uncategorized): the key
/// is `(space, folded title)`, and the value is `None` when two exported
/// pages in the same Space fold to the same title (the ambiguous-title
/// contract, scoped: such targets stay unresolved).
pub(crate) fn title_owner_map(pages: &[Page]) -> TitleOwners {
    let mut map: TitleOwners = HashMap::new();
    for page in pages {
        let key = (
            page.space.clone(),
            crate::db::MemoryDB::page_title_key(&page.title),
        );
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

/// Byte ranges of inline code spans outside the fenced ranges, using
/// CommonMark backtick runs: a run of N backticks opens a span that ends at
/// the next run of exactly N backticks in the same paragraph (a span never
/// crosses a blank line or a fenced code block). A run with no matching
/// closer is literal text and suppresses nothing after it.
pub(crate) fn inline_code_ranges(content: &str, fenced: &[Range<usize>]) -> Vec<Range<usize>> {
    // Byte offsets where a paragraph ends: each blank (whitespace-only) line
    // and each fenced block. A run's paragraph is the number of breaks
    // before it.
    let mut block_breaks: Vec<usize> = fenced.iter().map(|r| r.start).collect();
    let mut line_start = 0;
    for line in content.split_inclusive('\n') {
        if line.trim().is_empty() {
            block_breaks.push(line_start);
        }
        line_start += line.len();
    }
    block_breaks.sort_unstable();
    let paragraph = |offset: usize| block_breaks.partition_point(|&b| b < offset);
    let bytes = content.as_bytes();
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        if !fenced.iter().any(|r| r.contains(&start)) {
            runs.push((start, i - start, paragraph(start)));
        }
    }
    let mut ranges = Vec::new();
    let mut k = 0;
    while k < runs.len() {
        let (start, len, para) = runs[k];
        match runs[k + 1..]
            .iter()
            .position(|&(_, l, p)| l == len && p == para)
        {
            Some(offset) => {
                let (close_start, close_len, _) = runs[k + 1 + offset];
                ranges.push(start..close_start + close_len);
                k += 2 + offset;
            }
            // Unmatched: literal text, keep scanning after the run.
            None => k += 1,
        }
    }
    ranges
}

/// Replace regex matches outside code, splicing `replace` over each accepted
/// match. Fenced code-block ranges come from
/// `sources::obsidian::code_block_ranges`; inline code spans are scanned on
/// top (see [`inline_code_ranges`]). Both are recomputed on the current text
/// per pass (a previous pass shifts byte offsets).
fn replace_outside_code(
    content: &str,
    re: &regex::Regex,
    mut replace: impl FnMut(&regex::Captures<'_>) -> String,
) -> String {
    let fenced = crate::sources::obsidian::code_block_ranges(content);
    let inline = inline_code_ranges(content, &fenced);
    let in_code = |offset: usize| {
        fenced.iter().any(|r| r.contains(&offset)) || inline.iter().any(|r| r.contains(&offset))
    };
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

/// Convert one page body to bundle links. `page_space` is the linking page's
/// Space: `[[Target]]` resolves only against exported pages in the same Space
/// (see [`title_owner_map`]). `id_to_file` maps exported page ids to bundle
/// files, and `source_ids` is this page's cited memory ids.
///
/// - `[Text](concept_<id>)` / `[Text](page_<id>)`: exported page ->
///   `[Text](/pages/<file>)`, unknown id -> plain `Text`. Other
///   `[t](target)` links are already standard markdown: kept when `target`
///   is not an exported page id, rewritten when it is.
/// - `[[Target]]`, `[[Target|Display]]`, `[[Target#Heading]]`, `![[Target]]`:
///   resolved titles -> `[Display or Target](/pages/<file>)` with
///   `#<slugify(heading)>` when a heading is present; unresolved -> plain
///   `Display or Target`. Embeds become ordinary links.
/// - `[[<id>]]` where `<id>` is one of this page's `source_memory_ids` ->
///   `[<id>](/sources/<stub>)`.
/// - Every emitted `[label](...)` passes `label` through
///   `escape_index_link_text` so a bracket in page content cannot break the
///   link; plain-text fallbacks stay unescaped.
/// - Anything inside fenced code blocks or inline code spans is untouched.
pub(crate) fn convert_body(
    content: &str,
    page_space: Option<&str>,
    source_ids: &[String],
    id_to_file: &HashMap<String, String>,
    title_owner: &TitleOwners,
) -> String {
    use crate::export::knowledge::escape_index_link_text;
    let cited: HashSet<&str> = source_ids.iter().map(String::as_str).collect();
    // Pass 1: page-id links. The dedicated shape first (unknown ids collapse
    // to plain text), then any remaining `[t](target)` whose target is an
    // exported page id.
    let pass = replace_outside_code(content, &ID_LINK_RE, |cap| {
        let (text, id) = (&cap[1], &cap[2]);
        match id_to_file.get(id) {
            Some(file) => {
                let label = escape_index_link_text(text);
                format!("[{label}](/pages/{file})")
            }
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
            Some(file) => {
                let label = escape_index_link_text(text);
                format!("[{label}](/pages/{file})")
            }
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
            let label = escape_index_link_text(target);
            return format!("[{label}](/sources/{stub})");
        }
        let key = (
            page_space.map(|space| space.to_owned()),
            crate::db::MemoryDB::page_title_key(target),
        );
        let owner = title_owner.get(&key).and_then(|o| o.as_ref());
        match owner.and_then(|id| id_to_file.get(id)) {
            Some(file) => {
                let label = escape_index_link_text(display.unwrap_or(target));
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
    title_owner: &TitleOwners,
) -> String {
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&crate::export::knowledge::page_frontmatter(page, false));
    out.push_str("---\n\n");
    out.push_str(&convert_body(
        &page.content,
        page.space.as_deref(),
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
            let label = crate::export::knowledge::escape_index_link_text(id);
            out.push_str(&format!("- [{label}](/sources/{stub})\n"));
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
    if !marker.generated_by.starts_with("wenlan/") {
        return Err(WenlanError::Conflict(format!(
            "{} was not written by Wenlan (generated_by {:?}); refusing to touch {}",
            MARKER_FILE,
            marker.generated_by,
            root.display()
        )));
    }
    Ok(Some(marker.files))
}

/// Write the OKF bundle for `pages` into `target`.
///
/// The directory must not exist (it is created), be empty, or hold a
/// previous Wenlan OKF export (root marker naming Wenlan as its writer);
/// anything else is a [`WenlanError::Conflict`]. The root, `pages/` and
/// `sources/` must not be symlinks. A planned path that already exists
/// without being marker-listed is a Conflict — a foreign file is never
/// overwritten. Re-export deletes only marker-listed bundle files that are
/// no longer written (validated names only); user files beside them (`.git/`,
/// README, notes) are never touched. `index.md` links only pages actually
/// written. The marker is first rewritten with the union of old and planned
/// files, then — after a successful write — with only the new set.
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

    // Never overwrite a file Wenlan did not write: every planned path must
    // be absent or owned by the previous export. Sorted for a deterministic
    // message. Runs before any write, so a refusal leaves the tree untouched.
    let mut planned_sorted: Vec<&String> = planned.iter().collect();
    planned_sorted.sort();
    for entry in planned_sorted {
        if !old_files.contains(entry) && std::fs::symlink_metadata(target.join(entry)).is_ok() {
            return Err(WenlanError::Conflict(format!(
                "refusing to overwrite {entry}: not written by a previous Wenlan OKF export"
            )));
        }
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

    let mut written_ids: HashSet<&str> = HashSet::new();
    for (id, file) in &planned_pairs {
        let page = match by_id.get(id.as_str()) {
            Some(page) => page,
            None => continue,
        };
        let body = render_page_file(page, &id_to_file, &title_owner);
        match write_file_nofollow(&target.join(PAGES_DIR).join(file), body.as_bytes()) {
            Ok(()) => {
                stats.exported += 1;
                written_ids.insert(id.as_str());
            }
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
    // The index lists only pages actually written: a failed page file must
    // not be linked. The final marker still lists every planned file so a
    // later run can clean up.
    let written_pages: Vec<Page> = pages
        .iter()
        .filter(|page| written_ids.contains(page.id.as_str()))
        .cloned()
        .collect();
    let index = render_bundle_index(&written_pages, &id_to_file);
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

    fn test_page_in_space(id: &str, title: &str, content: &str, space: Option<&str>) -> Page {
        let mut page = test_page(id, title, content);
        page.space = space.map(|space| space.to_owned());
        page
    }

    fn link_context(pages: &[Page]) -> (HashMap<String, String>, TitleOwners) {
        let id_to_file: HashMap<String, String> = plan_page_filenames(pages).into_iter().collect();
        let title_owner = title_owner_map(pages);
        (id_to_file, title_owner)
    }

    fn convert(pages: &[Page], page: &Page) -> String {
        let (id_to_file, title_owner) = link_context(pages);
        convert_body(
            &page.content,
            page.space.as_deref(),
            &page.source_memory_ids,
            &id_to_file,
            &title_owner,
        )
    }

    fn convert_all(pages: &[Page], probe: &Page) -> String {
        let all: Vec<Page> = pages
            .iter()
            .cloned()
            .chain(std::iter::once(probe.clone()))
            .collect();
        convert(&all, probe)
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

    /// The bundle side of the reserved-name rule: `log.md` is a directory's
    /// update history in OKF (spec 3.1), never a concept document.
    #[test]
    fn a_page_titled_log_gets_log_page_in_the_bundle() {
        let pages = vec![
            test_page("concept_log", "Log", "x"),
            test_page("concept_log_page", "Log Page", "y"),
        ];
        let planned: Vec<String> = plan_page_filenames(&pages)
            .into_iter()
            .map(|(_, file)| file)
            .collect();
        assert_eq!(planned, vec!["log-page.md", "log-page-2.md"], "{planned:?}");
    }

    #[test]
    fn a_page_titled_index_and_one_titled_index_page_get_distinct_files() {
        let pages = vec![
            test_page("concept_a", "Index", "x"),
            test_page("concept_b", "Index Page", "y"),
            test_page("concept_c", "INDEX", "z"),
        ];
        let planned = plan_page_filenames(&pages);
        let files: Vec<&str> = planned.iter().map(|(_, f)| f.as_str()).collect();
        assert_eq!(
            files,
            vec!["index-page.md", "index-page-2.md", "index-page-3.md"]
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

    /// Same rule in the exported bundle: no summary, no ` - ` tail. The
    /// renderer is shared with the projection index, so this pins the bundle
    /// side of it against a future prefix-only change.
    #[test]
    fn bundle_index_line_for_a_page_without_a_summary_has_no_trailing_separator() {
        let described = test_page("concept_a", "Described", "x");
        let mut bare = test_page("concept_b", "Bare", "y");
        bare.summary = None;
        let pages = vec![described, bare];
        let id_to_file: HashMap<String, String> = plan_page_filenames(&pages).into_iter().collect();
        let index = render_bundle_index(&pages, &id_to_file);
        assert!(
            index.contains("* [Described](/pages/described.md) - Described summary\n"),
            "{index}"
        );
        assert!(index.contains("* [Bare](/pages/bare.md)\n"), "{index}");
        assert!(!index.contains("(/pages/bare.md) - "), "{index}");
    }

    /// `export_okf` never reports a skip of its own. The OKF export route
    /// (`wenlan-server/src/page_routes.rs`, `handle_export_pages`) filters the
    /// page list by `page_write_permit` BEFORE calling this, then overwrites
    /// `stats.skipped` with its own declined count -- so a skip counted here
    /// would be silently thrown away. Re-export, where stale files are removed,
    /// is the case most likely to want one.
    #[test]
    fn export_reports_no_skips_of_its_own_so_the_route_owns_that_count() {
        let dir = tempfile::TempDir::new().unwrap();
        let both = vec![
            test_page("concept_a", "Alpha", "x"),
            test_page("concept_b", "Beta", "y"),
        ];
        let first = export_okf(&both, dir.path()).unwrap();
        assert_eq!(first.exported, 2, "{first:?}");
        assert_eq!(first.skipped, 0, "{first:?}");

        // Second pass over the same directory with one page gone: the stale
        // file is removed, and that is still not a "skip".
        let second = export_okf(&both[..1], dir.path()).unwrap();
        assert_eq!(second.exported, 1, "{second:?}");
        assert_eq!(second.skipped, 0, "{second:?}");
        assert_eq!(second.failed, 0, "{second:?}");
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

    #[test]
    fn wikilinks_resolve_within_the_linking_page_space() {
        let alpha_a = test_page_in_space("page_alpha_a", "Alpha", "A", Some("a"));
        let alpha_b = test_page_in_space("page_alpha_b", "Alpha", "B", Some("b"));
        let beta_a = test_page_in_space("page_beta_a", "Beta", "C", Some("a"));
        let linker_a = test_page_in_space("page_linker_a", "Linker A", "see [[Alpha]]", Some("a"));
        let linker_b = test_page_in_space(
            "page_linker_b",
            "Linker B",
            "see [[Alpha]] and [[Beta]]",
            Some("b"),
        );
        let pages = vec![alpha_a, alpha_b, beta_a, linker_a.clone(), linker_b.clone()];
        // Globally "Alpha" is ambiguous, but per Space each linker sees one.
        let body_a = convert(&pages, &linker_a);
        assert!(body_a.contains("[Alpha](/pages/alpha.md)"), "{body_a}");
        let body_b = convert(&pages, &linker_b);
        assert!(body_b.contains("[Alpha](/pages/alpha-2.md)"), "{body_b}");
        // Beta lives only in space `a`, so the space-`b` link stays plain.
        assert!(body_b.contains("and Beta"), "{body_b}");
        assert!(!body_b.contains("/pages/beta.md"), "{body_b}");
    }

    #[test]
    fn page_id_links_resolve_and_unknown_collapse() {
        let target_id = "page_12345678-1234-4000-8000-1234567890ab";
        let pages = vec![
            test_page("concept_a", "Alpha", "x"),
            test_page(target_id, "Known", "body"),
        ];
        let probe = test_page(
            "concept_probe",
            "Probe",
            &format!("[Known]({target_id}) and [Gone](page_00000000-0000-4000-8000-00000000dead)"),
        );
        let body = convert_all(&pages, &probe);
        assert!(body.contains("[Known](/pages/known.md)"), "{body}");
        assert!(body.contains("and Gone"), "{body}");
        assert!(!body.contains("page_00000000"), "{body}");
    }

    #[test]
    fn inline_code_spans_are_untouched() {
        let pages = [
            test_page("concept_a", "Alpha", "x"),
            test_page("concept_b", "Target Page", "body"),
        ];
        let probe = test_page(
            "concept_probe",
            "Probe",
            "use `[[Target Page]]` verbatim and see [[Target Page]]",
        );
        let body = convert_all(&pages, &probe);
        assert!(body.contains("use `[[Target Page]]` verbatim"), "{body}");
        assert!(
            body.contains("see [Target Page](/pages/target-page.md)"),
            "{body}"
        );
    }

    #[test]
    fn double_backtick_span_with_inner_backtick_is_untouched() {
        let pages = [test_page("concept_b", "Target Page", "body")];
        let probe = test_page(
            "concept_probe",
            "Probe",
            "``a ` [[Target Page]]`` then [[Target Page]]",
        );
        let body = convert_all(&pages, &probe);
        assert!(body.contains("``a ` [[Target Page]]``"), "{body}");
        assert!(
            body.contains("then [Target Page](/pages/target-page.md)"),
            "{body}"
        );
    }

    #[test]
    fn inline_code_span_does_not_cross_a_blank_line() {
        let pages = [test_page("concept_b", "Target Page", "body")];
        let probe = test_page(
            "concept_probe",
            "Probe",
            "a ` stray\n\nsee [[Target Page]]\n\nanother ` stray",
        );
        let body = convert_all(&pages, &probe);
        assert!(
            body.contains("see [Target Page](/pages/target-page.md)"),
            "{body}"
        );
    }

    #[test]
    fn inline_code_span_does_not_cross_a_fenced_block() {
        let pages = [test_page("concept_b", "Target Page", "body")];
        let probe = test_page(
            "concept_probe",
            "Probe",
            "a ` stray\n```\ncode\n```\nsee [[Target Page]] and ` stray",
        );
        let body = convert_all(&pages, &probe);
        assert!(
            body.contains("see [Target Page](/pages/target-page.md)"),
            "{body}"
        );
    }

    #[test]
    fn unmatched_backtick_does_not_suppress_later_links() {
        let pages = [test_page("concept_b", "Target Page", "body")];
        let probe = test_page("concept_probe", "Probe", "a ` stray then [[Target Page]]");
        let body = convert_all(&pages, &probe);
        assert!(
            body.contains("then [Target Page](/pages/target-page.md)"),
            "{body}"
        );
    }

    #[test]
    fn emitted_link_labels_escape_brackets() {
        let pages = vec![
            test_page("concept_a", "Alpha", "x"),
            test_page("concept_b", "Target [Page", "body"),
        ];
        let probe = test_page("concept_probe", "Probe", "see [[Target [Page]]");
        let body = convert_all(&pages, &probe);
        assert!(
            body.contains("[Target \\[Page](/pages/target-page.md)"),
            "{body}"
        );
        assert!(!body.contains("[Target [Page]("), "{body}");
    }

    #[test]
    fn refuses_to_overwrite_user_file_not_in_marker() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("pages")).unwrap();
        std::fs::write(
            dir.path().join(MARKER_FILE),
            serde_json::to_vec(&serde_json::json!({
                "okf_version": "0.2",
                "generated_by": "wenlan/test",
                "files": ["index.md"],
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.path().join("pages/alpha.md"), b"user bytes").unwrap();
        let marker_before = std::fs::read(dir.path().join(MARKER_FILE)).unwrap();
        let pages = vec![test_page("concept_a", "Alpha", "x")];
        let err = export_okf(&pages, dir.path()).unwrap_err();
        assert!(
            err.to_string().contains(
                "refusing to overwrite pages/alpha.md: not written by a previous Wenlan OKF export"
            ),
            "{err}"
        );
        assert_eq!(
            std::fs::read(dir.path().join("pages/alpha.md")).unwrap(),
            b"user bytes"
        );
        assert_eq!(
            std::fs::read(dir.path().join(MARKER_FILE)).unwrap(),
            marker_before,
            "marker untouched"
        );
        assert!(!dir.path().join("index.md").exists(), "nothing written");
        assert!(!dir.path().join("sources").exists(), "nothing written");
    }

    #[test]
    fn foreign_marker_writer_is_a_conflict() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(MARKER_FILE),
            serde_json::to_vec(&serde_json::json!({
                "okf_version": "0.2",
                "generated_by": "someone-else",
                "files": [],
            }))
            .unwrap(),
        )
        .unwrap();
        let marker_before = std::fs::read(dir.path().join(MARKER_FILE)).unwrap();
        let pages = vec![test_page("concept_a", "Alpha", "x")];
        let err = export_okf(&pages, dir.path()).unwrap_err();
        assert!(matches!(err, WenlanError::Conflict(_)), "{err}");
        assert_eq!(
            std::fs::read(dir.path().join(MARKER_FILE)).unwrap(),
            marker_before,
            "marker untouched"
        );
        assert!(!dir.path().join("index.md").exists(), "nothing written");
        assert!(!dir.path().join("pages").exists(), "nothing written");
    }

    #[test]
    fn a_page_titled_index_is_exported_beside_the_index_not_as_one() {
        let dir = tempfile::TempDir::new().unwrap();
        // A bundle from 0.18.9 wrote such a page at `pages/index.md`.
        std::fs::create_dir_all(dir.path().join(PAGES_DIR)).unwrap();
        std::fs::write(dir.path().join("pages/index.md"), "old").unwrap();
        std::fs::write(
            dir.path().join(MARKER_FILE),
            serde_json::to_vec(&serde_json::json!({
                "okf_version": "0.2",
                "generated_by": "wenlan/test",
                "files": ["index.md", "pages/index.md"],
            }))
            .unwrap(),
        )
        .unwrap();
        let pages = vec![test_page("concept_index", "Index", "x")];
        let stats = export_okf(&pages, dir.path()).unwrap();
        assert_eq!(stats.exported, 1);
        assert!(
            !dir.path().join("pages/index.md").exists(),
            "stale file removed"
        );
        let page = std::fs::read_to_string(dir.path().join("pages/index-page.md")).unwrap();
        assert!(page.contains("title: \"Index\""), "{page}");
        let index = std::fs::read_to_string(dir.path().join("index.md")).unwrap();
        assert!(index.contains("[Index](/pages/index-page.md)"), "{index}");
    }

    #[test]
    fn index_lists_only_pages_actually_written() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(MARKER_FILE),
            serde_json::to_vec(&serde_json::json!({
                "okf_version": "0.2",
                "generated_by": "wenlan/test",
                "files": ["index.md", "pages/alpha.md"],
            }))
            .unwrap(),
        )
        .unwrap();
        // A directory where the bundle file should go makes that page's write fail.
        std::fs::create_dir_all(dir.path().join("pages/alpha.md")).unwrap();
        let pages = vec![
            test_page("concept_alpha", "Alpha", "x"),
            test_page("concept_beta", "Beta", "y"),
        ];
        let stats = export_okf(&pages, dir.path()).unwrap();
        assert_eq!(stats.exported, 1);
        assert_eq!(stats.failed, 1);
        let index = std::fs::read_to_string(dir.path().join("index.md")).unwrap();
        assert!(index.contains("/pages/beta.md"), "{index}");
        assert!(!index.contains("Alpha"), "{index}");
        assert!(!index.contains("alpha.md"), "{index}");
        // The final marker still lists every planned file for later cleanup.
        let marker: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join(MARKER_FILE)).unwrap()).unwrap();
        let files: HashSet<String> = marker["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(files.contains("pages/alpha.md"), "{files:?}");
        assert!(files.contains("pages/beta.md"), "{files:?}");
    }

    #[test]
    fn reexport_is_byte_stable() {
        let mut a = test_page("concept_a", "Alpha", "see [[Beta]]");
        a.source_memory_ids = vec!["mem_1".to_string()];
        let pages = vec![a, test_page("concept_b", "Beta", "plain")];
        let dir = tempfile::TempDir::new().unwrap();
        export_okf(&pages, dir.path()).unwrap();
        let first = snapshot_dir(dir.path());
        assert!(first.len() >= 5, "{first:?}");
        export_okf(&pages, dir.path()).unwrap();
        let second = snapshot_dir(dir.path());
        assert_eq!(first, second);
    }

    fn snapshot_dir(root: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        collect_all_files(root, &mut out);
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    fn collect_all_files(dir: &Path, out: &mut Vec<(std::path::PathBuf, Vec<u8>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                collect_all_files(&path, out);
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
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
