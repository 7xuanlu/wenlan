// SPDX-License-Identifier: Apache-2.0
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Method {
    Get,
    Post,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorPrecedence {
    None,
    HeaderThenBody,
    BodyThenHeader,
    QueryThenHeader,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    LocalRead,
    TrustedRead,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeOwner {
    Global,
    MemorySpace,
    PageWorkspace,
    EntitySpace,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectIdGate {
    NotApplicable,
    Required,
    Missing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossScopePolicy {
    Forbidden,
    AggregateOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSelectorPrecedence {
    EnvironmentThenConfigThenDefault,
}

pub const MODEL_SELECTOR_PRECEDENCE: ModelSelectorPrecedence =
    ModelSelectorPrecedence::EnvironmentThenConfigThenDefault;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensitiveReadRoute {
    pub method: Method,
    pub path: &'static str,
    pub data_class: &'static str,
    pub selector_precedence: SelectorPrecedence,
    pub capability: Capability,
    pub scope_owner: ScopeOwner,
    pub direct_id_gate: DirectIdGate,
    pub cross_scope_policy: CrossScopePolicy,
}

macro_rules! row {
    ($method:expr, $path:expr, $data:expr, $selector:expr, $capability:expr, $scope:expr, $gate:expr, $cross_scope:expr) => {
        SensitiveReadRoute {
            method: $method,
            path: $path,
            data_class: $data,
            selector_precedence: $selector,
            capability: $capability,
            scope_owner: $scope,
            direct_id_gate: $gate,
            cross_scope_policy: $cross_scope,
        }
    };
}
mod knowledge;
mod pages;
mod records;
mod retrieval;

pub fn sensitive_read_routes() -> Vec<SensitiveReadRoute> {
    let capacity = retrieval::ROUTES.len()
        + knowledge::ROUTES.len()
        + pages::ROUTES.len()
        + records::ROUTES.len();
    let mut routes = Vec::with_capacity(capacity);
    routes.extend_from_slice(retrieval::ROUTES);
    routes.extend_from_slice(knowledge::ROUTES);
    routes.extend_from_slice(pages::ROUTES);
    routes.extend_from_slice(records::ROUTES);
    routes
}
