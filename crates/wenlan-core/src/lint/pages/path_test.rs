use super::*;
use crate::lint::pages::fs::{normalize_target_path, PathIssueKind, TargetPathError};

#[test]
fn target_paths_normalize_separators_and_reject_unsafe_roots() {
    let normalized = normalize_target_path("folder\\.\\page.md").expect("normalize");
    assert_eq!(normalized.as_str(), "folder/page.md");
    assert!(!format!("{normalized:?}").contains("folder"));
    assert!(!format!("{normalized:?}").contains("page.md"));
    for path in [
        "/absolute.md",
        "\\absolute.md",
        "C:\\drive.md",
        "\\\\host\\share\\page.md",
        "../outside.md",
        "folder/../../outside.md",
    ] {
        assert!(matches!(
            normalize_target_path(path),
            Err(TargetPathError::Absolute
                | TargetPathError::Drive
                | TargetPathError::Unc
                | TargetPathError::Parent)
        ));
    }
}

#[test]
fn scanner_detects_state_and_filesystem_normalized_collisions() {
    let dir = root();
    #[cfg(unix)]
    {
        write(dir.path(), "Upper\\Page.md", b"manual\n");
        write(dir.path(), "upper/page.md", b"manual\n");
    }
    #[cfg(unix)]
    write(dir.path(), "nested\\page.md", b"manual\n");
    write(dir.path(), "nested/page.md", b"manual\n");
    write(
        dir.path(),
        ".wenlan/state.json",
        state(
            "{\"page_a\":{\"file\":\"nested/page.md\",\"version\":1},\"page_b\":{\"file\":\"nested\\\\page.md\",\"version\":1},\"page_c\":{\"file\":\"Case.md\",\"version\":1},\"page_d\":{\"file\":\"case.md\",\"version\":1}}",
        )
        .as_bytes(),
    );

    let scan = scan_page_root(dir.path()).expect("collision scan");
    assert!(scan
        .path_issues
        .iter()
        .any(|issue| issue.kind == PathIssueKind::LowercaseCollision));
    assert!(scan
        .path_issues
        .iter()
        .any(|issue| issue.kind == PathIssueKind::ExactDuplicate));
}

#[cfg(unix)]
#[test]
fn scanner_rejects_static_symlink_target_components() {
    use std::os::unix::fs::symlink;

    let dir = root();
    let outside = root();
    write(
        outside.path(),
        "outside.md",
        b"---\norigin_id: outside_secret\n---\n",
    );
    symlink(outside.path(), dir.path().join("linked")).expect("directory symlink");
    write(
        dir.path(),
        ".wenlan/state.json",
        state("{\"page_a\":{\"file\":\"linked/outside.md\",\"version\":1}}").as_bytes(),
    );

    let scan = scan_page_root(dir.path()).expect("symlink scan");
    assert!(scan
        .path_issues
        .iter()
        .any(|issue| issue.kind == PathIssueKind::SymlinkTraversal));
    assert!(!format!("{scan:?}").contains("outside_secret"));
}
