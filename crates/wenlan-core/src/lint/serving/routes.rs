#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Method {
    Get,
    Post,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectorPrecedence {
    None,
    BodyThenHeader,
    QueryOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    UnauthenticatedLocal,
    CallerAssertedAgentTrust,
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
    Enforced,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownScopePolicy {
    NotApplicable,
    FallsBackUnscoped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossScopePolicy {
    Forbidden,
    AggregateOnly,
    MixedRowsAndAggregates,
    GlobalRead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensitiveReadRoute {
    pub method: Method,
    pub path: &'static str,
    pub data_class: &'static str,
    pub selector_precedence: SelectorPrecedence,
    pub capability: Capability,
    pub scope_owner: ScopeOwner,
    pub direct_id_gate: DirectIdGate,
    pub unknown_scope: UnknownScopePolicy,
    pub cross_scope_policy: CrossScopePolicy,
}

impl SensitiveReadRoute {
    pub const fn scope_contract_violation(self) -> bool {
        !matches!(self.scope_owner, ScopeOwner::Global)
            && (matches!(self.direct_id_gate, DirectIdGate::Missing)
                || matches!(self.selector_precedence, SelectorPrecedence::None)
                || matches!(self.unknown_scope, UnknownScopePolicy::FallsBackUnscoped))
    }
}

macro_rules! row {
    ($method:expr,$path:expr,$data:expr,$selector:expr,$capability:expr,$scope:expr,$gate:expr,$unknown:ident,$cross:expr) => {
        SensitiveReadRoute {
            method: $method,
            path: $path,
            data_class: $data,
            selector_precedence: $selector,
            capability: $capability,
            scope_owner: $scope,
            direct_id_gate: $gate,
            unknown_scope: $crate::lint::serving::routes::UnknownScopePolicy::$unknown,
            cross_scope_policy: $cross,
        }
    };
}

mod knowledge;
mod pages;
mod records;
mod retrieval;

pub fn sensitive_read_routes() -> impl Iterator<Item = &'static SensitiveReadRoute> {
    retrieval::ROUTES
        .iter()
        .chain(knowledge::ROUTES)
        .chain(pages::ROUTES)
        .chain(records::ROUTES)
}

pub fn route(method: Method, path: &str) -> Option<&'static SensitiveReadRoute> {
    sensitive_read_routes().find(|row| row.method == method && row.path == path)
}

pub fn scope_contract_violations() -> impl Iterator<Item = &'static SensitiveReadRoute> {
    sensitive_read_routes().filter(|row| row.scope_contract_violation())
}
