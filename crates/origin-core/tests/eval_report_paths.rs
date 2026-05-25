// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "eval-harness")]
//! Path-layout collision tests across all (layer × task × variant) combinations.

use origin_core::eval::report::{comparable_env_hash, encode_baseline_path, ReportEnv};
use origin_core::eval::EvalLayer;

fn env(layer: EvalLayer, task: &str, variant: &str) -> ReportEnv {
    ReportEnv {
        layer: Some(layer),
        task: Some(task.to_string()),
        variant: Some(variant.to_string()),
        fixture_revision: "fix".to_string(),
        embedder_revision: "emb".to_string(),
        llm_provider_class: "on-device".to_string(),
        llm_model: "qwen3-4b".to_string(),
        schema_version: 1,
        schema_db_version: Some(46),
        similarity_fn_name: "cosine".to_string(),
        ..ReportEnv::default()
    }
}

#[test]
fn full_combination_grid_has_no_collisions() {
    let root = std::path::Path::new("/tmp/baselines");
    let layers = [EvalLayer::L1Db, EvalLayer::L2Http, EvalLayer::L3Mcp];
    let tasks = ["locomo", "longmemeval"];
    let variants = ["base", "reranked", "answer_quality"];

    let mut seen = std::collections::HashSet::new();
    for layer in layers {
        for task in tasks {
            for variant in variants {
                let p = encode_baseline_path(root, &env(layer, task, variant));
                assert!(seen.insert(p.clone()), "duplicate path: {:?}", p);
            }
        }
    }
    // 3 × 2 × 3 = 18 distinct paths.
    assert_eq!(seen.len(), 18);
}

#[test]
fn cross_layer_same_hash_when_comparable_fields_equal() {
    let e1 = env(EvalLayer::L1Db, "locomo", "base");
    let e2 = env(EvalLayer::L2Http, "locomo", "base");
    assert_eq!(comparable_env_hash(&e1), comparable_env_hash(&e2));
}
