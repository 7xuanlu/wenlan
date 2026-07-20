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
    DuplicateKey,
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

#[derive(Debug)]
struct JsonMember {
    key: String,
    leading_start: usize,
    value_start: usize,
    value_end: usize,
    comma_after: Option<usize>,
}

#[derive(Debug)]
struct JsonObject {
    members: Vec<JsonMember>,
}

#[derive(Debug)]
struct StateStructure {
    pages_objects: Vec<JsonObject>,
    pages_values: usize,
    has_duplicate_key: bool,
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
    let structure = match inspect_state_structure(raw) {
        Ok(structure) => structure,
        Err(()) => {
            return raw_state(
                RawStateKind::Malformed,
                Some(RawStateIssue::InvalidJson),
                Vec::new(),
                raw_digest,
            );
        }
    };
    if structure.has_duplicate_key {
        let edges = structure
            .pages_objects
            .iter()
            .flat_map(|object| parse_structural_edges(raw, object))
            .collect();
        return raw_state(
            RawStateKind::Malformed,
            Some(RawStateIssue::DuplicateKey),
            edges,
            raw_digest,
        );
    }
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

pub(crate) fn remove_unique_page_member(raw: &[u8], page_id: &str) -> Result<Vec<u8>, ()> {
    let structure = inspect_state_structure(raw)?;
    if structure.has_duplicate_key
        || structure.pages_values != 1
        || structure.pages_objects.len() != 1
    {
        return Err(());
    }
    let pages = &structure.pages_objects[0];
    let targets = pages
        .members
        .iter()
        .enumerate()
        .filter(|(_, member)| member.key == page_id)
        .collect::<Vec<_>>();
    let [(target_index, target)] = targets.as_slice() else {
        return Err(());
    };
    let removal = if *target_index + 1 < pages.members.len() {
        target.leading_start..target.comma_after.ok_or(())? + 1
    } else if *target_index > 0 {
        pages.members[*target_index - 1].comma_after.ok_or(())?..target.value_end
    } else {
        target.leading_start..target.value_end
    };
    let mut edited = Vec::with_capacity(raw.len() - removal.len());
    edited.extend_from_slice(&raw[..removal.start]);
    edited.extend_from_slice(&raw[removal.end..]);
    Ok(edited)
}

fn inspect_state_structure(raw: &[u8]) -> Result<StateStructure, ()> {
    if raw.len() > usize::try_from(super::fs::STATE_MAX_BYTES).map_err(|_| ())? {
        return Err(());
    }
    let parsed = serde_json::from_slice::<serde_json::Value>(raw).map_err(|_| ())?;
    if !parsed.is_object() {
        return Err(());
    }
    let root_start = skip_whitespace(raw, 0);
    let root = parse_object(raw, root_start)?;
    if skip_whitespace(raw, object_end(raw, root_start)?) != raw.len() {
        return Err(());
    }
    let has_duplicate_key = value_has_duplicate_keys(raw, root_start)?;
    let mut pages_objects = Vec::new();
    let mut pages_values = 0;
    for member in root.members.iter().filter(|member| member.key == "pages") {
        pages_values += 1;
        if raw.get(member.value_start) == Some(&b'{') {
            let pages = parse_object(raw, member.value_start)?;
            pages_objects.push(pages);
        }
    }
    Ok(StateStructure {
        pages_objects,
        pages_values,
        has_duplicate_key,
    })
}

fn has_duplicate_keys(object: &JsonObject) -> bool {
    let mut keys = std::collections::BTreeSet::new();
    object
        .members
        .iter()
        .any(|member| !keys.insert(member.key.as_str()))
}

fn value_has_duplicate_keys(raw: &[u8], start: usize) -> Result<bool, ()> {
    match raw.get(start) {
        Some(b'{') => {
            let object = parse_object(raw, start)?;
            let mut duplicate = has_duplicate_keys(&object);
            for member in &object.members {
                duplicate |= value_has_duplicate_keys(raw, member.value_start)?;
            }
            Ok(duplicate)
        }
        Some(b'[') => {
            let mut cursor = skip_whitespace(raw, start + 1);
            let mut duplicate = false;
            if raw.get(cursor) == Some(&b']') {
                return Ok(false);
            }
            loop {
                duplicate |= value_has_duplicate_keys(raw, cursor)?;
                cursor = skip_whitespace(raw, skip_json_value(raw, cursor)?);
                match raw.get(cursor) {
                    Some(b',') => cursor = skip_whitespace(raw, cursor + 1),
                    Some(b']') => return Ok(duplicate),
                    _ => return Err(()),
                }
            }
        }
        Some(_) => Ok(false),
        None => Err(()),
    }
}

fn parse_structural_edges(raw: &[u8], object: &JsonObject) -> Vec<StateEdge> {
    object
        .members
        .iter()
        .map(|member| {
            serde_json::from_slice::<serde_json::Value>(&raw[member.value_start..member.value_end])
                .map_or_else(
                    |_| malformed_edge(&member.key, StateEntryIssue::NotObject),
                    |value| parse_edge(&member.key, &value),
                )
        })
        .collect()
}

fn parse_object(raw: &[u8], start: usize) -> Result<JsonObject, ()> {
    if raw.get(start) != Some(&b'{') {
        return Err(());
    }
    let mut cursor = start + 1;
    let mut members = Vec::new();
    loop {
        let leading_start = cursor;
        cursor = skip_whitespace(raw, cursor);
        if raw.get(cursor) == Some(&b'}') {
            return Ok(JsonObject { members });
        }
        let key_start = cursor;
        let key_end = skip_json_string(raw, key_start)?;
        let key = serde_json::from_slice::<String>(&raw[key_start..key_end]).map_err(|_| ())?;
        cursor = skip_whitespace(raw, key_end);
        if raw.get(cursor) != Some(&b':') {
            return Err(());
        }
        let value_start = skip_whitespace(raw, cursor + 1);
        let value_end = skip_json_value(raw, value_start)?;
        cursor = skip_whitespace(raw, value_end);
        match raw.get(cursor) {
            Some(b',') => {
                members.push(JsonMember {
                    key,
                    leading_start,
                    value_start,
                    value_end,
                    comma_after: Some(cursor),
                });
                cursor += 1;
            }
            Some(b'}') => {
                members.push(JsonMember {
                    key,
                    leading_start,
                    value_start,
                    value_end,
                    comma_after: None,
                });
                return Ok(JsonObject { members });
            }
            _ => return Err(()),
        }
    }
}

fn object_end(raw: &[u8], start: usize) -> Result<usize, ()> {
    skip_json_value(raw, start)
}

fn skip_json_value(raw: &[u8], start: usize) -> Result<usize, ()> {
    match raw.get(start) {
        Some(b'"') => skip_json_string(raw, start),
        Some(b'{') | Some(b'[') => skip_json_compound(raw, start),
        Some(_) => {
            let mut cursor = start;
            while let Some(byte) = raw.get(cursor) {
                if byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b']') {
                    break;
                }
                cursor += 1;
            }
            (cursor > start).then_some(cursor).ok_or(())
        }
        None => Err(()),
    }
}

fn skip_json_compound(raw: &[u8], start: usize) -> Result<usize, ()> {
    let mut closers = vec![match raw.get(start) {
        Some(b'{') => b'}',
        Some(b'[') => b']',
        _ => return Err(()),
    }];
    let mut cursor = start + 1;
    while let Some(byte) = raw.get(cursor) {
        match byte {
            b'"' => cursor = skip_json_string(raw, cursor)?,
            b'{' => {
                closers.push(b'}');
                cursor += 1;
            }
            b'[' => {
                closers.push(b']');
                cursor += 1;
            }
            b'}' | b']' => {
                if closers.pop() != Some(*byte) {
                    return Err(());
                }
                cursor += 1;
                if closers.is_empty() {
                    return Ok(cursor);
                }
            }
            _ => cursor += 1,
        }
    }
    Err(())
}

fn skip_json_string(raw: &[u8], start: usize) -> Result<usize, ()> {
    if raw.get(start) != Some(&b'"') {
        return Err(());
    }
    let mut cursor = start + 1;
    while let Some(byte) = raw.get(cursor) {
        match byte {
            b'\\' => cursor = cursor.checked_add(2).ok_or(())?,
            b'"' => return Ok(cursor + 1),
            _ => cursor += 1,
        }
    }
    Err(())
}

fn skip_whitespace(raw: &[u8], mut cursor: usize) -> usize {
    while raw
        .get(cursor)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        cursor += 1;
    }
    cursor
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
