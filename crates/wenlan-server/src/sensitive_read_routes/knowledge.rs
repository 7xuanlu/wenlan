use super::{
    Capability::*, CrossScopePolicy::*, DirectIdGate::*, Method::*, ScopeOwner::*,
    SelectorPrecedence::*, SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/profile","profile",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/agents","agent_list",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/agents/{name}","agent_detail",None,TrustedRead,Global,Required,Forbidden),
    row!(Post,"/api/memory/entities/list","entity_list",BodyThenHeader,TrustedRead,EntitySpace,NotApplicable,Forbidden),
    row!(Post,"/api/memory/entities/search","entity_search",None,TrustedRead,EntitySpace,Missing,Forbidden),
    row!(Get,"/api/memory/entities/{entity_id}","entity_detail",None,TrustedRead,EntitySpace,Missing,Forbidden),
    row!(Get,"/api/memory/stats","memory_stats",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/home-stats","home_stats",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/entity-suggestions","entity_suggestions",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/spaces","space_list",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/sources","source_list",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/profile/narrative","profile_narrative",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/knowledge/recent-relations","relation_list",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/knowledge/count","knowledge_count",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/onboarding/milestones","onboarding_state",None,LocalRead,Global,NotApplicable,AggregateOnly),
];
