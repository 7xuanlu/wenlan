use super::{
    Capability::*, CrossScopePolicy::*, DirectIdGate::*, Method::*, ScopeOwner::*,
    SelectorPrecedence::*, SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Post,"/api/search","memory_search",BodyThenHeader,CallerAssertedAgentTrust,MemorySpace,NotApplicable,FallsBackUnscoped,Forbidden),
    row!(Post,"/api/context","memory_context",BodyThenHeader,CallerAssertedAgentTrust,MemorySpace,NotApplicable,FallsBackUnscoped,Forbidden),
    row!(Get,"/api/retrievals/recent","retrieval_activity",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/memory/recent","memory_activity",None,UnauthenticatedLocal,MemorySpace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/memory/unconfirmed","memory_list",None,UnauthenticatedLocal,MemorySpace,NotApplicable,NotApplicable,Forbidden),
    row!(Post,"/api/memory/search","memory_search",BodyThenHeader,CallerAssertedAgentTrust,MemorySpace,NotApplicable,FallsBackUnscoped,Forbidden),
    row!(Post,"/api/memory/list","memory_list",BodyThenHeader,UnauthenticatedLocal,MemorySpace,NotApplicable,FallsBackUnscoped,Forbidden),
    row!(Get,"/api/memory/nurture","memory_cards",QueryOnly,UnauthenticatedLocal,MemorySpace,NotApplicable,FallsBackUnscoped,Forbidden),
    row!(Get,"/api/memory/pinned","memory_list",None,UnauthenticatedLocal,MemorySpace,NotApplicable,NotApplicable,Forbidden),
];
