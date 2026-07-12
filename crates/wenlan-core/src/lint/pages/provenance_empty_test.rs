use super::*;
use crate::db::tests::test_db;
use crate::lint::context::{
    AppliedScope, CancellationToken, ExecutionGate, LintClock, LintContext,
};
use wenlan_types::lint::{LintMetricCode, LintMetricValue, LintOutcome};

#[tokio::test]
async fn empty_fresh_database_has_conclusive_zero_source_coverage() {
    let (db, _tmp) = test_db().await;
    let snapshot = db.open_lint_snapshot().await.unwrap();
    let scope = AppliedScope::global();
    let clock = LintClock::fixed();
    let gate = ExecutionGate::new(CancellationToken::new());
    let context = LintContext::new(
        &snapshot,
        &scope,
        None,
        &clock,
        &gate,
        wenlan_types::lint::LintProfile::General,
    );

    let checks = run(&context).await;
    let source = checks
        .iter()
        .find(|check| check.check_id() == SOURCE_COVERAGE_ID)
        .unwrap();

    assert_eq!(source.outcome(), LintOutcome::Pass);
    assert_eq!(source.coverage().denominator(), 0);
    for code in [
        LintMetricCode::EligibleRecords,
        LintMetricCode::ObservedRecords,
        LintMetricCode::AffectedRecords,
    ] {
        let value = source
            .metrics()
            .iter()
            .find(|metric| metric.code() == code)
            .map(|metric| metric.value());
        assert_eq!(value, Some(&LintMetricValue::Count { value: 0 }));
    }
}
