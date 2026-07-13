use super::fs::PageFsError;
use cap_std::fs::File;
use sha2::{Digest, Sha256};
use std::fmt;
use std::io::{BufRead, BufReader, Read};

pub(super) const FRONTMATTER_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum VersionValue {
    #[default]
    Missing,
    Integer(i64),
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrontmatterState {
    Unparsed,
    Absent,
    Parsed,
    Invalid,
    Malformed,
    Truncated,
    OverLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrontmatterIssue {
    NonStringKey,
    OriginIdType,
    OriginVersionType,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Frontmatter {
    pub(crate) state: FrontmatterState,
    pub(crate) origin_id: Option<String>,
    pub(crate) origin_version: VersionValue,
    pub(crate) issues: Vec<FrontmatterIssue>,
    digest: [u8; 32],
}

impl fmt::Debug for Frontmatter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Frontmatter")
            .field("state", &self.state)
            .field("has_origin_id", &self.origin_id.is_some())
            .field("origin_version", &self.origin_version)
            .field("issues", &self.issues)
            .field("digest", &self.digest)
            .finish()
    }
}

impl Default for Frontmatter {
    fn default() -> Self {
        Self::unparsed()
    }
}

impl Frontmatter {
    pub(super) fn unparsed() -> Self {
        Self {
            state: FrontmatterState::Unparsed,
            origin_id: None,
            origin_version: VersionValue::Missing,
            issues: Vec::new(),
            digest: digest(b"unparsed"),
        }
    }

    pub(super) fn digest(&self) -> [u8; 32] {
        self.digest
    }
}

pub(super) fn read_frontmatter(file: File) -> Result<Frontmatter, PageFsError> {
    parse_frontmatter(file)
}

pub(super) fn parse_frontmatter<R: Read>(source: R) -> Result<Frontmatter, PageFsError> {
    let limited = source.take(FRONTMATTER_LIMIT as u64);
    let mut reader = BufReader::new(limited);
    let mut prefix = Vec::new();
    let mut line = Vec::new();
    let opening_len = read_line(&mut reader, &mut line, &mut prefix)?;
    if opening_len == 0 || !is_delimiter_line(&line) {
        return Ok(absent());
    }

    loop {
        line.clear();
        let line_len = read_line(&mut reader, &mut line, &mut prefix)?;
        if line_len == 0 {
            return Ok(incomplete(&prefix));
        }
        if is_delimiter_line(&line) {
            return Ok(parse_closed(&prefix, opening_len, line_len));
        }
        if prefix.len() == FRONTMATTER_LIMIT {
            return Ok(with_state(FrontmatterState::OverLimit, &prefix));
        }
    }
}

fn read_line<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
    prefix: &mut Vec<u8>,
) -> Result<usize, PageFsError> {
    let read = reader
        .read_until(b'\n', line)
        .map_err(|_| PageFsError::ReadPrefix)?;
    prefix.extend_from_slice(line);
    Ok(read)
}

fn is_delimiter_line(line: &[u8]) -> bool {
    matches!(line, b"---" | b"---\n" | b"---\r\n")
}

fn incomplete(prefix: &[u8]) -> Frontmatter {
    if prefix.len() == FRONTMATTER_LIMIT {
        with_state(FrontmatterState::OverLimit, prefix)
    } else {
        with_state(FrontmatterState::Truncated, prefix)
    }
}

fn parse_closed(prefix: &[u8], opening_len: usize, closing_len: usize) -> Frontmatter {
    let yaml_end = prefix.len() - closing_len;
    let parsed = std::str::from_utf8(&prefix[opening_len..yaml_end])
        .ok()
        .and_then(|text| serde_yaml::from_str::<serde_yaml::Value>(text).ok());
    let Some(serde_yaml::Value::Mapping(values)) = parsed else {
        return with_state(FrontmatterState::Malformed, prefix);
    };

    let mut origin_id = None;
    let mut origin_version = VersionValue::Missing;
    let mut issues = Vec::new();
    for (key, value) in values {
        let Some(key) = key.as_str() else {
            issues.push(FrontmatterIssue::NonStringKey);
            continue;
        };
        match key {
            "origin_id" => match value.as_str() {
                Some(id) => origin_id = Some(id.to_string()),
                None => issues.push(FrontmatterIssue::OriginIdType),
            },
            "origin_version" => match value.as_i64() {
                Some(version) => origin_version = VersionValue::Integer(version),
                None => {
                    origin_version = VersionValue::Invalid;
                    issues.push(FrontmatterIssue::OriginVersionType);
                }
            },
            _ => {}
        }
    }

    Frontmatter {
        state: if issues.is_empty() {
            FrontmatterState::Parsed
        } else {
            FrontmatterState::Invalid
        },
        origin_id,
        origin_version,
        issues,
        digest: digest(prefix),
    }
}

fn absent() -> Frontmatter {
    with_state(FrontmatterState::Absent, b"absent")
}

fn with_state(state: FrontmatterState, bytes: &[u8]) -> Frontmatter {
    Frontmatter {
        state,
        origin_id: None,
        origin_version: VersionValue::Missing,
        issues: Vec::new(),
        digest: digest(bytes),
    }
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}
