// SPDX-License-Identifier: Apache-2.0
//! Pure helpers for the two-pool write-time resolution (T14).
//!
//! An incoming memory is resolved against TWO candidate pools in a single LLM
//! call: Pool A (near-duplicates by vector similarity) and Pool B (same
//! entity/domain rows that may *contradict* the incoming claim). The LLM returns
//! `{"duplicates":[...],"invalidates":[...]}` over a single continuous index
//! space (Pool A first, then Pool B). These helpers map indices back to pools,
//! keep the two pools disjoint, parse the LLM output defensively (silent-zero
//! guard), and decide the direction of temporal expiry.
//!
//! Everything here is pure: no DB, no axum, no tauri. The orchestrator in
//! `synthesis::refinement_queue` wires these to the database.

use std::collections::HashSet;

/// Which candidate pool an index falls into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pool {
    /// Near-duplicate by vector similarity.
    A,
    /// Same entity/domain, possibly-contradicting.
    B,
}

/// Parsed LLM decision over the continuous index space.
///
/// Indices are 0-based over `[Pool A ++ Pool B]`. Always sanitized: any index
/// `>= total_len` is dropped before this struct is returned by
/// [`parse_dual_pool`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DualPoolDecision {
    /// Indices the LLM flagged as duplicates of the incoming memory.
    pub duplicates: Vec<usize>,
    /// Indices the LLM flagged as invalidated-by / invalidating the incoming memory.
    pub invalidates: Vec<usize>,
}

/// Direction of temporal expiry once a Pool-B candidate is judged to conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExpiryDirection {
    /// The existing memory is older — soft-suppress the existing side.
    ExistingSuperseded,
    /// The existing memory is strictly *newer* (out-of-order backfill) —
    /// soft-suppress the *incoming* side instead.
    IncomingExpired,
}

/// A candidate memory fetched for resolution. Carries exactly what the
/// orchestrator needs to (a) render the prompt and (b) act on the result.
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub source_id: String,
    pub content: String,
    /// Valid-time event date (absolute epoch secs) when known, else `None`.
    pub event_date: Option<i64>,
    /// Row creation time (epoch secs) — the valid-time fallback.
    pub created_at: i64,
    /// Pinned by the user — never auto-mutate.
    pub pinned: bool,
    /// Stability tier; `"protected"` is never auto-mutated.
    pub stability: String,
}

impl Candidate {
    /// A candidate is *protected* (never auto-mutated) when pinned or when its
    /// stability tier is explicitly `protected`. Note this is intentionally
    /// narrower than `MemoryDB::is_memory_protected` (which also treats any
    /// `confirmed`/`learned` row as protected); T14's whole point is to
    /// soft-suppress *confirmed* stale facts, so only pin / explicit-protected
    /// status blocks the auto-mutation.
    pub fn is_protected(&self) -> bool {
        self.pinned || self.stability == "protected"
    }
}

/// Tunable knobs for the dual-pool resolution. Sensible defaults; not env-wired
/// (the master switch is the `WENLAN_ENABLE_DUAL_POOL_RESOLVE` flag).
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolutionConfig {
    /// Minimum cosine similarity for Pool A (near-duplicates).
    pub pool_a_cosine_threshold: f64,
    /// Maximum combined candidate count across both pools (bounds prompt cost).
    pub combined_cap: usize,
}

impl Default for ResolutionConfig {
    fn default() -> Self {
        Self {
            pool_a_cosine_threshold: 0.88,
            combined_cap: 12,
        }
    }
}

/// The incoming memory being resolved, plus the fields the pool builder needs.
#[derive(Debug, Clone)]
pub(crate) struct IncomingMemory {
    pub source_id: String,
    pub content: String,
    pub embedding: Vec<f32>,
    pub entity_id: Option<String>,
    pub domain: Option<String>,
    pub memory_type: String,
    pub structured_fields: Option<String>,
    pub event_date: Option<i64>,
    pub created_at: i64,
}

/// Map a continuous index into its originating pool.
///
/// Indices `0..a_len` belong to Pool A; `a_len..a_len+b_len` to Pool B;
/// anything `>= a_len + b_len` is out of range and returns `None`.
pub(crate) fn index_to_pool(idx: usize, a_len: usize, b_len: usize) -> Option<Pool> {
    if idx < a_len {
        Some(Pool::A)
    } else if idx < a_len + b_len {
        Some(Pool::B)
    } else {
        None
    }
}

/// Convert a continuous Pool-B index into a 0-based offset within Pool B.
/// Returns `None` when the index is not in the Pool-B range.
pub(crate) fn pool_b_offset(idx: usize, a_len: usize, b_len: usize) -> Option<usize> {
    if idx >= a_len && idx < a_len + b_len {
        Some(idx - a_len)
    } else {
        None
    }
}

/// Remove any raw Pool-B candidate whose `source_id` already appears in Pool A.
///
/// This keeps the two pools disjoint by construction: a row that is BOTH a
/// near-duplicate (Pool A) AND field-contradicting (raw Pool B) stays in Pool A
/// only, so the continuous index space never double-counts it.
pub(crate) fn subtract_pool_a(
    pool_a_ids: &HashSet<String>,
    raw_b: Vec<Candidate>,
) -> Vec<Candidate> {
    raw_b
        .into_iter()
        .filter(|c| !pool_a_ids.contains(&c.source_id))
        .collect()
}

/// Defensively parse the LLM's dual-pool response.
///
/// SILENT-ZERO GUARD (PR #147 class): returns an EMPTY decision on *any* parse
/// failure — malformed JSON, missing keys, wrong types, truncation. The applier
/// must never act on garbage, so the fail-safe is "do nothing". Out-of-range
/// indices (`>= total_len`) are silently dropped rather than failing the whole
/// parse, so one stray index can't void a valid decision.
///
/// Pipeline: `strip_think_tags` -> slice from first `{` to last `}` ->
/// `serde_json` -> drop out-of-range indices.
pub(crate) fn parse_dual_pool(raw: &str, total_len: usize) -> DualPoolDecision {
    let empty = DualPoolDecision::default();

    let stripped = crate::llm_provider::strip_think_tags(raw);

    // Narrow to the JSON object span. `find('{')` / `rfind('}')` mirrors
    // `engine::extract_json`; on any miss we bail to empty.
    let (start, end) = match (stripped.find('{'), stripped.rfind('}')) {
        (Some(s), Some(e)) if e >= s => (s, e),
        _ => return empty,
    };
    let slice = &stripped[start..=end];

    // Strict shape: both keys must be arrays of integers. Extra keys (e.g.
    // "reasoning") are ignored by the typed struct.
    #[derive(serde::Deserialize)]
    struct Raw {
        #[serde(default)]
        duplicates: Vec<i64>,
        #[serde(default)]
        invalidates: Vec<i64>,
    }

    let parsed: Raw = match serde_json::from_str(slice) {
        Ok(p) => p,
        Err(_) => return empty,
    };

    let sanitize = |v: Vec<i64>| -> Vec<usize> {
        v.into_iter()
            .filter_map(|i| usize::try_from(i).ok())
            .filter(|&i| i < total_len)
            .collect()
    };

    DualPoolDecision {
        duplicates: sanitize(parsed.duplicates),
        invalidates: sanitize(parsed.invalidates),
    }
}

/// Resolve the valid-time for a memory: the explicit `event_date` when set,
/// otherwise the row's `created_at`. (event_date is deliberately NULL for
/// anaphoric/durative facts; falling back to created_at degrades expiry to the
/// safe monotonic "new-beats-old" default.)
pub(crate) fn valid_time(event_date: Option<i64>, created_at: i64) -> i64 {
    event_date.unwrap_or(created_at)
}

/// Decide which side expires when the incoming memory conflicts with an existing
/// one. The existing memory wins (and the *incoming* expires) ONLY when its
/// valid-time is *strictly* later than the incoming's — the out-of-order
/// backfill case. Ties default to `ExistingSuperseded` (newer-ingested wins).
pub(crate) fn expiry_direction(incoming_valid: i64, existing_valid: i64) -> ExpiryDirection {
    if existing_valid > incoming_valid {
        ExpiryDirection::IncomingExpired
    } else {
        ExpiryDirection::ExistingSuperseded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(source_id: &str) -> Candidate {
        Candidate {
            source_id: source_id.into(),
            content: "c".into(),
            event_date: None,
            created_at: 0,
            pinned: false,
            stability: "new".into(),
        }
    }

    // ---- subtract_pool_a (Test 1) ----

    #[test]
    fn test_pools_disjoint_by_construction() {
        // Pool A = {a, b}; raw Pool B = {b, c, d} -> B becomes {c, d}.
        let pool_a_ids: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let raw_b = vec![cand("b"), cand("c"), cand("d")];
        let b = subtract_pool_a(&pool_a_ids, raw_b);
        let b_ids: Vec<&str> = b.iter().map(|c| c.source_id.as_str()).collect();
        assert_eq!(b_ids, vec!["c", "d"]);
        // No id appears in both pools; `b` stays in A only.
        assert!(!b_ids.contains(&"b"), "b must stay in Pool A only");
        for id in &b_ids {
            assert!(!pool_a_ids.contains(*id));
        }
    }

    #[test]
    fn subtract_pool_a_empty_a_keeps_all_b() {
        let pool_a_ids: HashSet<String> = HashSet::new();
        let raw_b = vec![cand("c"), cand("d")];
        let b = subtract_pool_a(&pool_a_ids, raw_b);
        assert_eq!(b.len(), 2);
    }

    // ---- index_to_pool (Tests 2-4) ----

    #[test]
    fn test_continuous_index_offset() {
        // a_len=2, b_len=3 -> 0,1 in A; 2,3,4 in B; 5 None.
        assert_eq!(index_to_pool(0, 2, 3), Some(Pool::A));
        assert_eq!(index_to_pool(1, 2, 3), Some(Pool::A));
        assert_eq!(index_to_pool(2, 2, 3), Some(Pool::B));
        assert_eq!(index_to_pool(3, 2, 3), Some(Pool::B));
        assert_eq!(index_to_pool(4, 2, 3), Some(Pool::B));
        assert_eq!(index_to_pool(5, 2, 3), None);
    }

    #[test]
    fn test_empty_pool_b_offset() {
        // a_len=2, b_len=0 -> 0,1 in A; 2 None; no panic.
        assert_eq!(index_to_pool(0, 2, 0), Some(Pool::A));
        assert_eq!(index_to_pool(1, 2, 0), Some(Pool::A));
        assert_eq!(index_to_pool(2, 2, 0), None);
    }

    #[test]
    fn test_empty_both_pools() {
        // a_len=0, b_len=0 -> any index None.
        assert_eq!(index_to_pool(0, 0, 0), None);
        assert_eq!(index_to_pool(7, 0, 0), None);
    }

    #[test]
    fn pool_b_offset_maps_continuous_to_local() {
        // a_len=2, b_len=3: index 2 -> offset 0, index 4 -> offset 2.
        assert_eq!(pool_b_offset(2, 2, 3), Some(0));
        assert_eq!(pool_b_offset(4, 2, 3), Some(2));
        // Pool-A index -> None.
        assert_eq!(pool_b_offset(1, 2, 3), None);
        // Out of range -> None.
        assert_eq!(pool_b_offset(5, 2, 3), None);
    }

    // ---- parse_dual_pool (Tests 5-10) ----

    #[test]
    fn test_parse_dual_pool_well_formed() {
        let d = parse_dual_pool(r#"{"duplicates":[0,1],"invalidates":[2]}"#, 3);
        assert_eq!(d.duplicates, vec![0, 1]);
        assert_eq!(d.invalidates, vec![2]);
    }

    #[test]
    fn test_parse_dual_pool_with_think_tags() {
        let raw = "<think>reasoning here</think>\n{\"duplicates\":[],\"invalidates\":[0]}";
        let d = parse_dual_pool(raw, 1);
        assert_eq!(d.duplicates, Vec::<usize>::new());
        assert_eq!(d.invalidates, vec![0]);
    }

    #[test]
    fn test_parse_dual_pool_noisy_preamble() {
        // Markdown fence + prose around the JSON object.
        let raw = "Here is the result:\n```json\n{\"duplicates\":[1],\"invalidates\":[0]}\n```";
        let d = parse_dual_pool(raw, 2);
        assert_eq!(d.duplicates, vec![1]);
        assert_eq!(d.invalidates, vec![0]);
    }

    #[test]
    fn test_parse_dual_pool_malformed_returns_empty() {
        // Silent-zero guard: non-JSON -> empty arrays, NOT an Err.
        let d = parse_dual_pool("not json at all", 3);
        assert_eq!(d, DualPoolDecision::default());
        assert!(d.duplicates.is_empty());
        assert!(d.invalidates.is_empty());
    }

    #[test]
    fn test_parse_dual_pool_missing_keys_returns_empty() {
        // No duplicates/invalidates keys -> both default to empty.
        let d = parse_dual_pool(r#"{"something_else":[1,2]}"#, 3);
        assert_eq!(d, DualPoolDecision::default());
    }

    #[test]
    fn test_parse_dual_pool_out_of_range_indices_dropped() {
        // total_len=3; 5 and 99 are out of range -> dropped.
        let d = parse_dual_pool(r#"{"duplicates":[5],"invalidates":[99]}"#, 3);
        assert!(d.duplicates.is_empty());
        assert!(d.invalidates.is_empty());
    }

    #[test]
    fn test_parse_dual_pool_partial_json() {
        // Truncated -> serde fails -> empty (fail-safe).
        let d = parse_dual_pool(r#"{"duplicates":[0]"#, 3);
        assert_eq!(d, DualPoolDecision::default());
    }

    #[test]
    fn test_parse_dual_pool_extra_keys_ignored() {
        let d = parse_dual_pool(r#"{"duplicates":[0],"invalidates":[1],"reasoning":"x"}"#, 3);
        assert_eq!(d.duplicates, vec![0]);
        assert_eq!(d.invalidates, vec![1]);
    }

    #[test]
    fn test_parse_dual_pool_empty_arrays() {
        let d = parse_dual_pool(r#"{"duplicates":[],"invalidates":[]}"#, 3);
        assert_eq!(d, DualPoolDecision::default());
    }

    #[test]
    fn test_parse_dual_pool_mixed_in_and_out_of_range() {
        // total_len=3; keep 0,2 drop 3,7.
        let d = parse_dual_pool(r#"{"duplicates":[0,3],"invalidates":[2,7]}"#, 3);
        assert_eq!(d.duplicates, vec![0]);
        assert_eq!(d.invalidates, vec![2]);
    }

    #[test]
    fn test_parse_dual_pool_negative_index_dropped() {
        // Negative -> usize::try_from fails -> dropped, rest kept.
        let d = parse_dual_pool(r#"{"duplicates":[-1,1],"invalidates":[]}"#, 3);
        assert_eq!(d.duplicates, vec![1]);
    }

    // ---- expiry_direction (Tests 11-13) ----

    #[test]
    fn test_expiry_incoming_newer_supersedes_existing() {
        // incoming=200 > existing=100 -> existing superseded.
        assert_eq!(
            expiry_direction(200, 100),
            ExpiryDirection::ExistingSuperseded
        );
    }

    #[test]
    fn test_expiry_existing_strictly_later_expires_incoming() {
        // incoming=100, existing=200 -> incoming expired (out-of-order backfill).
        assert_eq!(expiry_direction(100, 200), ExpiryDirection::IncomingExpired);
    }

    #[test]
    fn test_expiry_equal_dates_defaults_existing_superseded() {
        // tie -> newer-ingested wins; only STRICTLY-later flips.
        assert_eq!(
            expiry_direction(150, 150),
            ExpiryDirection::ExistingSuperseded
        );
    }

    // ---- valid_time (Tests 14-15) ----

    #[test]
    fn test_valid_time_uses_event_date_when_some() {
        assert_eq!(valid_time(Some(999), 100), 999);
    }

    #[test]
    fn test_expiry_null_event_date_falls_back_to_created_at() {
        // incoming event_date=None created_at=100, existing None created_at=200
        // -> valid_time uses created_at -> 200>100 -> IncomingExpired.
        let inc = valid_time(None, 100);
        let ex = valid_time(None, 200);
        assert_eq!(inc, 100);
        assert_eq!(ex, 200);
        assert_eq!(expiry_direction(inc, ex), ExpiryDirection::IncomingExpired);
    }

    #[test]
    fn test_expiry_incoming_null_existing_set() {
        // incoming event_date=None created_at=50, existing event_date=300
        // -> 300>50 -> IncomingExpired.
        let inc = valid_time(None, 50);
        let ex = valid_time(Some(300), 0);
        assert_eq!(expiry_direction(inc, ex), ExpiryDirection::IncomingExpired);
    }

    // ---- Candidate::is_protected ----

    #[test]
    fn candidate_protected_by_pin() {
        let mut c = cand("x");
        c.pinned = true;
        assert!(c.is_protected());
    }

    #[test]
    fn candidate_protected_by_stability() {
        let mut c = cand("x");
        c.stability = "protected".into();
        assert!(c.is_protected());
    }

    #[test]
    fn candidate_not_protected_when_confirmed_only() {
        // A confirmed/learned row is NOT T14-protected — T14 exists to
        // soft-suppress stale confirmed facts.
        let mut c = cand("x");
        c.stability = "confirmed".into();
        assert!(!c.is_protected());
    }
}
