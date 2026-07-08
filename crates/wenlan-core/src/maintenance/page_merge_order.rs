// SPDX-License-Identifier: Apache-2.0

use crate::db::MemoryDB;
use crate::error::WenlanError;
use crate::pages::Page;

#[derive(Debug)]
pub(super) struct OrderedPageMerge {
    pub(super) survivor_id: String,
    pub(super) absorbed_id: String,
}

#[derive(Debug)]
struct PageMergeCandidate {
    id: String,
    user_edited: bool,
    source_count: usize,
    created_at_key: (i64, u32),
}

pub(super) async fn order_survivor(
    db: &MemoryDB,
    left_id: &str,
    right_id: &str,
) -> Result<Option<OrderedPageMerge>, WenlanError> {
    let Some(left) = load_candidate(db, left_id).await? else {
        return Ok(None);
    };
    let Some(right) = load_candidate(db, right_id).await? else {
        return Ok(None);
    };

    let (survivor, absorbed) = if right_beats_left(&left, &right) {
        (right, left)
    } else {
        (left, right)
    };
    Ok(Some(OrderedPageMerge {
        survivor_id: survivor.id,
        absorbed_id: absorbed.id,
    }))
}

async fn load_candidate(
    db: &MemoryDB,
    id: &str,
) -> Result<Option<PageMergeCandidate>, WenlanError> {
    let Some(page) = db.get_page(id).await? else {
        return Ok(None);
    };
    let source_count = source_count(db, &page).await?;
    Ok(Some(PageMergeCandidate {
        id: page.id,
        user_edited: page.user_edited,
        source_count,
        created_at_key: created_at_key(&page.created_at),
    }))
}

async fn source_count(db: &MemoryDB, page: &Page) -> Result<usize, WenlanError> {
    let sources = db.get_page_sources(&page.id).await?;
    if sources.is_empty() {
        Ok(page.source_memory_ids.len())
    } else {
        Ok(sources.len())
    }
}

fn right_beats_left(left: &PageMergeCandidate, right: &PageMergeCandidate) -> bool {
    match (left.user_edited, right.user_edited) {
        (false, true) => true,
        (true, false) => false,
        _ => match right.source_count.cmp(&left.source_count) {
            std::cmp::Ordering::Greater => true,
            std::cmp::Ordering::Less => false,
            std::cmp::Ordering::Equal => {
                right.created_at_key < left.created_at_key
                    || (right.created_at_key == left.created_at_key && right.id < left.id)
            }
        },
    }
}

fn created_at_key(created_at: &str) -> (i64, u32) {
    chrono::DateTime::parse_from_rfc3339(created_at)
        .map(|dt| (dt.timestamp(), dt.timestamp_subsec_nanos()))
        .unwrap_or((i64::MAX, u32::MAX))
}
