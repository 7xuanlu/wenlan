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
