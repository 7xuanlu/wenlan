use axum::handler::Handler;
use axum::routing::MethodRouter;
use axum::Router;
use std::collections::BTreeMap;
use wenlan_core::lint::serving::routes::{self, Method};

pub(crate) struct TrackedRouter<S = ()> {
    inner: Router<S>,
    reads: BTreeMap<(Method, &'static str), usize>,
}

pub(crate) struct TrackedMethodRouter<S = ()> {
    inner: MethodRouter<S>,
    methods: Vec<RegisteredMethod>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegisteredMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl RegisteredMethod {
    const fn sensitive(self) -> Option<Method> {
        match self {
            Self::Get => Some(Method::Get),
            Self::Post => Some(Method::Post),
            Self::Put | Self::Delete => None,
        }
    }
}

macro_rules! top_level_method {
    ($name:ident, $method:ident) => {
        pub(crate) fn $name<H, T, S>(handler: H) -> TrackedMethodRouter<S>
        where
            H: Handler<T, S>,
            T: 'static,
            S: Clone + Send + Sync + 'static,
        {
            TrackedMethodRouter {
                inner: axum::routing::$name(handler),
                methods: vec![RegisteredMethod::$method],
            }
        }
    };
}

top_level_method!(get, Get);
top_level_method!(post, Post);
top_level_method!(put, Put);
top_level_method!(delete, Delete);

impl<S> TrackedMethodRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub(crate) fn post<H, T>(mut self, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: 'static,
    {
        self.inner = self.inner.post(handler);
        self.methods.push(RegisteredMethod::Post);
        self
    }

    pub(crate) fn put<H, T>(mut self, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: 'static,
    {
        self.inner = self.inner.put(handler);
        self.methods.push(RegisteredMethod::Put);
        self
    }

    pub(crate) fn delete<H, T>(mut self, handler: H) -> Self
    where
        H: Handler<T, S>,
        T: 'static,
    {
        self.inner = self.inner.delete(handler);
        self.methods.push(RegisteredMethod::Delete);
        self
    }
}

impl<S> TrackedRouter<S>
where
    S: Clone + Send + Sync + 'static,
{
    pub(crate) fn new() -> Self {
        Self {
            inner: Router::new(),
            reads: BTreeMap::new(),
        }
    }

    pub(crate) fn route(mut self, path: &'static str, route: TrackedMethodRouter<S>) -> Self {
        let rows = route
            .methods
            .iter()
            .filter_map(|method| {
                method
                    .sensitive()
                    .and_then(|method| routes::route(method, path))
            })
            .collect::<Vec<_>>();
        assert!(
            rows.len() == route.methods.len()
                || NON_SENSITIVE_PATHS.contains(&path)
                || route.methods.iter().all(|method| {
                    NON_SENSITIVE_MIXED_ROUTES.contains(&(*method, path))
                        || method
                            .sensitive()
                            .is_some_and(|method| routes::route(method, path).is_some())
                }),
            "unclassified router path: {path}"
        );
        for row in rows {
            *self.reads.entry((row.method, row.path)).or_default() += 1;
        }
        self.inner = self.inner.route(path, route.inner);
        self
    }

    pub(crate) fn finish(self) -> Router<S> {
        let expected = routes::sensitive_read_routes()
            .map(|row| ((row.method, row.path), 1usize))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(self.reads, expected, "sensitive route registration drift");
        self.inner
    }
}

#[rustfmt::skip]
const NON_SENSITIVE_PATHS: &[&str] = &[
    "/api/health", "/api/status", "/api/ping", "/api/llm/test", "/api/shutdown", "/api/debug/pipeline",
    "/api/steep", "/api/distill", "/api/distill/{page_id}",
    "/api/ingest/text", "/api/ingest/webpage", "/api/ingest/memory", "/api/documents/{source}/{source_id}",
    "/api/import/memories", "/api/import/chat-export", "/api/memory/store", "/api/memory/confirm/{source_id}",
    "/api/memory/delete/{source_id}", "/api/memory/reclassify/{source_id}", "/api/memory/revision/{id}/accept",
    "/api/memory/revision/{id}/dismiss", "/api/memory/contradiction/{source_id}/dismiss", "/api/memory/entities",
    "/api/memory/relations", "/api/memory/observations", "/api/memory/link-entity", "/api/spaces/{name}",
    "/api/spaces/{from}/move-to/{to}", "/api/pages/export", "/api/pages/{id}/export", "/api/pages/{id}/archive",
    "/api/refinery/queue/{id}/reject", "/api/refinery/queue/{id}/accept", "/api/sources/{id}", "/api/sources/{id}/sync",
    "/api/config", "/api/config/skip-apps", "/api/setup/status", "/api/setup/anthropic-key", "/api/on-device-model",
    "/api/on-device-model/download", "/api/chunks/{id}/update", "/api/chunks/time-range", "/api/chunks/delete-bulk",
    "/api/memory/entities/{id}/confirm", "/api/memory/entities/{id}/delete", "/api/memory/entities/{entity_id}/observations",
    "/api/memory/observations/{id}", "/api/memory/observations/{id}/confirm", "/api/spaces/{name}/pin",
    "/api/spaces/{name}/confirm", "/api/spaces/reorder", "/api/spaces/{name}/star", "/api/documents/{source_id}/space",
    "/api/tags/{name}", "/api/documents/{source_id}/tags", "/api/memory/{id}/update", "/api/memory/{id}/stability",
    "/api/memory/{id}/correct", "/api/profile/narrative/regenerate", "/api/memory/{id}/pin", "/api/memory/{id}/unpin",
    "/api/snapshots/{id}/delete", "/api/memory/{id}/update-page", "/api/knowledge/path",
    "/api/onboarding/milestones/{id}/acknowledge", "/api/onboarding/reset", "/ws/updates",
];

const NON_SENSITIVE_MIXED_ROUTES: &[(RegisteredMethod, &str)] = &[
    (RegisteredMethod::Put, "/api/profile"),
    (RegisteredMethod::Put, "/api/agents/{name}"),
    (RegisteredMethod::Delete, "/api/agents/{name}"),
    (RegisteredMethod::Post, "/api/spaces"),
    (RegisteredMethod::Put, "/api/spaces/{name}"),
    (RegisteredMethod::Delete, "/api/spaces/{name}"),
    (RegisteredMethod::Post, "/api/pages"),
    (RegisteredMethod::Put, "/api/pages/{id}"),
    (RegisteredMethod::Delete, "/api/pages/{id}"),
    (RegisteredMethod::Post, "/api/sources"),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[should_panic(expected = "unclassified router path")]
    fn dynamic_helper_registration_cannot_bypass_classification() {
        fn helper(router: TrackedRouter) -> TrackedRouter {
            router.route("/api/memory/dynamic-leak", super::get(|| async { "leak" }))
        }
        let _ = helper(TrackedRouter::new());
    }

    #[test]
    #[should_panic(expected = "unclassified router path")]
    fn wrong_method_cannot_satisfy_a_sensitive_registration() {
        let _ = TrackedRouter::<()>::new().route("/api/agents", super::post(|| async { "leak" }));
    }

    #[test]
    #[should_panic(expected = "sensitive route registration drift")]
    fn missing_nested_merged_or_route_service_inventory_fails_finalization() {
        let _ = TrackedRouter::<()>::new().finish();
    }
}
