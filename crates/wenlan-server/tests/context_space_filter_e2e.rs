// SPDX-License-Identifier: Apache-2.0
//! E2E acceptance: handle_context filters memories by space across all shelves.
//!
//! Verifies that when `space=alpha` is passed to `/api/context`:
//!   - Identity, preference, and decision shelf memories from space=beta do not
//!     appear in the response.
//!   - Identity, preference, and decision shelf memories from space=alpha do appear.
//!   - The `relevant_memories` (Tier 3 search) returns only alpha-space results.
//!   - The combined `context` string contains no beta-space marker content.
//!
//! ## Test level
//! Full HTTP router via `tower::ServiceExt::oneshot` — the real `handle_context`
//! handler is exercised end-to-end through the Axum router. Memories are inserted
//! directly via `db.upsert_documents` (bypassing the HTTP store handler and its
//! topic-match logic) then confirmed immediately. This approach is reliable because:
//!
//! 1. `load_memories_by_type` filters `confirmed != 0` — confirmed memories only.
//! 2. `handle_context` gates Tier 1 (identity) and Tier 2 (decisions) behind
//!    `trust_level = "full"`. The agent used to make the request must be registered
//!    in the DB with full trust. We call `db.register_agent()` for this — new
//!    registrations default to "full".
//! 3. Inserting via `db.upsert_documents` avoids the store handler's topic-match
//!    cascade which can set `pending_revision = true` on beta memories when alpha
//!    memories with the same type are already confirmed/protected (topic-match
//!    threshold = 0.80 when types match). The cascade would exclude beta memories
//!    from `load_memories_by_type` via the `pending_revision = 0` filter, making
//!    the cross-space leak test unreliable.
//!
//! ## Shelves covered
//! All three shelves: identity (Tier 1), preference (Tier 1), decision (Tier 2),
//! plus Tier 3 search results (`relevant_memories`).

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;
use wenlan_core::db::MemoryDB;
use wenlan_core::sources::RawDocument;
use wenlan_types::responses::ChatContextResponse;

const TEST_AGENT: &str = "test-e2e-space-filter";

async fn body_as_json<T: serde::de::DeserializeOwned>(response: axum::http::Response<Body>) -> T {
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).expect("response body is valid JSON of expected type")
}

/// Call `/api/context` as TEST_AGENT (registered as full-trust) with a space filter.
async fn chat_context(router: &common::AppRouter, query: &str, space: &str) -> ChatContextResponse {
    let body = serde_json::json!({
        "query": query,
        "space": space,
        "max_chunks": 20,
    });
    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/context")
                .header("content-type", "application/json")
                // Send the registered full-trust agent name so all three tiers load.
                // Without x-agent-name the handler resolves "unknown" trust and skips
                // Tier 1 (identity + preferences) and Tier 2 (decisions) entirely.
                .header("x-agent-name", TEST_AGENT)
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "chat-context must return 200"
    );
    body_as_json(resp).await
}

/// Insert a memory directly into the DB (bypassing HTTP + topic-match) and confirm it.
async fn insert_and_confirm(
    db: &Arc<MemoryDB>,
    source_id: &str,
    content: &str,
    memory_type: &str,
    space: &str,
) {
    insert_and_confirm_optional(db, source_id, content, memory_type, Some(space)).await;
}

async fn insert_and_confirm_optional(
    db: &Arc<MemoryDB>,
    source_id: &str,
    content: &str,
    memory_type: &str,
    space: Option<&str>,
) {
    let doc = RawDocument {
        source: "memory".to_string(),
        source_id: source_id.to_string(),
        title: format!("test-{}", source_id),
        content: content.to_string(),
        memory_type: Some(memory_type.to_string()),
        space: space.map(str::to_string),
        last_modified: chrono::Utc::now().timestamp(),
        // confirmed=None: upsert_documents stores confirmed=NULL (default 0).
        // We call confirm_memory immediately after to set confirmed=1.
        confirmed: None,
        stability: Some("new".to_string()),
        pending_revision: false,
        supersede_mode: "hide".to_string(),
        enrichment_status: "raw".to_string(),
        ..RawDocument::default()
    };
    db.upsert_documents(vec![doc])
        .await
        .unwrap_or_else(|e| panic!("upsert_documents failed for source_id={source_id}: {e}"));
    db.confirm_memory(source_id)
        .await
        .unwrap_or_else(|e| panic!("confirm_memory failed for source_id={source_id}: {e}"));
}

#[tokio::test]
async fn context_filters_memories_by_space_across_all_shelves() {
    let (router, _tmp, db) = common::test_app_no_gate().await;

    // Register TEST_AGENT so get_agent() in handle_context resolves it to
    // trust_level="full" (new registrations default to "full" per db.rs:10862).
    // Without this the handler falls back to "unknown" trust and Tier 1 / Tier 2
    // shelves are never loaded.
    db.register_agent(TEST_AGENT)
        .await
        .expect("register_agent must succeed");
    db.create_space("alpha", None, false).await.unwrap();
    db.create_space("beta", None, false).await.unwrap();

    // Insert and confirm memories for both spaces across three shelved types.
    // Content strings carry unique markers (ALPHA_MARKER vs BETA_MARKER) so we
    // can assert their absence/presence in the combined context string.
    for (source_id, content, memory_type, space) in [
        (
            "mem_alpha_id",
            "ALPHA_MARKER identity content",
            "identity",
            "alpha",
        ),
        (
            "mem_alpha_pref",
            "ALPHA_MARKER preference content",
            "preference",
            "alpha",
        ),
        (
            "mem_alpha_dec",
            "ALPHA_MARKER decision content",
            "decision",
            "alpha",
        ),
        (
            "mem_beta_id",
            "BETA_MARKER identity content",
            "identity",
            "beta",
        ),
        (
            "mem_beta_pref",
            "BETA_MARKER preference content",
            "preference",
            "beta",
        ),
        (
            "mem_beta_dec",
            "BETA_MARKER decision content",
            "decision",
            "beta",
        ),
    ] {
        insert_and_confirm(&db, source_id, content, memory_type, space).await;
    }

    // Call chat-context with space=alpha. The query is chosen to match the
    // inserted content.
    let ctx = chat_context(&router, "ALPHA_MARKER content", "alpha").await;

    // ── Tier 1: identity shelf ────────────────────────────────────────────────
    //
    // Note: the handler has a fallback — if load_memories_by_type returns empty
    // for the given space, it retries with space=None. We confirmed alpha-space
    // identity memories above, so the fallback must NOT fire and only ALPHA content
    // must appear.
    assert!(
        !ctx.profile.identity.is_empty(),
        "identity shelf must be populated (alpha space has confirmed identity memories)"
    );
    for item in &ctx.profile.identity {
        assert!(
            !item.contains("BETA_MARKER"),
            "identity shelf must not contain BETA_MARKER; got: {:?}",
            item
        );
    }

    // ── Tier 1: preference shelf ──────────────────────────────────────────────
    assert!(
        !ctx.profile.preferences.is_empty(),
        "preferences shelf must be populated (alpha space has confirmed preference memories)"
    );
    for item in &ctx.profile.preferences {
        assert!(
            !item.contains("BETA_MARKER"),
            "preferences shelf must not contain BETA_MARKER; got: {:?}",
            item
        );
    }

    // ── Tier 2: decision shelf ────────────────────────────────────────────────
    assert!(
        !ctx.knowledge.decisions.is_empty(),
        "decisions shelf must be populated (alpha space has confirmed decision memories)"
    );
    for item in &ctx.knowledge.decisions {
        assert!(
            !item.contains("BETA_MARKER"),
            "decisions shelf must not contain BETA_MARKER; got: {:?}",
            item
        );
    }

    // ── Tier 3: search results (relevant_memories) ────────────────────────────
    //
    // search_memory is called with space_filter=Some("alpha"), so all returned
    // chunks must have space=alpha. SearchResult carries the `space` field so
    // we can assert directly without content heuristics.
    for mem in &ctx.knowledge.relevant_memories {
        assert_ne!(
            mem.space.as_deref(),
            Some("beta"),
            "relevant_memories must not include beta-space chunks; \
             source_id={} content={:?}",
            mem.source_id,
            mem.content
        );
    }

    // ── Combined context string ───────────────────────────────────────────────
    //
    // The handler builds `context` by joining all sections. If any shelf above
    // leaked beta content, it would appear here too.
    assert!(
        !ctx.context.contains("BETA_MARKER"),
        "combined context string must not contain BETA_MARKER; context=\n{}",
        ctx.context
    );

    // Sanity: alpha content must appear somewhere in the combined context.
    assert!(
        ctx.context.contains("ALPHA_MARKER"),
        "combined context string must contain ALPHA_MARKER"
    );
}

#[tokio::test]
async fn context_uncategorized_filters_profile_lanes_to_null_space() {
    let (router, _tmp, db) = common::test_app_no_gate().await;
    db.register_agent(TEST_AGENT)
        .await
        .expect("register_agent must succeed");
    db.create_space("alpha", None, false).await.unwrap();

    for (source_id, content, memory_type, space) in [
        (
            "mem_uncategorized_id",
            "UNCAT_MARKER identity content",
            "identity",
            None,
        ),
        (
            "mem_uncategorized_pref",
            "UNCAT_MARKER preference content",
            "preference",
            None,
        ),
        (
            "mem_uncategorized_dec",
            "UNCAT_MARKER decision content",
            "decision",
            None,
        ),
        (
            "mem_alpha_id_for_uncat",
            "ALPHA_UNCAT_LEAK identity content",
            "identity",
            Some("alpha"),
        ),
        (
            "mem_alpha_pref_for_uncat",
            "ALPHA_UNCAT_LEAK preference content",
            "preference",
            Some("alpha"),
        ),
        (
            "mem_alpha_dec_for_uncat",
            "ALPHA_UNCAT_LEAK decision content",
            "decision",
            Some("alpha"),
        ),
    ] {
        insert_and_confirm_optional(&db, source_id, content, memory_type, space).await;
    }

    let ctx = chat_context(&router, "UNCAT_MARKER ALPHA_UNCAT_LEAK", "uncategorized").await;

    assert!(
        ctx.profile
            .identity
            .iter()
            .any(|item| item.contains("UNCAT_MARKER identity")),
        "uncategorized context must include NULL-space identity memories; got {:?}",
        ctx.profile.identity
    );
    assert!(
        ctx.profile
            .preferences
            .iter()
            .any(|item| item.contains("UNCAT_MARKER preference")),
        "uncategorized context must include NULL-space preference memories; got {:?}",
        ctx.profile.preferences
    );
    assert!(
        ctx.knowledge
            .decisions
            .iter()
            .any(|item| item.contains("UNCAT_MARKER decision")),
        "uncategorized context must include NULL-space decisions; got {:?}",
        ctx.knowledge.decisions
    );
    assert!(
        !ctx.context.contains("ALPHA_UNCAT_LEAK"),
        "uncategorized context must not leak registered-space profile memories; context=\n{}",
        ctx.context
    );
}

#[tokio::test]
async fn context_registered_space_does_not_fallback_to_unscoped_profile_lanes() {
    let (router, _tmp, db) = common::test_app_no_gate().await;
    db.register_agent(TEST_AGENT)
        .await
        .expect("register_agent must succeed");
    db.create_space("alpha", None, false).await.unwrap();

    for (source_id, content, memory_type) in [
        (
            "mem_unscoped_id_for_alpha_empty",
            "UNSCOPED_PROFILE_LEAK identity content",
            "identity",
        ),
        (
            "mem_unscoped_pref_for_alpha_empty",
            "UNSCOPED_PROFILE_LEAK preference content",
            "preference",
        ),
    ] {
        insert_and_confirm_optional(&db, source_id, content, memory_type, None).await;
    }

    let ctx = chat_context(&router, "UNSCOPED_PROFILE_LEAK", "alpha").await;

    assert!(
        ctx.profile.identity.is_empty(),
        "explicit registered scope with no scoped identity rows must stay empty, got {:?}",
        ctx.profile.identity
    );
    assert!(
        ctx.profile.preferences.is_empty(),
        "explicit registered scope with no scoped preference rows must stay empty, got {:?}",
        ctx.profile.preferences
    );
    assert!(
        !ctx.context.contains("UNSCOPED_PROFILE_LEAK"),
        "explicit registered scope must not fall back to unscoped profile memories; context=\n{}",
        ctx.context
    );
}

#[tokio::test]
async fn context_uncategorized_empty_profile_lanes_do_not_fallback_to_registered_spaces() {
    let (router, _tmp, db) = common::test_app_no_gate().await;
    db.register_agent(TEST_AGENT)
        .await
        .expect("register_agent must succeed");
    db.create_space("alpha", None, false).await.unwrap();

    for (source_id, content, memory_type) in [
        (
            "mem_alpha_id_for_empty_uncat",
            "ALPHA_EMPTY_UNCAT_LEAK identity content",
            "identity",
        ),
        (
            "mem_alpha_pref_for_empty_uncat",
            "ALPHA_EMPTY_UNCAT_LEAK preference content",
            "preference",
        ),
    ] {
        insert_and_confirm_optional(&db, source_id, content, memory_type, Some("alpha")).await;
    }

    let ctx = chat_context(&router, "ALPHA_EMPTY_UNCAT_LEAK", "uncategorized").await;

    assert!(
        ctx.profile.identity.is_empty(),
        "uncategorized scope with no NULL-space identity rows must stay empty, got {:?}",
        ctx.profile.identity
    );
    assert!(
        ctx.profile.preferences.is_empty(),
        "uncategorized scope with no NULL-space preference rows must stay empty, got {:?}",
        ctx.profile.preferences
    );
    assert!(
        !ctx.context.contains("ALPHA_EMPTY_UNCAT_LEAK"),
        "uncategorized scope must not fall back to registered-space profile memories; context=\n{}",
        ctx.context
    );
}

#[tokio::test]
async fn context_registered_space_filters_correction_facts() {
    let (router, _tmp, db) = common::test_app_no_gate().await;
    db.register_agent(TEST_AGENT)
        .await
        .expect("register_agent must succeed");
    db.create_space("alpha", None, false).await.unwrap();
    db.create_space("beta", None, false).await.unwrap();

    insert_and_confirm_optional(
        &db,
        "mem_beta_correction_fact",
        "CORRECTION_SPACE_LEAK beta fact content",
        "fact",
        Some("beta"),
    )
    .await;

    let ctx = chat_context(&router, "CORRECTION_SPACE_LEAK", "alpha").await;

    assert!(
        !ctx.context.contains("CORRECTION_SPACE_LEAK"),
        "registered alpha context must not include beta correction facts; context=\n{}",
        ctx.context
    );
    assert!(
        ctx.knowledge
            .relevant_memories
            .iter()
            .all(|result| result.space.as_deref() != Some("beta")),
        "registered alpha context must not include beta relevant memories; got {:?}",
        ctx.knowledge
            .relevant_memories
            .iter()
            .map(|result| (&result.source_id, result.space.as_deref()))
            .collect::<Vec<_>>()
    );
}
