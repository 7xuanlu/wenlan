// SPDX-License-Identifier: Apache-2.0
//! Doc-grounded revisions (L3 reconcile).
//!
//! A 30-min scheduler sweep detects direct factual contradictions between
//! ingested documents (`source_agent='folder'`) and agent captures, and stages
//! human-gated rewrite+cite revisions through the existing pending-revisions
//! surface. Doc rows are NEVER written; the capture is untouched until human
//! accept. Design: docs/superpowers/specs/2026-07-02-doc-grounded-revisions-design.md.

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
