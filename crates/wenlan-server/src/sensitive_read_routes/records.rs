use super::{
    Capability::*, CrossScopePolicy::*, DirectIdGate::*, Method::*, ScopeOwner::*,
    SelectorPrecedence::*, SensitiveReadRoute,
};

#[rustfmt::skip]
pub(super) const ROUTES: &[SensitiveReadRoute] = &[
    row!(Get,"/api/import/state","import_state",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/{source_id}/enrichment-status","memory_detail",None,TrustedRead,MemorySpace,Required,Forbidden),
    row!(Get,"/api/memory/{id}/revisions","memory_revisions",None,TrustedRead,MemorySpace,Required,Forbidden),
    row!(Get,"/api/memory/rejections","memory_rejections",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/refinery/queue","refinement_queue",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/indexed-files","indexed_files",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/chunks/{source_id}","document_chunks",None,TrustedRead,MemorySpace,Required,Forbidden),
    row!(Get,"/api/activities","agent_activity",QueryThenHeader,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/tags","tag_list",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/suggest-tags","tag_suggestions",QueryThenHeader,TrustedRead,MemorySpace,Required,Forbidden),
    row!(Get,"/api/capture-stats","capture_stats",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/{id}/detail","memory_detail",None,TrustedRead,MemorySpace,Required,Forbidden),
    row!(Get,"/api/memory/by-ids","memory_list",QueryThenHeader,TrustedRead,MemorySpace,Required,Forbidden),
    row!(Get,"/api/memory/{id}/versions","memory_versions",None,TrustedRead,MemorySpace,Required,Forbidden),
    row!(Get,"/api/decisions","decision_list",QueryThenHeader,TrustedRead,MemorySpace,NotApplicable,Forbidden),
    row!(Get,"/api/decisions/domains","decision_spaces",None,LocalRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/briefing","briefing",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/pending-revisions","memory_revisions",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/memory/pending-revision/{source_id}","memory_revision",None,TrustedRead,MemorySpace,Required,Forbidden),
    row!(Get,"/api/snapshots","snapshot_list",None,TrustedRead,Global,NotApplicable,AggregateOnly),
    row!(Get,"/api/snapshots/{id}/captures","snapshot_captures",None,TrustedRead,Global,Required,Forbidden),
    row!(Get,"/api/snapshots/{id}/captures-with-content","snapshot_content",None,TrustedRead,Global,Required,Forbidden),
];
