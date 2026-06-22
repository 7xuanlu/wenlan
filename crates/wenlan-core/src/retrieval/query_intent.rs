// SPDX-License-Identifier: Apache-2.0
// T19 -- Zero-LLM query intent classifier.
// Default-OFF: when ORIGIN_ENABLE_QUERY_INTENT is unset, weights=1.0, byte-identical.

use crate::router::classify::{RELATIONAL_KEYWORDS, TEMPORAL_KEYWORDS};

const FACTUAL_MAX_TOKENS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryIntent {
    Factual,
    Temporal,
    General,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChannelWeights {
    pub vector: f32,
    pub fts: f32,
    pub page: f32,
}

impl ChannelWeights {
    const IDENTITY: ChannelWeights = ChannelWeights {
        vector: 1.0,
        fts: 1.0,
        page: 1.0,
    };
}

fn fts_boost() -> f32 {
    std::env::var("ORIGIN_QUERY_INTENT_FTS_BOOST")
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(1.5)
}

pub fn query_intent_enabled() -> bool {
    std::env::var("ORIGIN_ENABLE_QUERY_INTENT")
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

pub(crate) fn classify_intent(q: &str) -> QueryIntent {
    let lower = q.to_lowercase();
    if TEMPORAL_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        return QueryIntent::Temporal;
    }
    let token_count = q.split_whitespace().count();
    let has_relational = RELATIONAL_KEYWORDS.iter().any(|kw| lower.contains(kw));
    if token_count <= FACTUAL_MAX_TOKENS && !has_relational {
        return QueryIntent::Factual;
    }
    QueryIntent::General
}

pub(crate) fn preset(intent: QueryIntent) -> ChannelWeights {
    match intent {
        QueryIntent::General | QueryIntent::Temporal => ChannelWeights::IDENTITY,
        QueryIntent::Factual => ChannelWeights {
            vector: 1.0,
            fts: fts_boost(),
            page: 1.0,
        },
    }
}

pub(crate) fn effective_weights(q: &str) -> ChannelWeights {
    if !query_intent_enabled() {
        return ChannelWeights::IDENTITY;
    }
    let intent = classify_intent(q);
    log::debug!("[query_intent] intent={:?}", intent);
    preset(intent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_short_factual_query_is_factual() {
        assert_eq!(
            classify_intent("what is the database password"),
            QueryIntent::Factual
        );
    }

    #[test]
    fn classify_temporal_query_is_temporal() {
        assert_eq!(
            classify_intent("what changed last week"),
            QueryIntent::Temporal
        );
    }

    #[test]
    fn classify_long_compositional_query_is_general() {
        assert_eq!(
            classify_intent("summarize my thoughts on the database design tradeoffs and overall system complexity"),
            QueryIntent::General,
        );
    }

    #[test]
    fn preset_general_is_identity() {
        let w = preset(QueryIntent::General);
        assert_eq!(w.vector, 1.0);
        assert_eq!(w.fts, 1.0);
        assert_eq!(w.page, 1.0);
    }

    #[test]
    fn preset_temporal_is_identity() {
        let w = preset(QueryIntent::Temporal);
        assert_eq!(w.vector, 1.0);
        assert_eq!(w.fts, 1.0);
        assert_eq!(w.page, 1.0);
    }

    #[test]
    fn preset_factual_boosts_fts_only() {
        let w = preset(QueryIntent::Factual);
        assert!(w.fts > 1.0, "Factual fts must be > 1.0 (got {})", w.fts);
        assert_eq!(w.vector, 1.0);
        assert_eq!(w.page, 1.0);
    }

    #[test]
    fn env_override_fts_boost() {
        temp_env::with_vars([("ORIGIN_QUERY_INTENT_FTS_BOOST", Some("3.0"))], || {
            assert_eq!(preset(QueryIntent::Factual).fts, 3.0);
        });
        temp_env::with_vars([("ORIGIN_QUERY_INTENT_FTS_BOOST", None::<&str>)], || {
            assert_eq!(preset(QueryIntent::Factual).fts, 1.5);
        });
    }

    #[test]
    fn effective_weights_is_identity_when_flag_off() {
        for flag_val in [None::<&str>, Some("0"), Some("false"), Some("")] {
            temp_env::with_vars([("ORIGIN_ENABLE_QUERY_INTENT", flag_val)], || {
                let w = effective_weights("what is X");
                assert_eq!(w.vector, 1.0);
                assert_eq!(w.fts, 1.0);
                assert_eq!(w.page, 1.0);
            });
        }
    }
}
