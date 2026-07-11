use super::{
    Capability::*, CrossScopePolicy::*, DirectIdGate::*, Method::*, ScopeOwner::*,
    SelectorPrecedence::*, SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/import/state","import_state",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/memory/{source_id}/enrichment-status","memory_detail",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/{id}/revisions","memory_revisions",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/rejections","memory_rejections",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/refinery/queue","refinement_queue",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/indexed-files","indexed_files",None,UnauthenticatedLocal,MemorySpace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/chunks/{source_id}","document_chunks",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/activities","agent_activity",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/tags","tag_list",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,AggregateOnly),
    row!(Get,"/api/suggest-tags","tag_suggestions",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/capture-stats","capture_stats",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/{id}/detail","memory_detail",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/by-ids","memory_list",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/{id}/versions","memory_versions",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/decisions","decision_list",QueryOnly,UnauthenticatedLocal,MemorySpace,NotApplicable,FallsBackUnscoped,Forbidden),
    row!(Get,"/api/decisions/domains","decision_spaces",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,AggregateOnly),
    row!(Get,"/api/briefing","briefing",None,UnauthenticatedLocal,MemorySpace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/memory/pending-revisions","memory_revisions",None,UnauthenticatedLocal,MemorySpace,NotApplicable,NotApplicable,Forbidden),
    row!(Get,"/api/memory/pending-revision/{source_id}","memory_revision",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/snapshots","snapshot_list",None,UnauthenticatedLocal,Global,NotApplicable,NotApplicable,GlobalRead),
    row!(Get,"/api/snapshots/{id}/captures","snapshot_captures",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
    row!(Get,"/api/snapshots/{id}/captures-with-content","snapshot_content",None,UnauthenticatedLocal,MemorySpace,Missing,NotApplicable,Forbidden),
];
