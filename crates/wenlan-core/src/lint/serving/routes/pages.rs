use super::{
    Capability::*, CrossScopePolicy::*, DirectIdGate::*, Method::*, ScopeOwner::*,
    SelectorPrecedence::*, SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/pages/recent","page_list",None,UnauthenticatedLocal,PageWorkspace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/pages/recent-changes","page_activity",None,UnauthenticatedLocal,PageWorkspace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/pages","page_list",QueryOnly,UnauthenticatedLocal,PageWorkspace,NotApplicable,FallsBackUnscoped,Forbidden),
    row!(Post,"/api/pages/search","page_search",None,UnauthenticatedLocal,PageWorkspace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/pages/orphan-links","page_links",None,UnauthenticatedLocal,PageWorkspace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/pages/{id}","page_detail",None,UnauthenticatedLocal,PageWorkspace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/pages/{id}/sources","page_sources",None,UnauthenticatedLocal,PageWorkspace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/pages/{id}/links","page_links",None,UnauthenticatedLocal,PageWorkspace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/pages/{id}/revisions","page_revisions",None,UnauthenticatedLocal,PageWorkspace,Missing,NotApplicable,Forbidden),
];
