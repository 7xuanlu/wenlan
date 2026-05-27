// SPDX-License-Identifier: Apache-2.0
//! Compare two `EvalReport` baselines.
//!
//! Two subcommands:
//!
//! ## `diff` (default)
//!
//! Aggregate metric delta between two comparable baselines. Refuses
//! incomparable inputs loudly (schema_version mismatch, env-hash mismatch,
//! single-run vs multi-run mismatch, missing env).
//!
//! Usage:
//!   `compare-baselines <before.json> <after.json>`            (legacy form)
//!   `compare-baselines diff <before.json> <after.json>`        (explicit)
//!
//! ## `paired-mcnemar`
//!
//! Paired McNemar exact + mid-p test on per-case NDCG@10 ≥ threshold ("correct")
//! between baseline and treatment. Reports the 2×2 contingency table, exact +
//! mid-p p-values, odds ratio, Wilson CIs, and Newcombe Method 10 paired-diff
//! CI. Comparable-env-hash and schema_version are still enforced.
//!
//! Usage:
//!   `compare-baselines paired-mcnemar <baseline.json> <treatment.json> \
//!        [--threshold 0.5] [--category <name>] [--json]`
//!
//! Exit codes:
//!   0 — comparable + results printed.
//!   1 — usage error or I/O / parse failure.
//!   2 — refused: inputs not comparable, or paired-mcnemar found no matched
//!       per-case rows (likely both baselines pre-date the per-case wiring).

use std::path::PathBuf;

use origin_core::eval::report::{
    comparable_env_hash, paired_mcnemar, EvalReport, PairedMcnemarReport,
};

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        anyhow::bail!("missing arguments");
    }

    let subcommand = match args[0].as_str() {
        "diff" => {
            args.remove(0);
            "diff"
        }
        "paired-mcnemar" => {
            args.remove(0);
            "paired-mcnemar"
        }
        "--help" | "-h" => {
            print_usage();
            return Ok(());
        }
        // Legacy form: `compare-baselines <before> <after>` with no subcommand.
        // Default to diff.
        _ => "diff",
    };

    match subcommand {
        "diff" => run_diff(args),
        "paired-mcnemar" => run_paired_mcnemar(args),
        _ => unreachable!(),
    }
}

fn print_usage() {
    eprintln!(
        "compare-baselines — compare two EvalReport baselines\n\n\
         Usage:\n  \
           compare-baselines <before.json> <after.json>\n  \
           compare-baselines diff <before.json> <after.json>\n  \
           compare-baselines paired-mcnemar <baseline.json> <treatment.json> \
[--threshold 0.5] [--category <name>] [--json]"
    );
}

fn load_report(path: &PathBuf) -> anyhow::Result<EvalReport> {
    serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {}", path.display(), e))
}

fn enforce_comparable(
    before_path: &PathBuf,
    after_path: &PathBuf,
    before: &EvalReport,
    after: &EvalReport,
) -> Option<()> {
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
    Some(())
}

fn run_diff(args: Vec<String>) -> anyhow::Result<()> {
    let mut iter = args.into_iter();
    let before_path: PathBuf = iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: compare-baselines [diff] <before> <after>"))?
        .into();
    let after_path: PathBuf = iter
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: compare-baselines [diff] <before> <after>"))?
        .into();
    if iter.next().is_some() {
        anyhow::bail!("usage: compare-baselines [diff] <before> <after>");
    }

    let before = load_report(&before_path)?;
    let after = load_report(&after_path)?;
    enforce_comparable(&before_path, &after_path, &before, &after);

    let b_env = before.env.as_ref().expect("comparable check passed");
    println!("comparable_env_hash: {}", comparable_env_hash(b_env));
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

fn run_paired_mcnemar(args: Vec<String>) -> anyhow::Result<()> {
    let mut positional: Vec<String> = Vec::new();
    let mut threshold: f64 = 0.5;
    let mut category_filter: Option<String> = None;
    let mut as_json = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--threshold" => {
                let v = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--threshold requires a value"))?;
                threshold = v
                    .parse()
                    .map_err(|e| anyhow::anyhow!("invalid threshold '{}': {}", v, e))?;
            }
            "--category" => {
                category_filter = Some(
                    iter.next()
                        .ok_or_else(|| anyhow::anyhow!("--category requires a value"))?,
                );
            }
            "--json" => as_json = true,
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            _ => positional.push(arg),
        }
    }

    if positional.len() != 2 {
        anyhow::bail!(
            "usage: compare-baselines paired-mcnemar <baseline.json> <treatment.json> \
             [--threshold 0.5] [--category <name>] [--json]"
        );
    }

    let baseline_path: PathBuf = positional[0].clone().into();
    let treatment_path: PathBuf = positional[1].clone().into();
    let baseline = load_report(&baseline_path)?;
    let treatment = load_report(&treatment_path)?;
    enforce_comparable(&baseline_path, &treatment_path, &baseline, &treatment);

    let baseline_cases: Vec<_> = match category_filter.as_ref() {
        Some(cat) => baseline
            .per_case
            .iter()
            .filter(|c| c.category.as_deref() == Some(cat.as_str()))
            .cloned()
            .collect(),
        None => baseline.per_case.clone(),
    };
    let treatment_cases: Vec<_> = match category_filter.as_ref() {
        Some(cat) => treatment
            .per_case
            .iter()
            .filter(|c| c.category.as_deref() == Some(cat.as_str()))
            .cloned()
            .collect(),
        None => treatment.per_case.clone(),
    };

    if baseline_cases.is_empty() || treatment_cases.is_empty() {
        eprintln!(
            "no matching per-case rows: baseline={}, treatment={} \
             (filter: {:?}). Either the baselines pre-date the per-case wiring, \
             or no rows match the requested category.",
            baseline_cases.len(),
            treatment_cases.len(),
            category_filter
        );
        std::process::exit(2);
    }

    let report = paired_mcnemar(&baseline_cases, &treatment_cases, threshold);

    if report.n_matched == 0 {
        eprintln!(
            "paired-mcnemar: 0 matched queries across baseline + treatment \
             (filter: {:?}). Are these reports from the same fixture revision?",
            category_filter
        );
        std::process::exit(2);
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_paired_mcnemar(&report, threshold, category_filter.as_deref());
    }
    Ok(())
}

fn print_paired_mcnemar(r: &PairedMcnemarReport, threshold: f64, category: Option<&str>) {
    println!(
        "Paired McNemar — NDCG@10 ≥ {:.3} threshold{}",
        threshold,
        category
            .map(|c| format!(" (category = {})", c))
            .unwrap_or_default()
    );
    println!("  matched queries:    {}", r.n_matched);
    if r.n_baseline_only > 0 || r.n_treatment_only > 0 {
        println!(
            "  unmatched:          baseline_only={}, treatment_only={}",
            r.n_baseline_only, r.n_treatment_only
        );
    }
    println!();
    println!("  contingency table:");
    println!("                            treatment correct   treatment wrong   total",);
    println!(
        "    baseline correct        a = {:>4}            b = {:>4}        {:>4}",
        r.a,
        r.b,
        r.a + r.b
    );
    println!(
        "    baseline wrong          c = {:>4}            d = {:>4}        {:>4}",
        r.c,
        r.d,
        r.c + r.d
    );
    println!(
        "    total                       {:>4}                {:>4}        {:>4}",
        r.a + r.c,
        r.b + r.d,
        r.a + r.b + r.c + r.d
    );
    println!();
    println!(
        "  baseline accuracy:  {:.4}  95% CI [{:.4}, {:.4}]",
        r.baseline_accuracy, r.baseline_ci_95.0, r.baseline_ci_95.1
    );
    println!(
        "  treatment accuracy: {:.4}  95% CI [{:.4}, {:.4}]",
        r.treatment_accuracy, r.treatment_ci_95.0, r.treatment_ci_95.1
    );
    println!(
        "  Δ accuracy (T−B):   {:+.4}  Newcombe 95% CI [{:+.4}, {:+.4}]",
        r.treatment_accuracy - r.baseline_accuracy,
        r.accuracy_diff_ci_95.0,
        r.accuracy_diff_ci_95.1
    );
    println!();
    println!("  McNemar exact p:    {:.6}", r.exact_p);
    println!("  McNemar mid-p:      {:.6}", r.mid_p);
    println!("  odds ratio (b/c):   {:.4}", r.odds_ratio);
    println!();
    println!(
        "  Decision rule (P0a): treatment ships when mid-p < 0.05 \
         AND Δ accuracy CI lower bound > 0."
    );
    let mid_p_pass = r.mid_p < 0.05;
    let ci_pass = r.accuracy_diff_ci_95.0 > 0.0;
    println!(
        "    mid-p < 0.05:       {} ({:.6})",
        if mid_p_pass { "PASS" } else { "FAIL" },
        r.mid_p
    );
    println!(
        "    Δ CI lower > 0:     {} ({:+.4})",
        if ci_pass { "PASS" } else { "FAIL" },
        r.accuracy_diff_ci_95.0
    );
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
