use super::{
    Capability::*, CrossScopePolicy::*, DirectIdGate::*, Method::*, ScopeOwner::*,
    SelectorPrecedence::*, SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/pages/recent","page_list",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/pages/recent-changes","page_activity",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/pages","page_list",QueryThenHeader,TrustedRead,PageWorkspace,NotApplicable,Forbidden),
    row!(Post,"/api/pages/search","page_search",None,TrustedRead,PageWorkspace,NotApplicable,Forbidden),
    row!(Get,"/api/pages/orphan-links","page_links",None,TrustedRead,PageWorkspace,NotApplicable,AggregateOnly),
    row!(Get,"/api/pages/{id}","page_detail",None,TrustedRead,PageWorkspace,Missing,Forbidden),
    row!(Get,"/api/pages/{id}/sources","page_sources",None,TrustedRead,PageWorkspace,Missing,Forbidden),
    row!(Get,"/api/pages/{id}/links","page_links",None,TrustedRead,PageWorkspace,Missing,Forbidden),
    row!(Get,"/api/pages/{id}/revisions","page_revisions",None,TrustedRead,PageWorkspace,Missing,Forbidden),
];
