// SPDX-License-Identifier: Apache-2.0
use super::{LintContractError, LintDigest, LintOpaqueId};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const LINT_AGENT_WORK_SCHEMA_VERSION: u16 = 2;
pub const LINT_AGENT_RECORD_CAP: usize = 96;
pub const LINT_AGENT_CANDIDATE_CAP: usize = 48;
pub const LINT_AGENT_EXCERPT_CHAR_CAP: usize = 600;
const LINT_AGENT_MEMORY_TYPE_CHAR_CAP: usize = 64;
const LINT_AGENT_REFS_PER_CANDIDATE_CAP: usize = 8;
const LINT_SEMANTIC_CONFIDENCE_MAX: u16 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LintSemanticCheckId {
    #[serde(rename = "memories.semantic.classification")]
    MemoryClassification,
    #[serde(rename = "memories.semantic.contradiction")]
    MemoryContradiction,
    #[serde(rename = "memories.semantic.staleness")]
    MemoryStaleness,
    #[serde(rename = "kg.semantic.memory_entity_links")]
    MemoryEntityLinks,
    #[serde(rename = "kg.semantic.entity_relations")]
    EntityRelations,
    #[serde(rename = "pages.semantic.faithfulness")]
    PageFaithfulness,
    #[serde(rename = "pages.semantic.provenance_adequacy")]
    PageProvenanceAdequacy,
    #[serde(rename = "pages.semantic.evidence_links")]
    PageEvidenceLinks,
    #[serde(rename = "serving.semantic.retrieval_quality")]
    RetrievalQuality,
}

impl LintSemanticCheckId {
    pub const ALL: [Self; 9] = [
        Self::MemoryClassification,
        Self::MemoryContradiction,
        Self::MemoryStaleness,
        Self::MemoryEntityLinks,
        Self::EntityRelations,
        Self::PageFaithfulness,
        Self::PageProvenanceAdequacy,
        Self::PageEvidenceLinks,
        Self::RetrievalQuality,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MemoryClassification => "memories.semantic.classification",
            Self::MemoryContradiction => "memories.semantic.contradiction",
            Self::MemoryStaleness => "memories.semantic.staleness",
            Self::MemoryEntityLinks => "kg.semantic.memory_entity_links",
            Self::EntityRelations => "kg.semantic.entity_relations",
            Self::PageFaithfulness => "pages.semantic.faithfulness",
            Self::PageProvenanceAdequacy => "pages.semantic.provenance_adequacy",
            Self::PageEvidenceLinks => "pages.semantic.evidence_links",
            Self::RetrievalQuality => "serving.semantic.retrieval_quality",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintAgentRecordKind {
    Memory,
    Page,
    Entity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSemanticCandidateKind {
    RecordReview,
    PairReview,
    MissingLink,
    ExistingLink,
    RetrievalTrace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSemanticAction {
    ReclassifyMemory,
    ReviewContradiction,
    ReviewStaleness,
    SupersedeMemory,
    AddMemoryEntityLink,
    RemoveMemoryEntityLink,
    AddEntityRelation,
    RemoveEntityRelation,
    ReviewPageClaim,
    AddPageEvidence,
    RemovePageEvidence,
    ReviewRetrieval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSemanticReasonCode {
    ClassificationMismatch,
    PotentialContradiction,
    PotentialStaleness,
    MentionWithoutLink,
    ExistingLinkMismatch,
    SharedContextWithoutRelation,
    ExistingRelationMismatch,
    PotentialUnfaithfulClaim,
    PotentialInadequateProvenance,
    ClaimOverlapWithoutEvidence,
    ExistingEvidenceMismatch,
    PotentialRetrievalMiss,
    DanglingOwner,
    TemporalEvolution,
    RelatedButNotEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSemanticDecision {
    Pass,
    Finding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSemanticProviderRoute {
    OnDevice,
    ConfiguredExternal,
    CallingAgent,
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
#[serde(deny_unknown_fields)]
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
        let source_excerpt_valid = source_excerpt.as_ref().is_none_or(|value| {
            !value.trim().is_empty() && value.chars().count() <= LINT_AGENT_EXCERPT_CHAR_CAP
        });
        let memory_type_valid = memory_type
            .as_ref()
            .is_none_or(|value| value.chars().count() <= LINT_AGENT_MEMORY_TYPE_CHAR_CAP);
        let shape_valid = match kind {
            LintAgentRecordKind::Memory => evidence_count.is_none() && source_excerpt.is_none(),
            LintAgentRecordKind::Page => memory_type.is_none(),
            LintAgentRecordKind::Entity => {
                memory_type.is_none() && evidence_count.is_none() && source_excerpt.is_none()
            }
        };
        if reference == 0
            || excerpt.trim().is_empty()
            || excerpt.chars().count() > LINT_AGENT_EXCERPT_CHAR_CAP
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
pub struct LintSemanticPopulation {
    check_id: LintSemanticCheckId,
    eligible: u64,
    candidates: u64,
    packet_candidates: u64,
    truncated: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LintSemanticPopulationWire {
    check_id: LintSemanticCheckId,
    eligible: u64,
    candidates: u64,
    packet_candidates: u64,
    truncated: bool,
}

impl LintSemanticPopulation {
    pub fn try_new(
        check_id: LintSemanticCheckId,
        eligible: u64,
        candidates: u64,
        packet_candidates: u64,
        truncated: bool,
    ) -> Result<Self, LintContractError> {
        if packet_candidates > candidates || truncated != (packet_candidates < candidates) {
            return Err(LintContractError::InvalidAgentWork);
        }
        Ok(Self {
            check_id,
            eligible,
            candidates,
            packet_candidates,
            truncated,
        })
    }

    pub const fn check_id(&self) -> LintSemanticCheckId {
        self.check_id
    }
    pub const fn eligible(&self) -> u64 {
        self.eligible
    }
    pub const fn candidates(&self) -> u64 {
        self.candidates
    }
    pub const fn packet_candidates(&self) -> u64 {
        self.packet_candidates
    }
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}

impl<'de> Deserialize<'de> for LintSemanticPopulation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintSemanticPopulationWire::deserialize(deserializer)?;
        Self::try_new(
            wire.check_id,
            wire.eligible,
            wire.candidates,
            wire.packet_candidates,
            wire.truncated,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintAgentCandidate {
    reference: u16,
    check_id: LintSemanticCheckId,
    kind: LintSemanticCandidateKind,
    evidence_refs: Vec<u16>,
    counterevidence_refs: Vec<u16>,
    proposed_action: LintSemanticAction,
    reason_code: LintSemanticReasonCode,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LintAgentCandidateWire {
    reference: u16,
    check_id: LintSemanticCheckId,
    kind: LintSemanticCandidateKind,
    evidence_refs: Vec<u16>,
    counterevidence_refs: Vec<u16>,
    proposed_action: LintSemanticAction,
    reason_code: LintSemanticReasonCode,
}

impl LintAgentCandidate {
    pub fn try_new(
        reference: u16,
        check_id: LintSemanticCheckId,
        kind: LintSemanticCandidateKind,
        evidence_refs: Vec<u16>,
        counterevidence_refs: Vec<u16>,
        proposed_action: LintSemanticAction,
        reason_code: LintSemanticReasonCode,
    ) -> Result<Self, LintContractError> {
        if reference == 0
            || !valid_refs(&evidence_refs, false)
            || !valid_refs(&counterevidence_refs, true)
            || evidence_refs
                .iter()
                .any(|reference| counterevidence_refs.contains(reference))
            || !action_matches_check(check_id, proposed_action)
        {
            return Err(LintContractError::InvalidAgentWork);
        }
        Ok(Self {
            reference,
            check_id,
            kind,
            evidence_refs,
            counterevidence_refs,
            proposed_action,
            reason_code,
        })
    }

    pub const fn reference(&self) -> u16 {
        self.reference
    }
    pub const fn check_id(&self) -> LintSemanticCheckId {
        self.check_id
    }
    pub const fn kind(&self) -> LintSemanticCandidateKind {
        self.kind
    }
    pub fn evidence_refs(&self) -> &[u16] {
        &self.evidence_refs
    }
    pub fn counterevidence_refs(&self) -> &[u16] {
        &self.counterevidence_refs
    }
    pub const fn proposed_action(&self) -> LintSemanticAction {
        self.proposed_action
    }
    pub const fn reason_code(&self) -> LintSemanticReasonCode {
        self.reason_code
    }
}

impl<'de> Deserialize<'de> for LintAgentCandidate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintAgentCandidateWire::deserialize(deserializer)?;
        Self::try_new(
            wire.reference,
            wire.check_id,
            wire.kind,
            wire.evidence_refs,
            wire.counterevidence_refs,
            wire.proposed_action,
            wire.reason_code,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintAgentWork {
    work_schema_version: u16,
    work_digest: LintDigest,
    populations: Vec<LintSemanticPopulation>,
    records: Vec<LintAgentRecord>,
    candidates: Vec<LintAgentCandidate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LintAgentWorkWire {
    work_schema_version: u16,
    work_digest: LintDigest,
    populations: Vec<LintSemanticPopulation>,
    records: Vec<LintAgentRecord>,
    candidates: Vec<LintAgentCandidate>,
}

impl LintAgentWork {
    pub fn try_new(
        work_digest: LintDigest,
        populations: Vec<LintSemanticPopulation>,
        records: Vec<LintAgentRecord>,
        candidates: Vec<LintAgentCandidate>,
    ) -> Result<Self, LintContractError> {
        let population_ids = populations
            .iter()
            .map(LintSemanticPopulation::check_id)
            .collect::<BTreeSet<_>>();
        let packet_counts = candidates
            .iter()
            .fold(BTreeMap::new(), |mut counts, candidate| {
                *counts.entry(candidate.check_id()).or_insert(0_u64) += 1;
                counts
            });
        let populations_valid = populations.len() == LintSemanticCheckId::ALL.len()
            && population_ids == LintSemanticCheckId::ALL.into_iter().collect()
            && populations.iter().all(|population| {
                packet_counts
                    .get(&population.check_id())
                    .copied()
                    .unwrap_or(0)
                    == population.packet_candidates()
            });
        let records_valid = records.len() <= LINT_AGENT_RECORD_CAP
            && records
                .iter()
                .enumerate()
                .all(|(index, record)| record.reference() == u16::try_from(index + 1).unwrap_or(0));
        let candidates_valid = candidates.len() <= LINT_AGENT_CANDIDATE_CAP
            && candidates.iter().enumerate().all(|(index, candidate)| {
                candidate.reference() == u16::try_from(index + 1).unwrap_or(0)
                    && candidate
                        .evidence_refs()
                        .iter()
                        .chain(candidate.counterevidence_refs())
                        .all(|reference| usize::from(*reference) <= records.len())
            });
        if !populations_valid || !records_valid || !candidates_valid {
            return Err(LintContractError::InvalidAgentWork);
        }
        Ok(Self {
            work_schema_version: LINT_AGENT_WORK_SCHEMA_VERSION,
            work_digest,
            populations,
            records,
            candidates,
        })
    }

    pub fn work_digest(&self) -> &LintDigest {
        &self.work_digest
    }
    pub fn populations(&self) -> &[LintSemanticPopulation] {
        &self.populations
    }
    pub fn records(&self) -> &[LintAgentRecord] {
        &self.records
    }
    pub fn candidates(&self) -> &[LintAgentCandidate] {
        &self.candidates
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
            wire.populations,
            wire.records,
            wire.candidates,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintAgentVerdict {
    candidate_ref: u16,
    decision: LintSemanticDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    second_decision: Option<LintSemanticDecision>,
    reason_code: LintSemanticReasonCode,
    confidence_basis_points: u16,
    counterevidence_refs: Vec<u16>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LintAgentVerdictWire {
    candidate_ref: u16,
    decision: LintSemanticDecision,
    #[serde(default)]
    second_decision: Option<LintSemanticDecision>,
    reason_code: LintSemanticReasonCode,
    confidence_basis_points: u16,
    #[serde(default)]
    counterevidence_refs: Vec<u16>,
}

impl LintAgentVerdict {
    pub fn try_new(
        candidate_ref: u16,
        decision: LintSemanticDecision,
        second_decision: Option<LintSemanticDecision>,
        reason_code: LintSemanticReasonCode,
        confidence_basis_points: u16,
        counterevidence_refs: Vec<u16>,
    ) -> Result<Self, LintContractError> {
        if candidate_ref == 0
            || confidence_basis_points > LINT_SEMANTIC_CONFIDENCE_MAX
            || !valid_refs(&counterevidence_refs, true)
        {
            return Err(LintContractError::InvalidAgentSubmission);
        }
        Ok(Self {
            candidate_ref,
            decision,
            second_decision,
            reason_code,
            confidence_basis_points,
            counterevidence_refs,
        })
    }

    pub const fn candidate_ref(&self) -> u16 {
        self.candidate_ref
    }
    pub const fn decision(&self) -> LintSemanticDecision {
        self.decision
    }
    pub const fn second_decision(&self) -> Option<LintSemanticDecision> {
        self.second_decision
    }
    pub const fn reason_code(&self) -> LintSemanticReasonCode {
        self.reason_code
    }
    pub const fn confidence_basis_points(&self) -> u16 {
        self.confidence_basis_points
    }
    pub fn counterevidence_refs(&self) -> &[u16] {
        &self.counterevidence_refs
    }
    pub fn has_unresolved_disagreement(&self) -> bool {
        self.second_decision
            .is_some_and(|second| second != self.decision)
    }
}

impl<'de> Deserialize<'de> for LintAgentVerdict {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = LintAgentVerdictWire::deserialize(deserializer)?;
        Self::try_new(
            wire.candidate_ref,
            wire.decision,
            wire.second_decision,
            wire.reason_code,
            wire.confidence_basis_points,
            wire.counterevidence_refs,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LintAgentSubmission {
    work_digest: LintDigest,
    verdicts: Vec<LintAgentVerdict>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LintAgentSubmissionWire {
    work_digest: LintDigest,
    verdicts: Vec<LintAgentVerdict>,
}

impl LintAgentSubmission {
    pub fn try_new(
        work_digest: LintDigest,
        verdicts: Vec<LintAgentVerdict>,
    ) -> Result<Self, LintContractError> {
        if verdicts.len() > LINT_AGENT_CANDIDATE_CAP
            || !verdicts
                .windows(2)
                .all(|pair| pair[0].candidate_ref() < pair[1].candidate_ref())
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LintSemanticFinding {
    candidate_id: LintOpaqueId,
    proposed_action: LintSemanticAction,
    reason_code: LintSemanticReasonCode,
    confidence_basis_points: u16,
    provider_route: LintSemanticProviderRoute,
    evidence_ids: Vec<LintDigest>,
    counterevidence_ids: Vec<LintDigest>,
    unresolved_disagreement: bool,
}

impl LintSemanticFinding {
    pub fn try_new(
        candidate_id: LintOpaqueId,
        proposed_action: LintSemanticAction,
        reason_code: LintSemanticReasonCode,
        confidence_basis_points: u16,
        provider_route: LintSemanticProviderRoute,
        evidence_ids: Vec<LintDigest>,
        counterevidence_ids: Vec<LintDigest>,
    ) -> Result<Self, LintContractError> {
        Self::try_new_with_disagreement(
            candidate_id,
            proposed_action,
            reason_code,
            confidence_basis_points,
            provider_route,
            evidence_ids,
            counterevidence_ids,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn try_new_with_disagreement(
        candidate_id: LintOpaqueId,
        proposed_action: LintSemanticAction,
        reason_code: LintSemanticReasonCode,
        confidence_basis_points: u16,
        provider_route: LintSemanticProviderRoute,
        evidence_ids: Vec<LintDigest>,
        counterevidence_ids: Vec<LintDigest>,
        unresolved_disagreement: bool,
    ) -> Result<Self, LintContractError> {
        if confidence_basis_points > LINT_SEMANTIC_CONFIDENCE_MAX
            || evidence_ids.len() > LINT_AGENT_REFS_PER_CANDIDATE_CAP
            || counterevidence_ids.len() > LINT_AGENT_REFS_PER_CANDIDATE_CAP
        {
            return Err(LintContractError::InvalidAgentSubmission);
        }
        Ok(Self {
            candidate_id,
            proposed_action,
            reason_code,
            confidence_basis_points,
            provider_route,
            evidence_ids,
            counterevidence_ids,
            unresolved_disagreement,
        })
    }

    pub const fn candidate_id(&self) -> LintOpaqueId {
        self.candidate_id
    }
    pub const fn proposed_action(&self) -> LintSemanticAction {
        self.proposed_action
    }
    pub const fn reason_code(&self) -> LintSemanticReasonCode {
        self.reason_code
    }
    pub const fn confidence_basis_points(&self) -> u16 {
        self.confidence_basis_points
    }
    pub const fn provider_route(&self) -> LintSemanticProviderRoute {
        self.provider_route
    }
    pub fn evidence_ids(&self) -> &[LintDigest] {
        &self.evidence_ids
    }
    pub fn counterevidence_ids(&self) -> &[LintDigest] {
        &self.counterevidence_ids
    }
    pub const fn unresolved_disagreement(&self) -> bool {
        self.unresolved_disagreement
    }
}

fn valid_refs(refs: &[u16], empty_allowed: bool) -> bool {
    (empty_allowed || !refs.is_empty())
        && refs.len() <= LINT_AGENT_REFS_PER_CANDIDATE_CAP
        && !refs.contains(&0)
        && refs.windows(2).all(|pair| pair[0] < pair[1])
}

fn action_matches_check(check_id: LintSemanticCheckId, action: LintSemanticAction) -> bool {
    match check_id {
        LintSemanticCheckId::MemoryClassification => action == LintSemanticAction::ReclassifyMemory,
        LintSemanticCheckId::MemoryContradiction => matches!(
            action,
            LintSemanticAction::ReviewContradiction | LintSemanticAction::SupersedeMemory
        ),
        LintSemanticCheckId::MemoryStaleness => matches!(
            action,
            LintSemanticAction::ReviewStaleness | LintSemanticAction::SupersedeMemory
        ),
        LintSemanticCheckId::MemoryEntityLinks => matches!(
            action,
            LintSemanticAction::AddMemoryEntityLink | LintSemanticAction::RemoveMemoryEntityLink
        ),
        LintSemanticCheckId::EntityRelations => matches!(
            action,
            LintSemanticAction::AddEntityRelation | LintSemanticAction::RemoveEntityRelation
        ),
        LintSemanticCheckId::PageFaithfulness | LintSemanticCheckId::PageProvenanceAdequacy => {
            action == LintSemanticAction::ReviewPageClaim
        }
        LintSemanticCheckId::PageEvidenceLinks => matches!(
            action,
            LintSemanticAction::AddPageEvidence | LintSemanticAction::RemovePageEvidence
        ),
        LintSemanticCheckId::RetrievalQuality => action == LintSemanticAction::ReviewRetrieval,
    }
}
