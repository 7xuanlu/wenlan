// SPDX-License-Identifier: Apache-2.0
//! PR-B Step 1: compat verify cached scenario DBs.
//!
//! Opens copies of the May-23 fullpipeline scenario DBs via `MemoryDB::new`,
//! which runs migrations 44..=54 idempotently. Then opens a separate libSQL
//! connection to report user_version, table counts, and 3 sample pages per DB.
//!
//! Operate on COPIES (set via `SCENARIO_DB_ROOT`), never on the canonical
//! `~/.cache/origin-eval/fullpipeline_*.db/` originals.
//!
//! Skipped unless `--ignored` is passed. Resolves the scenario DB root in this order:
//!   1. `SCENARIO_DB_ROOT` env var (highest priority — used by CI / cross-machine).
//!   2. `${EVAL_BASELINES_DIR}/scenario_seeded` (existing eval cache convention).
//!   3. `~/.cache/origin-eval/scenario_seeded/` (canonical default; seed via
//!      `scripts/seed-scenario-dbs.sh`).

use std::path::Path;
use std::sync::Arc;

use wenlan_core::db::MemoryDB;
use wenlan_core::eval::seed_contract::{check_seed_contract, SeedExpectations};
use wenlan_core::events::NoopEmitter;

/// Resolve the scenario DB root with sensible fallback:
/// 1. `SCENARIO_DB_ROOT` env override (highest priority).
/// 2. `EVAL_BASELINES_DIR` (chains the existing eval cache convention) + `scenario_seeded`.
/// 3. `~/.cache/origin-eval/scenario_seeded/` (canonical default; matches the
///    fullpipeline_*.db cache layout documented in AGENTS.md).
fn resolve_scenario_db_root() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("SCENARIO_DB_ROOT") {
        return std::path::PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("EVAL_BASELINES_DIR") {
        return std::path::PathBuf::from(p).join("scenario_seeded");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".cache")
        .join("origin-eval")
        .join("scenario_seeded")
}

async fn dump_counts(dir: &Path, label: &str) {
    let db_file = dir.join("origin_memory.db");
    let db = libsql::Builder::new_local(db_file.to_str().unwrap())
        .build()
        .await
        .expect("libsql open for counts");
    let conn = db.connect().expect("libsql connect for counts");

    let mut rows = conn.query("PRAGMA user_version", ()).await.unwrap();
    let version: i64 = rows
        .next()
        .await
        .unwrap()
        .map(|r| r.get::<i64>(0).unwrap_or(-1))
        .unwrap_or(-1);
    drop(rows);

    let tables = [
        "pages",
        "page_sources",
        "memories",
        "entities",
        "relations",
        "memory_entities",
    ];

    println!("=== {} ===", label);
    println!("user_version: {}", version);
    for t in &tables {
        let q = format!("SELECT COUNT(*) FROM {}", t);
        let n = match conn.query(&q, ()).await {
            Ok(mut rs) => rs
                .next()
                .await
                .ok()
                .flatten()
                .and_then(|r| r.get::<i64>(0).ok())
                .unwrap_or(-1),
            Err(_) => -1,
        };
        println!("  {:<18} {}", t, n);
    }

    // Sample 3 pages.
    let mut sample = conn
        .query(
            "SELECT id, title, length(content) AS content_len, \
                  (SELECT COUNT(*) FROM page_sources ps WHERE ps.page_id = p.id) AS source_count \
             FROM pages p ORDER BY RANDOM() LIMIT 3",
            (),
        )
        .await
        .unwrap();
    println!("  sample pages:");
    while let Some(row) = sample.next().await.unwrap() {
        let id: String = row.get(0).unwrap_or_default();
        let title: String = row.get(1).unwrap_or_default();
        let content_len: i64 = row.get(2).unwrap_or(-1);
        let src_count: i64 = row.get(3).unwrap_or(-1);
        println!(
            "    id={} title={:?} content_len={} sources={}",
            id, title, content_len, src_count
        );
    }
}

/// Live wiring of `eval::seed_contract` against the real cached seeds. The
/// in-mem unit tests prove the predicates; this runs them against the seeds the
/// eval path shares with production. Non-panicking `check_seed_contract` is used
/// (real seeds carry a known stray unclassified row, and locomo's classification
/// rate is not asserted here) so the operator sees every coverage number; only
/// the structural no-duplicate-`(source, source_id)` invariant — which must hold
/// for any valid seed — is hard-asserted. Reconstructs the intent of the
/// seed-contract integration wiring lost when the worktree was force-removed
/// mid-session (the unstaged change was never in git); placed here, where the
/// seeded DBs are already opened, rather than re-derived in eval_harness.
async fn seed_contract_check(dir: &Path, label: &str) {
    let db_file = dir.join("origin_memory.db");
    let db = libsql::Builder::new_local(db_file.to_str().unwrap())
        .build()
        .await
        .expect("libsql open for seed contract");
    let conn = db.connect().expect("libsql connect for seed contract");

    let report = check_seed_contract(&conn, &SeedExpectations::strict(label))
        .await
        .expect("check_seed_contract");
    println!(
        "[{}] seed_contract: rows={} distinct={} classified={} ({:.1}%) cue={} ({:.1}%) event_date={} ({:.1}%)",
        label,
        report.rows,
        report.distinct_keys,
        report.classified,
        (report.classified as f64 / report.rows.max(1) as f64) * 100.0,
        report.cue_nonempty,
        report.cue_coverage() * 100.0,
        report.event_date_nonempty,
        report.event_date_coverage() * 100.0,
    );
    if !report.violations.is_empty() {
        println!(
            "[{}] seed_contract violations: {:#?}",
            label, report.violations
        );
    }
    assert_eq!(
        report.rows, report.distinct_keys,
        "[{}] duplicate (source, source_id) memory heads in cached seed: {} rows vs {} distinct",
        label, report.rows, report.distinct_keys
    );
}

#[tokio::test]
#[ignore = "needs SCENARIO_DB_ROOT pointing at copies of cached scenario DBs (PR-B Step 1)"]
async fn cached_scenario_db_compat_check() {
    let root = resolve_scenario_db_root();

    for sub in &["locomo_v1", "lme_v1"] {
        let dir = root.join(sub);
        assert!(
            dir.join("origin_memory.db").exists(),
            "missing {}/origin_memory.db -- run scripts/seed-scenario-dbs.sh from the repo root to repopulate the cache",
            dir.display()
        );

        let start = std::time::Instant::now();
        {
            let _db = MemoryDB::new(&dir, Arc::new(NoopEmitter))
                .await
                .expect("MemoryDB::new (runs migrations)");
        }
        let elapsed = start.elapsed();
        println!(
            "\n[{}] MemoryDB::new (migrations) replay: {:?}",
            sub, elapsed
        );

        dump_counts(&dir, sub).await;
        seed_contract_check(&dir, sub).await;
    }
}
