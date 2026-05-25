// SPDX-License-Identifier: Apache-2.0
//! Compare two `EvalReport` baselines. Refuses incomparable inputs loudly.
//!
//! Exit codes:
//!   0 — comparable + deltas printed.
//!   1 — usage error or I/O / parse failure.
//!   2 — refused: inputs not comparable (schema mismatch, env-hash mismatch,
//!       single-run vs multi-run mismatch, or missing env stamp).
//!
//! Usage: `compare-baselines <before.json> <after.json>`

use std::path::PathBuf;

use origin_core::eval::report::{comparable_env_hash, EvalReport};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let before_path: PathBuf = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: compare-baselines <before> <after>"))?
        .into();
    let after_path: PathBuf = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: compare-baselines <before> <after>"))?
        .into();
    if args.next().is_some() {
        anyhow::bail!("usage: compare-baselines <before> <after>");
    }

    let before: EvalReport = serde_json::from_slice(&std::fs::read(&before_path)?)
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {}", before_path.display(), e))?;
    let after: EvalReport = serde_json::from_slice(&std::fs::read(&after_path)?)
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {}", after_path.display(), e))?;

    let b_env = match before.env.as_ref() {
        Some(e) => e,
        None => {
            eprintln!(
                "incomparable: {} lacks env stamp (pre-P0b baseline). \
                 Rerun save_*_baseline to regenerate with env.",
                before_path.display()
            );
            std::process::exit(2);
        }
    };
    let a_env = match after.env.as_ref() {
        Some(e) => e,
        None => {
            eprintln!(
                "incomparable: {} lacks env stamp. \
                 Rerun save_*_baseline to regenerate with env.",
                after_path.display()
            );
            std::process::exit(2);
        }
    };

    if b_env.schema_version != a_env.schema_version {
        eprintln!(
            "incomparable: schema_version mismatch ({} vs {}). \
             Rerun save_*_baseline on both sides to regenerate.",
            b_env.schema_version, a_env.schema_version
        );
        std::process::exit(2);
    }

    if b_env.is_single_run != a_env.is_single_run {
        eprintln!(
            "incomparable: single-run vs multi-run ({} is_single_run={}, {} is_single_run={}). \
             Use multi-run for both before comparison.",
            before_path.display(),
            b_env.is_single_run,
            after_path.display(),
            a_env.is_single_run,
        );
        std::process::exit(2);
    }

    let b_hash = comparable_env_hash(b_env);
    let a_hash = comparable_env_hash(a_env);
    if b_hash != a_hash {
        eprintln!(
            "incomparable: comparable_env_hash differs ({} vs {}). \
             Fixture/embedder/provider/model/schema mismatch.",
            b_hash, a_hash
        );
        std::process::exit(2);
    }

    println!("comparable_env_hash: {}", b_hash);
    println!(
        "schema_version: {} | schema_db_version: {:?} | single_run: {}",
        b_env.schema_version, b_env.schema_db_version, b_env.is_single_run
    );
    println!("\nMetric deltas (after - before):");
    print_delta("NDCG@10", before.ndcg_at_10, after.ndcg_at_10);
    print_delta("NDCG@5", before.ndcg_at_5, after.ndcg_at_5);
    print_delta("MAP@10", before.map_at_10, after.map_at_10);
    print_delta("MAP@5", before.map_at_5, after.map_at_5);
    print_delta("MRR", before.mrr, after.mrr);
    print_delta("Recall@5", before.recall_at_5, after.recall_at_5);
    print_delta("Recall@3", before.recall_at_3, after.recall_at_3);
    print_delta("Recall@1", before.recall_at_1, after.recall_at_1);
    print_delta("HitRate@1", before.hit_rate_at_1, after.hit_rate_at_1);
    print_delta("Precision@5", before.precision_at_5, after.precision_at_5);

    if b_env.is_single_run {
        println!(
            "\nNOTE: both baselines are single-run. Deltas are NOT a regression gate. \
             Use the P1.5 multi-run protocol (mean ± stddev over ≥3 runs) for statistical comparison."
        );
    }

    Ok(())
}

/// Print a metric delta. When both sides are exactly 0.0 the metric was
/// likely not computed by the runner that produced the baseline (LoCoMo +
/// LongMemEval runners zero-fill fields they don't measure when
/// projecting via `to_eval_report`). Mark those rows as not-computed so an
/// operator doesn't read the row as evidence of "no change."
fn print_delta(label: &str, before: f64, after: f64) {
    if before == 0.0 && after == 0.0 {
        println!("  {:<12} (not computed by this runner)", label);
        return;
    }
    let delta = after - before;
    println!(
        "  {:<12} {:.4} → {:.4}  (Δ {:+.4})",
        label, before, after, delta
    );
}
