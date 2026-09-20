// SPDX-License-Identifier: Apache-2.0
//! One place for "move this file aside" instead of unlinking it.
//!
//! The projection root is very often the user's own Obsidian vault, so every
//! writer under `export/` that decides a file is no longer wanted answers the
//! same question first: the database can rebuild whatever Wenlan generated,
//! but it cannot rebuild what a person typed into the file afterwards. A
//! marker in frontmatter says Wenlan WROTE a file; it never says the bytes are
//! still Wenlan's, because an edit made in place in the vault changes the body
//! and leaves the frontmatter alone. So a file leaves the projection by moving
//! into `archive/`, in plain sight, rather than by an unlink.

use crate::error::WenlanError;
use crate::export::knowledge::page_stem_clear_of_reserved;
use cap_fs_ext::DirExt as _;
use cap_std::fs::Dir;

/// Plain and visible, in the projection root. Not a dotfile: a person looking
/// for a file that disappeared should be able to find it without being told
/// where to look.
pub(crate) const ARCHIVE_DIR: &str = "archive";

/// How many suffixed names to try before giving up on a free destination.
const MAX_ARCHIVE_ATTEMPTS: u32 = 100;

/// Open (creating if needed) `archive/` under the projection root.
pub(crate) fn open_archive_dir(root: &Dir) -> Result<Dir, WenlanError> {
    match root.symlink_metadata(ARCHIVE_DIR) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(WenlanError::Conflict(
                "page_archive_target_invalid".to_string(),
            ))
        }
        // `AlreadyExists` here means something created `archive/` between the
        // probe above and this call -- the user's sync client, or a second
        // pass. That is the state we wanted, not a failure; letting it
        // propagate would strand a file that is still readable.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match root.create_dir(ARCHIVE_DIR) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(WenlanError::Io(e)),
            }
        }
        Err(error) => return Err(WenlanError::Io(error)),
    }
    root.open_dir_nofollow(ARCHIVE_DIR).map_err(WenlanError::Io)
}

/// The name an archived copy of `filename` takes inside `archive/`.
///
/// OKF reserves `index.md` and `log.md` at EVERY level of the hierarchy (spec
/// 3.1), and `archive/` is a level. Archiving a file that is being moved off a
/// reserved name under that same name would push the violation one directory
/// down rather than end it, so a reserved name takes the `-page` suffix the
/// projection already gives a page titled "Index" or "Log".
fn archive_basename(filename: &str) -> (String, &'static str) {
    match filename.strip_suffix(".md") {
        Some(stem) => (page_stem_clear_of_reserved(stem.to_string()), ".md"),
        None => (filename.to_string(), ""),
    }
}

/// Move `filename` out of `source` and into the projection root's `archive/`.
///
/// `source` is usually `root` itself; the `_sources/` stub GC passes its own
/// directory so an edited stub lands in the same visible place a page does.
/// Refuses anything that is not a plain file: a directory or special file
/// where a file was expected is a conflict rather than something to move, and
/// no symlink is followed. Already-absent is success -- the invariant is that
/// the file is gone from `source`, not that this pass is the one that moved
/// it.
pub(crate) fn archive_file_from(
    root: &Dir,
    source: &Dir,
    filename: &str,
) -> Result<(), WenlanError> {
    match source.symlink_metadata(filename) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(WenlanError::Conflict(
                "page_projection_target_invalid".to_string(),
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(WenlanError::Io(error)),
    }
    let archive = open_archive_dir(root)?;
    // `rename` replaces its destination without a word, and the destination
    // here is an older archived copy -- the one thing under this root that the
    // database cannot reproduce. Suffix until free.
    let (stem, extension) = archive_basename(filename);
    let mut target = format!("{stem}{extension}");
    let mut attempt = 1;
    while archive.symlink_metadata(&target).is_ok() {
        attempt += 1;
        if attempt > MAX_ARCHIVE_ATTEMPTS {
            return Err(WenlanError::Conflict(
                "page_archive_target_exhausted".to_string(),
            ));
        }
        target = format!("{stem}-{attempt}{extension}");
    }
    source
        .rename(filename, &archive, &target)
        .map_err(WenlanError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> (tempfile::TempDir, Dir) {
        let dir = tempfile::tempdir().unwrap();
        let root = Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        (dir, root)
    }

    /// An ordinary page archives under its own name.
    #[test]
    fn archives_under_the_original_name() {
        let (dir, root) = temp_root();
        std::fs::write(dir.path().join("alpha.md"), b"body").unwrap();
        archive_file_from(&root, &root, "alpha.md").unwrap();
        assert!(!dir.path().join("alpha.md").exists());
        assert_eq!(
            std::fs::read(dir.path().join("archive/alpha.md")).unwrap(),
            b"body"
        );
    }

    /// A file leaving a reserved name does not take that name into `archive/`,
    /// where OKF reserves it just as firmly (spec 3.1).
    #[test]
    fn reserved_names_do_not_follow_the_file_into_archive() {
        let (dir, root) = temp_root();
        std::fs::write(dir.path().join("log.md"), b"a concept, not a log").unwrap();
        std::fs::write(dir.path().join("index.md"), b"a concept, not an index").unwrap();
        archive_file_from(&root, &root, "log.md").unwrap();
        archive_file_from(&root, &root, "index.md").unwrap();
        assert!(!dir.path().join("archive/log.md").exists());
        assert!(!dir.path().join("archive/index.md").exists());
        assert_eq!(
            std::fs::read(dir.path().join("archive/log-page.md")).unwrap(),
            b"a concept, not a log"
        );
        assert_eq!(
            std::fs::read(dir.path().join("archive/index-page.md")).unwrap(),
            b"a concept, not an index"
        );
    }

    /// A second archived copy never overwrites the first.
    #[test]
    fn suffixes_past_an_occupied_destination() {
        let (dir, root) = temp_root();
        std::fs::create_dir(dir.path().join("archive")).unwrap();
        std::fs::write(dir.path().join("archive/log-page.md"), b"older").unwrap();
        std::fs::write(dir.path().join("log.md"), b"newer").unwrap();
        archive_file_from(&root, &root, "log.md").unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("archive/log-page.md")).unwrap(),
            b"older"
        );
        assert_eq!(
            std::fs::read(dir.path().join("archive/log-page-2.md")).unwrap(),
            b"newer"
        );
    }

    /// A file in a subdirectory archives into the ROOT's `archive/`, which is
    /// how `_sources/` stubs reach the same visible place pages do.
    #[test]
    fn archives_from_a_subdirectory_into_the_root_archive() {
        let (dir, root) = temp_root();
        std::fs::create_dir(dir.path().join("_sources")).unwrap();
        std::fs::write(dir.path().join("_sources/note.md"), b"mine").unwrap();
        let sources = root.open_dir("_sources").unwrap();
        archive_file_from(&root, &sources, "note.md").unwrap();
        assert!(!dir.path().join("_sources/note.md").exists());
        assert_eq!(
            std::fs::read(dir.path().join("archive/note.md")).unwrap(),
            b"mine"
        );
    }

    /// Already gone is success, not an error.
    #[test]
    fn missing_file_is_success() {
        let (_dir, root) = temp_root();
        archive_file_from(&root, &root, "gone.md").unwrap();
    }
}
