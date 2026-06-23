// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "eval-harness")]
//! Roundtrip tests confirming EvalReport + ReportEnv serialize + deserialize losslessly.

use wenlan_core::eval::report::ReportEnv;
use wenlan_core::eval::EvalLayer;

#[test]
fn report_env_roundtrip_with_all_fields() {
    let env = ReportEnv {
        layer: Some(EvalLayer::L1Db),
        task: Some("locomo".to_string()),
        variant: Some("base".to_string()),
        embed_dim: Some(768),
        similarity_fn_name: "cosine".to_string(),
        judge_model_id: Some("claude-haiku-4-5".to_string()),
        mcp_schema_hash: None,
        skill_prompt_hash: None,
        schema_version: 1,
        schema_db_version: Some(46),
        migrations_hash: Some("abc123def456abcd".to_string()),
        n_runs: 1,
        is_single_run: true,
        run_id: Some("01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string()),
        timestamp_utc: Some("2026-05-24T12:34:56Z".to_string()),
        origin_version: "0.7.0".to_string(),
        git_sha: Some("deadbeef".to_string()),
        warmup_iterations: 5,
        eval_max_usd_baseline_cap: Some(1.0),
        eval_max_usd_run_cap: Some(5.0),
        eval_max_wall_secs_cap: Some(14400),
        total_cost_usd: 0.42,
        total_wall_secs: 1234,
        ..ReportEnv::default()
    };
    let json = serde_json::to_string_pretty(&env).unwrap();
    let parsed: ReportEnv = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.layer, Some(EvalLayer::L1Db));
    assert_eq!(parsed.task.as_deref(), Some("locomo"));
    assert!(parsed.is_single_run);
    assert_eq!(parsed.warmup_iterations, 5);
    assert_eq!(parsed.total_cost_usd, 0.42);
}

#[test]
fn report_env_deserialize_legacy_shape_without_new_fields() {
    // Simulate an old saved baseline with none of the P0a fields present.
    // Include only the 9 pre-existing required fields the original ReportEnv had:
    //   fixture_revision, embedder_model, embedder_revision, retrieval_method,
    //   llm_provider_class, llm_model, judge_model, origin_version, eval_timestamp_unix
    let legacy_json = r#"{
        "fixture_revision": "old_hash",
        "embedder_model": "bge-base-en-v1.5-q",
        "embedder_revision": "abc123",
        "retrieval_method": "search_memory",
        "llm_provider_class": "on_device",
        "llm_model": "qwen3-4b",
        "judge_model": null,
        "origin_version": "0.6.0",
        "eval_timestamp_unix": 1716556800
    }"#;
    let parsed: ReportEnv = serde_json::from_str(legacy_json).unwrap();
    // New P0a fields default cleanly.
    assert_eq!(parsed.layer, None);
    assert_eq!(parsed.schema_version, 1);
    assert!(!parsed.is_single_run);
    assert_eq!(parsed.n_runs, 1);
    assert_eq!(parsed.similarity_fn_name, "cosine");
}

#[test]
fn report_env_default_matches_serde_defaults() {
    // Verify hand-rolled Default impl mirrors the serde default= attributes.
    // This catches any drift between Default::default() and deserialized-from-empty state.
    let d = ReportEnv::default();
    assert_eq!(d.similarity_fn_name, "cosine", "similarity_fn_name default");
    assert_eq!(d.schema_version, 1, "schema_version default");
    assert_eq!(d.n_runs, 1, "n_runs default");
    // Cross-check: deserializing a JSON that includes the 9 required legacy fields
    // but omits all P0a fields. The serde `default= "fn"` attributes must produce
    // the same values as our hand-rolled Default impl.
    let legacy_only_json = r#"{
        "fixture_revision": "",
        "embedder_model": "",
        "embedder_revision": "",
        "retrieval_method": "",
        "llm_provider_class": "",
        "llm_model": "",
        "judge_model": null,
        "origin_version": "",
        "eval_timestamp_unix": 0
    }"#;
    let from_json: ReportEnv = serde_json::from_str(legacy_only_json).unwrap();
    assert_eq!(
        from_json.similarity_fn_name, d.similarity_fn_name,
        "similarity_fn_name must match between Default and serde"
    );
    assert_eq!(
        from_json.schema_version, d.schema_version,
        "schema_version must match between Default and serde"
    );
    assert_eq!(
        from_json.n_runs, d.n_runs,
        "n_runs must match between Default and serde"
    );
}
