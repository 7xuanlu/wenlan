// SPDX-License-Identifier: Apache-2.0
//! LLM-judge variant of the KG-faithfulness benchmark.

use serde::{Deserialize, Serialize};

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
}
