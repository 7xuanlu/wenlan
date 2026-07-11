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
) -> Result<(), PageFsError> {
    visit(
        root,
        "",
        entries,
        state_bytes,
        manifest_bytes,
        manifest_too_large,
    )
}

fn visit(
    directory: &Dir,
    prefix: &str,
    entries: &mut Vec<PageEntry>,
    state_bytes: &mut Option<Vec<u8>>,
    manifest_bytes: &mut Option<Vec<u8>>,
    manifest_too_large: &mut bool,
) -> Result<(), PageFsError> {
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
        visit_entry(
            directory,
            prefix,
            &name,
            entries,
            state_bytes,
            manifest_bytes,
            manifest_too_large,
        )?;
    }
    Ok(())
}

fn visit_entry(
    directory: &Dir,
    prefix: &str,
    name: &OsStr,
    entries: &mut Vec<PageEntry>,
    state_bytes: &mut Option<Vec<u8>>,
    manifest_bytes: &mut Option<Vec<u8>>,
    manifest_too_large: &mut bool,
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
            entries.push(page_entry(
                &path,
                EntryKind::Directory,
                &opened,
                Frontmatter::unparsed(),
            )?);
            visit(
                &child,
                &path,
                entries,
                state_bytes,
                manifest_bytes,
                manifest_too_large,
            )
        }
        EntryKind::File => {
            let mut file = directory
                .open_with(Path::new(name), &read_nofollow())
                .map_err(|_| PageFsError::ReadEntry)?;
            let opened = file.metadata().map_err(|_| PageFsError::ReadMetadata)?;
            let scope = scope_for(&path, EntryKind::File);
            if path == ".wenlan/state.json" {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes)
                    .map_err(|_| PageFsError::ReadPrefix)?;
                *state_bytes = Some(bytes);
            } else if path == "_sources/.manifest.json" {
                if opened.len() > super::fs::MANIFEST_MAX_BYTES {
                    *manifest_too_large = true;
                } else {
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes)
                        .map_err(|_| PageFsError::ReadPrefix)?;
                    *manifest_bytes = Some(bytes);
                }
            }
            let frontmatter = if scope == EntryScope::PageMarkdown {
                read_frontmatter(file)?
            } else {
                Frontmatter::unparsed()
            };
            entries.push(page_entry(&path, EntryKind::File, &opened, frontmatter)?);
            Ok(())
        }
        EntryKind::Symlink | EntryKind::Other => {
            entries.push(special_entry(
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
) -> Result<PageEntry, PageFsError> {
    Ok(PageEntry {
        path: path.to_string(),
        kind,
        scope: scope_for(path, kind),
        prefix_digest: frontmatter.digest(),
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
