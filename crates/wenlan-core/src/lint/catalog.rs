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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LintCatalogEntry {
    pub id: &'static str,
    pub scope_policy: ScopePolicy,
    pub scope_axis: ScopeAxis,
}

const CATALOG: &[LintCatalogEntry] = &[
    entry("pages.archive_inventory", ScopePolicy::ScopedRows),
    entry("pages.citations.partitions", ScopePolicy::ScopedRows),
    entry("pages.db.partitions", ScopePolicy::ScopedRows),
    entry("pages.duplicate_active_titles", ScopePolicy::ScopedRows),
    entry("pages.links.orphan_labels", ScopePolicy::ScopedRows),
    entry("pages.project.artifact_inventory", ScopePolicy::GlobalOnly),
    entry(
        "pages.projection.identity",
        ScopePolicy::DbAnchoredProjection,
    ),
    entry(
        "pages.projection.manifest_inventory",
        ScopePolicy::GlobalAggregateOnly,
    ),
    entry(
        "pages.projection.state_contract",
        ScopePolicy::DbAnchoredProjection,
    ),
    entry(
        "pages.projection.version_alignment",
        ScopePolicy::DbAnchoredProjection,
    ),
    entry(
        "pages.provenance.source_evidence_coverage",
        ScopePolicy::DbAnchoredProjection,
    ),
    entry("pages.review_status_inventory", ScopePolicy::ScopedRows),
];

const fn entry(id: &'static str, scope_policy: ScopePolicy) -> LintCatalogEntry {
    LintCatalogEntry {
        id,
        scope_policy,
        scope_axis: ScopeAxis::PagesWorkspace,
    }
}

pub fn catalog() -> &'static [LintCatalogEntry] {
    CATALOG
}

pub fn catalog_entry(check_id: &str) -> Option<&'static LintCatalogEntry> {
    CATALOG
        .binary_search_by_key(&check_id, |entry| entry.id)
        .ok()
        .map(|index| &CATALOG[index])
}
