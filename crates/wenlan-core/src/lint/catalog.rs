#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScopePolicy {
    ScopedRows,
    DbAnchoredProjection,
    GlobalAggregateOnly,
    GlobalOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeAxis {
    PagesWorkspace,
    MemoriesSpace,
    EntitiesSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintCheckGroup {
    KnowledgeGraph,
    Memories,
    Pages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintCatalogEntry {
    pub id: &'static str,
    pub scope_policy: ScopePolicy,
    pub scope_axis: ScopeAxis,
    pub group: LintCheckGroup,
}

const CATALOG: &[LintCatalogEntry] = &[
    entity_entry("entities.partition_inventory", ScopePolicy::ScopedRows),
    entity_entry("entities.structural_integrity", ScopePolicy::ScopedRows),
    entity_entry("kg.advisory_inventory", ScopePolicy::GlobalAggregateOnly),
    entity_entry("kg.aggregate_inventory", ScopePolicy::GlobalAggregateOnly),
    memory_kg_entry("kg.substrate_liveness", ScopePolicy::ScopedRows),
    memory_entry("memories.derived.episode", ScopePolicy::ScopedRows),
    memory_entry("memories.derived.fact", ScopePolicy::ScopedRows),
    memory_entry("memories.derived.page_links", ScopePolicy::ScopedRows),
    memory_entry("memories.derived.summary", ScopePolicy::ScopedRows),
    memory_entry("memories.derived.temporal", ScopePolicy::ScopedRows),
    memory_entry("memories.embedding_integrity", ScopePolicy::ScopedRows),
    memory_entry("memories.enrichment_failures", ScopePolicy::ScopedRows),
    memory_entry("memories.lifecycle_integrity", ScopePolicy::ScopedRows),
    memory_entry("memories.partition_inventory", ScopePolicy::ScopedRows),
    memory_entry("memories.supersession_integrity", ScopePolicy::ScopedRows),
    memory_kg_entry("memory_entities.integrity", ScopePolicy::ScopedRows),
    entity_entry("observations.integrity", ScopePolicy::ScopedRows),
    page_entry("pages.archive_inventory", ScopePolicy::ScopedRows),
    page_entry("pages.citations.partitions", ScopePolicy::ScopedRows),
    page_entry("pages.db.partitions", ScopePolicy::ScopedRows),
    page_entry("pages.duplicate_active_titles", ScopePolicy::ScopedRows),
    page_entry("pages.links.orphan_labels", ScopePolicy::ScopedRows),
    page_entry("pages.project.artifact_inventory", ScopePolicy::GlobalOnly),
    page_entry(
        "pages.projection.identity",
        ScopePolicy::DbAnchoredProjection,
    ),
    page_entry(
        "pages.projection.manifest_inventory",
        ScopePolicy::GlobalAggregateOnly,
    ),
    page_entry(
        "pages.projection.state_contract",
        ScopePolicy::DbAnchoredProjection,
    ),
    page_entry(
        "pages.projection.version_alignment",
        ScopePolicy::DbAnchoredProjection,
    ),
    page_entry(
        "pages.provenance.source_evidence_coverage",
        ScopePolicy::DbAnchoredProjection,
    ),
    page_entry("pages.review_status_inventory", ScopePolicy::ScopedRows),
    entity_entry("relations.integrity", ScopePolicy::ScopedRows),
];

const fn entity_entry(id: &'static str, scope_policy: ScopePolicy) -> LintCatalogEntry {
    LintCatalogEntry {
        id,
        scope_policy,
        scope_axis: ScopeAxis::EntitiesSpace,
        group: LintCheckGroup::KnowledgeGraph,
    }
}

const fn memory_kg_entry(id: &'static str, scope_policy: ScopePolicy) -> LintCatalogEntry {
    LintCatalogEntry {
        id,
        scope_policy,
        scope_axis: ScopeAxis::MemoriesSpace,
        group: LintCheckGroup::KnowledgeGraph,
    }
}

const fn memory_entry(id: &'static str, scope_policy: ScopePolicy) -> LintCatalogEntry {
    LintCatalogEntry {
        id,
        scope_policy,
        scope_axis: ScopeAxis::MemoriesSpace,
        group: LintCheckGroup::Memories,
    }
}

const fn page_entry(id: &'static str, scope_policy: ScopePolicy) -> LintCatalogEntry {
    LintCatalogEntry {
        id,
        scope_policy,
        scope_axis: ScopeAxis::PagesWorkspace,
        group: LintCheckGroup::Pages,
    }
}

pub fn catalog() -> &'static [LintCatalogEntry] {
    CATALOG
}

pub(crate) fn catalog_group(
    group: LintCheckGroup,
) -> impl Iterator<Item = &'static LintCatalogEntry> {
    CATALOG.iter().filter(move |entry| entry.group == group)
}

pub fn catalog_entry(check_id: &str) -> Option<&'static LintCatalogEntry> {
    CATALOG
        .binary_search_by_key(&check_id, |entry| entry.id)
        .ok()
        .map(|index| &CATALOG[index])
}
