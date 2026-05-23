// SPDX-License-Identifier: Apache-2.0
//! KG-faithfulness benchmark — measures whether the extractor produces entities
//! and relations grounded in the source memory text.

use serde::{Deserialize, Serialize};

use crate::eval::report::ReportEnv;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KgCaseResult {
    pub fixture_path: String,
    pub case_id: String,
    pub entity_precision: f64,
    pub entity_recall: f64,
    pub entity_f1: f64,
    pub relation_precision: f64,
    pub relation_recall: f64,
    pub relation_f1: f64,
    pub unfaithful_entities: Vec<String>,
    pub unfaithful_relations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KgFaithfulnessReport {
    pub fixture_count: usize,
    pub case_count: usize,
    pub entity_precision: f64,
    pub entity_recall: f64,
    pub entity_f1: f64,
    pub relation_precision: f64,
    pub relation_recall: f64,
    pub relation_f1: f64,
    pub per_case: Vec<KgCaseResult>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub env: Option<ReportEnv>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub latency: Option<crate::eval::latency::LatencySummary>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KgFixture {
    pub description: String,
    pub case: Vec<KgFixtureCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KgFixtureCase {
    pub id: String,
    pub source_text: String,
    pub expected_entities: Vec<KgExpectedEntity>,
    pub expected_relations: Vec<KgExpectedRelation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KgExpectedEntity {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KgExpectedRelation {
    pub from: String,
    pub to: String,
    pub relation_type: String,
}

pub fn load_kg_fixture(path: &std::path::Path) -> Result<KgFixture, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    toml::from_str(&content).map_err(|e| format!("failed to parse {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kg_faithfulness_report_default_is_empty() {
        let r = KgFaithfulnessReport::default();
        assert_eq!(r.fixture_count, 0);
        assert_eq!(r.entity_f1, 0.0);
        assert!(r.per_case.is_empty());
        assert!(r.env.is_none());
    }

    #[test]
    fn case_result_holds_per_fixture_metrics() {
        let c = KgCaseResult {
            fixture_path: "test.toml".to_string(),
            case_id: "c1".to_string(),
            entity_precision: 0.8,
            entity_recall: 0.6,
            entity_f1: 0.686,
            relation_precision: 0.5,
            relation_recall: 0.5,
            relation_f1: 0.5,
            unfaithful_entities: vec!["fake_entity".to_string()],
            unfaithful_relations: vec![],
        };
        assert!((c.entity_f1 - 0.686).abs() < 0.01);
    }

    #[test]
    fn load_kg_fixture_parses_minimal_toml() {
        let toml_str = r#"
            description = "smoke"

            [[case]]
            id = "c1"
            source_text = "Rust guarantees memory safety."
            expected_entities = [
                { name = "Rust", kind = "language" },
                { name = "memory safety", kind = "concept" },
            ]
            expected_relations = [
                { from = "Rust", to = "memory safety", relation_type = "guarantees" },
            ]
        "#;
        let f: KgFixture = toml::from_str(toml_str).expect("toml parses");
        assert_eq!(f.case.len(), 1);
        assert_eq!(f.case[0].expected_entities.len(), 2);
        assert_eq!(f.case[0].expected_relations[0].relation_type, "guarantees");
    }

    #[test]
    fn load_kg_fixture_from_path_reads_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.toml");
        std::fs::write(
            &path,
            r#"
            description = "file load"
            [[case]]
            id = "c1"
            source_text = "X."
            expected_entities = []
            expected_relations = []
        "#,
        )
        .unwrap();
        let f = load_kg_fixture(&path).expect("loads");
        assert_eq!(f.description, "file load");
    }
}
