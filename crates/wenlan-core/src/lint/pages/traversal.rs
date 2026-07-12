use super::frontmatter::{read_frontmatter, Frontmatter};
use super::fs::{EntryKind, EntryScope, PageEntry, PageFsError};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt};
use cap_std::fs::{Dir, Metadata, OpenOptions};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;

pub(super) fn open_root(root: &Path) -> Result<Dir, PageFsError> {
    let parent = root.parent().ok_or(PageFsError::RootNotDirectory)?;
    let basename = root.file_name().ok_or(PageFsError::RootNotDirectory)?;
    let ambient_parent = Dir::open_ambient_dir(parent, cap_std::ambient_authority())
        .map_err(|_| PageFsError::ReadDirectory)?;
    let metadata = ambient_parent
        .symlink_metadata(Path::new(basename))
        .map_err(|_| PageFsError::ReadMetadata)?;
    if metadata.file_type().is_symlink() {
        return Err(PageFsError::RootSymlink);
    }
    if !metadata.is_dir() {
        return Err(PageFsError::RootNotDirectory);
    }
    ambient_parent
        .open_dir_nofollow(Path::new(basename))
        .map_err(|_| PageFsError::ReadDirectory)
}

pub(super) fn collect_entries(
    root: &Dir,
    entries: &mut Vec<PageEntry>,
    state_bytes: &mut Option<Vec<u8>>,
    manifest_bytes: &mut Option<Vec<u8>>,
    manifest_too_large: &mut bool,
    include_body_digests: bool,
    body_bytes_remaining: &mut u64,
) -> Result<(), PageFsError> {
    let mut traversal = Traversal {
        entries,
        state_bytes,
        manifest_bytes,
        manifest_too_large,
        include_body_digests,
        body_bytes_remaining,
    };
    visit(root, "", &mut traversal)
}

struct Traversal<'a> {
    entries: &'a mut Vec<PageEntry>,
    state_bytes: &'a mut Option<Vec<u8>>,
    manifest_bytes: &'a mut Option<Vec<u8>>,
    manifest_too_large: &'a mut bool,
    include_body_digests: bool,
    body_bytes_remaining: &'a mut u64,
}

fn visit(directory: &Dir, prefix: &str, traversal: &mut Traversal<'_>) -> Result<(), PageFsError> {
    let mut names = directory
        .entries()
        .map_err(|_| PageFsError::ReadDirectory)?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|_| PageFsError::ReadEntry)
        })
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    for name in names {
        visit_entry(directory, prefix, &name, traversal)?;
    }
    Ok(())
}

fn visit_entry(
    directory: &Dir,
    prefix: &str,
    name: &OsStr,
    traversal: &mut Traversal<'_>,
) -> Result<(), PageFsError> {
    let component = component_string(name)?;
    let path = if prefix.is_empty() {
        component
    } else {
        format!("{prefix}/{component}")
    };
    let metadata = directory
        .symlink_metadata(Path::new(name))
        .map_err(|_| PageFsError::ReadMetadata)?;
    let classified = entry_kind(&metadata);

    match classified {
        EntryKind::Directory => {
            let child = directory
                .open_dir_nofollow(Path::new(name))
                .map_err(|_| PageFsError::ReadDirectory)?;
            let opened = child
                .dir_metadata()
                .map_err(|_| PageFsError::ReadMetadata)?;
            traversal.entries.push(page_entry(
                &path,
                EntryKind::Directory,
                &opened,
                Frontmatter::unparsed(),
                None,
            )?);
            visit(&child, &path, traversal)
        }
        EntryKind::File => {
            let mut file = directory
                .open_with(Path::new(name), &read_nofollow())
                .map_err(|_| PageFsError::ReadEntry)?;
            let opened = file.metadata().map_err(|_| PageFsError::ReadMetadata)?;
            let scope = scope_for(&path, EntryKind::File);
            if path == ".wenlan/state.json" {
                let mut bytes = Vec::new();
                (&mut file)
                    .take(super::fs::STATE_MAX_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| PageFsError::ReadPrefix)?;
                if bytes.len() > usize::try_from(super::fs::STATE_MAX_BYTES).unwrap_or(usize::MAX) {
                    return Err(PageFsError::StateBudgetExceeded);
                }
                *traversal.state_bytes = Some(bytes);
            } else if path == "_sources/.manifest.json" {
                let mut bytes = Vec::new();
                (&mut file)
                    .take(super::fs::MANIFEST_MAX_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| PageFsError::ReadPrefix)?;
                if bytes.len()
                    > usize::try_from(super::fs::MANIFEST_MAX_BYTES).unwrap_or(usize::MAX)
                {
                    *traversal.manifest_too_large = true;
                } else {
                    *traversal.manifest_bytes = Some(bytes);
                }
            }
            let (frontmatter, body_digest) = if scope == EntryScope::PageMarkdown
                && traversal.include_body_digests
            {
                if opened.len() > super::fs::DEEP_PAGE_BODY_MAX_BYTES
                    || opened.len() > *traversal.body_bytes_remaining
                {
                    return Err(PageFsError::BodyBudgetExceeded);
                }
                *traversal.body_bytes_remaining =
                    (*traversal.body_bytes_remaining).saturating_sub(opened.len());
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)
                    .map_err(|_| PageFsError::ReadPrefix)?;
                let frontmatter = super::frontmatter::parse_frontmatter(bytes.as_slice())?;
                let content = std::str::from_utf8(&bytes).map_err(|_| PageFsError::ReadPrefix)?;
                let (_, body) = crate::sources::obsidian::extract_frontmatter(content);
                let canonical = crate::export::provenance::canonicalize_page_body(body);
                (
                    frontmatter,
                    Some(Sha256::digest(canonical.as_bytes()).into()),
                )
            } else if scope == EntryScope::PageMarkdown {
                (read_frontmatter(file)?, None)
            } else {
                (Frontmatter::unparsed(), None)
            };
            traversal.entries.push(page_entry(
                &path,
                EntryKind::File,
                &opened,
                frontmatter,
                body_digest,
            )?);
            Ok(())
        }
        EntryKind::Symlink | EntryKind::Other => {
            traversal.entries.push(special_entry(
                directory, name, &path, classified, &metadata,
            )?);
            Ok(())
        }
    }
}

fn special_entry(
    directory: &Dir,
    name: &OsStr,
    path: &str,
    kind: EntryKind,
    metadata: &Metadata,
) -> Result<PageEntry, PageFsError> {
    let modified_ns = modified_ns(metadata)?;
    let prefix_digest = if kind == EntryKind::Symlink {
        let target = directory
            .read_link_contents(Path::new(name))
            .map_err(|_| PageFsError::ReadMetadata)?;
        digest_parts(&[
            b"wenlan-page-symlink-v1",
            target.as_os_str().as_encoded_bytes(),
        ])
    } else {
        digest_parts(&[
            b"wenlan-page-special-v1",
            &metadata.len().to_le_bytes(),
            &modified_ns.to_le_bytes(),
        ])
    };
    Ok(PageEntry {
        path: path.to_string(),
        kind,
        scope: scope_for(path, kind),
        frontmatter: Frontmatter::unparsed(),
        length: metadata.len(),
        modified_ns,
        prefix_digest,
        body_digest: None,
    })
}

fn digest_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part);
    }
    digest.finalize().into()
}

fn read_nofollow() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).follow(FollowSymlinks::No);
    options
}

fn page_entry(
    path: &str,
    kind: EntryKind,
    metadata: &Metadata,
    frontmatter: Frontmatter,
    body_digest: Option<[u8; 32]>,
) -> Result<PageEntry, PageFsError> {
    Ok(PageEntry {
        path: path.to_string(),
        kind,
        scope: scope_for(path, kind),
        prefix_digest: frontmatter.digest(),
        body_digest,
        frontmatter,
        length: metadata.len(),
        modified_ns: modified_ns(metadata)?,
    })
}

fn entry_kind(metadata: &Metadata) -> EntryKind {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        EntryKind::Symlink
    } else if file_type.is_file() {
        EntryKind::File
    } else if file_type.is_dir() {
        EntryKind::Directory
    } else {
        EntryKind::Other
    }
}

fn scope_for(path: &str, kind: EntryKind) -> EntryScope {
    let first = path.split('/').next().unwrap_or_default();
    if first == ".wenlan" {
        EntryScope::StateControl
    } else if first == "_sources" {
        EntryScope::SourceInventory
    } else if kind == EntryKind::File && path.to_ascii_lowercase().ends_with(".md") {
        EntryScope::PageMarkdown
    } else {
        EntryScope::Other
    }
}

pub(crate) fn relative_path_string(relative: &Path) -> Result<String, PageFsError> {
    relative
        .to_str()
        .map(|path| path.replace('\\', "/"))
        .ok_or(PageFsError::UnsupportedFilenameEncoding)
}

fn component_string(name: &OsStr) -> Result<String, PageFsError> {
    relative_path_string(Path::new(name))
}

fn modified_ns(metadata: &Metadata) -> Result<u128, PageFsError> {
    metadata
        .modified()
        .map_err(|_| PageFsError::ReadMetadata)
        .and_then(|time| {
            time.into_std()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .map_err(|_| PageFsError::ReadMetadata)
        })
}
