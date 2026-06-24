//! Fail-loud drift guards (test-only). Each `#[test]` here is a CI + pre-push gate
//! that makes a class of doc/flag/config drift structurally hard. Mirrors the
//! `seed_contract.rs` teeth pattern. See docs/superpowers/specs/2026-06-19-drift-defense-system-design.md.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Repo root, resolved at compile time from this crate's manifest dir
/// (crates/wenlan-core -> ../.. == repo root).
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
    let sources = [
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
See `crates/wenlan-core/src/db.rs` for details.
Visit https://docs/example.com for nothing.
```
docs/in/a/fence.rs should be ignored
```
This crates/wenlan-core/src/eval/seed_contract.rs:42 line ref.
A made-up path crates/does/not/exist.rs here. <!-- drift-ok -->
";
    let refs = extract_repo_path_refs(md);
    assert!(refs.contains(&"crates/wenlan-core/src/db.rs".to_string()));
    assert!(refs.contains(&"crates/wenlan-core/src/eval/seed_contract.rs".to_string()));
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
    "WENLAN_PORT",            // transport
    "WENLAN_HOST",            // transport
    "WENLAN_BIND_ADDR",       // transport
    "WENLAN_DATA_DIR",        // path
    "WENLAN_PORT_FILE",       // path
    "WENLAN_LISTENING_ON",    // runtime status
    "WENLAN_GIT_SHA",         // build stamp
    "WENLAN_MCP_CACHE_DIR",   // path
    "WENLAN_MIGRATIONS_HASH", // build stamp
    "WENLAN_BATCH_LOG",       // debug logging
    "WENLAN_CHATGPT_ZIP",     // import path
];

/// BASELINE: behavioral flags undocumented when this contract was introduced
/// (2026-06-19). Grandfathered so the gate lands green on a repo with an existing
/// backlog; a NEW undocumented flag still fails fail-closed. BURN DOWN by
/// documenting each in an AGENTS.md and deleting it from this list. (Pure test/infra
/// flags — e.g. WENLAN_TEST_FASTEMBED_CACHE — should instead move to FLAG_ALLOWLIST.)
const BASELINE_UNDOCUMENTED: &[&str] = &[
    "WENLAN_COT_MAX_ITER",
    "WENLAN_COT_ROUND_TIMEOUT_SECS",
    "WENLAN_ENABLE_CONTEXT_COMPRESS",
    "WENLAN_ENABLE_COT_RETRIEVAL",
    "WENLAN_ENABLE_DUAL_POOL_RESOLVE",
    "WENLAN_ENABLE_ENTITY_MINHASH",
    "WENLAN_ENABLE_EPISODE_CHANNEL",
    "WENLAN_ENABLE_EVICTION",
    "WENLAN_ENABLE_FACT_CHANNEL",
    "WENLAN_ENABLE_FTS_HARDENING",
    "WENLAN_ENABLE_GLOBAL_PRELUDE",
    "WENLAN_ENABLE_GRAPH_GATE",
    "WENLAN_ENABLE_GRAPH_SEED",
    "WENLAN_ENABLE_REFLECTION_DEBOUNCE",
    "WENLAN_ENABLE_RERANK_BLEND",
    "WENLAN_ENABLE_SALIENCE_PRIOR",
    "WENLAN_ENABLE_SESSION_DIVERSITY",
    "WENLAN_ENABLE_TEMPORAL_GROUNDING",
    "WENLAN_EPISODE_CHANNEL_LIMIT",
    "WENLAN_EPISODE_WORD_GATE",
    "WENLAN_EVAL_ANSWER_PROMPT_V2",
    "WENLAN_EXPAND_TEMP",
    "WENLAN_FACT_CHANNEL_LIMIT",
    "WENLAN_GRAPH_FRONTIER_CAP",
    "WENLAN_GRAPH_HOP_DEPTH",
    "WENLAN_GRAPH_HUB_CAP",
    "WENLAN_GRAPH_KHOP_DEPTH",
    "WENLAN_GRAPH_KHOP_MAX_NODES",
    "WENLAN_GRAPH_SEED_TOP_K",
    "WENLAN_GRAPH_SURFACE_BUDGET",
    // Helper-read LLM batching flags (parse_clamped_*_env call sites in llm_provider.rs),
    // surfaced by the broadened read-detector. Pre-existing + undocumented at contract intro.
    "WENLAN_LLM_COALESCE_MS",
    "WENLAN_LLM_PARALLEL_SEQS",
    "WENLAN_LLM_WORKERS",
    "WENLAN_MAGNITUDE_FUSION",
    "WENLAN_MERGE_SHRINK_GUARD",
    "WENLAN_PAGE_CHANNEL_LIMIT",
    "WENLAN_PRELUDE_BUCKET_K",
    "WENLAN_PRELUDE_MIN_MEMBERS",
    "WENLAN_PRF_ROUNDS",
    "WENLAN_QUERY_DECOMP_MAX_SUBQUERIES",
    "WENLAN_QUERY_INTENT_FTS_BOOST",
    "WENLAN_SESSION_DIVERSITY_MAX",
    "WENLAN_SPACE",
    "WENLAN_TEST_FASTEMBED_CACHE",
];

/// Every WENLAN_* flag read in production source (`crates/*/src`). Matches the flag
/// name as a string-literal argument to an env reader — `env::var("…")`, `var_os("…")`,
/// or any `*_env("…")` helper (e.g. the `parse_clamped_*_env` idiom, whose name arg is
/// a literal at the call site) — so indirect reads through a helper aren't silently
/// missed. Whitespace-tolerant so multi-line call sites (name on its own line) match.
fn flags_read_in_code(root: &Path) -> BTreeSet<String> {
    let re = regex::Regex::new(r#"(?:var_os|var|_env)\s*\(\s*"(WENLAN_[A-Z0-9_]+)""#).unwrap();
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

/// Every WENLAN_* flag mentioned in any tracked AGENTS.md (the prose flag docs).
fn documented_flags(root: &Path) -> BTreeSet<String> {
    let re = regex::Regex::new(r"WENLAN_[A-Z0-9_]+").unwrap();
    let mut flags = BTreeSet::new();
    for f in git_ls_files(root, "*AGENTS.md") {
        let txt = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
        for m in re.find_iter(&txt) {
            flags.insert(m.as_str().to_string());
        }
    }
    flags
}

/// Fail-closed set-difference: flags read in code but neither documented nor exempt.
/// Extracted so the gate AND a positive-control test exercise the same logic.
fn undocumented_flags(
    read: &BTreeSet<String>,
    documented: &BTreeSet<String>,
    exempt: &BTreeSet<String>,
) -> Vec<String> {
    read.iter()
        .filter(|f| !documented.contains(*f) && !exempt.contains(*f))
        .cloned()
        .collect()
}

#[test]
fn flag_collectors_basic() {
    let root = repo_root();
    let doc = documented_flags(&root);
    assert!(
        doc.contains("WENLAN_GRAPH_MEMORY_STREAM"),
        "expected a known documented flag to be found"
    );
    let read = flags_read_in_code(&root);
    assert!(
        read.contains("WENLAN_GRAPH_HUB_CAP"),
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

    let missing = undocumented_flags(&read, &documented, &exempt);

    assert!(
        missing.is_empty(),
        "NEW undocumented behavioral WENLAN_* flag(s). Fix: document in an *AGENTS.md* \
         (only AGENTS.md files are scanned for docs — docs/ and READMEs do NOT count), \
         or add to FLAG_ALLOWLIST / BASELINE_UNDOCUMENTED with a reason:\n{}",
        missing.join("\n")
    );
}

#[test]
fn flag_doc_contract_detects_undocumented() {
    // Positive control: the SAME set-difference the gate uses must flag a
    // read-but-undocumented flag while leaving a documented one alone. Proves the
    // tooth bites (the failure path), not just that the live repo happens to be green.
    let read: BTreeSet<String> = ["WENLAN_REAL", "WENLAN_FAKE_UNDOCUMENTED"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let documented: BTreeSet<String> = ["WENLAN_REAL"].iter().map(|s| s.to_string()).collect();
    let exempt: BTreeSet<String> = BTreeSet::new();
    let missing = undocumented_flags(&read, &documented, &exempt);
    assert_eq!(missing, vec!["WENLAN_FAKE_UNDOCUMENTED".to_string()]);
}

#[test]
fn flag_default_mismatch_warns() {
    // Best-effort, non-blocking: report (never fail on) same-line `unwrap_or(<lit>)`
    // code defaults for human cross-check against the doc bullet. Multi-line
    // defaults are skipped (warn-by-omission).
    let root = repo_root();
    let read_re = regex::Regex::new(
        r#"env::var\("(WENLAN_[A-Z0-9_]+)"\).*unwrap_or\(([0-9]+(?:\.[0-9]+)?|true|false)\)"#,
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

#[test]
fn root_agents_md_stays_lean() {
    // Teeth #4 — size budget on the ONE always-loaded instruction file.
    // Root AGENTS.md (which CLAUDE.md re-imports) is paid in full context EVERY
    // session; subtree AGENTS.md load on-demand. It silently accreted 39.9KB ->
    // 57.3KB as each retrieval/engine PR appended its flag wall to the path of
    // least resistance (the file it was already editing). This gate makes the
    // agents.md hierarchical convention the DEFAULT-BY-FORCE: exceed the budget
    // and the only green path is moving crate-specific reference into the owning
    // crate's AGENTS.md, not raising this number. No verifier control needed —
    // the check is a byte comparison, not parsing logic.
    const BUDGET: u64 = 44_000; // ~11k tok. Today ~39.8KB after the 2026-06-23 extraction.
    let path = repo_root().join("AGENTS.md");
    let bytes = std::fs::metadata(&path).expect("stat root AGENTS.md").len();
    assert!(
        bytes <= BUDGET,
        "root AGENTS.md is {bytes}B > {BUDGET}B budget. It loads in FULL every session. \
         Push crate-specific reference (env-flag docs, deep internals) into the owning crate's \
         subtree AGENTS.md — they load on-demand and still satisfy the teeth-#2 flag-doc contract \
         (it scans every tracked *AGENTS.md). Raising BUDGET is the wrong fix."
    );
}
