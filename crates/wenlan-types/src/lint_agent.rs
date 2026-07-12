// SPDX-License-Identifier: Apache-2.0
use super::{LintContractError, LintDigest};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::collections::BTreeSet;

pub const LINT_AGENT_WORK_SCHEMA_VERSION: u16 = 1;
pub const LINT_AGENT_RECORD_CAP: usize = 12;
pub const LINT_AGENT_EXCERPT_CHAR_CAP: usize = 600;
const LINT_AGENT_MEMORY_TYPE_CHAR_CAP: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LintSemanticCheckId {
    #[serde(rename = "memories.semantic.classification")]
    MemoryClassification,
    #[serde(rename = "memories.semantic.contradiction")]
    MemoryContradiction,
    #[serde(rename = "memories.semantic.staleness")]
    MemoryStaleness,
    #[serde(rename = "pages.semantic.faithfulness")]
    PageFaithfulness,
    #[serde(rename = "pages.semantic.provenance_adequacy")]
    PageProvenanceAdequacy,
    #[serde(rename = "serving.semantic.retrieval_quality")]
    RetrievalQuality,
}

impl LintSemanticCheckId {
    pub const ALL: [Self; 6] = [
        Self::MemoryClassification,
        Self::MemoryContradiction,
        Self::MemoryStaleness,
        Self::PageFaithfulness,
        Self::PageProvenanceAdequacy,
        Self::RetrievalQuality,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MemoryClassification => "memories.semantic.classification",
            Self::MemoryContradiction => "memories.semantic.contradiction",
            Self::MemoryStaleness => "memories.semantic.staleness",
            Self::PageFaithfulness => "pages.semantic.faithfulness",
            Self::PageProvenanceAdequacy => "pages.semantic.provenance_adequacy",
            Self::RetrievalQuality => "serving.semantic.retrieval_quality",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintAgentRecordKind {
    Memory,
    Page,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintAgentRecord {
    reference: u16,
    kind: LintAgentRecordKind,
    excerpt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evidence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_excerpt: Option<String>,
}

#[derive(Deserialize)]
struct LintAgentRecordWire {
    reference: u16,
    kind: LintAgentRecordKind,
    excerpt: String,
    #[serde(default)]
    memory_type: Option<String>,
    #[serde(default)]
    evidence_count: Option<u64>,
    #[serde(default)]
    source_excerpt: Option<String>,
}

impl LintAgentRecord {
    pub fn try_new(
        reference: u16,
        kind: LintAgentRecordKind,
        excerpt: String,
        memory_type: Option<String>,
        evidence_count: Option<u64>,
        source_excerpt: Option<String>,
    ) -> Result<Self, LintContractError> {
        let excerpt_len = excerpt.chars().count();
        let source_excerpt_valid = source_excerpt.as_ref().is_none_or(|value| {
            !value.trim().is_empty() && value.chars().count() <= LINT_AGENT_EXCERPT_CHAR_CAP
        });
        let memory_type_valid = memory_type
            .as_ref()
            .is_none_or(|value| value.chars().count() <= LINT_AGENT_MEMORY_TYPE_CHAR_CAP);
        let shape_valid = match kind {
            LintAgentRecordKind::Memory => evidence_count.is_none() && source_excerpt.is_none(),
            LintAgentRecordKind::Page => memory_type.is_none(),
        };
        if reference == 0
            || excerpt.trim().is_empty()
            || excerpt_len > LINT_AGENT_EXCERPT_CHAR_CAP
            || !source_excerpt_valid
            || !memory_type_valid
            || !shape_valid
        {
            return Err(LintContractError::InvalidAgentRecord);
        }
        Ok(Self {
            reference,
            kind,
            excerpt,
            memory_type,
            evidence_count,
            source_excerpt,
        })
    }

    pub const fn reference(&self) -> u16 {
        self.reference
    }

    pub const fn kind(&self) -> LintAgentRecordKind {
        self.kind
    }

    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }

    pub fn memory_type(&self) -> Option<&str> {
        self.memory_type.as_deref()
    }

    pub const fn evidence_count(&self) -> Option<u64> {
        self.evidence_count
    }

    pub fn source_excerpt(&self) -> Option<&str> {
        self.source_excerpt.as_deref()
    }
}

impl<'de> Deserialize<'de> for LintAgentRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintAgentRecordWire::deserialize(deserializer)?;
        Self::try_new(
            wire.reference,
            wire.kind,
            wire.excerpt,
            wire.memory_type,
            wire.evidence_count,
            wire.source_excerpt,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintAgentWork {
    work_schema_version: u16,
    work_digest: LintDigest,
    memory_eligible: u64,
    page_eligible: u64,
    faithful_page_eligible: u64,
    records: Vec<LintAgentRecord>,
}

#[derive(Deserialize)]
struct LintAgentWorkWire {
    work_schema_version: u16,
    work_digest: LintDigest,
    memory_eligible: u64,
    page_eligible: u64,
    faithful_page_eligible: u64,
    records: Vec<LintAgentRecord>,
}

impl LintAgentWork {
    pub fn try_new(
        work_digest: LintDigest,
        memory_eligible: u64,
        page_eligible: u64,
        faithful_page_eligible: u64,
        records: Vec<LintAgentRecord>,
    ) -> Result<Self, LintContractError> {
        if records.is_empty()
            || records.len() > LINT_AGENT_RECORD_CAP
            || records
                .iter()
                .enumerate()
                .any(|(index, record)| record.reference() != u16::try_from(index + 1).unwrap_or(0))
        {
            return Err(LintContractError::InvalidAgentWork);
        }
        Ok(Self {
            work_schema_version: LINT_AGENT_WORK_SCHEMA_VERSION,
            work_digest,
            memory_eligible,
            page_eligible,
            faithful_page_eligible,
            records,
        })
    }

    pub fn work_digest(&self) -> &LintDigest {
        &self.work_digest
    }

    pub const fn memory_eligible(&self) -> u64 {
        self.memory_eligible
    }

    pub const fn page_eligible(&self) -> u64 {
        self.page_eligible
    }

    pub const fn faithful_page_eligible(&self) -> u64 {
        self.faithful_page_eligible
    }

    pub fn records(&self) -> &[LintAgentRecord] {
        &self.records
    }
}

impl<'de> Deserialize<'de> for LintAgentWork {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintAgentWorkWire::deserialize(deserializer)?;
        if wire.work_schema_version != LINT_AGENT_WORK_SCHEMA_VERSION {
            return Err(D::Error::custom(LintContractError::InvalidAgentWork));
        }
        Self::try_new(
            wire.work_digest,
            wire.memory_eligible,
            wire.page_eligible,
            wire.faithful_page_eligible,
            wire.records,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintAgentVerdict {
    check_id: LintSemanticCheckId,
    refs: Vec<u16>,
}

#[derive(Deserialize)]
struct LintAgentVerdictWire {
    check_id: LintSemanticCheckId,
    refs: Vec<u16>,
}

impl LintAgentVerdict {
    pub fn try_new(
        check_id: LintSemanticCheckId,
        refs: Vec<u16>,
    ) -> Result<Self, LintContractError> {
        if refs.len() > LINT_AGENT_RECORD_CAP
            || refs.contains(&0)
            || !refs.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(LintContractError::InvalidAgentSubmission);
        }
        Ok(Self { check_id, refs })
    }

    pub const fn check_id(&self) -> LintSemanticCheckId {
        self.check_id
    }

    pub fn refs(&self) -> &[u16] {
        &self.refs
    }
}

impl<'de> Deserialize<'de> for LintAgentVerdict {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintAgentVerdictWire::deserialize(deserializer)?;
        Self::try_new(wire.check_id, wire.refs).map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintAgentSubmission {
    work_digest: LintDigest,
    verdicts: Vec<LintAgentVerdict>,
}

#[derive(Deserialize)]
struct LintAgentSubmissionWire {
    work_digest: LintDigest,
    verdicts: Vec<LintAgentVerdict>,
}

impl LintAgentSubmission {
    pub fn try_new(
        work_digest: LintDigest,
        verdicts: Vec<LintAgentVerdict>,
    ) -> Result<Self, LintContractError> {
        let ids = verdicts
            .iter()
            .map(LintAgentVerdict::check_id)
            .collect::<BTreeSet<_>>();
        if verdicts.len() != LintSemanticCheckId::ALL.len()
            || ids != LintSemanticCheckId::ALL.into_iter().collect()
        {
            return Err(LintContractError::InvalidAgentSubmission);
        }
        Ok(Self {
            work_digest,
            verdicts,
        })
    }

    pub fn work_digest(&self) -> &LintDigest {
        &self.work_digest
    }

    pub fn verdicts(&self) -> &[LintAgentVerdict] {
        &self.verdicts
    }
}

impl<'de> Deserialize<'de> for LintAgentSubmission {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintAgentSubmissionWire::deserialize(deserializer)?;
        Self::try_new(wire.work_digest, wire.verdicts).map_err(D::Error::custom)
    }
}
