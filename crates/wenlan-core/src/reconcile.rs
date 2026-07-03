// SPDX-License-Identifier: Apache-2.0
//! Doc-grounded revisions (L3 reconcile).
//!
//! A 30-min scheduler sweep detects direct factual contradictions between
//! ingested documents (`source_agent='folder'`) and agent captures, and stages
//! human-gated rewrite+cite revisions through the existing pending-revisions
//! surface. Doc rows are NEVER written; the capture is untouched until human
//! accept. Design: docs/superpowers/specs/2026-07-02-doc-grounded-revisions-design.md.

use std::sync::Arc;

use crate::db::MemoryDB;
use crate::llm_provider::LlmProvider;
use crate::prompts::PromptRegistry;
use crate::tuning::{DistillationConfig, RefineryConfig};
use wenlan_types::RawDocument;

/// Minimum cosine SIMILARITY for a frontier item / candidate pair to reach the
/// LLM judge. Known recall ceiling (contradictions need not be embedding-near);
/// measured post-ship per spec §7.
pub const RECONCILE_COSINE_GATE: f64 = 0.70;
/// Hard cap on LLM judge calls per tick across both frontiers (GPU contention).
pub const RECONCILE_JUDGE_CALLS_PER_TICK: usize = 25;
/// Back-pressure: sweep holds entirely while this many doc-grounded revisions
/// await human review.
pub const RECONCILE_PENDING_CAP: usize = 20;
/// Max frontier rows fetched per frontier per tick.
pub const RECONCILE_BATCH_PER_FRONTIER: usize = 50;
/// Vector top-k candidates per frontier item.
// Consumed by the candidate-matching step landing in a later task.
#[allow(dead_code)]
pub(crate) const RECONCILE_TOP_K: usize = 5;
/// Consecutive failed ticks on the same head item before poison-pill ejection.
// Consumed by the poison-pill ejection step landing in a later task.
#[allow(dead_code)]
pub(crate) const RECONCILE_POISON_TICKS: u32 = 3;

/// Inputs for staging one doc-grounded revision row.
#[derive(Debug, Clone)]
pub struct RevisionInput<'a> {
    pub capture_source_id: &'a str,
    pub capture_space: Option<&'a str>,
    /// File-level doc source_id ("{source_id}::{path}", shared by all chunks).
    pub doc_file_source_id: &'a str,
    pub doc_chunk_index: i64,
    pub doc_hash: &'a str,
    pub revised_content: &'a str,
}

/// Stage ONE revision row via the canonical store + enrichment path (ingest
/// parity: embedding at store time, Phase-1 classify/tags when an LLM is
/// available). pending_revision=1 keeps it out of Phase-3 pools; the human
/// gate owns activation. Returns the new revision source_id.
pub async fn write_revision(
    db: &MemoryDB,
    input: RevisionInput<'_>,
    llm: Option<&Arc<dyn LlmProvider>>,
    prompts: &PromptRegistry,
    refinery: &RefineryConfig,
    distillation: &DistillationConfig,
) -> Result<String, crate::WenlanError> {
    let source_id = format!(
        "mem_{}",
        uuid::Uuid::new_v4()
            .to_string()
            .replace('-', "")
            .chars()
            .take(12)
            .collect::<String>()
    );
    let structured = serde_json::json!({
        "revises": input.capture_source_id,
        "grounded_in": input.doc_file_source_id,
        "grounded_chunk": input.doc_chunk_index,
        "doc_hash": input.doc_hash,
    })
    .to_string();

    let row = RawDocument {
        source: "memory".to_string(),
        source_id: source_id.clone(),
        title: input.revised_content.chars().take(80).collect(),
        content: input.revised_content.to_string(),
        last_modified: chrono::Utc::now().timestamp(),
        space: input.capture_space.map(str::to_string),
        source_agent: Some("reconcile".to_string()),
        confirmed: None,
        supersedes: Some(input.capture_source_id.to_string()),
        pending_revision: true,
        structured_fields: Some(structured.clone()),
        ..Default::default()
    };
    db.upsert_documents(vec![row]).await?;

    let opts = crate::ingest::EnrichmentOpts {
        initial_memory_type: "identity".to_string(),
        initial_domain: input.capture_space.map(str::to_string),
        rejected_explicit_domain: false,
        initial_supersede_mode: "hide".to_string(),
        initial_structured_fields: Some(structured),
        agent_supplied_memory_type: false,
        agent_supplied_profile_alias: false,
        // Protect the provenance JSON from Phase-1 overwrite.
        agent_supplied_structured_fields: true,
    };
    crate::ingest::run_canonical_enrichment(
        db,
        &source_id,
        input.revised_content,
        None,
        llm,
        prompts,
        refinery,
        distillation,
        None,
        &opts,
        None,
    )
    .await;
    Ok(source_id)
}

/// A frontier row awaiting reconciliation (doc chunk or capture).
#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileItem {
    pub source_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub space: Option<String>,
    pub last_modified: i64,
    /// Per-file SHA-256 (docs only; None for captures).
    pub content_hash: Option<String>,
}

/// A vector-matched candidate on the opposite side of a frontier item.
#[derive(Debug, Clone, PartialEq)]
pub struct ReconcileCandidate {
    pub source_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub last_modified: i64,
    pub created_at: i64,
    pub content_hash: Option<String>,
    pub source_agent: Option<String>,
    pub cosine: f64,
}

/// One judge-confirmed conflict: candidate index + the corrected capture text.
// Consumed by the judge-call wiring step landing in a later task.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ConflictProposal {
    pub idx: usize,
    pub revised_content: String,
}

/// Defensively parse the judge's response. Mirrors `parse_dual_pool`'s
/// silent-zero guard: ANY parse failure returns empty (the sweep must never
/// act on garbage). Out-of-range indices and blank rewrites are dropped.
// Consumed by the judge-call wiring step landing in a later task.
#[allow(dead_code)]
pub(crate) fn parse_doc_reconcile(raw: &str, total_len: usize) -> Vec<ConflictProposal> {
    let stripped = crate::llm_provider::strip_think_tags(raw);
    let (start, end) = match (stripped.find('{'), stripped.rfind('}')) {
        (Some(s), Some(e)) if e >= s => (s, e),
        _ => return Vec::new(),
    };
    #[derive(serde::Deserialize)]
    struct RawConflict {
        idx: i64,
        #[serde(default)]
        revised_content: String,
    }
    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        conflicts: Vec<RawConflict>,
    }
    let parsed: Raw = match serde_json::from_str(&stripped[start..=end]) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    parsed
        .conflicts
        .into_iter()
        .filter_map(|c| {
            let idx = usize::try_from(c.idx).ok()?;
            if idx >= total_len || c.revised_content.trim().is_empty() {
                return None;
            }
            Some(ConflictProposal {
                idx,
                revised_content: c.revised_content,
            })
        })
        .collect()
}

// Consumed by the judge-call wiring step landing in a later task.
#[allow(dead_code)]
fn date_label(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| ts.to_string())
}

/// Render the judge user-prompt: focus text + numbered candidates, both sides
/// dated so a stale doc can be weighed against a newer confirmed capture.
// Consumed by the judge-call wiring step landing in a later task.
#[allow(dead_code)]
pub(crate) fn build_reconcile_prompt(
    focus_is_doc: bool,
    focus_content: &str,
    focus_date: i64,
    candidates: &[ReconcileCandidate],
) -> String {
    let (focus_label, cand_label) = if focus_is_doc {
        ("DOCUMENT", "CAPTURE")
    } else {
        ("CAPTURE", "DOCUMENT")
    };
    let mut p = format!(
        "FOCUS ({focus_label}, dated {}):\n{}\n\nCANDIDATES ({cand_label} side):\n",
        date_label(focus_date),
        focus_content
    );
    for (i, c) in candidates.iter().enumerate() {
        p.push_str(&format!(
            "[{i}] (dated {}) {}\n",
            date_label(c.last_modified),
            c.content
        ));
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, content: &str) -> ReconcileCandidate {
        ReconcileCandidate {
            source_id: id.to_string(),
            chunk_index: 0,
            content: content.to_string(),
            last_modified: 1_700_000_000,
            created_at: 1_700_000_000,
            content_hash: None,
            source_agent: None,
            cosine: 0.9,
        }
    }

    #[test]
    fn parse_well_formed_conflicts() {
        let raw = r#"{"conflicts":[{"idx":0,"revised_content":"Port is 7878."},{"idx":2,"revised_content":"Uses libSQL."}]}"#;
        let out = parse_doc_reconcile(raw, 3);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].idx, 0);
        assert_eq!(out[0].revised_content, "Port is 7878.");
        assert_eq!(out[1].idx, 2);
    }

    #[test]
    fn parse_tolerates_think_tags_and_fences() {
        let raw = "<think>hmm</think>\n```json\n{\"conflicts\":[{\"idx\":1,\"revised_content\":\"x\"}]}\n```";
        assert_eq!(parse_doc_reconcile(raw, 2).len(), 1);
    }

    #[test]
    fn parse_garbage_and_no_conflicts_return_empty() {
        assert!(parse_doc_reconcile("not json", 3).is_empty());
        assert!(parse_doc_reconcile(r#"{"conflicts":[]}"#, 3).is_empty());
        assert!(parse_doc_reconcile(r#"{"other":1}"#, 3).is_empty());
    }

    #[test]
    fn parse_drops_out_of_range_negative_and_empty_content() {
        let raw = r#"{"conflicts":[{"idx":9,"revised_content":"x"},{"idx":-1,"revised_content":"x"},{"idx":0,"revised_content":"  "},{"idx":1,"revised_content":"keep"}]}"#;
        let out = parse_doc_reconcile(raw, 3);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].idx, 1);
    }

    #[test]
    fn prompt_numbers_candidates_and_carries_dates_and_roles() {
        let cands = vec![cand("a", "claim A"), cand("b", "claim B")];
        let p = build_reconcile_prompt(true, "doc text", 1_700_000_000, &cands);
        assert!(p.contains("FOCUS (DOCUMENT"));
        assert!(p.contains("CANDIDATES (CAPTURE side)"));
        assert!(p.contains("[0]") && p.contains("[1]"));
        assert!(
            p.contains("2023-11-14"),
            "epoch 1700000000 renders as its UTC date"
        );
        let p2 = build_reconcile_prompt(false, "capture text", 1_700_000_000, &cands);
        assert!(p2.contains("FOCUS (CAPTURE"));
        assert!(p2.contains("CANDIDATES (DOCUMENT side)"));
    }

    #[test]
    fn registry_carries_doc_reconcile_default() {
        let reg = crate::prompts::PromptRegistry::default();
        assert!(reg.doc_reconcile.contains("mutually exclusive"));
        assert!(reg.doc_reconcile.contains("conflicts"));
    }
}
