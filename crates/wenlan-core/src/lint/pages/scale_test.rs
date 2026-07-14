use super::*;
use std::time::{Duration, Instant};

#[test]
fn buffered_frontmatter_scan_stays_bounded_for_sparse_scale_fixture() {
    let dir = root();
    let mut bytes = b"---\norigin_id: page_scale\n".to_vec();
    bytes.resize(65_536, b'x');
    for index in 0..200 {
        write(dir.path(), &format!("page-{index:03}.md"), &bytes);
    }

    let started = Instant::now();
    let scan = scan_page_root(dir.path()).expect("scale scan");
    let elapsed = started.elapsed();

    assert_eq!(scan.page_markdown().len(), 200);
    assert!(
        elapsed < Duration::from_secs(2),
        "bounded scan took {elapsed:?}"
    );
}
