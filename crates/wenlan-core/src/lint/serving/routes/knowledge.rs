use super::{
    Capability::*, CrossScopePolicy::*, DirectIdGate::*, Method::*, ScopeOwner::*,
    SelectorPrecedence::*, SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/profile","profile",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/agents","agent_list",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/agents/{name}","agent_detail",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Post,"/api/memory/entities/list","entity_list",BodyThenHeader,UnauthenticatedLocal,EntitySpace,NotApplicable,FallsBackUnscoped,Forbidden),
    row!(Post,"/api/memory/entities/search","entity_search",None,UnauthenticatedLocal,EntitySpace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/memory/entities/{entity_id}","entity_detail",None,UnauthenticatedLocal,EntitySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/stats","memory_stats",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,AggregateOnly),
    row!(Get,"/api/home-stats","home_stats",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/entity-suggestions","entity_suggestions",None,UnauthenticatedLocal,EntitySpace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/spaces","space_list",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/sources","source_list",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/profile/narrative","profile_narrative",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/knowledge/recent-relations","relation_list",None,UnauthenticatedLocal,EntitySpace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/knowledge/count","knowledge_count",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,AggregateOnly),
    row!(Get,"/api/onboarding/milestones","onboarding_state",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
];
