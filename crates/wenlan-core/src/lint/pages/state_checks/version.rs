use super::super::frontmatter::VersionValue;
use super::super::fs::PageScan;
use super::assessment::{Assessment, Level};
use super::identity::normalize_id;
use super::DbPage;
use std::collections::BTreeMap;

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
        let state_version = valid_version(edge.state_version);
        let file_version = valid_version(edge.frontmatter.origin_version);
        let db_version = db_versions.get(normalized_id.as_str()).copied();
        let (level, inventory) = if state_version.is_err()
            || file_version.is_err_and(|missing| !missing)
            || db_version.is_some_and(|version| version < 0)
        {
            (Level::Error, false)
        } else {
            let Ok(state_version) = state_version else {
                result.push(Level::Error, false);
                continue;
            };
            match file_version {
                Err(_) => (Level::Pass, true),
                Ok(file_version)
                    if file_version != state_version
                        || db_version.is_some_and(|version| version != state_version) =>
                {
                    (Level::Warning, false)
                }
                Ok(_) => (Level::Pass, false),
            }
        };
        result.push(level, inventory);
    }
    result
}

fn valid_version(value: VersionValue) -> Result<i64, bool> {
    match value {
        VersionValue::Integer(version) if version >= 0 => Ok(version),
        VersionValue::Missing => Err(true),
        VersionValue::Integer(_) | VersionValue::Invalid => Err(false),
    }
}
