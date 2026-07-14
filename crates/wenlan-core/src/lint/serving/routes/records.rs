use super::{
    Capability::*,
    CrossScopePolicy::*,
    Method::*,
    ScopeBinding::*,
    SelectionGate::{
        BatchMissing, NotApplicable as NoGate, ParentCollectionMissing, SingleIdMissing,
    },
    SelectorPrecedence::{Missing, NotApplicable as NoSelector, QueryThenHeader},
    SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/import/state","import_state",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/memory/{source_id}/enrichment-status","memory_detail",Missing,UnauthenticatedLocal,MemorySpace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/{id}/revisions","memory_revisions",Missing,UnauthenticatedLocal,MemorySpace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/rejections","memory_rejections",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/refinery/queue","refinement_queue",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/indexed-files","indexed_files",Missing,UnauthenticatedLocal,MemorySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/chunks/{source_id}","document_chunks",Missing,UnauthenticatedLocal,MemorySpace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/activities","agent_activity",Missing,UnauthenticatedLocal,MemorySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/tags","document_tag_map",Missing,UnauthenticatedLocal,MemorySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/suggest-tags","tag_suggestions",Missing,UnauthenticatedLocal,MemorySpace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/capture-stats","capture_stats",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/{id}/detail","memory_detail",Missing,UnauthenticatedLocal,MemorySpace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/by-ids","memory_list",Missing,UnauthenticatedLocal,MemorySpace,BatchMissing,NotApplicable,Forbidden),
    row!(Get,"/api/memory/{id}/versions","memory_versions",Missing,UnauthenticatedLocal,MemorySpace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/decisions","decision_list",QueryThenHeader,UnauthenticatedLocal,MemorySpace,NoGate,FallsBackUnscoped,Forbidden),
    row!(Get,"/api/decisions/domains","decision_spaces",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,AggregateOnly),
    row!(Get,"/api/briefing","briefing",Missing,UnauthenticatedLocal,MemorySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/memory/pending-revisions","memory_revisions",Missing,UnauthenticatedLocal,MemorySpace,NoGate,NotApplicable,Forbidden),
    row!(Get,"/api/memory/pending-revision/{source_id}","memory_revision",Missing,UnauthenticatedLocal,MemorySpace,SingleIdMissing,NotApplicable,Forbidden),
    row!(Get,"/api/snapshots","snapshot_list",NoSelector,UnauthenticatedLocal,Global,NoGate,NotApplicable,GlobalRead),
    row!(Get,"/api/snapshots/{id}/captures","snapshot_captures",Missing,UnauthenticatedLocal,MemorySpace,ParentCollectionMissing,NotApplicable,Forbidden),
    row!(Get,"/api/snapshots/{id}/captures-with-content","snapshot_content",Missing,UnauthenticatedLocal,MemorySpace,ParentCollectionMissing,NotApplicable,Forbidden),
];
