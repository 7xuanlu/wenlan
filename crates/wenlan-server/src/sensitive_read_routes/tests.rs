use super::{
    route, sensitive_read_routes, Capability, CrossScopePolicy, DirectIdGate, Method,
    SelectorPrecedence, UnknownScopePolicy,
};
use std::collections::BTreeSet;

#[test]
fn canonical_matrix_is_unique_and_matches_observed_handler_contracts() {
    let rows = sensitive_read_routes().collect::<Vec<_>>();
    let keys = rows
        .iter()
        .map(|row| (row.method, row.path))
        .collect::<BTreeSet<_>>();
    assert_eq!(keys.len(), rows.len(), "duplicate sensitive read row");

    let search = route(Method::Post, "/api/search").expect("search");
    assert_eq!(
        search.selector_precedence,
        SelectorPrecedence::BodyThenHeader
    );
    assert_eq!(search.capability, Capability::CallerAssertedAgentTrust);
    assert_eq!(search.unknown_scope, UnknownScopePolicy::FallsBackUnscoped);

    let page_search = route(Method::Post, "/api/pages/search").expect("page search");
    assert_eq!(page_search.selector_precedence, SelectorPrecedence::None);
    assert!(page_search.scope_contract_violation());

    let home_stats = route(Method::Get, "/api/home-stats").expect("home stats");
    assert_eq!(home_stats.data_class, "home_stats_with_memory_rows");
    assert_eq!(
        home_stats.cross_scope_policy,
        CrossScopePolicy::MixedRowsAndAggregates
    );

    let tags = route(Method::Get, "/api/tags").expect("tags");
    assert_eq!(tags.data_class, "document_tag_map");
    assert_eq!(tags.cross_scope_policy, CrossScopePolicy::GlobalRead);

    for path in [
        "/api/memory/{id}/detail",
        "/api/memory/by-ids",
        "/api/memory/{id}/versions",
        "/api/chunks/{source_id}",
        "/api/memory/pending-revision/{source_id}",
        "/api/snapshots/{id}/captures",
        "/api/snapshots/{id}/captures-with-content",
    ] {
        assert_eq!(
            route(Method::Get, path).expect("direct row").direct_id_gate,
            DirectIdGate::Missing
        );
    }
}

#[test]
fn every_canonical_sensitive_row_is_bound_by_router_construction() {
    let state = std::sync::Arc::new(tokio::sync::RwLock::new(
        crate::state::ServerState::default(),
    ));
    let _: crate::router::AppRouter = crate::router::build_router(state);
}
