use super::{
    Capability::*, CrossScopePolicy::*, DirectIdGate::*, Method::*, ScopeOwner::*,
    SelectorPrecedence::*, SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Post,"/api/search","memory_search",HeaderThenBody,TrustedRead,MemorySpace,NotApplicable,Forbidden),
    row!(Post,"/api/context","memory_context",HeaderThenBody,TrustedRead,MemorySpace,NotApplicable,Forbidden),
    row!(Get,"/api/retrievals/recent","retrieval_activity",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/recent","memory_activity",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/unconfirmed","memory_list",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Post,"/api/memory/search","memory_search",BodyThenHeader,TrustedRead,MemorySpace,NotApplicable,Forbidden),
    row!(Post,"/api/memory/list","memory_list",BodyThenHeader,TrustedRead,MemorySpace,NotApplicable,Forbidden),
    row!(Get,"/api/memory/nurture","memory_cards",QueryThenHeader,TrustedRead,MemorySpace,NotApplicable,Forbidden),
    row!(Get,"/api/memory/pinned","memory_list",None,TrustedRead,Global,NotApplicable,AggregateOnly),
];
