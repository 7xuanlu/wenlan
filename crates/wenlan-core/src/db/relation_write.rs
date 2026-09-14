// SPDX-License-Identifier: Apache-2.0
//! Mechanical canonical-write extraction for `relates` edges.
//!
//! Both helpers below are transaction-scoped: they run on a
//! caller-provided `&libsql::Connection` inside a caller-owned
//! transaction. They NEVER acquire `self.conn`, BEGIN, COMMIT, ROLLBACK,
//! log activity, or update in-memory graph dirty state themselves.
//! Returned [`super::CommunityGenerationUpdate`]s must be published via
//! `record_community_dirty_nodes` only after the caller's commit.

use super::{CommunityGenerationUpdate, MemoryDB, UNFILED_SPACE_ID};

#[cfg(test)]
#[path = "relation_write_tests.rs"]
mod relation_write_tests;

/// Borrowed inputs for [`MemoryDB::create_relation_on_connection`].
/// `canonical` is the already-resolved (vocabulary-normalized) relation
/// type; `now` is the caller-observed timestamp (`chrono::Utc::now()` at
/// the public entry) passed explicitly so the helper stays deterministic.
pub(crate) struct RelationWriteInput<'a> {
    pub from_entity: &'a str,
    pub to_entity: &'a str,
    pub canonical: &'a str,
    pub source_agent: Option<&'a str>,
    pub confidence: Option<f64>,
    pub explanation: Option<&'a str>,
    pub source_memory_id: Option<&'a str>,
    pub span_quote: Option<&'a str>,
    pub source_content: Option<&'a str>,
    pub model_version: Option<&'a str>,
    pub prompt_version: Option<&'a str>,
    pub now: i64,
}

impl MemoryDB {
    /// Inner write of `create_relation_with_span`, extracted verbatim:
    /// higher-confidence merge, immutable asserted_at/source-agent, span
    /// offsets, source provenance fill, same/cross-space classification,
    /// `dual_write_edge_with_payload` and generation bumps.
    /// Caller owns the transaction and post-commit publication.
    pub(crate) async fn create_relation_on_connection(
        conn: &libsql::Connection,
        input: RelationWriteInput<'_>,
    ) -> Result<(String, bool, Vec<CommunityGenerationUpdate>), libsql::Error> {
        // G6 Stage 2 PR 2b: the `relations` upsert (mint/re-assert plus
        // its "keep the higher confidence" merge) stops here -- `edges`
        // is the sole live producer of `relates` edges now. `edge_id` is
        // content-addressed over (edge_type, src, dst, relation_type),
        // so the row this call targets is locatable without a
        // relations-table round-trip; read it directly to replicate the
        // same merge the old UPSERT enforced.
        let edge_id = crate::provenance::compute_edge_id(
            "relates",
            "entity",
            input.from_entity,
            "entity",
            input.to_entity,
            input.canonical,
        );
        let mut prior_rows = conn
            .query(
                "SELECT json_extract(payload, '$.confidence'), \
                        json_extract(payload, '$.explanation'), \
                        json_extract(payload, '$.source_agent'), \
                        json_extract(payload, '$.asserted_at') \
                 FROM edges WHERE edge_id = ?1",
                libsql::params![edge_id.clone()],
            )
            .await?;
        // The edge's semantic patch mirrors the STORED row, not this
        // call's arguments — a weaker re-assert must not regress the
        // edge's confidence/explanation, and `source_agent`/`asserted_at`
        // stay frozen at the row's first mint (the old `relations`
        // UPSERT never touched those two columns on conflict either).
        // `asserted_at` never becomes `now` on a re-assert — that is
        // its whole point (G6 Stage 1.2 trap 1: edges.created_at is not
        // a substitute).
        let existed_before;
        let (stored_conf, stored_expl, stored_agent, stored_asserted_at): (
            Option<f64>,
            Option<String>,
            Option<String>,
            Option<i64>,
        ) = match prior_rows.next().await? {
            Some(row) => {
                existed_before = true;
                (
                    row.get::<Option<f64>>(0).unwrap_or(None),
                    row.get::<Option<String>>(1).unwrap_or(None),
                    row.get::<Option<String>>(2).unwrap_or(None),
                    row.get::<Option<i64>>(3).unwrap_or(None),
                )
            }
            None => {
                existed_before = false;
                (None, None, None, None)
            }
        };
        drop(prior_rows);
        let stronger = input.confidence.is_some() && input.confidence > stored_conf;
        let (merged_conf, merged_expl) = if stronger {
            (
                input.confidence,
                input.explanation.map(|s| s.to_string()).or(stored_expl),
            )
        } else {
            (stored_conf, stored_expl)
        };
        let merged_agent = if existed_before {
            stored_agent
        } else {
            input.source_agent.map(|s| s.to_string())
        };
        let asserted_at = stored_asserted_at.unwrap_or(input.now);
        let semantic_patch = Self::relates_semantic_patch(
            merged_conf,
            merged_expl.as_deref(),
            merged_agent.as_deref(),
            asserted_at,
        );

        // Dual-write (M2 PR-1): mirror `backfill_edges_from_relations`'s
        // classification exactly so a live-written edge and a backfilled
        // edge for the same fact converge on the same edge_id.
        //
        // G6 Stage 2 PR 2c item 2: space authority ported from `entities`
        // to the entity shadow page (`pages` via `entity_page_map`) --
        // the parity receipt found 965/965 entities mapped, zero
        // `pages.space`/`entities.space` drift, and every entity-space
        // move (`update_space`, `delete_space`, `reassign_memories_space`)
        // syncing the shadow page in the same transaction. Zero rows on
        // either side (no shadow page) matches the prior "entity not
        // found" fallback.
        let mut space_rows = conn
            .query(
                "SELECT pf.space, pt.space FROM entity_page_map mf, pages pf, entity_page_map mt, pages pt \
                 WHERE mf.entity_id = ?1 AND pf.id = mf.page_id AND mt.entity_id = ?2 AND pt.id = mt.page_id",
                libsql::params![
                    input.from_entity.to_string(),
                    input.to_entity.to_string()
                ],
            )
            .await?;
        let (from_space, to_space): (Option<String>, Option<String>) =
            match space_rows.next().await? {
                Some(row) => (row.get(0).unwrap_or(None), row.get(1).unwrap_or(None)),
                None => (None, None),
            };
        drop(space_rows);
        // `relates` resolves two endpoints, not one destination, so it does
        // not go through `resolved_space_downgrades`: SAME-SPACE requires
        // both entity spaces present and equal; anything else (differing, or
        // either unresolved) is a cross-space/indeterminate downgrade. dst
        // is always `entity` (never fence-exempt external). Mirrors
        // `backfill_edges_from_relations`.
        let (lineage, space, cross_space_downgrade) = match (&from_space, &to_space) {
            (Some(fs), Some(ts)) if fs == ts => ("assertion", fs.clone(), false),
            _ => (
                "legacy",
                from_space
                    .or(to_space)
                    .unwrap_or_else(|| UNFILED_SPACE_ID.to_string()),
                true,
            ),
        };
        // M3g Stage A span capture (§2.3/§2.4): CODE locates the
        // model-supplied quote as an exact char-offset substring of the
        // source memory's content -- never guessed.
        //
        // G6 Stage 2 PR 2b: `source_memory_id` is written whenever the
        // caller supplies one, INDEPENDENT of whether this call also
        // carries span/model/prompt provenance. Before the cutover the
        // plain `create_relation` wrapper (all four extraction args
        // `None`) could leave `payload=NULL` because
        // `relations.source_memory_id` still held the linkage; now that
        // `edges` is the sole live store, dropping it here would lose
        // the provenance outright -- and `payload.$.source_memory_id` is
        // already the canonical home every migrated reader uses
        // (migration 116, `supersede_relation`'s archived snapshot, the
        // M3g candidate scan). Only a call with NO source and NO
        // extraction data still writes `payload=NULL`.
        //
        // Keys are omitted rather than emitted as JSON null: `json_patch`
        // reads a null as "remove this key", so an absent value must be
        // absent from the object (same rule as `relates_semantic_patch`).
        let payload = {
            let mut obj = serde_json::Map::new();
            if let Some(sid) = input.source_memory_id {
                obj.insert("source_memory_id".into(), serde_json::json!(sid));
            }
            if let Some(quote) = input.span_quote {
                let offsets = input
                    .source_content
                    .and_then(|content| crate::extract::locate_span_chars(content, quote));
                obj.insert(
                    "span".into(),
                    serde_json::json!({
                        "quote": quote,
                        "char_start": offsets.map(|(start, _)| start),
                        "char_end": offsets.map(|(_, end)| end),
                    }),
                );
            }
            if let Some(v) = input.model_version {
                obj.insert("model_version".into(), serde_json::json!(v));
            }
            if let Some(v) = input.prompt_version {
                obj.insert("prompt_version".into(), serde_json::json!(v));
            }
            (!obj.is_empty()).then(|| serde_json::Value::Object(obj).to_string())
        };

        let (edge_id, graph_changes) = Self::dual_write_edge_with_payload(
            conn,
            "relates",
            "entity",
            input.from_entity,
            "entity",
            input.to_entity,
            input.canonical,
            lineage,
            &space,
            cross_space_downgrade,
            None,
            payload.as_deref(),
            Some(input.canonical),
            semantic_patch.as_deref(),
        )
        .await?;
        let generation_updates =
            Self::bump_community_graph_generations(conn, graph_changes).await?;

        Ok((edge_id, existed_before, generation_updates))
    }

    /// Inner write of `supersede_relation`, extracted verbatim: exact
    /// relates/active filters, archived snapshot, soft invalidation with
    /// superseded_by=None, generation bump. Never deletes entities or
    /// sources. Caller owns the transaction and post-commit publication.
    pub(crate) async fn retire_relation_on_connection(
        conn: &libsql::Connection,
        loser_id: &str,
    ) -> Result<(Option<serde_json::Value>, Vec<CommunityGenerationUpdate>), libsql::Error> {
        let mut rows = conn
            .query(
                "SELECT src_id, dst_id, semantic_type, payload, created_at \
                 FROM edges WHERE edge_id = ?1 AND edge_type = 'relates' \
                   AND valid_until IS NULL",
                libsql::params![loser_id],
            )
            .await?;

        let snapshot = rows.next().await?.map(|row| {
            let payload: serde_json::Value = row
                .get::<Option<String>>(3)
                .unwrap_or(None)
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "id":               loser_id,
                "from_entity":      row.get::<String>(0).ok(),
                "to_entity":        row.get::<String>(1).ok(),
                "relation_type":    row.get::<Option<String>>(2).unwrap_or(None),
                "source_agent":     payload.get("source_agent"),
                "confidence":       payload.get("confidence"),
                "explanation":      payload.get("explanation"),
                "source_memory_id": payload.get("source_memory_id"),
                "created_at":       row.get::<i64>(4).ok(),
            })
        });
        drop(rows);

        // G6 Stage 2 PR 2b: the relations hard-delete stops here — the
        // snapshot above already captured everything the caller needs.
        // Dual-write (M2 PR-1): soft-invalidate the corresponding edge
        // rather than hard-delete it (append-only-with-soft-supersession,
        // spec v3 §2). `loser_id` IS the edge_id now, so no re-derivation
        // is needed (previously recomputed from the relations snapshot's
        // from/to/relation_type columns).
        let mut graph_changes = Vec::new();
        if snapshot.is_some() {
            if let Some(change) = Self::dual_write_invalidate_edge(conn, loser_id, None).await? {
                graph_changes.push(change);
            }
        }
        let generation_updates =
            Self::bump_community_graph_generations(conn, graph_changes).await?;

        Ok((snapshot, generation_updates))
    }
}
