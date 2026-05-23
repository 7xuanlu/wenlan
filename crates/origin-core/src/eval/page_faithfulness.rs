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

#[derive(Debug, Clone, Deserialize)]
pub struct PageFixture {
    pub description: String,
    pub case: Vec<PageFixtureCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PageFixtureCase {
    pub id: String,
    pub source_memories: Vec<String>,
    pub distilled_page_body: String,
    pub expected_min_faithfulness: f64,
}

const STOPWORDS: &[&str] = &[
    "with", "from", "that", "this", "these", "those", "have", "been", "will", "would", "could",
    "should", "their", "there", "where", "when", "what", "which", "while", "about", "after",
    "before", "between", "into", "over", "under", "very", "more", "most", "some", "such", "than",
    "then", "they", "them", "your", "yours",
];

/// Split a page body into sentences. Uses regex on terminal punctuation
/// followed by whitespace. Final sentence may not have trailing whitespace.
pub fn split_sentences(body: &str) -> Vec<&str> {
    let re = regex::Regex::new(r"(?m)[.!?]+\s+").expect("static regex");
    re.split(body).filter(|s| !s.trim().is_empty()).collect()
}

/// Extract content-bearing tokens from a sentence: lowercase, length >= 4,
/// excluding stopwords. Used for faithfulness overlap scoring.
pub fn content_tokens(sentence: &str) -> Vec<String> {
    sentence
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() >= 4 && !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// Returns true if at least 50% of the sentence's content tokens appear
/// as whole-word matches in the source text. Sentences with zero content
/// tokens (pure punctuation / all stopwords) are vacuously faithful.
pub fn score_sentence_faithful(sentence: &str, source: &str) -> bool {
    let toks = content_tokens(sentence);
    if toks.is_empty() {
        return true;
    }
    let lo_source = source.to_ascii_lowercase();
    let mut hits = 0usize;
    for t in &toks {
        let pattern = format!(r"\b{}\b", regex::escape(t));
        let found = regex::Regex::new(&pattern)
            .map(|re| re.is_match(&lo_source))
            .unwrap_or_else(|_| lo_source.contains(t));
        if found {
            hits += 1;
        }
    }
    hits * 2 >= toks.len() // >= 50%
}

pub fn load_page_fixture(path: &std::path::Path) -> Result<PageFixture, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    toml::from_str(&content).map_err(|e| format!("failed to parse {}: {e}", path.display()))
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

    #[test]
    fn load_page_fixture_parses_minimal_toml() {
        let toml_str = r#"
            description = "smoke"

            [[case]]
            id = "c1"
            source_memories = ["Rust is fast.", "Rust is safe."]
            distilled_page_body = "Rust is fast and safe."
            expected_min_faithfulness = 0.8
        "#;
        let f: PageFixture = toml::from_str(toml_str).expect("toml parses");
        assert_eq!(f.case.len(), 1);
        assert_eq!(f.case[0].source_memories.len(), 2);
        assert!((f.case[0].expected_min_faithfulness - 0.8).abs() < 1e-9);
    }

    #[test]
    fn load_page_fixture_from_path_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.toml");
        std::fs::write(
            &path,
            r#"
            description = "file load"
            [[case]]
            id = "c1"
            source_memories = ["x"]
            distilled_page_body = "x"
            expected_min_faithfulness = 0.5
        "#,
        )
        .unwrap();
        let f = load_page_fixture(&path).expect("loads");
        assert_eq!(f.description, "file load");
    }

    #[test]
    fn split_sentences_basic_punctuation() {
        let body = "First sentence. Second sentence! Third question? Final.";
        let s = split_sentences(body);
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn content_tokens_strips_stopwords_and_short() {
        let toks = content_tokens("This is a Rust programming language with memory safety.");
        assert!(toks.contains(&"rust".to_string()));
        assert!(toks.contains(&"programming".to_string()));
        assert!(toks.contains(&"language".to_string()));
        assert!(toks.contains(&"memory".to_string()));
        assert!(toks.contains(&"safety".to_string()));
        assert!(!toks.contains(&"this".to_string()));
        assert!(!toks.contains(&"with".to_string()));
        assert!(!toks.contains(&"is".to_string()));
    }

    #[test]
    fn score_sentence_faithful_majority_overlap() {
        let sentence = "Rust provides memory safety guarantees.";
        let sources_all = ["Rust", "provides", "memory safety", "guarantees"].join(" ");
        assert!(score_sentence_faithful(sentence, &sources_all));

        let sources_one = "Rust is great".to_string();
        assert!(!score_sentence_faithful(sentence, &sources_one));
    }

    #[test]
    fn score_sentence_faithful_empty_sentence_is_faithful() {
        assert!(score_sentence_faithful(".", "anything"));
        assert!(score_sentence_faithful("a is the", "anything"));
    }
}
