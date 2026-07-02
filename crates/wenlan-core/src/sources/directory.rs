// SPDX-License-Identifier: Apache-2.0
//! Directory source connector — walk a local filesystem tree and collect
//! indexable files (md, txt, pdf) with size and symlink guards.

use std::fs;
use std::path::{Path, PathBuf};

/// Maximum file size for text/markdown files (1 MB).
const MAX_TEXT_SIZE: u64 = 1024 * 1024;

/// Maximum file size for PDF files (10 MB).
const MAX_PDF_SIZE: u64 = 10 * 1024 * 1024;

/// Directories to skip during traversal (relative path components).
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".obsidian",
    "node_modules",
    ".cache",
    ".venv",
    "venv",
];

/// Scan a directory tree and return all indexable files (md, txt, pdf).
///
/// # Behavior
/// - Walks recursively, following directory structure
/// - Does NOT follow symlinks (prevents cycles)
/// - Skips hidden directories and files (leading '.')
/// - Skips known non-indexable directories (`.git`, `.obsidian`, etc.)
/// - Filters to extensions: `.md`, `.txt`, `.pdf` only
/// - Applies per-file size caps (1MB for text/md, 10MB for pdf)
/// - Single-file root: if root is a file, yields exactly it (no validation)
///
/// # Returns
/// A vector of `PathBuf` to all indexable files found, sorted.
pub fn scan_directory(root: &Path) -> Vec<PathBuf> {
    let mut results = Vec::new();

    // If root is a file, yield it as-is.
    if root.is_file() {
        results.push(root.to_path_buf());
        return results;
    }

    // If root is not a directory, return empty.
    if !root.is_dir() {
        return results;
    }

    walk_recursive(root, &mut results);
    results.sort();
    results
}

/// Recursively walk a directory, collecting indexable files.
fn walk_recursive(dir: &Path, results: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip hidden files/directories (leading '.').
        if let Some(name) = path.file_name() {
            if let Some(name_str) = name.to_str() {
                if name_str.starts_with('.') {
                    continue;
                }
            }
        }

        // If it's a directory, recurse (but skip known non-indexable dirs).
        if path.is_dir() {
            if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                if SKIP_DIRS.contains(&dir_name) {
                    continue;
                }
            }
            // Do NOT follow symlinks to avoid cycles.
            if fs::symlink_metadata(&path)
                .map(|m| m.is_symlink())
                .unwrap_or(false)
            {
                continue;
            }
            walk_recursive(&path, results);
        }

        // If it's a file, check if it's indexable.
        if path.is_file() && is_indexable(&path) {
            results.push(path);
        }
    }
}

/// Check if a file is indexable: right extension and within size limits.
fn is_indexable(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let size_limit = match ext.as_str() {
        "md" | "txt" => MAX_TEXT_SIZE,
        "pdf" => MAX_PDF_SIZE,
        _ => return false,
    };

    // Check file size.
    match fs::metadata(path) {
        Ok(m) => m.len() <= size_limit,
        Err(_) => false,
    }
}

/// Guard against scanning the reserved pages directory.
///
/// Checks if the given path is the pages directory (~/.wenlan/pages),
/// which should never be registered as an ingest source (prevents recursion).
pub fn is_reserved_ingest_root(path: &Path, knowledge_path: &Path) -> bool {
    // Normalize both paths to check for exact match.
    if let (Ok(p1), Ok(p2)) = (path.canonicalize(), knowledge_path.canonicalize()) {
        p1 == p2
    } else {
        // If canonicalization fails, do a simple comparison.
        path == knowledge_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    const MAX_TEXT_SIZE: u64 = 1024 * 1024;
    const MAX_PDF_SIZE: u64 = 10 * 1024 * 1024;

    #[test]
    fn test_scan_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_scan_single_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.md");
        fs::File::create(&file_path)
            .unwrap()
            .write_all(b"# Test")
            .unwrap();

        let results = scan_directory(&file_path);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], file_path);
    }

    #[test]
    fn test_scan_filters_extensions() {
        let tmp = TempDir::new().unwrap();

        create_file(tmp.path(), "test.md", b"# Markdown");
        create_file(tmp.path(), "test.txt", b"Text content");
        create_file(tmp.path(), "test.pdf", b"PDF");
        create_file(tmp.path(), "test.png", b"PNG");
        create_file(tmp.path(), "test.doc", b"DOC");

        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 3);

        let names: Vec<_> = results
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
            .collect();
        assert!(names.contains(&"test.md"));
        assert!(names.contains(&"test.txt"));
        assert!(names.contains(&"test.pdf"));
        assert!(!names.contains(&"test.png"));
        assert!(!names.contains(&"test.doc"));
    }

    #[test]
    fn test_scan_skips_hidden_files() {
        let tmp = TempDir::new().unwrap();

        create_file(tmp.path(), "visible.md", b"# Visible");
        create_file(tmp.path(), ".hidden.md", b"# Hidden");

        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].file_name().unwrap().to_str().unwrap() == "visible.md");
    }

    #[test]
    fn test_scan_skips_hidden_directories() {
        let tmp = TempDir::new().unwrap();

        fs::create_dir(tmp.path().join("visible")).unwrap();
        fs::create_dir(tmp.path().join(".hidden")).unwrap();

        create_file(&tmp.path().join("visible"), "file.md", b"# Visible");
        create_file(&tmp.path().join(".hidden"), "file.md", b"# Hidden");

        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].components().any(|c| c.as_os_str() == "visible"));
    }

    #[test]
    fn test_scan_skips_known_dirs() {
        let tmp = TempDir::new().unwrap();

        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::create_dir(tmp.path().join(".obsidian")).unwrap();
        fs::create_dir(tmp.path().join("node_modules")).unwrap();

        create_file(&tmp.path().join(".git"), "file.md", b"# Git");
        create_file(&tmp.path().join(".obsidian"), "file.md", b"# Obsidian");
        create_file(
            &tmp.path().join("node_modules"),
            "file.md",
            b"# NodeModules",
        );

        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_scan_respects_text_size_limit() {
        let tmp = TempDir::new().unwrap();

        let ok_content = vec![b'a'; (MAX_TEXT_SIZE - 1) as usize];
        let mut ok_file = fs::File::create(tmp.path().join("ok.txt")).unwrap();
        ok_file.write_all(&ok_content).unwrap();

        let oversized_content = vec![b'b'; (MAX_TEXT_SIZE + 1) as usize];
        let mut oversized_file = fs::File::create(tmp.path().join("oversized.txt")).unwrap();
        oversized_file.write_all(&oversized_content).unwrap();

        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].file_name().unwrap().to_str().unwrap() == "ok.txt");
    }

    #[test]
    fn test_scan_respects_pdf_size_limit() {
        let tmp = TempDir::new().unwrap();

        let ok_content = vec![b'a'; (MAX_PDF_SIZE - 1) as usize];
        let mut ok_file = fs::File::create(tmp.path().join("ok.pdf")).unwrap();
        ok_file.write_all(&ok_content).unwrap();

        let oversized_content = vec![b'b'; (MAX_PDF_SIZE + 1) as usize];
        let mut oversized_file = fs::File::create(tmp.path().join("oversized.pdf")).unwrap();
        oversized_file.write_all(&oversized_content).unwrap();

        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].file_name().unwrap().to_str().unwrap() == "ok.pdf");
    }

    #[test]
    fn test_scan_recursive() {
        let tmp = TempDir::new().unwrap();

        fs::create_dir(tmp.path().join("subdir")).unwrap();
        fs::create_dir(tmp.path().join("subdir/nested")).unwrap();

        create_file(tmp.path(), "root.md", b"# Root");
        create_file(&tmp.path().join("subdir"), "sub.md", b"# Sub");
        create_file(&tmp.path().join("subdir/nested"), "nested.md", b"# Nested");

        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_scan_ignores_symlink_cycles() {
        let tmp = TempDir::new().unwrap();

        create_file(tmp.path(), "file.md", b"# File");

        let symlink_path = tmp.path().join("cycle_link");
        #[cfg(unix)]
        {
            use std::os::unix::fs as unix_fs;
            let _ = unix_fs::symlink(tmp.path(), &symlink_path);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs as win_fs;
            let _ = win_fs::symlink_dir(tmp.path(), &symlink_path);
        }

        let results = scan_directory(tmp.path());
        assert_eq!(results.len(), 1);
        assert!(results[0].file_name().unwrap().to_str().unwrap() == "file.md");
    }

    #[test]
    fn test_is_reserved_ingest_root_true() {
        let tmp = TempDir::new().unwrap();
        let pages_path = tmp.path().join("pages");
        fs::create_dir(&pages_path).unwrap();

        assert!(is_reserved_ingest_root(&pages_path, &pages_path));
    }

    #[test]
    fn test_is_reserved_ingest_root_false() {
        let tmp = TempDir::new().unwrap();
        let pages_path = tmp.path().join("pages");
        let other_path = tmp.path().join("other");

        fs::create_dir(&pages_path).unwrap();
        fs::create_dir(&other_path).unwrap();

        assert!(!is_reserved_ingest_root(&other_path, &pages_path));
    }

    fn create_file(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
        let path = dir.join(name);
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }
}
