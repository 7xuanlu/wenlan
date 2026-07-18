use crate::repair::canonical_lint_review_source_ids;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
enum Action {
    ConsolidateDuplicate,
    CrossSpaceDiscovery,
    DedupMerge,
    DetectContradiction,
    EntityMerge,
    LintRepairReview,
    PageKeepOrArchive,
    PageMerge,
    RelationConflict,
    SuggestEntity,
}

impl Action {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "consolidate_duplicate" => Some(Self::ConsolidateDuplicate),
            "cross_space_discovery" => Some(Self::CrossSpaceDiscovery),
            "dedup_merge" => Some(Self::DedupMerge),
            "detect_contradiction" => Some(Self::DetectContradiction),
            "entity_merge" => Some(Self::EntityMerge),
            "lint_repair_review" => Some(Self::LintRepairReview),
            "page_keep_or_archive" => Some(Self::PageKeepOrArchive),
            "page_merge" => Some(Self::PageMerge),
            "relation_conflict" => Some(Self::RelationConflict),
            "suggest_entity" => Some(Self::SuggestEntity),
            _ => None,
        }
    }

    fn valid_cardinality(self, count: usize) -> bool {
        match self {
            Self::CrossSpaceDiscovery => count >= 2,
            Self::LintRepairReview => count >= 1,
            Self::PageKeepOrArchive | Self::SuggestEntity => count == 1,
            Self::ConsolidateDuplicate
            | Self::DedupMerge
            | Self::DetectContradiction
            | Self::EntityMerge
            | Self::PageMerge
            | Self::RelationConflict => count == 2,
        }
    }

    fn valid_status(self, status: Status) -> bool {
        match self {
            Self::DetectContradiction => match status {
                Status::Pending
                | Status::AwaitingReview
                | Status::AutoApplied
                | Status::Resolved
                | Status::Dismissed => true,
            },
            Self::DedupMerge => match status {
                Status::Pending
                | Status::AwaitingReview
                | Status::AutoApplied
                | Status::Dismissed => true,
                Status::Resolved => false,
            },
            Self::SuggestEntity => match status {
                Status::Pending | Status::AwaitingReview | Status::Dismissed => true,
                Status::AutoApplied | Status::Resolved => false,
            },
            Self::ConsolidateDuplicate => match status {
                Status::Dismissed => true,
                Status::Pending
                | Status::AwaitingReview
                | Status::AutoApplied
                | Status::Resolved => false,
            },
            Self::LintRepairReview => match status {
                Status::AwaitingReview | Status::Resolved | Status::Dismissed => true,
                Status::Pending | Status::AutoApplied => false,
            },
            Self::CrossSpaceDiscovery
            | Self::EntityMerge
            | Self::PageKeepOrArchive
            | Self::PageMerge
            | Self::RelationConflict => match status {
                Status::Pending | Status::AwaitingReview | Status::Resolved | Status::Dismissed => {
                    true
                }
                Status::AutoApplied => false,
            },
        }
    }
}

#[derive(Clone, Copy)]
enum Status {
    Pending,
    AwaitingReview,
    AutoApplied,
    Resolved,
    Dismissed,
}

impl Status {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "awaiting_review" => Some(Self::AwaitingReview),
            "auto_applied" => Some(Self::AutoApplied),
            "resolved" => Some(Self::Resolved),
            "dismissed" => Some(Self::Dismissed),
            _ => None,
        }
    }
}

pub(super) fn valid_refinement_row(action: &str, status: &str, ids: &[String]) -> bool {
    let (Some(action), Some(status)) = (Action::parse(action), Status::parse(status)) else {
        return false;
    };
    let ids_are_valid = ids.iter().all(|id| !id.is_empty() && id.trim() == id)
        && ids.iter().collect::<BTreeSet<_>>().len() == ids.len();
    let canonical_owner_order = !matches!(action, Action::LintRepairReview)
        || canonical_lint_review_source_ids(ids).is_ok_and(|canonical| canonical == ids);
    ids_are_valid
        && canonical_owner_order
        && action.valid_cardinality(ids.len())
        && action.valid_status(status)
}

pub(super) fn valid_refinement_payload(
    id: &str,
    action: &str,
    source_ids: &[String],
    payload: Option<&str>,
) -> bool {
    if action != "lint_repair_review" {
        return true;
    }
    let Some(payload) = payload else {
        return false;
    };
    crate::db::validate_lint_review_contract(id, source_ids, payload).is_ok()
}
