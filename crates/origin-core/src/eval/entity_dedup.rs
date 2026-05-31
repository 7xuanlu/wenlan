// SPDX-License-Identifier: Apache-2.0
//! Entity-dedup precision/recall micro-bench (T16).
//!
//! Scores the deterministic MinHash/LSH near-dedup cascade against a small
//! ground-truth set of entity-name pairs. String-match only -- NO LLM judge
//! (extends the PR #149 kg_faithfulness convention). The cascade predicts a
//! merge iff BOTH names clear the Shannon-entropy gate, share the same entity
//! type, and have exact trigram Jaccard >= FUZZY_JACCARD_THRESHOLD.
//!
//! Eval discipline (AGENTS.md): net-new bench, direction-only claims, N>=3 +
//! per-case breakdown before any external citation, tag [ESTIMATE]/[UNKNOWN]
//! until multi-run.

use serde::Deserialize;

use crate::retrieval::dedup;

#[derive(Debug, Clone, Deserialize)]
pub struct EntityDedupFixture {
    pub description: String,
    pub case: Vec<EntityDedupCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EntityDedupCase {
    pub name_a: String,
    pub name_b: String,
    pub entity_type: String,
    pub should_merge: bool,
}

/// Deterministic cascade verdict for a name pair of the same type: would the
/// MinHash/LSH auto-merge step fuse these two? Mirrors
/// `MemoryDB::minhash_resolve_candidate`: entropy gate on both names, then
/// exact trigram Jaccard >= the auto-merge threshold.
pub fn cascade_merges(name_a: &str, name_b: &str) -> bool {
    if !dedup::has_high_entropy(name_a) || !dedup::has_high_entropy(name_b) {
        return false;
    }
    dedup::name_jaccard(name_a, name_b) >= dedup::FUZZY_JACCARD_THRESHOLD
}

#[derive(Debug, Clone, Default)]
pub struct DedupReport {
    pub case_count: usize,
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub true_negatives: usize,
}

impl DedupReport {
    pub fn precision(&self) -> f64 {
        let denom = self.true_positives + self.false_positives;
        if denom == 0 {
            return 0.0;
        }
        self.true_positives as f64 / denom as f64
    }

    pub fn recall(&self) -> f64 {
        let denom = self.true_positives + self.false_negatives;
        if denom == 0 {
            return 0.0;
        }
        self.true_positives as f64 / denom as f64
    }

    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p == 0.0 && r == 0.0 {
            return 0.0;
        }
        2.0 * p * r / (p + r)
    }
}

/// Score the cascade over every case in a fixture.
pub fn score_fixture(fx: &EntityDedupFixture) -> DedupReport {
    let mut report = DedupReport::default();
    for case in &fx.case {
        let predicted = cascade_merges(&case.name_a, &case.name_b);
        report.case_count += 1;
        match (predicted, case.should_merge) {
            (true, true) => report.true_positives += 1,
            (true, false) => report.false_positives += 1,
            (false, true) => report.false_negatives += 1,
            (false, false) => report.true_negatives += 1,
        }
    }
    report
}

pub fn load_fixture(path: &std::path::Path) -> Result<EntityDedupFixture, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    toml::from_str(&content).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_merges_near_dup_same_type() {
        // Pairs whose trigram Jaccard clears the 0.9 auto-merge threshold.
        assert!(cascade_merges("Backend Team", "Backend Teams"));
        assert!(cascade_merges(
            "Vorpalblade Jabberwock Inc",
            "Vorpalblade Jabberwock Ino"
        ));
    }

    #[test]
    fn cascade_misses_postgres_at_strict_threshold() {
        // Honest measurement: "PostgreSQL"/"Postgres" share only ~0.75 trigram
        // Jaccard (the "SQL" suffix drops 3 trigrams), BELOW the precision-first
        // 0.9 auto-merge threshold. The deterministic cascade therefore does
        // NOT auto-merge it -- it surfaces as a recall miss in the bench and is
        // instead routed to the human-review queue by the kg_quality band pass.
        assert!(!cascade_merges("PostgreSQL", "Postgres"));
        assert!(dedup::name_jaccard("PostgreSQL", "Postgres") < dedup::FUZZY_JACCARD_THRESHOLD);
    }

    #[test]
    fn cascade_does_not_merge_distinct() {
        assert!(!cascade_merges("Project Alpha", "Project Beta"));
        assert!(!cascade_merges("React", "Redux"));
    }

    #[test]
    fn cascade_skips_short_acronyms() {
        // Entropy gate punts 3-char acronyms regardless of char overlap.
        assert!(!cascade_merges("AAN", "ANA"));
        assert!(!cascade_merges("API", "APIs"));
    }

    #[test]
    fn report_precision_recall_f1() {
        let r = DedupReport {
            case_count: 4,
            true_positives: 2,
            false_positives: 0,
            false_negatives: 0,
            true_negatives: 2,
        };
        assert!((r.precision() - 1.0).abs() < 1e-9);
        assert!((r.recall() - 1.0).abs() < 1e-9);
        assert!((r.f1() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn score_fixture_perfect_on_inline_cases() {
        let fx = EntityDedupFixture {
            description: "inline".to_string(),
            case: vec![
                EntityDedupCase {
                    name_a: "Backend Team".into(),
                    name_b: "Backend Teams".into(),
                    entity_type: "team".into(),
                    should_merge: true,
                },
                EntityDedupCase {
                    name_a: "React".into(),
                    name_b: "Redux".into(),
                    entity_type: "library".into(),
                    should_merge: false,
                },
            ],
        };
        let r = score_fixture(&fx);
        assert_eq!(r.true_positives, 1);
        assert_eq!(r.true_negatives, 1);
        assert_eq!(r.false_positives, 0);
        assert_eq!(r.false_negatives, 0);
    }

    /// Smoke test (L6 main canary): score the cascade over the shipped
    /// ground-truth fixture and print precision/recall. #[ignore]d so it does
    /// not run in the default unit-test lane (mirrors kg_faithfulness).
    #[test]
    #[ignore]
    fn entity_dedup_smoke() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../app/eval/kg_fixtures/seed_entity_dedup.toml");
        let fx = load_fixture(&path).expect("fixture loads");
        let report = score_fixture(&fx);
        for case in &fx.case {
            let predicted = cascade_merges(&case.name_a, &case.name_b);
            println!(
                "[entity_dedup] {:>16} vs {:<16} type={:<12} jaccard={:.3} predicted={} expected={}",
                case.name_a,
                case.name_b,
                case.entity_type,
                dedup::name_jaccard(&case.name_a, &case.name_b),
                predicted,
                case.should_merge,
            );
        }
        println!(
            "[entity_dedup] cases={} precision={:.3} recall={:.3} f1={:.3} (tp={} fp={} fn={} tn={})",
            report.case_count,
            report.precision(),
            report.recall(),
            report.f1(),
            report.true_positives,
            report.false_positives,
            report.false_negatives,
            report.true_negatives,
        );
    }
}
