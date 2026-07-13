use super::frontmatter::{Frontmatter, VersionValue};
use sha2::{Digest, Sha256};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawStateKind {
    Missing,
    Malformed,
    WriterDefaultV0,
    LegacyV1,
    ImplicitV2,
    ExplicitV2,
    FutureU32(u32),
    NonU32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawStateIssue {
    InvalidJson,
    RootNotObject,
    MissingCollection,
    InvalidCollection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateEntryStatus {
    Valid,
    Malformed(StateEntryIssue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StateEntryIssue {
    NotObject,
    MissingFile,
    InvalidFile,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct StateEdge {
    pub(crate) state_id: String,
    pub(crate) raw_target_path: Option<String>,
    pub(crate) target_path: Option<String>,
    pub(crate) state_version: VersionValue,
    pub(crate) frontmatter: Frontmatter,
    pub(crate) status: StateEntryStatus,
}

impl fmt::Debug for StateEdge {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StateEdge")
            .field("status", &self.status)
            .field("has_target_path", &self.target_path.is_some())
            .field("state_version", &self.state_version)
            .field("frontmatter", &self.frontmatter)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct RawState {
    pub(crate) kind: RawStateKind,
    pub(crate) issue: Option<RawStateIssue>,
    pub(crate) edges: Vec<StateEdge>,
    pub(crate) raw_digest: Option<[u8; 32]>,
}

impl fmt::Debug for RawState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let malformed = self
            .edges
            .iter()
            .filter(|edge| matches!(edge.status, StateEntryStatus::Malformed(_)))
            .count();
        formatter
            .debug_struct("RawState")
            .field("kind", &self.kind)
            .field("issue", &self.issue)
            .field("edge_count", &self.edges.len())
            .field("malformed_entry_count", &malformed)
            .field("raw_digest", &self.raw_digest)
            .finish()
    }
}

pub(super) fn parse_raw_state(raw: Option<&[u8]>) -> RawState {
    let Some(raw) = raw else {
        return raw_state(RawStateKind::Missing, None, Vec::new(), None);
    };
    let raw_digest = Some(Sha256::digest(raw).into());
    let parsed = match serde_json::from_slice(raw) {
        Ok(value) => value,
        Err(_) => {
            return raw_state(
                RawStateKind::Malformed,
                Some(RawStateIssue::InvalidJson),
                Vec::new(),
                raw_digest,
            );
        }
    };
    let serde_json::Value::Object(root) = parsed else {
        return raw_state(
            RawStateKind::Malformed,
            Some(RawStateIssue::RootNotObject),
            Vec::new(),
            raw_digest,
        );
    };
    let kind = classify(&root);
    let collection = if kind == RawStateKind::LegacyV1 {
        "concepts"
    } else {
        "pages"
    };
    match root.get(collection) {
        None => raw_state(
            RawStateKind::Malformed,
            Some(RawStateIssue::MissingCollection),
            Vec::new(),
            raw_digest,
        ),
        Some(serde_json::Value::Object(edges)) => {
            raw_state(kind, None, parse_edges(edges), raw_digest)
        }
        Some(_) => raw_state(
            RawStateKind::Malformed,
            Some(RawStateIssue::InvalidCollection),
            Vec::new(),
            raw_digest,
        ),
    }
}

fn raw_state(
    kind: RawStateKind,
    issue: Option<RawStateIssue>,
    edges: Vec<StateEdge>,
    raw_digest: Option<[u8; 32]>,
) -> RawState {
    RawState {
        kind,
        issue,
        edges,
        raw_digest,
    }
}

fn classify(root: &serde_json::Map<String, serde_json::Value>) -> RawStateKind {
    match root.get("schema_version") {
        Some(value) => match value.as_u64().and_then(|value| u32::try_from(value).ok()) {
            Some(0) => RawStateKind::WriterDefaultV0,
            Some(1) => RawStateKind::LegacyV1,
            Some(2) => RawStateKind::ExplicitV2,
            Some(version) => RawStateKind::FutureU32(version),
            None => RawStateKind::NonU32,
        },
        None if root.contains_key("concepts") => RawStateKind::LegacyV1,
        None if root.contains_key("pages") => RawStateKind::ImplicitV2,
        None => RawStateKind::Malformed,
    }
}

fn parse_edges(pages: &serde_json::Map<String, serde_json::Value>) -> Vec<StateEdge> {
    let mut edges = pages
        .iter()
        .map(|(state_id, value)| parse_edge(state_id, value))
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| left.state_id.cmp(&right.state_id));
    edges
}

fn parse_edge(state_id: &str, value: &serde_json::Value) -> StateEdge {
    let Some(fields) = value.as_object() else {
        return malformed_edge(state_id, StateEntryIssue::NotObject);
    };
    let (raw_target_path, status) = match fields.get("file") {
        None => (
            None,
            StateEntryStatus::Malformed(StateEntryIssue::MissingFile),
        ),
        Some(value) => match value.as_str() {
            Some(path) => (Some(path.to_string()), StateEntryStatus::Valid),
            None => (
                None,
                StateEntryStatus::Malformed(StateEntryIssue::InvalidFile),
            ),
        },
    };
    StateEdge {
        state_id: state_id.to_string(),
        raw_target_path,
        target_path: None,
        state_version: fields.get("version").map(version_value).unwrap_or_default(),
        frontmatter: Frontmatter::default(),
        status,
    }
}

fn malformed_edge(state_id: &str, issue: StateEntryIssue) -> StateEdge {
    StateEdge {
        state_id: state_id.to_string(),
        raw_target_path: None,
        target_path: None,
        state_version: VersionValue::Missing,
        frontmatter: Frontmatter::default(),
        status: StateEntryStatus::Malformed(issue),
    }
}

fn version_value(value: &serde_json::Value) -> VersionValue {
    value
        .as_i64()
        .map(VersionValue::Integer)
        .unwrap_or(VersionValue::Invalid)
}
