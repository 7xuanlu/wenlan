//! Read-only, bounded inspection of a Page markdown projection.

use super::frontmatter::Frontmatter;
use super::path::{self, collect_path_issue};
use super::state::{self, parse_raw_state};
use super::traversal::{collect_entries, open_root};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) const MANIFEST_MAX_BYTES: u64 = 1024 * 1024;
pub(super) const STATE_MAX_BYTES: u64 = 4 * 1024 * 1024;
pub(super) const DEEP_PAGE_BODY_MAX_BYTES: u64 = 4 * 1024 * 1024;
pub(super) const DEEP_PAGE_TREE_MAX_BYTES: u64 = 64 * 1024 * 1024;

#[cfg(test)]
pub(crate) use super::frontmatter::{FrontmatterState, VersionValue};
#[cfg(test)]
pub(crate) use super::path::TargetPathError;
pub(crate) use super::path::{normalize_target_path, PathIssueKind};
#[cfg(test)]
pub(crate) use super::state::{RawStateIssue, RawStateKind, StateEntryIssue, StateEntryStatus};

#[derive(Debug, thiserror::Error)]
pub enum PageFsError {
    #[error("page root is not a directory")]
    RootNotDirectory,
    #[error("page root is a symlink")]
    RootSymlink,
    #[error("page scanner could not enumerate a directory")]
    ReadDirectory,
    #[error("page scanner could not inspect an entry")]
    ReadEntry,
    #[error("page scanner could not inspect metadata")]
    ReadMetadata,
    #[error("page scanner could not read a structural prefix")]
    ReadPrefix,
    #[error("page scanner body budget exceeded")]
    BodyBudgetExceeded,
    #[error("page scanner state budget exceeded")]
    StateBudgetExceeded,
    #[error("page scanner found an unsupported filename encoding")]
    UnsupportedFilenameEncoding,
    #[error("page scanner canceled")]
    Canceled,
    #[error("page scanner deadline exceeded")]
    DeadlineExceeded,
}

#[derive(Clone)]
pub(crate) struct PageScanControl {
    canceled: Arc<AtomicBool>,
    deadline: Option<Instant>,
}

impl PageScanControl {
    fn unbounded() -> Self {
        Self {
            canceled: Arc::new(AtomicBool::new(false)),
            deadline: None,
        }
    }

    pub(crate) fn with_timeout(timeout: Duration) -> Self {
        Self {
            canceled: Arc::new(AtomicBool::new(false)),
            deadline: Instant::now().checked_add(timeout),
        }
    }

    pub(crate) fn cancel(&self) {
        self.canceled.store(true, Ordering::Release);
    }

    pub(super) fn check(&self) -> Result<(), PageFsError> {
        if self.canceled.load(Ordering::Acquire) {
            Err(PageFsError::Canceled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Err(PageFsError::DeadlineExceeded)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntryScope {
    PageMarkdown,
    StateControl,
    SourceInventory,
    Other,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PageEntry {
    pub(crate) path: String,
    pub(crate) kind: EntryKind,
    pub(crate) scope: EntryScope,
    pub(crate) frontmatter: Frontmatter,
    pub(super) length: u64,
    pub(super) modified_ns: u128,
    pub(super) prefix_digest: [u8; 32],
    pub(crate) body_digest: Option<[u8; 32]>,
}

impl fmt::Debug for PageEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageEntry")
            .field("kind", &self.kind)
            .field("scope", &self.scope)
            .field("frontmatter", &self.frontmatter)
            .field("length", &self.length)
            .field("modified_ns", &self.modified_ns)
            .field("prefix_digest", &self.prefix_digest)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PathIssue {
    pub(crate) kind: PathIssueKind,
}

#[derive(Clone)]
pub struct PageScan {
    pub(crate) entries: Vec<PageEntry>,
    pub(crate) raw_state: state::RawState,
    pub(crate) manifest: ManifestProjection,
    pub(crate) path_issues: Vec<PathIssue>,
    before_tree: [u8; 32],
    before_state: Option<[u8; 32]>,
    includes_body_digests: bool,
}

impl fmt::Debug for PageScan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageScan")
            .field("entry_count", &self.entries.len())
            .field("state_edge_count", &self.raw_state.edges.len())
            .field("manifest", &self.manifest)
            .field("path_issue_count", &self.path_issues.len())
            .field("tree_digest", &self.before_tree)
            .field("state_digest", &self.before_state)
            .finish()
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) enum ManifestProjection {
    #[default]
    Missing,
    Invalid,
    Parsed(BTreeMap<String, Vec<String>>),
}

impl fmt::Debug for ManifestProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("Missing"),
            Self::Invalid => formatter.write_str("Invalid"),
            Self::Parsed(pages) => formatter
                .debug_struct("Parsed")
                .field("page_count", &pages.len())
                .field(
                    "reference_count",
                    &pages.values().map(Vec::len).sum::<usize>(),
                )
                .finish(),
        }
    }
}

impl PageScan {
    pub fn entry(&self, path: &str) -> Option<&PageEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    pub fn page_markdown(&self) -> Vec<&PageEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.scope == EntryScope::PageMarkdown)
            .collect()
    }

    pub fn normalized_bytes(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(self.before_tree);
        digest.update(self.before_state.unwrap_or([0; 32]));
        for edge in &self.raw_state.edges {
            digest.update(edge.state_id.as_bytes());
            digest.update(edge.target_path.as_deref().unwrap_or_default().as_bytes());
        }
        digest.finalize().into()
    }

    pub fn verify_unchanged(&self, root: &Path) -> Result<PageFsReceipt, PageFsError> {
        self.verify_unchanged_with_control(root, &PageScanControl::unbounded())
    }

    pub(crate) fn verify_unchanged_with_control(
        &self,
        root: &Path,
        control: &PageScanControl,
    ) -> Result<PageFsReceipt, PageFsError> {
        let after = scan_page_root_internal(root, self.includes_body_digests, control)?;
        Ok(PageFsReceipt {
            before_tree: self.before_tree,
            after_tree: after.before_tree,
            before_state: self.before_state,
            after_state: after.before_state,
            after_normalized: after.normalized_bytes(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFsReceipt {
    before_tree: [u8; 32],
    after_tree: [u8; 32],
    before_state: Option<[u8; 32]>,
    after_state: Option<[u8; 32]>,
    after_normalized: [u8; 32],
}

impl PageFsReceipt {
    pub fn is_consistent(self) -> bool {
        self.before_tree == self.after_tree && self.before_state == self.after_state
    }

    pub(crate) fn after_normalized_bytes(self) -> [u8; 32] {
        self.after_normalized
    }
}

pub fn scan_page_root(root: &Path) -> Result<PageScan, PageFsError> {
    scan_page_root_internal(root, false, &PageScanControl::unbounded())
}

#[cfg(test)]
pub(crate) fn scan_page_root_deep(root: &Path) -> Result<PageScan, PageFsError> {
    scan_page_root_internal(root, true, &PageScanControl::unbounded())
}

pub(crate) fn scan_page_root_controlled(
    root: &Path,
    include_body_digests: bool,
    control: &PageScanControl,
) -> Result<PageScan, PageFsError> {
    scan_page_root_internal(root, include_body_digests, control)
}

fn scan_page_root_internal(
    root: &Path,
    include_body_digests: bool,
    control: &PageScanControl,
) -> Result<PageScan, PageFsError> {
    control.check()?;
    let root = open_root(root)?;
    let mut entries = Vec::new();
    let mut state_bytes = None;
    let mut manifest_bytes = None;
    let mut manifest_too_large = false;
    let mut body_bytes_remaining = DEEP_PAGE_TREE_MAX_BYTES;
    collect_entries(
        &root,
        &mut entries,
        &mut state_bytes,
        &mut manifest_bytes,
        &mut manifest_too_large,
        include_body_digests,
        &mut body_bytes_remaining,
        control,
    )?;
    control.check()?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    let mut raw_state = parse_raw_state(state_bytes.as_deref());
    let manifest = parse_manifest(manifest_bytes.as_deref(), manifest_too_large);
    let mut path_issues = collect_path_issues(&entries, &mut raw_state);
    let before_tree = tree_digest(&entries);
    let before_state = state_bytes.map(|bytes| Sha256::digest(bytes).into());
    path_issues.sort_by_key(|issue| issue.kind);

    Ok(PageScan {
        entries,
        raw_state,
        manifest,
        path_issues,
        before_tree,
        before_state,
        includes_body_digests: include_body_digests,
    })
}

#[derive(serde::Deserialize)]
struct RawManifest {
    pages: BTreeMap<String, Vec<String>>,
}

fn parse_manifest(bytes: Option<&[u8]>, too_large: bool) -> ManifestProjection {
    if too_large {
        return ManifestProjection::Invalid;
    }
    let Some(bytes) = bytes else {
        return ManifestProjection::Missing;
    };
    serde_json::from_slice::<RawManifest>(bytes).map_or(ManifestProjection::Invalid, |manifest| {
        ManifestProjection::Parsed(manifest.pages)
    })
}

fn collect_path_issues(entries: &[PageEntry], raw_state: &mut state::RawState) -> Vec<PathIssue> {
    let frontmatter = entries
        .iter()
        .map(|entry| (entry.path.clone(), entry.frontmatter.clone()))
        .collect::<BTreeMap<_, _>>();
    let symlinks = entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::Symlink)
        .map(|entry| entry.path.as_str())
        .collect::<BTreeSet<_>>();
    let mut state_paths = Vec::new();
    let mut issues = Vec::new();
    for edge in &mut raw_state.edges {
        let Some(raw_target_path) = edge.raw_target_path.as_deref() else {
            continue;
        };
        match normalize_target_path(raw_target_path) {
            Ok(target) => {
                edge.target_path = Some(target.as_str().to_string());
                edge.frontmatter = frontmatter
                    .get(target.as_str())
                    .cloned()
                    .unwrap_or_default();
                if crosses_symlink(&target, &symlinks) {
                    issues.push(PathIssue {
                        kind: PathIssueKind::SymlinkTraversal,
                    });
                }
                state_paths.push(target);
            }
            Err(error) => issues.push(PathIssue {
                kind: collect_path_issue(error),
            }),
        }
    }
    issues.extend(path::duplicate_issues(&state_paths));
    let filesystem_paths = entries
        .iter()
        .map(|entry| path::NormalizedTarget::from_scanned(&entry.path))
        .collect::<Vec<_>>();
    issues.extend(path::duplicate_issues(&filesystem_paths));
    issues
}

fn crosses_symlink(target: &path::NormalizedTarget, symlinks: &BTreeSet<&str>) -> bool {
    let mut prefix = String::new();
    target.as_str().split('/').any(|component| {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        symlinks.contains(prefix.as_str())
    })
}

fn tree_digest(entries: &[PageEntry]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"wenlan-page-tree-v1");
    for entry in entries {
        digest.update(entry.path.as_bytes());
        digest.update([entry.kind as u8, entry.scope as u8]);
        digest.update(entry.length.to_le_bytes());
        digest.update(entry.modified_ns.to_le_bytes());
        digest.update(entry.prefix_digest);
        digest.update(entry.body_digest.unwrap_or([0; 32]));
    }
    digest.finalize().into()
}

#[cfg(test)]
#[path = "fs_test.rs"]
mod tests;
