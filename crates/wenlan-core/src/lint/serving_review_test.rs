use super::super::{
    routes::{route, Capability, DirectIdGate, Method, SelectorPrecedence, UnknownScopePolicy},
    CHANNEL_EPISODE_ID, CHANNEL_PAGE_ID, OBSERVABILITY_ID, RERANKER_ID, ROUTE_SCOPE_ID,
};
use crate::db::tests::test_db;
use crate::lint::context::{CancellationToken, LintClock};
use crate::lint::runner::LintRunner;
use wenlan_types::lint::{
    LintApplicability, LintMetricCode, LintMetricValue, LintOutcome, LintQuery,
};

#[test]
fn route_contract_records_observed_scope_and_trust_semantics() {
    let search = route(Method::Post, "/api/search").expect("search row");
    assert_eq!(
        search.selector_precedence,
        SelectorPrecedence::BodyThenHeader
    );
    assert_eq!(search.capability, Capability::CallerAssertedAgentTrust);
    assert_eq!(search.unknown_scope, UnknownScopePolicy::FallsBackUnscoped);

    let page_search = route(Method::Post, "/api/pages/search").expect("page search row");
    assert_eq!(page_search.selector_precedence, SelectorPrecedence::None);
    assert!(page_search.scope_contract_violation());

    for path in [
        "/api/memory/{id}/detail",
        "/api/memory/by-ids",
        "/api/memory/{id}/versions",
        "/api/chunks/{source_id}",
        "/api/snapshots/{id}/captures-with-content",
    ] {
        let row = route(Method::Get, path).expect("direct read row");
        assert_eq!(row.direct_id_gate, DirectIdGate::Missing);
        assert!(row.scope_contract_violation());
    }
}

#[tokio::test]
async fn route_scope_result_is_derived_from_canonical_contract() {
    let (db, _tmp) = test_db().await;
    let report = run(&db, None).await;
    let result = check(&report, ROUTE_SCOPE_ID);
    let defects = super::super::routes::scope_contract_violations().count() as u64;
    assert!(
        defects > 6,
        "review proved more than the old fixed six defects"
    );
    assert_eq!(metric(result, LintMetricCode::AffectedRecords), defects);
}

#[tokio::test]
async fn page_serving_uses_retrieval_flag_not_projection_flag() {
    let (db, _tmp) = test_db().await;
    insert_memory(&db, "page-eligible", "work").await;
    let disabled = temp_env::async_with_vars(
        [("WENLAN_ENABLE_PAGE_CHANNEL", Some("0"))],
        run_with_projection(&db, false),
    )
    .await;
    let report = temp_env::async_with_vars(
        [("WENLAN_ENABLE_PAGE_CHANNEL", Some("1"))],
        run_with_projection(&db, false),
    )
    .await;
    let page = check(&report, CHANNEL_PAGE_ID);
    assert_eq!(page.outcome(), LintOutcome::Finding);
    assert_eq!(page.applicability(), LintApplicability::Applicable);
    assert_ne!(disabled.config_fingerprint(), report.config_fingerprint());
}

#[tokio::test]
async fn episode_liveness_uses_episode_specific_eligibility() {
    let (db, _tmp) = test_db().await;
    insert_memory_with_content(&db, "short", "work", "too short", 2).await;
    let conn = db.conn.lock().await;
    conn.execute(
        "UPDATE memories SET source_text='one two three four five six seven eight' WHERE source_id='short'",
        (),
    )
    .await
    .expect("source text");
    drop(conn);
    let report = temp_env::async_with_vars(
        [("WENLAN_ENABLE_EPISODE_CHANNEL", Some("1"))],
        run(&db, None),
    )
    .await;
    let episode = check(&report, CHANNEL_EPISODE_ID);
    assert_eq!(metric(episode, LintMetricCode::EligibleRecords), 1);
    assert_eq!(episode.outcome(), LintOutcome::Finding);
}

#[tokio::test]
async fn telemetry_and_reranker_inventory_reports_observed_configuration() {
    let (db, _tmp) = test_db().await;
    let conn = db.conn.lock().await;
    conn.execute(
        "INSERT INTO access_log (source_id,accessed_at) VALUES ('m',1)",
        (),
    )
    .await
    .expect("access row");
    conn.execute("INSERT INTO agent_activity (timestamp,agent_name,action,memory_ids,query,detail) VALUES (1,'agent','search','[]',NULL,'one')", ())
        .await
        .expect("activity row");
    drop(conn);
    let report =
        temp_env::async_with_vars([("WENLAN_RERANKER_MODE", Some("lite"))], run(&db, None)).await;
    let telemetry = check(&report, OBSERVABILITY_ID);
    assert_eq!(metric(telemetry, LintMetricCode::AccessTelemetryRows), 1);
    assert_eq!(
        metric(telemetry, LintMetricCode::AgentActivityTelemetryRows),
        1
    );
    assert_eq!(
        metric(telemetry, LintMetricCode::UnattributedServingChannels),
        2
    );
    let reranker = check(&report, RERANKER_ID);
    assert_eq!(metric(reranker, LintMetricCode::RerankerConfiguredPaths), 2);
    assert_eq!(
        metric(
            reranker,
            LintMetricCode::RerankerRuntimeReadinessUnavailable
        ),
        1
    );
}

pub(super) async fn run(
    db: &crate::db::MemoryDB,
    space: Option<&str>,
) -> wenlan_types::lint::LintReport {
    run_with_projection_and_space(db, false, space).await
}

async fn run_with_projection(
    db: &crate::db::MemoryDB,
    projection: bool,
) -> wenlan_types::lint::LintReport {
    run_with_projection_and_space(db, projection, None).await
}

async fn run_with_projection_and_space(
    db: &crate::db::MemoryDB,
    projection: bool,
    space: Option<&str>,
) -> wenlan_types::lint::LintReport {
    LintRunner::new(LintClock::fixed(), CancellationToken::new())
        .run(
            db,
            &LintQuery {
                space: space.map(str::to_string),
            },
            None,
            projection,
        )
        .await
        .expect("lint report")
}

pub(super) fn check<'a>(
    report: &'a wenlan_types::lint::LintReport,
    id: &str,
) -> &'a wenlan_types::lint::LintCheckResult {
    report
        .checks()
        .iter()
        .find(|check| check.check_id() == id)
        .expect("check")
}

pub(super) fn metric(check: &wenlan_types::lint::LintCheckResult, code: LintMetricCode) -> u64 {
    check
        .metrics()
        .iter()
        .find_map(|metric| {
            (metric.code() == code).then(|| match metric.value() {
                LintMetricValue::Count { value } => *value,
                _ => 0,
            })
        })
        .expect("metric")
}

pub(super) async fn insert_memory(db: &crate::db::MemoryDB, id: &str, space: &str) {
    insert_memory_with_content(db, id, space, "one two three four five six seven eight", 8).await;
}

async fn insert_memory_with_content(
    db: &crate::db::MemoryDB,
    id: &str,
    space: &str,
    content: &str,
    words: i64,
) {
    let conn = db.conn.lock().await;
    conn.execute(
        "INSERT INTO memories (id,content,source,source_id,title,chunk_index,last_modified,chunk_type,stability,supersede_mode,needs_reembed,memory_type,word_count,space,pending_revision,is_recap) VALUES (?1,?3,'memory',?1,?1,0,1,'text','new','hide',1,'fact',?4,?2,0,0)",
        libsql::params![id, space, content, words],
    ).await.expect("memory");
}
