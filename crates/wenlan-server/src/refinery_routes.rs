// SPDX-License-Identifier: Apache-2.0
//! HTTP routes for the refinement queue surface (read + reject).
//!
//! Convergent with daemon-side `synthesis::refinement_queue::resolve_proposal`.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use wenlan_core::synthesis::refinement_queue::{apply_refinement, resolve_proposal, ResolveStatus};
use wenlan_types::responses::{
    AcceptRefinementResponse, ListRefinementsResponse, ProposalAction, RefinementPayload,
    RefinementProposalSummary, RejectRefinementResponse,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::ServerError;
use crate::memory_routes::extract_agent_name;
use crate::state::ServerState;

#[derive(Debug, Deserialize)]
pub struct ListRefinementsQuery {
    pub action: Option<String>,
    pub limit: Option<usize>,
}

pub async fn handle_list_refinements(
    State(state): State<Arc<RwLock<ServerState>>>,
    Query(q): Query<ListRefinementsQuery>,
) -> Result<Json<ListRefinementsResponse>, ServerError> {
    let limit = q.limit.unwrap_or(50).min(500);

    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };

    let pending = db
        .get_pending_refinements()
        .await
        .map_err(ServerError::from)?;

    let proposals: Vec<RefinementProposalSummary> = pending
        .into_iter()
        .filter(|p| p.status == "awaiting_review")
        .filter(|p| match &q.action {
            Some(a) => &p.action == a,
            None => true,
        })
        .take(limit)
        .filter_map(|p| {
            let action = match parse_action(&p.action) {
                Some(a) => a,
                None => {
                    tracing::warn!(
                        proposal_id = %p.id,
                        action = %p.action,
                        "refinery: skipping proposal with unknown action variant"
                    );
                    return None;
                }
            };
            Some(RefinementProposalSummary {
                action,
                payload: parse_payload(p.payload.as_deref(), &p.action),
                id: p.id,
                source_ids: p.source_ids,
                confidence: p.confidence,
                created_at: p.created_at,
            })
        })
        .collect();

    Ok(Json(ListRefinementsResponse { proposals }))
}

pub async fn handle_reject_refinement(
    State(state): State<Arc<RwLock<ServerState>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<RejectRefinementResponse>, ServerError> {
    let agent = extract_agent_name(&headers, None);

    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };

    let result = resolve_proposal(&db, &id, ResolveStatus::Dismissed, &agent)
        .await
        .map_err(ServerError::from)?;

    Ok(Json(RejectRefinementResponse { id: result.id }))
}

pub async fn handle_accept_refinement(
    State(state): State<Arc<RwLock<ServerState>>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<AcceptRefinementResponse>, ServerError> {
    let agent = extract_agent_name(&headers, None);

    let db = {
        let s = state.read().await;
        s.db.clone().ok_or(ServerError::DbNotInitialized)?
    };

    let outcome = apply_refinement(&db, &id, &agent)
        .await
        .map_err(ServerError::from)?;

    Ok(Json(AcceptRefinementResponse {
        id: outcome.id,
        action_applied: outcome.action_applied,
    }))
}

fn parse_action(s: &str) -> Option<ProposalAction> {
    match s {
        "entity_merge" => Some(ProposalAction::EntityMerge),
        "relation_conflict" => Some(ProposalAction::RelationConflict),
        "detect_contradiction" => Some(ProposalAction::DetectContradiction),
        "suggest_entity" => Some(ProposalAction::SuggestEntity),
        "dedup_merge" => Some(ProposalAction::DedupMerge),
        _ => None,
    }
}

/// Convert the raw daemon payload string into the typed wire enum.
///
/// Variant decode rules:
/// - `detect_contradiction` and `dedup_merge`: unit variants, return Some regardless of payload
/// - `suggest_entity`: raw string payload lifted into `name_hint`
/// - `entity_merge`, `relation_conflict`: JSON object payload; inject `action` tag.
///   Returns `None` if payload is malformed JSON or fails to deserialize.
/// - Unknown action: falls through to JSON-decode path; returns `None` for unknown shapes.
fn parse_payload(payload: Option<&str>, action: &str) -> Option<RefinementPayload> {
    match action {
        "detect_contradiction" => return Some(RefinementPayload::DetectContradiction),
        "dedup_merge" => return Some(RefinementPayload::DedupMerge),
        "suggest_entity" => {
            return Some(RefinementPayload::SuggestEntity {
                name_hint: payload.map(|s| s.to_string()),
            });
        }
        _ => {}
    }

    let raw = payload?;
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(raw).ok()?;
    let mut tagged = map;
    tagged.insert(
        "action".to_string(),
        serde_json::Value::String(action.to_string()),
    );
    serde_json::from_value(serde_json::Value::Object(tagged)).ok()
}

#[cfg(test)]
mod parse_payload_tests {
    use super::*;

    #[test]
    fn parses_entity_merge() {
        let raw = r#"{"existing_id":"e1","new_id":"e2","similarity":0.86}"#;
        let parsed = parse_payload(Some(raw), "entity_merge").unwrap();
        match parsed {
            RefinementPayload::EntityMerge {
                existing_id,
                new_id,
                similarity,
            } => {
                assert_eq!(existing_id, "e1");
                assert_eq!(new_id, "e2");
                assert!((similarity - 0.86).abs() < 1e-9);
            }
            _ => panic!("expected EntityMerge"),
        }
    }

    #[test]
    fn parses_relation_conflict_with_from_to() {
        let raw = r#"{"existing_id":"r1","new_id":"r2","from":"e_a","to":"e_b","old_type":"works_at","new_type":"founded"}"#;
        let parsed = parse_payload(Some(raw), "relation_conflict").unwrap();
        match parsed {
            RefinementPayload::RelationConflict { from, to, .. } => {
                assert_eq!(from, "e_a");
                assert_eq!(to, "e_b");
            }
            _ => panic!("expected RelationConflict"),
        }
    }

    #[test]
    fn detect_contradiction_unit_even_when_payload_none() {
        let parsed = parse_payload(None, "detect_contradiction").unwrap();
        assert!(matches!(parsed, RefinementPayload::DetectContradiction));
    }

    #[test]
    fn suggest_entity_lifts_raw_string_into_name_hint() {
        let parsed = parse_payload(Some("PostgreSQL"), "suggest_entity").unwrap();
        match parsed {
            RefinementPayload::SuggestEntity { name_hint } => {
                assert_eq!(name_hint.as_deref(), Some("PostgreSQL"));
            }
            _ => panic!("expected SuggestEntity"),
        }
    }

    #[test]
    fn dedup_merge_unit_with_no_payload() {
        let parsed = parse_payload(None, "dedup_merge").unwrap();
        assert!(matches!(parsed, RefinementPayload::DedupMerge));
    }

    #[test]
    fn malformed_json_returns_none() {
        let parsed = parse_payload(Some("not json"), "entity_merge");
        assert!(
            parsed.is_none(),
            "malformed payload should decode to None, not crash"
        );
    }

    #[test]
    fn parse_action_unknown_returns_none() {
        assert!(super::parse_action("entity_split_v2").is_none());
        assert!(super::parse_action("").is_none());
    }
}
