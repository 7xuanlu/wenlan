// SPDX-License-Identifier: Apache-2.0
//! Page-distillation faithfulness benchmark — measures whether distilled
//! pages introduce claims not grounded in their source memories.

use serde::{Deserialize, Serialize};

use crate::eval::report::ReportEnv;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageCaseResult {
    pub fixture_path: String,
    pub case_id: String,
    pub sentence_count: usize,
    pub faithful_count: usize,
    pub faithfulness: f64,
    pub expected_min: f64,
    pub unfaithful_sentences: Vec<String>,
}

impl PageCaseResult {
    pub fn meets_threshold(&self) -> bool {
        self.faithfulness >= self.expected_min
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageFaithfulnessReport {
    pub fixture_count: usize,
    pub case_count: usize,
    pub mean_faithfulness: f64,
    pub below_threshold_count: usize,
    pub per_case: Vec<PageCaseResult>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env: Option<ReportEnv>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latency: Option<crate::eval::latency::LatencySummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_faithfulness_report_default_is_empty() {
        let r = PageFaithfulnessReport::default();
        assert_eq!(r.fixture_count, 0);
        assert_eq!(r.mean_faithfulness, 0.0);
        assert_eq!(r.below_threshold_count, 0);
        assert!(r.per_case.is_empty());
        assert!(r.env.is_none());
    }

    #[test]
    fn page_case_result_holds_per_fixture_data() {
        let c = PageCaseResult {
            fixture_path: "test.toml".to_string(),
            case_id: "c1".to_string(),
            sentence_count: 5,
            faithful_count: 4,
            faithfulness: 0.8,
            expected_min: 0.7,
            unfaithful_sentences: vec!["This is hallucinated.".to_string()],
        };
        assert!(c.meets_threshold());
    }
}
