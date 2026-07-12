use crate::db::tests::test_db;
use crate::lint::context::{CancellationToken, LintClock};
use crate::lint::runner::LintRunner;
use wenlan_types::lint::{LintOutcome, LintQuery};

const KG_CHECKS: [&str; 8] = [
    "entities.partition_inventory",
    "entities.structural_integrity",
    "kg.advisory_inventory",
    "kg.aggregate_inventory",
    "kg.substrate_liveness",
    "memory_entities.integrity",
    "observations.integrity",
    "relations.integrity",
];

#[tokio::test]
async fn empty_uncategorized_scope_keeps_kg_checks_conclusive() {
    let (db, _tmp) = test_db().await;
    let report = LintRunner::new(LintClock::fixed(), CancellationToken::new())
        .run(
            &db,
            &LintQuery {
                profile: None,
                space: Some("uncategorized".to_string()),
            },
            None,
            false,
        )
        .await
        .unwrap();

    for check_id in KG_CHECKS {
        let check = report
            .checks()
            .iter()
            .find(|check| check.check_id() == check_id)
            .unwrap();
        assert_ne!(check.outcome(), LintOutcome::FailedToRun, "{check_id}");
    }
}
