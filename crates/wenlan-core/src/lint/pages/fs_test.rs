use super::{
    scan_page_root, scan_page_root_deep, EntryScope, FrontmatterState, PageFsError, RawStateIssue,
    RawStateKind, StateEntryIssue, StateEntryStatus, VersionValue, DEEP_PAGE_BODY_MAX_BYTES,
    STATE_MAX_BYTES,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

#[path = "frontmatter_test.rs"]
mod frontmatter_cases;
#[path = "path_test.rs"]
mod path_cases;
#[path = "scale_test.rs"]
mod scale_cases;
#[path = "security_test.rs"]
mod security_cases;

fn root() -> TempDir {
    tempfile::tempdir().expect("temporary page root")
}

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("fixture parent");
    }
    fs::write(path, bytes).expect("fixture file");
}

fn state(pages: &str) -> String {
    format!("{{\"schema_version\":2,\"pages\":{pages}}}")
}

#[test]
fn scanner_distinguishes_every_raw_state_shape() {
    let cases = [
        (None, RawStateKind::Missing),
        (Some("{"), RawStateKind::Malformed),
        (
            Some("{\"schema_version\":0,\"pages\":{}}"),
            RawStateKind::WriterDefaultV0,
        ),
        (Some("{\"concepts\":{}}"), RawStateKind::LegacyV1),
        (Some("{\"pages\":{}}"), RawStateKind::ImplicitV2),
        (
            Some("{\"schema_version\":2,\"pages\":{}}"),
            RawStateKind::ExplicitV2,
        ),
        (
            Some("{\"schema_version\":4294967295,\"pages\":{}}"),
            RawStateKind::FutureU32(u32::MAX),
        ),
        (
            Some("{\"schema_version\":4294967296,\"pages\":{}}"),
            RawStateKind::NonU32,
        ),
        (
            Some("{\"schema_version\":\"2\",\"pages\":{}}"),
            RawStateKind::NonU32,
        ),
    ];

    for (raw, expected) in cases {
        let dir = root();
        if let Some(raw) = raw {
            write(dir.path(), ".wenlan/state.json", raw.as_bytes());
        }
        let scan = scan_page_root(dir.path()).expect("state scan");
        assert_eq!(scan.raw_state.kind, expected);
    }
}

#[test]
fn scanner_types_malformed_state_roots_and_collections() {
    let cases = [
        ("{", RawStateIssue::InvalidJson),
        ("[]", RawStateIssue::RootNotObject),
        ("{\"schema_version\":2}", RawStateIssue::MissingCollection),
        (
            "{\"schema_version\":2,\"pages\":[]}",
            RawStateIssue::InvalidCollection,
        ),
        (
            "{\"schema_version\":1,\"pages\":{}}",
            RawStateIssue::MissingCollection,
        ),
        (
            "{\"schema_version\":1,\"concepts\":false}",
            RawStateIssue::InvalidCollection,
        ),
    ];

    for (raw, issue) in cases {
        let dir = root();
        write(dir.path(), ".wenlan/state.json", raw.as_bytes());
        let scan = scan_page_root(dir.path()).expect("malformed collection scan");
        assert_eq!(scan.raw_state.kind, RawStateKind::Malformed);
        assert_eq!(scan.raw_state.issue, Some(issue));
        assert!(scan.raw_state.edges.is_empty());
    }
}

#[test]
fn scanner_preserves_missing_state_with_and_without_projection() {
    let empty = root();
    let empty_scan = scan_page_root(empty.path()).expect("empty scan");
    assert_eq!(empty_scan.raw_state.kind, RawStateKind::Missing);
    assert!(empty_scan.page_markdown().is_empty());

    let projected = root();
    write(
        projected.path(),
        "nested/projected.md",
        b"---\norigin_id: page_projected\n---\nbody\n",
    );
    let projected_scan = scan_page_root(projected.path()).expect("projected scan");
    assert_eq!(projected_scan.raw_state.kind, RawStateKind::Missing);
    assert_eq!(projected_scan.page_markdown().len(), 1);
}

#[test]
fn scanner_preserves_exact_and_legacy_state_edges() {
    let dir = root();
    write(
        dir.path(),
        "nested/legacy.md",
        b"---\norigin_id: page_legacy\norigin_version: 8\n---\nbody\n",
    );
    write(
        dir.path(),
        ".wenlan/state.json",
        b"{\"concepts\":{\"concept_legacy\":{\"file\":\"nested/legacy.md\",\"version\":8}},\"schema_version\":1}",
    );

    let scan = scan_page_root(dir.path()).expect("legacy edge scan");
    let edge = &scan.raw_state.edges[0];
    assert_eq!(edge.state_id, "concept_legacy");
    assert_eq!(edge.target_path.as_deref(), Some("nested/legacy.md"));
    assert_eq!(edge.state_version, VersionValue::Integer(8));
    assert_eq!(edge.frontmatter.origin_id.as_deref(), Some("page_legacy"));

    write(
        dir.path(),
        ".wenlan/state.json",
        state("{\"page_legacy\":{\"file\":\"nested/legacy.md\",\"version\":8}}").as_bytes(),
    );
    let exact = scan_page_root(dir.path()).expect("exact edge scan");
    assert_eq!(exact.raw_state.edges[0].state_id, "page_legacy");
}

#[test]
fn scanner_retains_malformed_state_entries_without_debug_leaks() {
    let dir = root();
    write(
        dir.path(),
        ".wenlan/state.json",
        state(
            "{\"secret_bad_object\":7,\"secret_bad_file\":{\"file\":9,\"version\":1},\"secret_missing_file\":{\"version\":2}}",
        )
        .as_bytes(),
    );

    let scan = scan_page_root(dir.path()).expect("malformed state scan");
    let debug = format!("{:?}", scan.raw_state);
    assert_eq!(scan.raw_state.edges.len(), 3);
    assert_eq!(
        scan.raw_state.edges[0].status,
        StateEntryStatus::Malformed(StateEntryIssue::InvalidFile)
    );
    assert_eq!(
        scan.raw_state.edges[1].status,
        StateEntryStatus::Malformed(StateEntryIssue::NotObject)
    );
    assert_eq!(
        scan.raw_state.edges[2].status,
        StateEntryStatus::Malformed(StateEntryIssue::MissingFile)
    );
    assert!(debug.contains("malformed_entry_count"));
    assert!(!debug.contains("secret_bad"));
}

#[test]
fn raw_identifier_debug_views_are_redacted() {
    let dir = root();
    write(
        dir.path(),
        "secret-path.md",
        b"---\norigin_id: secret_frontmatter_id\n---\nbody\n",
    );
    write(
        dir.path(),
        ".wenlan/state.json",
        state("{\"secret_state_id\":{\"file\":\"secret-path.md\",\"version\":1}}").as_bytes(),
    );

    let scan = scan_page_root(dir.path()).expect("redaction scan");
    let edge_debug = format!("{:?}", scan.raw_state.edges[0]);
    let frontmatter_debug = format!("{:?}", scan.raw_state.edges[0].frontmatter);
    let entry_debug = format!("{:?}", scan.entry("secret-path.md").expect("secret entry"));
    assert!(!edge_debug.contains("secret_state_id"));
    assert!(!edge_debug.contains("secret-path"));
    assert!(!edge_debug.contains("secret_frontmatter_id"));
    assert!(!frontmatter_debug.contains("secret_frontmatter_id"));
    assert!(!entry_debug.contains("secret-path"));
    assert!(!entry_debug.contains("secret_frontmatter_id"));
}

#[test]
fn scanner_classifies_nested_markdown_but_reserves_control_trees() {
    let dir = root();
    write(dir.path(), "manual.md", b"manual\n");
    write(
        dir.path(),
        "nested/page.md",
        b"---\norigin_id: page_nested\n---\nbody\n",
    );
    write(
        dir.path(),
        ".wenlan/control.md",
        b"---\norigin_id: secret\n---\n",
    );
    write(
        dir.path(),
        "_sources/mem_secret.md",
        b"---\norigin_id: secret\n---\n",
    );

    let scan = scan_page_root(dir.path()).expect("scope scan");
    assert_eq!(
        scan.entry("manual.md").expect("manual").scope,
        EntryScope::PageMarkdown
    );
    assert_eq!(
        scan.entry("nested/page.md").expect("nested").scope,
        EntryScope::PageMarkdown
    );
    assert_eq!(
        scan.entry(".wenlan/control.md").expect("control").scope,
        EntryScope::StateControl
    );
    assert_eq!(
        scan.entry("_sources/mem_secret.md").expect("source").scope,
        EntryScope::SourceInventory
    );
    assert_eq!(
        scan.entry("_sources/mem_secret.md")
            .expect("source")
            .frontmatter
            .state,
        FrontmatterState::Unparsed
    );
    assert_eq!(scan.page_markdown().len(), 2);
}

#[test]
fn scanner_receipt_detects_mutation_and_stays_deterministic() {
    let dir = root();
    write(
        dir.path(),
        "page.md",
        b"---\norigin_id: page_a\n---\nbody\n",
    );
    let first = scan_page_root(dir.path()).expect("first scan");
    let repeated = scan_page_root(dir.path()).expect("repeated scan");
    assert_eq!(first.normalized_bytes(), repeated.normalized_bytes());
    assert!(first
        .verify_unchanged(dir.path())
        .expect("stable receipt")
        .is_consistent());

    write(
        dir.path(),
        "page.md",
        b"---\norigin_id: page_b\n---\nbody\n",
    );
    assert!(!first
        .verify_unchanged(dir.path())
        .expect("changed receipt")
        .is_consistent());
}

#[test]
fn deep_scan_hashes_only_canonical_page_body() {
    let dir = root();
    write(
        dir.path(),
        "page.md",
        b"---\norigin_id: page_a\norigin_version: 1\n---\nbody\n\n<!-- origin:sources:start -->\n## Sources\n- [[mem_a]]\n<!-- origin:sources:end -->\n",
    );

    let general = scan_page_root(dir.path()).expect("general scan");
    assert_eq!(general.entry("page.md").unwrap().body_digest, None);
    let deep = scan_page_root_deep(dir.path()).expect("deep scan");
    assert_eq!(
        deep.entry("page.md").unwrap().body_digest,
        Some(Sha256::digest(b"body").into())
    );
}

#[test]
fn general_scan_stays_bounded_while_deep_scan_enforces_body_budget() {
    let dir = root();
    let oversized = vec![b'x'; usize::try_from(DEEP_PAGE_BODY_MAX_BYTES + 1).unwrap()];
    write(dir.path(), "large.md", &oversized);

    assert!(scan_page_root(dir.path()).is_ok());
    assert!(matches!(
        scan_page_root_deep(dir.path()),
        Err(PageFsError::BodyBudgetExceeded)
    ));
}

#[test]
fn state_file_read_is_bounded_for_both_profiles() {
    let dir = root();
    let oversized = vec![b'x'; usize::try_from(STATE_MAX_BYTES + 1).unwrap()];
    write(dir.path(), ".wenlan/state.json", &oversized);

    assert!(matches!(
        scan_page_root(dir.path()),
        Err(PageFsError::StateBudgetExceeded)
    ));
    assert!(matches!(
        scan_page_root_deep(dir.path()),
        Err(PageFsError::StateBudgetExceeded)
    ));
}
