use super::fs::PathIssue;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct NormalizedTarget(String);

impl fmt::Debug for NormalizedTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NormalizedTarget")
            .field("component_count", &self.0.split('/').count())
            .finish()
    }
}

impl NormalizedTarget {
    pub(super) fn from_scanned(path: &str) -> Self {
        Self(path.replace('\\', "/"))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetPathError {
    Absolute,
    Drive,
    Unc,
    Parent,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PathIssueKind {
    Absolute,
    Drive,
    Unc,
    Parent,
    Empty,
    SymlinkTraversal,
    ExactDuplicate,
    LowercaseCollision,
}

pub(crate) fn normalize_target_path(raw: &str) -> Result<NormalizedTarget, TargetPathError> {
    if raw.starts_with("//") || raw.starts_with("\\\\") {
        return Err(TargetPathError::Unc);
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(TargetPathError::Absolute);
    }
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(TargetPathError::Drive);
    }

    let parts = raw
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Err(TargetPathError::Empty);
    }
    if parts.contains(&"..") {
        return Err(TargetPathError::Parent);
    }
    Ok(NormalizedTarget(parts.join("/")))
}

pub(super) fn collect_path_issue(error: TargetPathError) -> PathIssueKind {
    match error {
        TargetPathError::Absolute => PathIssueKind::Absolute,
        TargetPathError::Drive => PathIssueKind::Drive,
        TargetPathError::Unc => PathIssueKind::Unc,
        TargetPathError::Parent => PathIssueKind::Parent,
        TargetPathError::Empty => PathIssueKind::Empty,
    }
}

pub(super) fn duplicate_issues(paths: &[NormalizedTarget]) -> Vec<PathIssue> {
    let mut exact = BTreeMap::<&str, usize>::new();
    let mut lowercase = BTreeMap::<String, &str>::new();
    let mut issues = Vec::new();
    for path in paths {
        let value = path.as_str();
        if exact.insert(value, 1).is_some() {
            issues.push(PathIssue {
                kind: PathIssueKind::ExactDuplicate,
            });
            continue;
        }
        let key = value.to_lowercase();
        if lowercase
            .insert(key, value)
            .is_some_and(|previous| previous != value)
        {
            issues.push(PathIssue {
                kind: PathIssueKind::LowercaseCollision,
            });
        }
    }
    issues
}
