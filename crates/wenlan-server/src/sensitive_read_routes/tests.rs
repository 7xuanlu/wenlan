use super::{
    route, sensitive_read_routes, Capability, Method, ScopeBinding, SelectionGate,
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
    assert_eq!(search.unknown_scope, UnknownScopePolicy::Rejected);
    assert!(!search.scope_contract_violation());

    let page_search = route(Method::Post, "/api/pages/search").expect("page search");
    assert_eq!(page_search.selector_precedence, SelectorPrecedence::Missing);
    assert!(page_search.scope_contract_violation());

    let home_stats = route(Method::Get, "/api/home-stats").expect("home stats");
    assert_eq!(home_stats.data_class, "home_stats_with_memory_rows");
    assert_eq!(home_stats.scope_binding, ScopeBinding::MemorySpace);
    assert!(home_stats.scope_contract_violation());

    let tags = route(Method::Get, "/api/tags").expect("tags");
    assert_eq!(tags.data_class, "document_tag_map");
    assert_eq!(tags.scope_binding, ScopeBinding::MemorySpace);
    assert!(tags.scope_contract_violation());

    for path in [
        "/api/memory/{id}/detail",
        "/api/memory/by-ids",
        "/api/memory/{id}/versions",
        "/api/chunks/{source_id}",
        "/api/memory/pending-revision/{source_id}",
        "/api/snapshots/{id}/captures",
        "/api/snapshots/{id}/captures-with-content",
    ] {
        assert!(matches!(
            route(Method::Get, path).expect("direct row").selection_gate,
            SelectionGate::SingleIdMissing
                | SelectionGate::BatchMissing
                | SelectionGate::ParentCollectionMissing
        ));
    }
}

#[test]
fn canonical_matrix_freezes_exact_global_and_scoped_keys() {
    const GLOBAL: &[(Method, &str)] = &[
        (Method::Get, "/api/profile"),
        (Method::Get, "/api/agents"),
        (Method::Get, "/api/agents/{name}"),
        (Method::Get, "/api/memory/stats"),
        (Method::Get, "/api/spaces"),
        (Method::Get, "/api/sources"),
        (Method::Get, "/api/profile/narrative"),
        (Method::Get, "/api/knowledge/count"),
        (Method::Get, "/api/onboarding/milestones"),
        (Method::Get, "/api/import/state"),
        (Method::Get, "/api/memory/rejections"),
        (Method::Get, "/api/refinery/queue"),
        (Method::Get, "/api/capture-stats"),
        (Method::Get, "/api/decisions/domains"),
        (Method::Get, "/api/snapshots"),
    ];
    const SCOPED: &[(Method, &str)] = &[
        (Method::Post, "/api/search"),
        (Method::Post, "/api/context"),
        (Method::Get, "/api/memory/recent"),
        (Method::Get, "/api/memory/unconfirmed"),
        (Method::Post, "/api/memory/search"),
        (Method::Post, "/api/memory/list"),
        (Method::Get, "/api/memory/nurture"),
        (Method::Get, "/api/memory/pinned"),
        (Method::Get, "/api/retrievals/recent"),
        (Method::Get, "/api/home-stats"),
        (Method::Get, "/api/memory/{source_id}/enrichment-status"),
        (Method::Get, "/api/memory/{id}/revisions"),
        (Method::Get, "/api/indexed-files"),
        (Method::Get, "/api/chunks/{source_id}"),
        (Method::Get, "/api/activities"),
        (Method::Get, "/api/tags"),
        (Method::Get, "/api/suggest-tags"),
        (Method::Get, "/api/memory/{id}/detail"),
        (Method::Get, "/api/memory/by-ids"),
        (Method::Get, "/api/memory/{id}/versions"),
        (Method::Get, "/api/decisions"),
        (Method::Get, "/api/briefing"),
        (Method::Get, "/api/memory/pending-revisions"),
        (Method::Get, "/api/memory/pending-revision/{source_id}"),
        (Method::Get, "/api/snapshots/{id}/captures"),
        (Method::Get, "/api/snapshots/{id}/captures-with-content"),
        (Method::Get, "/api/pages/recent"),
        (Method::Get, "/api/pages/recent-changes"),
        (Method::Get, "/api/pages"),
        (Method::Post, "/api/pages/search"),
        (Method::Get, "/api/pages/orphan-links"),
        (Method::Get, "/api/pages/{id}"),
        (Method::Get, "/api/pages/{id}/sources"),
        (Method::Get, "/api/pages/{id}/links"),
        (Method::Get, "/api/pages/{id}/revisions"),
        (Method::Post, "/api/memory/entities/list"),
        (Method::Post, "/api/memory/entities/search"),
        (Method::Get, "/api/memory/entities/{entity_id}"),
        (Method::Get, "/api/memory/entity-suggestions"),
        (Method::Get, "/api/knowledge/recent-relations"),
    ];

    let rows = sensitive_read_routes().collect::<Vec<_>>();
    let keys = rows
        .iter()
        .map(|row| (row.method, row.path))
        .collect::<BTreeSet<_>>();
    let global = rows
        .iter()
        .filter(|row| row.scope_binding == ScopeBinding::Global)
        .map(|row| (row.method, row.path))
        .collect::<BTreeSet<_>>();
    let scoped = rows
        .iter()
        .filter(|row| row.scope_binding != ScopeBinding::Global)
        .map(|row| (row.method, row.path))
        .collect::<BTreeSet<_>>();

    assert_eq!(rows.len(), 55);
    assert_eq!(keys.len(), 55, "duplicate sensitive route key");
    assert_eq!(global, GLOBAL.iter().copied().collect());
    assert_eq!(scoped, SCOPED.iter().copied().collect());
    assert_eq!(
        rows.iter()
            .filter(|row| row.scope_contract_violation())
            .count(),
        32
    );
}

#[test]
fn every_canonical_sensitive_row_is_bound_by_router_construction() {
    let state = std::sync::Arc::new(tokio::sync::RwLock::new(
        crate::state::ServerState::default(),
    ));
    let _: crate::router::AppRouter = crate::router::build_router(state);
}
