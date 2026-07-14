use super::*;

#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::sync::{Arc, Barrier};

#[cfg(unix)]
fn swap_names(left: &Path, right: &Path, hold: &Path) {
    fs::rename(left, hold).expect("left to hold");
    fs::rename(right, left).expect("right to left");
    fs::rename(hold, right).expect("hold to right");
}

#[cfg(unix)]
#[test]
fn scanner_never_reads_outside_sentinel_during_file_and_directory_swaps() {
    use std::os::unix::fs::symlink;

    let dir = root();
    let outside = root();
    write(
        outside.path(),
        "outside.md",
        b"---\norigin_id: outside_secret\n---\nsecret\n",
    );
    write(
        dir.path(),
        "page.md",
        b"---\norigin_id: inside_page\n---\nsafe\n",
    );
    symlink(
        outside.path().join("outside.md"),
        dir.path().join("page-link.md"),
    )
    .expect("file symlink");
    write(
        dir.path(),
        "nested/page.md",
        b"---\norigin_id: inside_nested\n---\nsafe\n",
    );
    symlink(outside.path(), dir.path().join("nested-link")).expect("directory symlink");

    let start = Arc::new(Barrier::new(2));
    let stop = Arc::new(AtomicBool::new(false));
    let worker_root = dir.path().to_path_buf();
    let worker_start = Arc::clone(&start);
    let worker_stop = Arc::clone(&stop);
    let worker = std::thread::spawn(move || {
        worker_start.wait();
        while !worker_stop.load(Ordering::Relaxed) {
            swap_names(
                &worker_root.join("page.md"),
                &worker_root.join("page-link.md"),
                &worker_root.join("page-hold"),
            );
            swap_names(
                &worker_root.join("nested"),
                &worker_root.join("nested-link"),
                &worker_root.join("nested-hold"),
            );
        }
    });

    start.wait();
    let mut successful_scans = 0;
    let mut expected_failures = 0;
    let mut sentinel_checks = 0;
    for _ in 0..2_000 {
        match scan_page_root(dir.path()) {
            Ok(scan) => {
                successful_scans += 1;
                let debug = format!("{scan:?}");
                assert!(!debug.contains("outside_secret"));
                assert!(scan.page_markdown().iter().all(|entry| entry
                    .frontmatter
                    .origin_id
                    .as_deref()
                    != Some("outside_secret")));
                sentinel_checks += 1;
            }
            Err(
                PageFsError::ReadDirectory | PageFsError::ReadEntry | PageFsError::ReadMetadata,
            ) => {
                expected_failures += 1;
            }
            Err(error) => panic!("unexpected swap scan error: {error}"),
        }
    }
    stop.store(true, Ordering::Relaxed);
    worker.join().expect("swap worker");
    println!(
        "successful_scans={successful_scans} expected_failures={expected_failures} sentinel_checks={sentinel_checks}"
    );
    assert!(
        successful_scans > 0,
        "swap stress produced no successful scan"
    );
    assert_eq!(sentinel_checks, successful_scans);
    assert_eq!(successful_scans + expected_failures, 2_000);
}

#[cfg(unix)]
#[test]
fn receipt_detects_same_path_symlink_retarget_and_special_metadata() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::symlink;
    use std::process::Command;

    let dir = root();
    let outside = root();
    write(outside.path(), "target-a", b"a");
    write(outside.path(), "target-b", b"b");
    let link = dir.path().join("retarget");
    symlink(outside.path().join("target-a"), &link).expect("first target");
    let fifo = dir.path().join("special-fifo");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo")
        .success());

    let before = scan_page_root(dir.path()).expect("before retarget");
    let link_before = before.entry("retarget").expect("link before");
    let special = before.entry("special-fifo").expect("special entry");
    assert_ne!(
        special.prefix_digest,
        <[u8; 32]>::from(Sha256::digest(b"unparsed"))
    );
    assert!(special.modified_ns > 0);

    fs::remove_file(&link).expect("remove first target");
    symlink(outside.path().join("target-b"), &link).expect("second target");
    let after = scan_page_root(dir.path()).expect("after retarget");
    let link_after = after.entry("retarget").expect("link after");
    assert_eq!(link_before.length, link_after.length);
    assert_ne!(link_before.prefix_digest, link_after.prefix_digest);
    assert!(!before
        .verify_unchanged(dir.path())
        .expect("retarget receipt")
        .is_consistent());
}

#[cfg(unix)]
#[test]
fn scanner_rejects_root_symlink_and_non_utf8_name() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;

    let target = root();
    let parent = root();
    let link = parent.path().join("root-link");
    symlink(target.path(), &link).expect("root symlink");
    assert!(matches!(
        scan_page_root(&link),
        Err(PageFsError::RootSymlink)
    ));

    let non_utf8 = std::path::PathBuf::from(OsString::from_vec(b"bad-\xff.md".to_vec()));
    #[cfg(not(target_os = "macos"))]
    {
        fs::write(target.path().join(&non_utf8), b"body\n").expect("non-UTF-8 fixture");
        assert!(matches!(
            scan_page_root(target.path()),
            Err(PageFsError::UnsupportedFilenameEncoding)
        ));
    }
    #[cfg(target_os = "macos")]
    assert!(matches!(
        super::super::super::traversal::relative_path_string(&non_utf8),
        Err(PageFsError::UnsupportedFilenameEncoding)
    ));
}

#[cfg(windows)]
#[test]
fn scanner_rejects_windows_symlinks_and_mount_point_junctions() {
    use std::os::windows::fs::{symlink_dir, symlink_file};
    use std::process::Command;

    let dir = root();
    let outside = root();
    write(
        outside.path(),
        "outside.md",
        b"---\norigin_id: outside_secret\n---\n",
    );
    symlink_file(
        outside.path().join("outside.md"),
        dir.path().join("file-link.md"),
    )
    .expect("file symlink");
    symlink_dir(outside.path(), dir.path().join("dir-link")).expect("directory symlink");
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(dir.path().join("junction-link"))
        .arg(outside.path())
        .status()
        .expect("junction command");
    assert!(status.success());

    let scan = scan_page_root(dir.path()).expect("Windows reparse scan");
    for path in ["file-link.md", "dir-link", "junction-link"] {
        assert_eq!(
            scan.entry(path).expect("reparse entry").kind,
            super::super::EntryKind::Symlink
        );
    }
    assert!(!format!("{scan:?}").contains("outside_secret"));

    let parent = root();
    let root_link = parent.path().join("root-link");
    symlink_dir(dir.path(), &root_link).expect("root directory symlink");
    assert!(matches!(
        scan_page_root(&root_link),
        Err(PageFsError::RootSymlink)
    ));
}
