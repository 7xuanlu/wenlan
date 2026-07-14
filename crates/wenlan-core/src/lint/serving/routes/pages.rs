use super::{
    Capability::*,
    CrossScopePolicy::*,
    Method::*,
    ScopeBinding::*,
    SelectionGate::{NotApplicable as NoGate, ParentCollectionMissing, SingleIdMissing},
    SelectorPrecedence::{Missing, QueryThenHeader},
    SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/pages/recent","page_list",Missing,UnauthenticatedLocal,PageWorkspace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/pages/recent-changes","page_activity",Missing,UnauthenticatedLocal,PageWorkspace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/pages","page_list",QueryThenHeader,UnauthenticatedLocal,PageWorkspace,NoGate,FallsBackUnscoped,Forbidden),
    row!(Post,"/api/pages/search","page_search",Missing,UnauthenticatedLocal,PageWorkspace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/pages/orphan-links","page_links",Missing,UnauthenticatedLocal,PageWorkspace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/pages/{id}","page_detail",Missing,UnauthenticatedLocal,PageWorkspace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/pages/{id}/sources","page_sources",Missing,UnauthenticatedLocal,PageWorkspace,ParentCollectionMissing,NotApplicable,Forbidden),
    row!(Get,"/api/pages/{id}/links","page_links",Missing,UnauthenticatedLocal,PageWorkspace,ParentCollectionMissing,NotApplicable,Forbidden),
    row!(Get,"/api/pages/{id}/revisions","page_revisions",Missing,UnauthenticatedLocal,PageWorkspace,ParentCollectionMissing,NotApplicable,Forbidden),
];
