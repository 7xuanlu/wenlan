use super::super::FACT_STARVATION_ID;
use super::review_tests::{check, insert_memory, metric, run};
use crate::db::tests::test_db;
use wenlan_types::lint::{LintMetricCode, LintOutcome};

#[tokio::test]
async fn snapshot_fact_probe_distinguishes_starvation_from_non_starvation() {
    let (starved, _tmp) = fact_fixture(false).await;
    let finding = temp_env::async_with_vars(
        [
            ("WENLAN_ENABLE_FACT_CHANNEL", Some("1")),
            ("WENLAN_FACT_CHANNEL_LIMIT", Some("1")),
        ],
        run(&starved, Some("work")),
    )
    .await;
    let finding = check(&finding, FACT_STARVATION_ID);
    assert_eq!(finding.outcome(), LintOutcome::Finding);
    assert_eq!(metric(finding, LintMetricCode::EligibleRecords), 1);
    assert_eq!(metric(finding, LintMetricCode::AffectedRecords), 1);
    assert_eq!(finding.evidence().len(), 1);

    let (live, _tmp) = fact_fixture(true).await;
    let pass = temp_env::async_with_vars(
        [
            ("WENLAN_ENABLE_FACT_CHANNEL", Some("1")),
            ("WENLAN_FACT_CHANNEL_LIMIT", Some("1")),
        ],
        run(&live, Some("work")),
    )
    .await;
    let pass = check(&pass, FACT_STARVATION_ID);
    assert_eq!(pass.outcome(), LintOutcome::Pass);
    assert_eq!(metric(pass, LintMetricCode::EligibleRecords), 1);
    assert_eq!(metric(pass, LintMetricCode::AffectedRecords), 0);
    assert!(pass.evidence().is_empty());
}

async fn fact_fixture(selected_near: bool) -> (crate::db::MemoryDB, tempfile::TempDir) {
    let (db, tmp) = test_db().await;
    db.create_space("work", None, false).await.expect("space");
    let memories = if selected_near {
        vec![("work-a", "work")]
    } else {
        vec![
            ("other-a", "other"),
            ("other-b", "other"),
            ("other-c", "other"),
            ("work-a", "work"),
        ]
    };
    for (id, space) in memories {
        insert_memory(&db, id, space).await;
    }
    insert_children(&db, selected_near).await;
    (db, tmp)
}

async fn insert_children(db: &crate::db::MemoryDB, selected_near: bool) {
    let near = vector(1.0, 0.0);
    let far = vector(0.0, 1.0);
    let work = if selected_near { &near } else { &far };
    let children = if selected_near {
        vec![("a-work", "work-a", work)]
    } else {
        vec![
            ("a-probe", "other-a", &near),
            ("b-other", "other-b", &near),
            ("c-other", "other-c", &near),
            ("d-work", "work-a", work),
        ]
    };
    let conn = db.conn.lock().await;
    for (id, parent, embedding) in children {
        conn.execute(
            "INSERT INTO child_vectors (id,parent_kind,parent_id,field,content,embedding) VALUES (?1,'memory',?2,'fact','body',vector32(?3))",
            libsql::params![id, parent, embedding.clone()],
        )
        .await
        .expect("child vector");
    }
}

fn vector(x: f32, y: f32) -> String {
    let mut values = vec!["0"; 768];
    values[0] = if x == 0.0 { "0" } else { "1" };
    values[1] = if y == 0.0 { "0" } else { "1" };
    format!("[{}]", values.join(","))
}
