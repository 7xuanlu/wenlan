use super::{LintCatalogEntry, LintCheckGroup, ScopeAxis, ScopePolicy};

pub(crate) fn has_valid_owner(entry: &LintCatalogEntry) -> bool {
    match entry.group {
        LintCheckGroup::Identity => {
            entry.id.starts_with("identity.")
                && matches!(
                    entry.scope_axis,
                    ScopeAxis::IdentityGlobal | ScopeAxis::MemoriesSpace
                )
        }
        LintCheckGroup::KnowledgeGraph => {
            (entry.id.starts_with("entities.")
                || entry.id.starts_with("kg.")
                || entry.id.starts_with("memory_entities.")
                || entry.id.starts_with("observations.")
                || entry.id.starts_with("relations."))
                && matches!(
                    entry.scope_axis,
                    ScopeAxis::EntitiesSpace | ScopeAxis::MemoriesSpace
                )
        }
        LintCheckGroup::Memories => {
            entry.id.starts_with("memories.") && entry.scope_axis == ScopeAxis::MemoriesSpace
        }
        LintCheckGroup::Operations => {
            entry.id.starts_with("operations.")
                && entry.scope_axis == ScopeAxis::OperationsGlobal
                && entry.scope_policy == ScopePolicy::GlobalAggregateOnly
        }
        LintCheckGroup::Pages => {
            entry.id.starts_with("pages.") && entry.scope_axis == ScopeAxis::PagesWorkspace
        }
        LintCheckGroup::Runtime => {
            entry.id.starts_with("runtime.") && entry.scope_axis == ScopeAxis::RuntimeGlobal
        }
        LintCheckGroup::Serving => entry.id.starts_with("serving."),
    }
}
