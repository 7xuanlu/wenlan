use super::{
    Capability::*,
    CrossScopePolicy::*,
    Method::*,
    ScopeBinding::*,
    SelectionGate::{NotApplicable as NoGate, SingleIdMissing},
    SelectorPrecedence::{BodyThenHeader, Missing, NotApplicable as NoSelector},
    SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/profile","profile",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/agents","agent_list",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/agents/{name}","agent_detail",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Post,"/api/memory/entities/list","entity_list",BodyThenHeader,UnauthenticatedLocal,EntitySpace,NoGate,FallsBackUnscoped,Forbidden),
    row!(Post,"/api/memory/entities/search","entity_search",Missing,UnauthenticatedLocal,EntitySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/memory/entities/{entity_id}","entity_detail",Missing,UnauthenticatedLocal,EntitySpace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/stats","memory_stats",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,AggregateOnly),
    row!(Get,"/api/home-stats","home_stats_with_memory_rows",Missing,UnauthenticatedLocal,MemorySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/memory/entity-suggestions","entity_suggestions",Missing,UnauthenticatedLocal,MemorySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/spaces","space_list",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/sources","source_list",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/profile/narrative","profile_narrative",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/knowledge/recent-relations","relation_list",Missing,UnauthenticatedLocal,EntitySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/knowledge/count","knowledge_count",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,AggregateOnly),
    row!(Get,"/api/onboarding/milestones","onboarding_state",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
];
