// SPDX-License-Identifier: Apache-2.0
//! Temporal channel for retrieval.
//!
//! Addresses the LoCoMo temporal 2.2% catastrophic gap by giving the retrieval
//! pipeline an explicit time-aware signal. Pure embedding + FTS hybrid scoring
//! has no notion of "when," so questions like "what did we agree on last week"
//! degrade to lexical luck.
//!
//! Two phases planned:
//! - **Phase A (anchored-decay)** — multiplies the hybrid score by a decay
//!   factor anchored to an [`AnchorField`]. Cheap, in-process, no schema churn
//!   beyond exposing the anchor timestamp.
//! - **Phase D (parallel-stream)** — optional separate temporal candidate
//!   stream fused into RRF alongside vector + FTS. Deferred until anchored
//!   decay has measurable headroom on the LoCoMo temporal slice.
//!
//! This module is scaffold only: the [`AnchorField`] enum and a pure resolver
//! [`resolve_anchor_timestamp`] callers will use once the decay path lands.

/// Which timestamp the temporal decay anchors to.
///
/// Decay anchored to `last_modified` (ingestion/edit time), NOT `event_date` —
/// an old email imported today should rank as freshly ingested. `event_date`
/// is for the user's mental timeline, NOT recency decay.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnchorField {
    #[default]
    LastModified,
    EventDate,
}

/// Resolve the anchor timestamp for a memory.
///
/// `EventDate` falls back to `last_modified` when no event date is set; the
/// default `LastModified` always returns `last_modified` regardless of any
/// event date the memory may carry.
pub fn resolve_anchor_timestamp(
    anchor: AnchorField,
    last_modified: i64,
    event_date: Option<i64>,
) -> i64 {
    match anchor {
        AnchorField::EventDate => event_date.unwrap_or(last_modified),
        AnchorField::LastModified => last_modified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_modified_default() {
        assert_eq!(AnchorField::default(), AnchorField::LastModified);
    }

    #[test]
    fn resolve_anchor_event_date_when_some() {
        let got = resolve_anchor_timestamp(AnchorField::EventDate, 100, Some(42));
        assert_eq!(got, 42);
    }

    #[test]
    fn resolve_anchor_event_date_falls_back_when_none() {
        let got = resolve_anchor_timestamp(AnchorField::EventDate, 100, None);
        assert_eq!(got, 100);
    }

    #[test]
    fn resolve_anchor_last_modified_ignores_event_date() {
        let got = resolve_anchor_timestamp(AnchorField::LastModified, 100, Some(42));
        assert_eq!(got, 100);
    }
}
