use super::super::frontmatter::FrontmatterState;
use super::super::fs::{EntryKind, EntryScope, PageScan};
use super::super::path::normalize_target_path;
use super::super::state::{RawStateKind, StateEdge, StateEntryStatus};
use super::assessment::{Assessment, Level};
use super::DbPage;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn evaluate_state(scan: &PageScan, selected_ids: Option<&BTreeSet<&str>>) -> Assessment {
    let mut result = Assessment::default();
    let recognized_projection = scan.page_markdown().iter().any(|entry| {
        entry.frontmatter.origin_id.as_deref().is_some_and(|id| {
            selected_ids.is_none_or(|ids| ids.contains(normalize_id(id).as_str()))
        })
    });
    let root_level = match scan.raw_state.kind {
        RawStateKind::Missing if recognized_projection => Level::Error,
        RawStateKind::Missing => {
            result.push(Level::Pass, true);
            Level::Pass
        }
        RawStateKind::WriterDefaultV0 | RawStateKind::LegacyV1 | RawStateKind::ImplicitV2 => {
            Level::Warning
        }
        RawStateKind::ExplicitV2 => Level::Pass,
        RawStateKind::FutureU32(_) => Level::Prerequisite,
        RawStateKind::NonU32 | RawStateKind::Malformed => Level::Error,
    };
    if !matches!(scan.raw_state.kind, RawStateKind::Missing) || recognized_projection {
        result.push(root_level, false);
    }
    for edge in &scan.raw_state.edges {
        if selected_ids.is_some_and(|ids| !ids.contains(normalize_id(&edge.state_id).as_str())) {
            continue;
        }
        result.push(
            if edge.status == StateEntryStatus::Valid {
                Level::Pass
            } else {
                Level::Error
            },
            false,
        );
    }
    if selected_ids.is_none() {
        for _ in &scan.path_issues {
            result.push(Level::Error, false);
        }
    }
    result
}

pub(super) fn evaluate_identity(scan: &PageScan, pages: &[DbPage], selected: bool) -> Assessment {
    let mut result = Assessment::default();
    let active = ids_by_status(pages, "active");
    let archived = ids_by_status(pages, "archived");
    let db_ids = active.union(&archived).copied().collect::<BTreeSet<_>>();
    let edges = scan
        .raw_state
        .edges
        .iter()
        .filter(|edge| !selected || db_ids.contains(normalize_id(&edge.state_id).as_str()))
        .collect::<Vec<_>>();
    let state_ids = edges
        .iter()
        .map(|edge| normalize_id(&edge.state_id))
        .collect::<BTreeSet<_>>();
    let raw_counts = normalized_state_counts(&edges);
    let collision_edges = target_collision_edges(&edges);
    let frontmatter_counts = frontmatter_counts(scan, selected, &db_ids);

    for page in pages {
        let present = state_ids.contains(page.id.as_str());
        match (page.status.as_str(), present) {
            ("active", false) => result.push(Level::Warning, false),
            ("archived", false) => result.push(Level::Pass, true),
            _ => result.push(Level::Pass, false),
        }
    }
    for (index, edge) in edges.iter().enumerate() {
        let normalized = normalize_id(&edge.state_id);
        let mut level = Level::Pass;
        let mut inventory = false;
        if edge.status != StateEntryStatus::Valid {
            level = Level::Error;
        }
        if edge.state_id.starts_with("concept_") || raw_counts[&normalized] > 1 {
            level = level.max(Level::Warning);
        }
        if collision_edges.contains(&index) {
            level = Level::Error;
        }
        let (edge_level, edge_inventory) = inspect_target(scan, edge);
        level = level.max(edge_level);
        inventory |= edge_inventory;
        if !selected && !db_ids.contains(normalized.as_str()) {
            level = level.max(Level::Warning);
        }
        result.push(level, inventory);
    }
    for entry in scan.page_markdown() {
        let normalized = entry.frontmatter.origin_id.as_deref().map(normalize_id);
        if selected && normalized.as_deref().is_none_or(|id| !db_ids.contains(id)) {
            continue;
        }
        let mut level = match entry.frontmatter.state {
            FrontmatterState::Invalid
            | FrontmatterState::Malformed
            | FrontmatterState::Truncated
            | FrontmatterState::OverLimit
            | FrontmatterState::Unparsed => Level::Error,
            FrontmatterState::Absent | FrontmatterState::Parsed => Level::Pass,
        };
        let mut inventory = false;
        match normalized {
            None => inventory = true,
            Some(id) => {
                if frontmatter_counts.get(&id).copied().unwrap_or_default() > 1 {
                    level = Level::Error;
                }
                if !state_ids.contains(&id) {
                    level = level.max(Level::Warning);
                }
            }
        }
        result.push(level, inventory);
    }
    result
}

fn ids_by_status<'a>(pages: &'a [DbPage], status: &str) -> BTreeSet<&'a str> {
    pages
        .iter()
        .filter(|page| page.status == status)
        .map(|page| page.id.as_str())
        .collect()
}

fn normalized_state_counts(edges: &[&StateEdge]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for edge in edges {
        *counts.entry(normalize_id(&edge.state_id)).or_default() += 1;
    }
    counts
}

fn frontmatter_counts(
    scan: &PageScan,
    selected: bool,
    db_ids: &BTreeSet<&str>,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for entry in scan.page_markdown() {
        let Some(id) = entry.frontmatter.origin_id.as_deref().map(normalize_id) else {
            continue;
        };
        if selected && !db_ids.contains(id.as_str()) {
            continue;
        }
        *counts.entry(id).or_default() += 1;
    }
    counts
}

fn inspect_target(scan: &PageScan, edge: &StateEdge) -> (Level, bool) {
    if edge.status != StateEntryStatus::Valid {
        return (Level::Error, false);
    }
    let Some(raw_target) = edge.raw_target_path.as_deref() else {
        return (Level::Error, false);
    };
    let Ok(target) = normalize_target_path(raw_target) else {
        return (Level::Error, false);
    };
    let target = target.as_str();
    if target.starts_with(".wenlan/")
        || target.starts_with("_sources/")
        || !target.to_ascii_lowercase().ends_with(".md")
        || target_crosses_symlink(scan, target)
    {
        return (Level::Error, false);
    }
    let Some(entry) = scan.entry(target) else {
        return (Level::Warning, false);
    };
    if entry.scope != EntryScope::PageMarkdown {
        return (Level::Error, false);
    }
    match entry.frontmatter.origin_id.as_deref() {
        Some(id) if normalize_id(id) == normalize_id(&edge.state_id) => (Level::Pass, false),
        Some(_) => (Level::Error, false),
        None => (Level::Pass, true),
    }
}

fn target_collision_edges(edges: &[&StateEdge]) -> BTreeSet<usize> {
    let mut collisions = BTreeSet::new();
    let mut exact = BTreeMap::<String, (String, usize)>::new();
    let mut lowercase = BTreeMap::<String, (String, String, usize)>::new();
    for (index, edge) in edges.iter().enumerate() {
        let Some(raw_target) = edge.raw_target_path.as_deref() else {
            continue;
        };
        let Ok(target) = normalize_target_path(raw_target) else {
            continue;
        };
        let target = target.as_str().to_string();
        let identity = normalize_id(&edge.state_id);
        if let Some((previous_identity, previous_index)) =
            exact.insert(target.clone(), (identity.clone(), index))
        {
            if previous_identity != identity {
                collisions.extend([previous_index, index]);
            }
            continue;
        }
        let key = target.to_lowercase();
        if let Some((previous_target, previous_identity, previous_index)) =
            lowercase.insert(key, (target.clone(), identity.clone(), index))
        {
            if previous_target != target || previous_identity != identity {
                collisions.extend([previous_index, index]);
            }
        }
    }
    collisions
}

fn target_crosses_symlink(scan: &PageScan, target: &str) -> bool {
    let mut prefix = String::new();
    target.split('/').any(|component| {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(component);
        scan.entry(&prefix)
            .is_some_and(|entry| entry.kind == EntryKind::Symlink)
    })
}

pub(super) fn normalize_id(id: &str) -> String {
    id.strip_prefix("concept_")
        .map(|suffix| format!("page_{suffix}"))
        .unwrap_or_else(|| id.to_string())
}
