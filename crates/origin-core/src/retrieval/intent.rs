//! LLM-emitted query intent (#15). Distinct from the zero-LLM T19 `query_intent`
//! module: this emits a structured routing object from the deep-path LLM call.
//! Slice-1 wires only `use_graph`; `temporal_window` + `subqueries` are emitted
//! and logged (parked for #13/#11). Reconciliation: `use_graph` here supersedes
//! the T7 `route.rs` strategy router's graph routing on the deep path; the
//! `subqueries` field reuses the JSON-array contract of `decompose.rs`.

use crate::temporal_query::DateRange;

/// LLM-emitted query routing signals. All fields default to empty/false/None so
/// a malformed sub-field can never discard a well-formed one (per-field tolerance).
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QueryIntentLlm {
    pub expansions: Vec<String>,
    pub use_graph: bool,
    pub entities: Vec<String>,
    pub temporal_window: Option<DateRange>,
    pub subqueries: Vec<String>,
}

/// True iff `ORIGIN_ENABLE_INTENT_LLM` is set truthy. DISTINCT from the shipped
/// zero-LLM T19 `ORIGIN_ENABLE_QUERY_INTENT` so eval baselines never confound.
#[allow(dead_code)]
pub fn intent_llm_enabled() -> bool {
    std::env::var("ORIGIN_ENABLE_INTENT_LLM")
        .ok()
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
}

/// Parse an intent object from possibly-prose LLM output. Returns `None` when no
/// JSON object can be parsed; otherwise every field is read independently with a
/// default fallback, reusing the engine's lenient object extractor.
#[allow(dead_code)]
pub fn parse_query_intent_llm(text: &str) -> Option<QueryIntentLlm> {
    let json_str = crate::engine::extract_json(text)?;
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let str_array = |key: &str| -> Vec<String> {
        v.get(key)
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };

    let temporal_window = v.get("temporal_window").and_then(|tw| {
        let start = tw.get("start")?.as_i64()?;
        let end = tw.get("end")?.as_i64()?;
        Some(DateRange { start, end })
    });

    Some(QueryIntentLlm {
        expansions: str_array("expansions"),
        use_graph: v
            .get("use_graph")
            .and_then(|g| g.as_bool())
            .unwrap_or(false),
        entities: str_array("entities"),
        temporal_window,
        subqueries: str_array("subqueries"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_object_populates_all_fields() {
        let txt = r#"prose {"expansions":["a","b"],"use_graph":true,"entities":["Alice"],"temporal_window":{"start":100,"end":200},"subqueries":["q1","q2"]} trailing"#;
        let intent = parse_query_intent_llm(txt).expect("should parse object");
        assert_eq!(intent.expansions, vec!["a".to_string(), "b".to_string()]);
        assert!(intent.use_graph);
        assert_eq!(intent.entities, vec!["Alice".to_string()]);
        let tw = intent.temporal_window.expect("temporal_window present");
        assert_eq!(tw.start, 100);
        assert_eq!(tw.end, 200);
        assert_eq!(intent.subqueries, vec!["q1".to_string(), "q2".to_string()]);
    }

    #[test]
    fn malformed_temporal_window_does_not_discard_use_graph() {
        // temporal_window is a string, not an object: must default to None,
        // but use_graph + expansions must survive (per-field tolerance).
        let txt = r#"{"expansions":["x"],"use_graph":true,"temporal_window":"garbage"}"#;
        let intent = parse_query_intent_llm(txt).expect("should still parse");
        assert!(intent.use_graph);
        assert_eq!(intent.expansions, vec!["x".to_string()]);
        assert!(intent.temporal_window.is_none());
    }

    #[test]
    fn missing_fields_use_defaults() {
        let txt = r#"{"use_graph":false}"#;
        let intent = parse_query_intent_llm(txt).expect("parse minimal object");
        assert!(!intent.use_graph);
        assert!(intent.expansions.is_empty());
        assert!(intent.entities.is_empty());
        assert!(intent.subqueries.is_empty());
        assert!(intent.temporal_window.is_none());
    }

    #[test]
    fn no_json_object_returns_none() {
        assert!(parse_query_intent_llm("the model said nothing useful").is_none());
    }

    #[test]
    fn flag_off_by_default() {
        // Pin the var unset (matches the crate's env-flag test convention).
        temp_env::with_vars([("ORIGIN_ENABLE_INTENT_LLM", None::<&str>)], || {
            assert!(!intent_llm_enabled());
        });
    }
}
