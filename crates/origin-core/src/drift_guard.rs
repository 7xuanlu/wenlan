//! Fail-loud drift guards (test-only). Each `#[test]` here is a CI + pre-push gate
//! that makes a class of doc/flag/config drift structurally hard. Mirrors the
//! `seed_contract.rs` teeth pattern. See docs/superpowers/specs/2026-06-19-drift-defense-system-design.md.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Repo root, resolved at compile time from this crate's manifest dir
/// (crates/origin-core -> ../.. == repo root).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("resolve repo root")
}

/// Tracked files matching a git pathspec, relative to repo root.
fn git_ls_files(root: &Path, pattern: &str) -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", pattern])
        .output()
        .expect("run git ls-files");
    assert!(out.status.success(), "git ls-files failed for {pattern}");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|s| s.to_string())
        .collect()
}

// ── Teeth #3: version-file byte-identical assert ──

/// The version string carried by each release-please-managed source of truth.
/// The 4 daemon crates use `version.workspace = true`, so the only Cargo version
/// is the root workspace one — 3 sources total, not 6.
fn version_sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out = Vec::new();

    let vt = std::fs::read_to_string(root.join("version.txt")).expect("read version.txt");
    out.push(("version.txt".to_string(), vt.trim().to_string()));

    let mf =
        std::fs::read_to_string(root.join(".release-please-manifest.json")).expect("read manifest");
    let mfj: serde_json::Value = serde_json::from_str(&mf).expect("parse manifest json");
    out.push((
        ".release-please-manifest.json".to_string(),
        mfj["."].as_str().expect("manifest \".\" key").to_string(),
    ));

    let ct = std::fs::read_to_string(root.join("Cargo.toml")).expect("read root Cargo.toml");
    let line = ct
        .lines()
        .find(|l| l.contains("x-release-please-version"))
        .expect("workspace version line with x-release-please-version marker");
    let re = regex::Regex::new(r#""([0-9]+\.[0-9]+\.[0-9]+[^"]*)""#).unwrap();
    let v = re.captures(line).expect("version literal on marker line")[1].to_string();
    out.push(("Cargo.toml".to_string(), v));

    out
}

#[test]
fn version_files_are_in_sync() {
    let sources = version_sources();
    let (_, first) = &sources[0];
    let mismatched: Vec<&(String, String)> = sources.iter().filter(|(_, v)| v != first).collect();
    assert!(
        mismatched.is_empty(),
        "version drift across release-please files: {sources:?} (expected all == {first})"
    );
}

#[test]
fn version_sync_detects_mismatch() {
    // Pure-logic guard: a hand-built mismatched set must be flagged.
    let sources = vec![
        ("a".to_string(), "0.8.4".to_string()),
        ("b".to_string(), "0.8.5".to_string()),
    ];
    let (_, first) = &sources[0];
    let mismatched: Vec<_> = sources.iter().filter(|(_, v)| v != first).collect();
    assert_eq!(mismatched.len(), 1, "mismatch must be detected");
}

// ── Teeth #1: repo pointer/path resolver ──

const REPO_TOP_DIRS: &[&str] = &["crates/", "docs/", "app/", "scripts/", ".github/"];

/// Extract candidate in-repo path references from one markdown file's text.
/// Ignores code fences, URLs, prose, and `<!-- drift-ok -->` lines.
fn extract_repo_path_refs(md: &str) -> Vec<String> {
    let token = regex::Regex::new(r"[A-Za-z0-9_./\-]+").unwrap();
    let mut refs = Vec::new();
    let mut in_fence = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || line.contains("<!-- drift-ok -->") {
            continue;
        }
        for m in token.find_iter(line) {
            let t = m.as_str();
            if t.starts_with("http") {
                continue;
            }
            if t.contains('/') && REPO_TOP_DIRS.iter().any(|p| t.starts_with(p)) {
                let path = t
                    .split(':')
                    .next()
                    .unwrap()
                    .trim_end_matches(['.', ',', ')', '`']);
                if !path.is_empty() {
                    refs.push(path.to_string());
                }
            }
        }
    }
    refs
}

#[test]
fn path_extractor_finds_real_and_ignores_noise() {
    let md = "\
See `crates/origin-core/src/db.rs` for details.
Visit https://docs/example.com for nothing.
```
docs/in/a/fence.rs should be ignored
```
This crates/origin-core/src/eval/seed_contract.rs:42 line ref.
A made-up path crates/does/not/exist.rs here. <!-- drift-ok -->
";
    let refs = extract_repo_path_refs(md);
    assert!(refs.contains(&"crates/origin-core/src/db.rs".to_string()));
    assert!(refs.contains(&"crates/origin-core/src/eval/seed_contract.rs".to_string()));
    assert!(
        !refs.iter().any(|r| r.contains("fence")),
        "fenced path leaked"
    );
    assert!(
        !refs.iter().any(|r| r.contains("does/not/exist")),
        "drift-ok line leaked"
    );
    assert!(!refs.iter().any(|r| r.starts_with("http")), "url leaked");
}

#[test]
fn doc_path_references_resolve() {
    let root = repo_root();
    let mut dangling = Vec::new();
    for f in git_ls_files(&root, "*.md") {
        // Skip docs that legitimately reference aspirational / moved / extracted paths:
        // plan & design docs (not-yet-created), and AUDIT.md historical audits (may
        // reference code since extracted to other repos, e.g. the Tauri app -> origin-app).
        if f.starts_with("docs/plans/")
            || f.starts_with("docs/superpowers/")
            || f.ends_with("AUDIT.md")
        {
            continue;
        }
        let txt = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
        for r in extract_repo_path_refs(&txt) {
            // Only resolve file-like refs (have an extension); skip directory and
            // glob-stem references, which aren't precise enough to verify.
            if Path::new(&r).extension().is_none() {
                continue;
            }
            if !root.join(&r).exists() {
                dangling.push(format!("{f} -> {r}"));
            }
        }
    }
    assert!(
        dangling.is_empty(),
        "dangling in-repo path references (fix the doc or add <!-- drift-ok -->):\n{}",
        dangling.join("\n")
    );
}

// ── Teeth #2: retrieval/eval-flag doc contract (fail-closed) ──

/// Infra/transport/path flags exempt from the documentation requirement.
/// Extend deliberately, each with a one-line reason.
const FLAG_ALLOWLIST: &[&str] = &[
    "ORIGIN_PORT",            // transport
    "ORIGIN_HOST",            // transport
    "ORIGIN_BIND_ADDR",       // transport
    "ORIGIN_DATA_DIR",        // path
    "ORIGIN_PORT_FILE",       // path
    "ORIGIN_LISTENING_ON",    // runtime status
    "ORIGIN_GIT_SHA",         // build stamp
    "ORIGIN_MCP_CACHE_DIR",   // path
    "ORIGIN_MIGRATIONS_HASH", // build stamp
    "ORIGIN_BATCH_LOG",       // debug logging
    "ORIGIN_CHATGPT_ZIP",     // import path
];

/// BASELINE: behavioral flags undocumented when this contract was introduced
/// (2026-06-19). Grandfathered so the gate lands green on a repo with an existing
/// backlog; a NEW undocumented flag still fails fail-closed. BURN DOWN by
/// documenting each in an AGENTS.md and deleting it from this list. (Pure test/infra
/// flags — e.g. ORIGIN_TEST_FASTEMBED_CACHE — should instead move to FLAG_ALLOWLIST.)
const BASELINE_UNDOCUMENTED: &[&str] = &[
    "ORIGIN_COT_MAX_ITER",
    "ORIGIN_COT_ROUND_TIMEOUT_SECS",
    "ORIGIN_ENABLE_CONTEXT_COMPRESS",
    "ORIGIN_ENABLE_COT_RETRIEVAL",
    "ORIGIN_ENABLE_DUAL_POOL_RESOLVE",
    "ORIGIN_ENABLE_ENTITY_MINHASH",
    "ORIGIN_ENABLE_EPISODE_CHANNEL",
    "ORIGIN_ENABLE_EVICTION",
    "ORIGIN_ENABLE_FACT_CHANNEL",
    "ORIGIN_ENABLE_FTS_HARDENING",
    "ORIGIN_ENABLE_GLOBAL_PRELUDE",
    "ORIGIN_ENABLE_GRAPH_GATE",
    "ORIGIN_ENABLE_GRAPH_SEED",
    "ORIGIN_ENABLE_REFLECTION_DEBOUNCE",
    "ORIGIN_ENABLE_RERANK_BLEND",
    "ORIGIN_ENABLE_SALIENCE_PRIOR",
    "ORIGIN_ENABLE_SESSION_DIVERSITY",
    "ORIGIN_ENABLE_TEMPORAL_GROUNDING",
    "ORIGIN_EPISODE_CHANNEL_LIMIT",
    "ORIGIN_EPISODE_WORD_GATE",
    "ORIGIN_EVAL_ANSWER_PROMPT_V2",
    "ORIGIN_EXPAND_TEMP",
    "ORIGIN_FACT_CHANNEL_LIMIT",
    "ORIGIN_GRAPH_FRONTIER_CAP",
    "ORIGIN_GRAPH_HOP_DEPTH",
    "ORIGIN_GRAPH_HUB_CAP",
    "ORIGIN_GRAPH_KHOP_DEPTH",
    "ORIGIN_GRAPH_KHOP_MAX_NODES",
    "ORIGIN_GRAPH_SEED_TOP_K",
    "ORIGIN_GRAPH_SURFACE_BUDGET",
    "ORIGIN_MAGNITUDE_FUSION",
    "ORIGIN_MERGE_SHRINK_GUARD",
    "ORIGIN_PAGE_CHANNEL_LIMIT",
    "ORIGIN_PRELUDE_BUCKET_K",
    "ORIGIN_PRELUDE_MIN_MEMBERS",
    "ORIGIN_PRF_ROUNDS",
    "ORIGIN_QUERY_DECOMP_MAX_SUBQUERIES",
    "ORIGIN_QUERY_INTENT_FTS_BOOST",
    "ORIGIN_SESSION_DIVERSITY_MAX",
    "ORIGIN_SPACE",
    "ORIGIN_TEST_FASTEMBED_CACHE",
];

/// Every ORIGIN_* flag actually read via `env::var("ORIGIN_…")` in production
/// source (crates/*/src, excluding tests/).
fn flags_read_in_code(root: &Path) -> BTreeSet<String> {
    let re = regex::Regex::new(r#"env::var\("(ORIGIN_[A-Z0-9_]+)"\)"#).unwrap();
    let mut flags = BTreeSet::new();
    for f in git_ls_files(root, "*.rs") {
        if !f.starts_with("crates/") || !f.contains("/src/") {
            continue; // production source only
        }
        let txt = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
        for c in re.captures_iter(&txt) {
            flags.insert(c[1].to_string());
        }
    }
    flags
}

/// Every ORIGIN_* flag mentioned in any tracked AGENTS.md (the prose flag docs).
fn documented_flags(root: &Path) -> BTreeSet<String> {
    let re = regex::Regex::new(r"ORIGIN_[A-Z0-9_]+").unwrap();
    let mut flags = BTreeSet::new();
    for f in git_ls_files(root, "*AGENTS.md") {
        let txt = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
        for m in re.find_iter(&txt) {
            flags.insert(m.as_str().to_string());
        }
    }
    flags
}

#[test]
fn flag_collectors_basic() {
    let root = repo_root();
    let doc = documented_flags(&root);
    assert!(
        doc.contains("ORIGIN_GRAPH_MEMORY_STREAM"),
        "expected a known documented flag to be found"
    );
    let read = flags_read_in_code(&root);
    assert!(
        read.contains("ORIGIN_GRAPH_HUB_CAP"),
        "expected a known code-read flag to be found"
    );
}

#[test]
fn behavioral_flags_are_documented() {
    let root = repo_root();
    let read = flags_read_in_code(&root);
    let documented = documented_flags(&root);
    // Exempt = explicit infra allowlist ∪ the grandfathered burn-down baseline.
    let exempt: BTreeSet<String> = FLAG_ALLOWLIST
        .iter()
        .chain(BASELINE_UNDOCUMENTED.iter())
        .map(|s| s.to_string())
        .collect();

    let missing: Vec<String> = read
        .iter()
        .filter(|f| !documented.contains(*f) && !exempt.contains(*f))
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "NEW undocumented behavioral ORIGIN_* flag(s) (document in an AGENTS.md, or add to FLAG_ALLOWLIST/BASELINE_UNDOCUMENTED with a reason):\n{}",
        missing.join("\n")
    );
}

#[test]
fn flag_default_mismatch_warns() {
    // Best-effort, non-blocking: report (never fail on) same-line `unwrap_or(<lit>)`
    // code defaults for human cross-check against the doc bullet. Multi-line
    // defaults are skipped (warn-by-omission).
    let root = repo_root();
    let read_re = regex::Regex::new(
        r#"env::var\("(ORIGIN_[A-Z0-9_]+)"\).*unwrap_or\(([0-9]+(?:\.[0-9]+)?|true|false)\)"#,
    )
    .unwrap();
    let mut code_defaults = BTreeMap::new();
    for f in git_ls_files(&root, "*.rs") {
        if !f.starts_with("crates/") || !f.contains("/src/") {
            continue;
        }
        let txt = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
        for c in read_re.captures_iter(&txt) {
            code_defaults.insert(c[1].to_string(), c[2].to_string());
        }
    }
    for (flag, def) in &code_defaults {
        eprintln!(
            "[drift-guard] {flag} same-line code default = {def} — verify the doc bullet matches."
        );
    }
}
