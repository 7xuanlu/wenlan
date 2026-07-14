use super::{
    Capability::*,
    CrossScopePolicy::*,
    Method::*,
    ScopeBinding::*,
    SelectionGate::{NotApplicable as NoGate, ParentCollectionFiltered, SingleId404},
    SelectorPrecedence::{BodyThenHeader, HeaderOnly, QueryThenHeader},
    SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/pages/recent","page_list",HeaderOnly,UnauthenticatedLocal,PageWorkspace,NoGate,Rejected,Forbidden),
    row!(Get,"/api/pages/recent-changes","page_activity",HeaderOnly,UnauthenticatedLocal,PageWorkspace,NoGate,Rejected,Forbidden),
    row!(Get,"/api/pages","page_list",QueryThenHeader,UnauthenticatedLocal,PageWorkspace,NoGate,Rejected,Forbidden),
    row!(Post,"/api/pages/search","page_search",BodyThenHeader,UnauthenticatedLocal,PageWorkspace,NoGate,Rejected,Forbidden),
    row!(Get,"/api/pages/orphan-links","page_links",HeaderOnly,UnauthenticatedLocal,PageWorkspace,NoGate,Rejected,Forbidden),
    row!(Get,"/api/pages/{id}","page_detail",HeaderOnly,UnauthenticatedLocal,PageWorkspace,SingleId404,Rejected,Forbidden),
    row!(Get,"/api/pages/{id}/sources","page_sources",HeaderOnly,UnauthenticatedLocal,PageWorkspace,ParentCollectionFiltered,Rejected,Forbidden),
    row!(Get,"/api/pages/{id}/links","page_links",HeaderOnly,UnauthenticatedLocal,PageWorkspace,ParentCollectionFiltered,Rejected,Forbidden),
    row!(Get,"/api/pages/{id}/revisions","page_revisions",HeaderOnly,UnauthenticatedLocal,PageWorkspace,ParentCollectionFiltered,Rejected,Forbidden),
];
