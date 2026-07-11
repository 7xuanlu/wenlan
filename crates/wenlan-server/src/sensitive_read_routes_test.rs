use crate::sensitive_read_routes::{
    sensitive_read_routes, Capability, CrossScopePolicy, DirectIdGate, ScopeOwner,
    SelectorPrecedence, MODEL_SELECTOR_PRECEDENCE,
};
use std::collections::BTreeSet;

#[test]
fn sensitive_read_matrix_is_unique_and_complete_for_current_router() {
    let matrix = sensitive_read_routes();
    let keys = matrix
        .iter()
        .map(|row| (row.method, row.path))
        .collect::<BTreeSet<_>>();
    assert_eq!(keys.len(), matrix.len(), "duplicate sensitive read row");

    let expected = registered_sensitive_reads();
    assert_eq!(keys, expected, "sensitive router/matrix coverage drift");
    for (method, path) in &expected {
        let needle = format!("\"{path}\"");
        assert!(
            include_str!("router.rs").contains(&needle),
            "matrix route absent from router: {method:?} {path}"
        );
    }
}

fn registered_sensitive_reads() -> BTreeSet<(crate::sensitive_read_routes::Method, &'static str)> {
    let expected = expected_sensitive_reads();
    let source = include_str!("router.rs");
    let mut registered = BTreeSet::new();
    for chunk in source.split(".route(").skip(1) {
        let Some(path) = chunk.split('"').nth(1) else {
            continue;
        };
        let Some((_, canonical_path)) = expected.iter().find(|(_, item)| *item == path) else {
            if chunk.contains("get(") && !non_sensitive_get_routes().contains(path) {
                panic!("unclassified GET route must be matrixed or allowlisted: {path}");
            }
            if chunk.contains("post(") && !non_sensitive_post_routes().contains(path) {
                panic!("unclassified POST route must be matrixed or allowlisted: {path}");
            }
            continue;
        };
        for method in [
            crate::sensitive_read_routes::Method::Get,
            crate::sensitive_read_routes::Method::Post,
        ] {
            let token = match method {
                crate::sensitive_read_routes::Method::Get => "get(",
                crate::sensitive_read_routes::Method::Post => "post(",
            };
            if chunk.contains(token) && expected.contains(&(method, *canonical_path)) {
                registered.insert((method, *canonical_path));
            }
        }
    }
    registered
}

#[rustfmt::skip]
fn non_sensitive_post_routes() -> BTreeSet<&'static str> {
    [
        "/api/llm/test", "/api/shutdown", "/api/steep", "/api/distill", "/api/distill/{page_id}",
        "/api/ingest/text", "/api/ingest/webpage", "/api/ingest/memory", "/api/import/memories", "/api/import/chat-export",
        "/api/memory/store", "/api/memory/confirm/{source_id}", "/api/memory/reclassify/{source_id}",
        "/api/memory/revision/{id}/accept", "/api/memory/revision/{id}/dismiss", "/api/memory/contradiction/{source_id}/dismiss",
        "/api/memory/entities", "/api/memory/relations", "/api/memory/observations", "/api/memory/link-entity",
        "/api/spaces", "/api/spaces/{from}/move-to/{to}", "/api/pages", "/api/pages/export", "/api/pages/{id}/export", "/api/pages/{id}/archive",
        "/api/refinery/queue/{id}/reject", "/api/refinery/queue/{id}/accept", "/api/sources", "/api/sources/{id}/sync",
        "/api/on-device-model/download", "/api/chunks/delete-bulk", "/api/memory/entities/{entity_id}/observations",
        "/api/spaces/{name}/pin", "/api/spaces/{name}/confirm", "/api/spaces/reorder", "/api/spaces/{name}/star",
        "/api/documents/{source_id}/space", "/api/memory/{id}/correct", "/api/profile/narrative/regenerate",
        "/api/memory/{id}/pin", "/api/memory/{id}/unpin", "/api/snapshots/{id}/delete", "/api/memory/{id}/update-page",
        "/api/onboarding/milestones/{id}/acknowledge", "/api/onboarding/reset",
    ].into_iter().collect()
}

fn non_sensitive_get_routes() -> BTreeSet<&'static str> {
    [
        "/api/health",
        "/api/status",
        "/api/ping",
        "/api/debug/pipeline",
        "/api/config",
        "/api/config/skip-apps",
        "/api/setup/status",
        "/api/on-device-model",
        "/api/knowledge/path",
        "/ws/updates",
    ]
    .into_iter()
    .collect()
}

#[test]
fn every_row_freezes_all_route_capabilities() {
    assert_eq!(
        MODEL_SELECTOR_PRECEDENCE,
        crate::sensitive_read_routes::ModelSelectorPrecedence::EnvironmentThenConfigThenDefault
    );
    for row in sensitive_read_routes() {
        assert!(!row.data_class.is_empty());
        assert!(matches!(
            row.selector_precedence,
            SelectorPrecedence::None
                | SelectorPrecedence::HeaderThenBody
                | SelectorPrecedence::BodyThenHeader
                | SelectorPrecedence::QueryThenHeader
        ));
        assert!(matches!(
            row.capability,
            Capability::LocalRead | Capability::TrustedRead
        ));
        assert!(matches!(
            row.scope_owner,
            ScopeOwner::Global
                | ScopeOwner::MemorySpace
                | ScopeOwner::PageWorkspace
                | ScopeOwner::EntitySpace
        ));
        assert!(matches!(
            row.direct_id_gate,
            DirectIdGate::NotApplicable | DirectIdGate::Required | DirectIdGate::Missing
        ));
        assert!(matches!(
            row.cross_scope_policy,
            CrossScopePolicy::Forbidden | CrossScopePolicy::AggregateOnly
        ));
    }
}

#[test]
fn known_direct_page_and_entity_scope_bypasses_remain_fail_loud() {
    let missing = sensitive_read_routes()
        .into_iter()
        .filter(|row| row.direct_id_gate == DirectIdGate::Missing)
        .map(|row| row.path)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        missing,
        [
            "/api/memory/entities/search",
            "/api/memory/entities/{entity_id}",
            "/api/pages/{id}",
            "/api/pages/{id}/sources",
            "/api/pages/{id}/links",
            "/api/pages/{id}/revisions",
        ]
        .into_iter()
        .collect()
    );
}

fn expected_sensitive_reads() -> BTreeSet<(crate::sensitive_read_routes::Method, &'static str)> {
    use crate::sensitive_read_routes::Method::{Get, Post};
    [
        (Post, "/api/search"),
        (Post, "/api/context"),
        (Get, "/api/retrievals/recent"),
        (Get, "/api/memory/recent"),
        (Get, "/api/memory/unconfirmed"),
        (Get, "/api/pages/recent"),
        (Get, "/api/pages/recent-changes"),
        (Get, "/api/import/state"),
        (Post, "/api/memory/search"),
        (Post, "/api/memory/list"),
        (Get, "/api/memory/{source_id}/enrichment-status"),
        (Get, "/api/profile"),
        (Get, "/api/agents"),
        (Get, "/api/agents/{name}"),
        (Post, "/api/memory/entities/list"),
        (Post, "/api/memory/entities/search"),
        (Get, "/api/memory/entities/{entity_id}"),
        (Get, "/api/memory/stats"),
        (Get, "/api/home-stats"),
        (Get, "/api/memory/entity-suggestions"),
        (Get, "/api/memory/nurture"),
        (Get, "/api/spaces"),
        (Get, "/api/pages"),
        (Post, "/api/pages/search"),
        (Get, "/api/pages/orphan-links"),
        (Get, "/api/pages/{id}"),
        (Get, "/api/pages/{id}/sources"),
        (Get, "/api/pages/{id}/links"),
        (Get, "/api/pages/{id}/revisions"),
        (Get, "/api/memory/{id}/revisions"),
        (Get, "/api/memory/rejections"),
        (Get, "/api/refinery/queue"),
        (Get, "/api/sources"),
        (Get, "/api/indexed-files"),
        (Get, "/api/chunks/{source_id}"),
        (Get, "/api/activities"),
        (Get, "/api/tags"),
        (Get, "/api/suggest-tags"),
        (Get, "/api/capture-stats"),
        (Get, "/api/memory/{id}/detail"),
        (Get, "/api/memory/by-ids"),
        (Get, "/api/memory/{id}/versions"),
        (Get, "/api/decisions"),
        (Get, "/api/decisions/domains"),
        (Get, "/api/briefing"),
        (Get, "/api/profile/narrative"),
        (Get, "/api/memory/pinned"),
        (Get, "/api/memory/pending-revisions"),
        (Get, "/api/memory/pending-revision/{source_id}"),
        (Get, "/api/snapshots"),
        (Get, "/api/snapshots/{id}/captures"),
        (Get, "/api/snapshots/{id}/captures-with-content"),
        (Get, "/api/knowledge/recent-relations"),
        (Get, "/api/knowledge/count"),
        (Get, "/api/onboarding/milestones"),
    ]
    .into_iter()
    .collect()
}
