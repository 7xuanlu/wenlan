// SPDX-License-Identifier: Apache-2.0

mod common;
mod space_scoping;

#[tokio::test]
async fn wave_1_rejects_unknown_selectors() {
    space_scoping::retrieval_cases::unknown_selectors_are_rejected().await;
}

#[tokio::test]
async fn wave_1_honors_primary_and_header_precedence() {
    space_scoping::retrieval_cases::primary_and_header_precedence().await;
}

#[tokio::test]
async fn wave_1_filters_collections_before_limit() {
    space_scoping::retrieval_cases::collections_filter_before_limit().await;
}

#[tokio::test]
async fn wave_1_handles_blank_fallback_and_reserved_collision() {
    space_scoping::retrieval_cases::blank_primary_falls_back_and_reserved_collision_rejects().await;
}

#[tokio::test]
async fn wave_1_ranked_routes_exclude_cross_scope_rows() {
    space_scoping::retrieval_cases::ranked_routes_exclude_cross_scope_rows().await;
}

#[test]
fn wave_1_registry_matches_completed_catalog_contracts() {
    space_scoping::case_runner::assert_wave_1_catalog_contract();
}

#[tokio::test]
async fn wave_2_records_reject_unknown_selectors() {
    space_scoping::record_cases::unknown_selectors_are_rejected().await;
}

#[tokio::test]
async fn wave_2_records_hide_mismatched_direct_ids() {
    space_scoping::record_cases::direct_routes_do_not_disclose_mismatched_ids().await;
}

#[tokio::test]
async fn wave_2_records_filter_collections_before_materialization() {
    space_scoping::record_cases::collections_filter_before_materialization().await;
}

#[tokio::test]
async fn wave_2_records_scope_history_and_chunk_source_priority() {
    space_scoping::record_cases::history_and_chunk_source_priority_are_scoped().await;
}

#[test]
fn wave_2_records_registry_matches_completed_catalog_contracts() {
    space_scoping::record_cases::registry_matches_catalog();
}
