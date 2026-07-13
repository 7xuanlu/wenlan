use super::*;
use crate::lint::pages::frontmatter::{parse_frontmatter, FRONTMATTER_LIMIT};
use std::io::{self, Read};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct LimitOracle {
    bytes: Vec<u8>,
    position: usize,
    read: Arc<AtomicUsize>,
}

impl Read for LimitOracle {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        assert!(self.position < FRONTMATTER_LIMIT, "attempted byte 65537");
        let remaining = FRONTMATTER_LIMIT - self.position;
        let available = self.bytes.len() - self.position;
        let amount = remaining.min(available).min(buffer.len());
        buffer[..amount].copy_from_slice(&self.bytes[self.position..self.position + amount]);
        self.position += amount;
        self.read.fetch_add(amount, Ordering::Relaxed);
        Ok(amount)
    }
}

#[test]
fn frontmatter_requires_complete_opening_and_closing_delimiter_lines() {
    let dir = root();
    write(
        dir.path(),
        "opening.md",
        b"---suffix\norigin_id: secret_open\n---\n",
    );
    write(
        dir.path(),
        "closing.md",
        b"---\norigin_id: secret_close\n---suffix\n",
    );
    write(
        dir.path(),
        "crlf.md",
        b"---\r\norigin_id: page_crlf\r\n---\r\nbody\r\n",
    );

    let scan = scan_page_root(dir.path()).expect("delimiter scan");
    assert_eq!(
        scan.entry("opening.md").expect("opening").frontmatter.state,
        FrontmatterState::Absent
    );
    assert_eq!(
        scan.entry("closing.md").expect("closing").frontmatter.state,
        FrontmatterState::Truncated
    );
    assert_eq!(
        scan.entry("crlf.md")
            .expect("crlf")
            .frontmatter
            .origin_id
            .as_deref(),
        Some("page_crlf")
    );
}

#[test]
fn frontmatter_reports_invalid_keys_and_structural_value_types_without_debug_leaks() {
    let dir = root();
    write(
        dir.path(),
        "invalid.md",
        b"---\n7: secret_key\norigin_id: 42\norigin_version: secret_version\n---\nbody\n",
    );

    let scan = scan_page_root(dir.path()).expect("invalid frontmatter scan");
    let frontmatter = &scan.entry("invalid.md").expect("invalid").frontmatter;
    let debug = format!("{frontmatter:?}");
    assert_eq!(frontmatter.state, FrontmatterState::Invalid);
    assert!(debug.contains("NonStringKey"));
    assert!(debug.contains("OriginIdType"));
    assert!(debug.contains("OriginVersionType"));
    assert!(!debug.contains("secret_"));
}

#[test]
fn frontmatter_distinguishes_malformed_truncated_and_over_limit() {
    let dir = root();
    write(
        dir.path(),
        "malformed.md",
        b"---\norigin_id: [\n---\nbody\n",
    );
    write(dir.path(), "truncated.md", b"---\norigin_id: page_a\n");
    let mut over_limit = b"---\norigin_id: page_bound\n".to_vec();
    over_limit.resize(65_536, b'x');
    over_limit.extend_from_slice(b"SENTINEL_MUST_NOT_BE_READ");
    write(dir.path(), "over-limit.md", &over_limit);

    let scan = scan_page_root(dir.path()).expect("frontmatter scan");
    assert_eq!(
        scan.entry("malformed.md")
            .expect("malformed")
            .frontmatter
            .state,
        FrontmatterState::Malformed
    );
    assert_eq!(
        scan.entry("truncated.md")
            .expect("truncated")
            .frontmatter
            .state,
        FrontmatterState::Truncated
    );
    assert_eq!(
        scan.entry("over-limit.md")
            .expect("over-limit")
            .frontmatter
            .state,
        FrontmatterState::OverLimit
    );
    assert!(!format!("{scan:?}").contains("SENTINEL_MUST_NOT_BE_READ"));
}

#[test]
fn frontmatter_never_reads_byte_65537() {
    let mut bytes = b"---\norigin_id: page_bound\n".to_vec();
    bytes.resize(FRONTMATTER_LIMIT + 1, b'x');
    let read = Arc::new(AtomicUsize::new(0));
    let reader = LimitOracle {
        bytes,
        position: 0,
        read: Arc::clone(&read),
    };

    let frontmatter = parse_frontmatter(reader).expect("bounded parser");
    assert_eq!(frontmatter.state, FrontmatterState::OverLimit);
    assert_eq!(read.load(Ordering::Relaxed), FRONTMATTER_LIMIT);
}

#[test]
fn structural_prefix_digest_does_not_hash_markdown_body() {
    let dir = root();
    write(
        dir.path(),
        "page.md",
        b"---\norigin_id: page_a\n---\nbody-a\n",
    );
    let first = scan_page_root(dir.path()).expect("first body scan");
    let first_digest = first.entry("page.md").expect("first page").prefix_digest;

    write(
        dir.path(),
        "page.md",
        b"---\norigin_id: page_a\n---\nbody-b\n",
    );
    let second = scan_page_root(dir.path()).expect("second body scan");
    assert_eq!(
        first_digest,
        second.entry("page.md").expect("second page").prefix_digest
    );
}
