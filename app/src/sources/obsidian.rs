// SPDX-License-Identifier: AGPL-3.0-only
//! App-local Obsidian helpers used at source registration time.
//! Only the filesystem-scanning utilities are needed by the app.
//! The heavy note_to_documents conversion stays in origin-core (used by daemon).
use std::path::Path;

/// Directories to skip when scanning a vault.
const SKIP_DIRS: &[&str] = &[".obsidian", ".trash", ".git", "templates"];

/// Check if a path should be skipped (any component matches a skip directory).
fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        SKIP_DIRS.contains(&name.as_ref())
    })
}

/// Short-circuit check: returns `true` as soon as any `.md` file is found in
/// `root` or any (non-skipped) subdirectory. Used at source registration time
/// so we don't need to walk the whole vault.
pub fn has_any_markdown(root: &Path) -> bool {
    has_any_markdown_recursive(root, root)
}

/// Convert a title string into a URL-safe slug (lowercase, spaces to hyphens,
/// non-alphanumeric chars removed). Inlined from origin-core::export::obsidian.
pub fn slugify(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else if c == ' ' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|&c| c != '\0')
        .collect::<String>()
}

fn has_any_markdown_recursive(root: &Path, dir: &Path) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if should_skip(relative) {
            continue;
        }

        if path.is_dir() {
            if has_any_markdown_recursive(root, &path) {
                return true;
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            return true;
        }
    }
    false
}
