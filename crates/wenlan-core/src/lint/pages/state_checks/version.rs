use super::super::frontmatter::VersionValue;
use super::super::fs::PageScan;
use super::assessment::{Assessment, Level};
use super::identity::normalize_id;
use super::DbPage;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectionVersionMismatch {
    pub(crate) page_id: String,
    pub(crate) target_path: String,
    pub(crate) database_version: i64,
}

pub(super) fn evaluate_versions(scan: &PageScan, pages: &[DbPage], selected: bool) -> Assessment {
    let mut result = Assessment::default();
    let db_versions = pages
        .iter()
        .map(|page| (page.id.as_str(), page.version))
        .collect::<BTreeMap<_, _>>();
    for edge in &scan.raw_state.edges {
        let normalized_id = normalize_id(&edge.state_id);
        if selected && !db_versions.contains_key(normalized_id.as_str()) {
            continue;
        }
        let (level, inventory) = assess_versions(
            edge.state_version,
            edge.frontmatter.origin_version,
            db_versions.get(normalized_id.as_str()).copied(),
        );
        result.push(level, inventory);
    }
    result
}

pub(crate) fn projection_version_mismatches(
    scan: &PageScan,
    pages: &[DbPage],
    selected: bool,
) -> Vec<ProjectionVersionMismatch> {
    let db_versions = pages
        .iter()
        .map(|page| (page.id.as_str(), page.version))
        .collect::<BTreeMap<_, _>>();
    let mut mismatches = Vec::new();
    for edge in &scan.raw_state.edges {
        let page_id = normalize_id(&edge.state_id);
        let Some(database_version) = db_versions.get(page_id.as_str()).copied() else {
            continue;
        };
        if selected && !db_versions.contains_key(page_id.as_str()) {
            continue;
        }
        let (level, _) = assess_versions(
            edge.state_version,
            edge.frontmatter.origin_version,
            Some(database_version),
        );
        if level == Level::Pass {
            continue;
        }
        let Some(target_path) = edge.target_path.clone() else {
            continue;
        };
        mismatches.push(ProjectionVersionMismatch {
            page_id,
            target_path,
            database_version,
        });
    }
    mismatches.sort_by(|left, right| left.page_id.cmp(&right.page_id));
    mismatches.dedup_by(|left, right| left.page_id == right.page_id);
    mismatches
}

fn assess_versions(
    state_value: VersionValue,
    file_value: VersionValue,
    db_version: Option<i64>,
) -> (Level, bool) {
    let state_version = valid_version(state_value);
    let file_version = valid_version(file_value);
    if state_version.is_err()
        || file_version.is_err_and(|missing| !missing)
        || db_version.is_some_and(|version| version < 0)
    {
        return (Level::Error, false);
    }
    let Ok(state_version) = state_version else {
        return (Level::Error, false);
    };
    match file_version {
        Err(_) if db_version.is_some_and(|version| version != state_version) => {
            (Level::Warning, true)
        }
        Err(_) => (Level::Pass, true),
        Ok(file_version)
            if file_version != state_version
                || db_version.is_some_and(|version| version != state_version) =>
        {
            (Level::Warning, false)
        }
        Ok(_) => (Level::Pass, false),
    }
}

fn valid_version(value: VersionValue) -> Result<i64, bool> {
    match value {
        VersionValue::Integer(version) if version >= 0 => Ok(version),
        VersionValue::Missing => Err(true),
        VersionValue::Integer(_) | VersionValue::Invalid => Err(false),
    }
}
