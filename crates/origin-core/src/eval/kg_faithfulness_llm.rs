// SPDX-License-Identifier: Apache-2.0
//! LLM-judge variant of the KG-faithfulness benchmark.

use serde::{Deserialize, Serialize};

pub fn build_entity_judge_prompt(source: &str, entity_name: &str, entity_type: &str) -> String {
    format!(
        "Source text:\n{}\n\n\
         An extractor claimed the following entity is mentioned in (or paraphrased from) the source above:\n\
         - Entity: \"{}\" (type: {})\n\n\
         Verdict question: Is this entity faithful to the source — meaning the entity appears verbatim OR as a clear paraphrase/synonym of something in the source?\n\n\
         Respond with a single JSON object: {{\"verdict\": \"faithful\" | \"hallucinated\" | \"partial\", \"reason\": \"<one short sentence>\"}}\n\
         Do not wrap in markdown fences. Do not add prose before or after the JSON.",
        source, entity_name, entity_type
    )
}

pub fn build_relation_judge_prompt(
    source: &str,
    from: &str,
    to: &str,
    relation_type: &str,
) -> String {
    format!(
        "Source text:\n{}\n\n\
         An extractor claimed the following relation is mentioned in (or paraphrased from) the source above:\n\
         - Relation: \"{}\" --{}--> \"{}\"\n\n\
         Verdict question: Is this relation faithful to the source — meaning both endpoints are present (verbatim or paraphrase) AND the relation type matches a statement in the source?\n\n\
         Respond with a single JSON object: {{\"verdict\": \"faithful\" | \"hallucinated\" | \"partial\", \"reason\": \"<one short sentence>\"}}\n\
         Do not wrap in markdown fences. Do not add prose before or after the JSON.",
        source, from, relation_type, to
    )
}

#[derive(Debug, Deserialize)]
struct JudgeResponseRaw {
    verdict: KgJudgeVerdict,
    #[serde(default)]
    reason: String,
}

pub fn parse_judge_response(raw: &str) -> Result<(KgJudgeVerdict, String), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("empty response".to_string());
    }
    let body = if let Some(after) = trimmed.strip_prefix("```json") {
        after
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };
    let parsed: JudgeResponseRaw = serde_json::from_str(body).map_err(|e| {
        format!(
            "parse failed: {e} (input: {})",
            &trimmed.chars().take(80).collect::<String>()
        )
    })?;
    Ok((parsed.verdict, parsed.reason))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KgJudgeVerdict {
    Faithful,
    Hallucinated,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgJudgedEntity {
    pub name: String,
    pub verdict: KgJudgeVerdict,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KgJudgedRelation {
    pub from: String,
    pub to: String,
    pub relation_type: String,
    pub verdict: KgJudgeVerdict,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KgJudgedCase {
    pub case_id: String,
    pub entities: Vec<KgJudgedEntity>,
    pub relations: Vec<KgJudgedRelation>,
    pub entity_faithful_rate: f64,
    pub relation_faithful_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmJudgedKgReport {
    pub case_count: usize,
    pub judge_model: String,
    pub mean_entity_faithful_rate: f64,
    pub mean_relation_faithful_rate: f64,
    pub per_case: Vec<KgJudgedCase>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kg_judge_verdict_serde_roundtrip() {
        let v = KgJudgeVerdict::Hallucinated;
        let j = serde_json::to_string(&v).unwrap();
        assert_eq!(j, "\"hallucinated\"");
        let parsed: KgJudgeVerdict = serde_json::from_str(&j).unwrap();
        assert_eq!(parsed, KgJudgeVerdict::Hallucinated);
    }

    #[test]
    fn llm_judged_kg_report_default() {
        let r = LlmJudgedKgReport::default();
        assert_eq!(r.case_count, 0);
        assert_eq!(r.judge_model, "");
        assert!(r.per_case.is_empty());
    }

    #[test]
    fn build_entity_judge_prompt_includes_source_and_entity() {
        let p = build_entity_judge_prompt("Rust is fast.", "Rust", "language");
        assert!(p.contains("Rust is fast."));
        assert!(p.contains("Rust"));
        assert!(p.contains("language"));
        assert!(p.contains("faithful") || p.contains("verdict"));
    }

    #[test]
    fn build_relation_judge_prompt_includes_triple_and_source() {
        let p = build_relation_judge_prompt("Origin uses libSQL.", "Origin", "libSQL", "uses");
        assert!(p.contains("Origin uses libSQL."));
        assert!(p.contains("Origin"));
        assert!(p.contains("libSQL"));
        assert!(p.contains("uses"));
    }

    #[test]
    fn parse_judge_response_accepts_direct_json() {
        let raw = r#"{"verdict":"faithful","reason":"present verbatim"}"#;
        let (v, r) = parse_judge_response(raw).expect("ok");
        assert_eq!(v, KgJudgeVerdict::Faithful);
        assert_eq!(r, "present verbatim");
    }

    #[test]
    fn parse_judge_response_accepts_fenced_json() {
        let raw = "```json\n{\"verdict\":\"hallucinated\",\"reason\":\"not in source\"}\n```";
        let (v, _) = parse_judge_response(raw).expect("ok");
        assert_eq!(v, KgJudgeVerdict::Hallucinated);
    }

    #[test]
    fn parse_judge_response_rejects_malformed() {
        assert!(parse_judge_response("not json").is_err());
        assert!(parse_judge_response("").is_err());
        assert!(parse_judge_response(r#"{"verdict":"bogus"}"#).is_err());
    }
}
