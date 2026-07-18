// SPDX-License-Identifier: Apache-2.0
//! Page Map "improve" pass (stage 3) — generates `status = 'suggested'`
//! nodes/edges for one page's map. See
//! docs/superpowers/plans/2026-07-18-page-map-api-spec.md.
//!
//! Design invariants, held BY CONSTRUCTION:
//! - **Insert-only.** Every write goes through
//!   [`MemoryDB::create_suggested_map_node`] /
//!   [`MemoryDB::create_suggested_map_edge`], whose `ON CONFLICT ... DO NOTHING`
//!   makes a re-proposal of an existing fingerprint a `Duplicate` and of a
//!   dismissed fingerprint a `Tombstoned` — both no-ops. The pass therefore
//!   never updates, deletes, resurrects, or overwrites any existing row
//!   (pinned / active / dismissed alike).
//! - **Grounded.** Every candidate node references a REAL object read straight
//!   from the page row: its entity, its own markdown sections, its source
//!   memories, and the pages its wikilinks resolve to. There are no free-text
//!   nodes, so grounding needs no separate existence check.
//! - **Deterministic-first.** The whole pass is deterministic db reads +
//!   string parsing; no LLM is invoked. (This is the central judgment call —
//!   see the module's report notes.)
//! - **Watermarked, not revision-bumped.** After sourcing, the pass stamps
//!   `page_maps.generated_at` WITHOUT a revision bump, so the proactive
//!   scheduler can skip unchanged pages without spawning 409s at clients.

use crate::db::page_map::{CreateEdgeOutcome, CreateNodeOutcome};
use crate::db::MemoryDB;
use crate::WenlanError;

/// Upper bound on memory nodes proposed for one page, so a page distilled from
/// hundreds of memories doesn't produce an unusable hairball. The first N by
/// the page's own `source_memory_ids` order.
const MAX_MEMORY_NODES: usize = 20;

/// How many active pages the proactive pass scans per tick (most-recently-
/// modified first). Bounded so an idle tick is cheap on a large corpus.
const PROACTIVE_SCAN_LIMIT: i64 = 200;

/// What one improve pass did. `*_skipped` counts fingerprints that were already
/// present (duplicate) or tombstoned (dismissed) — the insert-only no-ops.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImproveOutcome {
    pub nodes_created: usize,
    pub edges_created: usize,
    pub nodes_skipped: usize,
    pub edges_skipped: usize,
}

fn improve_provenance(basis: &str) -> String {
    serde_json::json!({ "pass": "improve", "basis": basis }).to_string()
}

/// ATX markdown headings (`#`..`######`), de-duplicated case-insensitively in
/// first-seen order. Setext (`===`/`---`) headings are intentionally ignored.
fn extract_headings(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            let heading = trimmed.trim_start_matches('#').trim();
            if !heading.is_empty() && seen.insert(heading.to_lowercase()) {
                out.push(heading.to_string());
            }
        }
    }
    out
}

/// `[[Wikilink]]` targets, de-duplicated case-insensitively in first-seen
/// order. `[[Title|alias]]` keeps `Title`; `[[Title#section]]` keeps `Title`.
/// Byte offsets from `find` land on the ASCII `[[` / `]]` delimiters, so the
/// slicing is UTF-8 safe.
fn extract_wikilinks(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut rest = content;
    while let Some(open) = rest.find("[[") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("]]") else { break };
        let inner = &after[..close];
        let title = inner
            .split('|')
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("")
            .trim();
        if !title.is_empty() && seen.insert(title.to_lowercase()) {
            out.push(title.to_string());
        }
        rest = &after[close + 2..];
    }
    out
}

/// Result of one node proposal.
enum NodeStep {
    /// Node exists after the call (freshly created or already present) — carries
    /// the live node id for wiring an edge to it.
    Present(String),
    /// Fingerprint is tombstoned (dismissed); nothing to link.
    Tombstoned,
    /// The map's revision moved under the pass; stop gracefully.
    Conflict,
}

#[allow(clippy::too_many_arguments)]
async fn add_suggested_node(
    db: &MemoryDB,
    page_id: &str,
    revision: &mut i64,
    parent_id: &str,
    ref_kind: &str,
    ref_id: &str,
    label: Option<&str>,
    rank: f64,
    basis: &str,
    outcome: &mut ImproveOutcome,
) -> Result<NodeStep, WenlanError> {
    match db
        .create_suggested_map_node(
            page_id,
            *revision,
            parent_id,
            ref_kind,
            ref_id,
            label,
            rank,
            &improve_provenance(basis),
        )
        .await
    {
        Ok(CreateNodeOutcome::Created(node)) => {
            *revision += 1;
            outcome.nodes_created += 1;
            Ok(NodeStep::Present(node.id))
        }
        Ok(CreateNodeOutcome::Duplicate(node)) => {
            outcome.nodes_skipped += 1;
            Ok(NodeStep::Present(node.id))
        }
        Ok(CreateNodeOutcome::Tombstoned) => {
            outcome.nodes_skipped += 1;
            Ok(NodeStep::Tombstoned)
        }
        Err(WenlanError::Conflict(_)) => Ok(NodeStep::Conflict),
        Err(e) => Err(e),
    }
}

/// Whether the edge step hit a revision conflict (stop) or completed.
enum EdgeStep {
    Done,
    Conflict,
}

async fn add_suggested_edge(
    db: &MemoryDB,
    page_id: &str,
    revision: &mut i64,
    from_node: &str,
    to_node: &str,
    outcome: &mut ImproveOutcome,
) -> Result<EdgeStep, WenlanError> {
    match db
        .create_suggested_map_edge(
            page_id,
            *revision,
            from_node,
            to_node,
            "suggested",
            None,
            &improve_provenance("wikilink"),
        )
        .await
    {
        Ok(CreateEdgeOutcome::Created(_)) => {
            *revision += 1;
            outcome.edges_created += 1;
            Ok(EdgeStep::Done)
        }
        Ok(CreateEdgeOutcome::Duplicate(_)) | Ok(CreateEdgeOutcome::Tombstoned) => {
            outcome.edges_skipped += 1;
            Ok(EdgeStep::Done)
        }
        Err(WenlanError::Conflict(_)) => Ok(EdgeStep::Conflict),
        Err(e) => Err(e),
    }
}

/// Runs the improve pass for one page: proposes `status = 'suggested'` nodes
/// (entity, sections, memories, wikilinked pages) plus suggested cross-link
/// edges for wikilinks, then stamps `generated_at`. Idempotent — re-running is
/// a no-op except for genuinely new content, because the fingerprint unique key
/// dedups. 404s if the page doesn't exist.
pub async fn improve_page_map(db: &MemoryDB, page_id: &str) -> Result<ImproveOutcome, WenlanError> {
    let page = db
        .get_page(page_id)
        .await?
        .ok_or_else(|| WenlanError::NotFound(format!("page {page_id} not found")))?;

    // Ensure the map + root exist (idempotent); capture root id + revision.
    let map = db.init_page_map(page_id).await?;
    let root_id = map
        .nodes
        .iter()
        .find(|n| n.parent_id.is_none())
        .map(|n| n.id.clone())
        .ok_or_else(|| WenlanError::VectorDb(format!("page_map {page_id} has no root node")))?;

    let mut revision = map.map.revision;
    let mut outcome = ImproveOutcome::default();

    // A concurrent client write between our reads flips a create to Conflict;
    // `source` returns early on it (a graceful stop, not an error). Any real
    // error propagates via `?`.
    source_suggestions(db, page_id, &page, &root_id, &mut revision, &mut outcome).await?;

    // Watermark regardless of an early conflict stop, so the proactive
    // scheduler doesn't re-pick the same page every tick.
    db.stamp_page_map_generated_at(page_id).await?;
    Ok(outcome)
}

async fn source_suggestions(
    db: &MemoryDB,
    page_id: &str,
    page: &wenlan_types::pages::Page,
    root_id: &str,
    revision: &mut i64,
    outcome: &mut ImproveOutcome,
) -> Result<(), WenlanError> {
    // Ranks give newly-suggested siblings a stable order under the root.
    let mut rank = 1.0f64;

    // 1. Entity this page is about.
    if let Some(entity_id) = page.entity_id.as_deref() {
        if let NodeStep::Conflict = add_suggested_node(
            db, page_id, revision, root_id, "entity", entity_id, None, rank, "entity", outcome,
        )
        .await?
        {
            return Ok(());
        }
        rank += 1.0;
    }

    // 2. The page's own markdown sections.
    for heading in extract_headings(&page.content) {
        let ref_id = format!("{page_id}#{}", crate::export::obsidian::slugify(&heading));
        if let NodeStep::Conflict = add_suggested_node(
            db,
            page_id,
            revision,
            root_id,
            "section",
            &ref_id,
            Some(&heading),
            rank,
            "section",
            outcome,
        )
        .await?
        {
            return Ok(());
        }
        rank += 1.0;
    }

    // 3. Source memories that compose the page (bounded).
    for source_id in page.source_memory_ids.iter().take(MAX_MEMORY_NODES) {
        if let NodeStep::Conflict = add_suggested_node(
            db, page_id, revision, root_id, "memory", source_id, None, rank, "memory", outcome,
        )
        .await?
        {
            return Ok(());
        }
        rank += 1.0;
    }

    // 4. Wikilinks → a linked-page node + a suggested cross-link edge.
    for title in extract_wikilinks(&page.content) {
        let Some(linked_id) = db.find_active_page_id_by_title(&title).await? else {
            continue; // unresolved title — stay grounded, propose nothing
        };
        if linked_id == page_id {
            continue; // self-link would collide with the root fingerprint
        }
        let node_id = match add_suggested_node(
            db,
            page_id,
            revision,
            root_id,
            "page",
            &linked_id,
            Some(&title),
            rank,
            "wikilink",
            outcome,
        )
        .await?
        {
            NodeStep::Present(id) => id,
            NodeStep::Tombstoned => {
                rank += 1.0;
                continue;
            }
            NodeStep::Conflict => return Ok(()),
        };
        rank += 1.0;
        if let EdgeStep::Conflict =
            add_suggested_edge(db, page_id, revision, root_id, &node_id, outcome).await?
        {
            return Ok(());
        }
    }

    Ok(())
}

/// Bounded proactive pass: improves up to `budget` active pages whose content
/// changed since their map was last generated (or was never generated).
/// Returns the number of pages improved. Config-gated by the caller (the
/// refinery `Phase::PageMaps`), never gating the explicit improve route.
pub async fn run_proactive_page_maps(db: &MemoryDB, budget: usize) -> Result<usize, WenlanError> {
    if budget == 0 {
        return Ok(0);
    }
    // Most-recently-modified first — those most likely to want fresh
    // suggestions. A single bounded window suffices for an idle tick.
    let pages = db.list_pages("active", PROACTIVE_SCAN_LIMIT, 0).await?;
    let mut improved = 0usize;
    for page in pages {
        if improved >= budget {
            break;
        }
        // Skip pages unchanged since their last generation. Both timestamps are
        // RFC3339 UTC strings written off the same clock (`pages.last_modified`
        // in `create_page_with_tuning`, `page_maps.generated_at` in
        // `stamp_page_map_generated_at`), but we compare them as parsed instants
        // rather than lexically — RFC3339's variable fractional-second precision
        // makes string order unreliable near equal times. An unparseable
        // timestamp fails safe to eligible (a redundant improve is idempotent;
        // wrongly skipping a changed page is not).
        let eligible = match db.page_map_generated_at(&page.id).await? {
            None => true,
            Some(generated_at) => {
                match (
                    chrono::DateTime::parse_from_rfc3339(&page.last_modified),
                    chrono::DateTime::parse_from_rfc3339(&generated_at),
                ) {
                    (Ok(modified), Ok(generated)) => modified > generated,
                    _ => true,
                }
            }
        };
        if !eligible {
            continue;
        }
        improve_page_map(db, &page.id).await?;
        improved += 1;
    }
    Ok(improved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::page_map::{CreateNodeOutcome, NodePatch};
    use crate::db::tests::test_db;
    use wenlan_types::requests::CreateConceptRequest;

    // --- Pure parsing helpers ---

    #[test]
    fn extract_headings_parses_atx_and_dedups() {
        let content =
            "# Overview\n\nsome text\n## Details\n### Details\ntext\n#NoSpace\nplain line";
        let headings = extract_headings(content);
        // "### Details" dedups against "## Details" (case-insensitive);
        // "#NoSpace" trims to "NoSpace".
        assert_eq!(headings, vec!["Overview", "Details", "NoSpace"]);
    }

    #[test]
    fn extract_wikilinks_parses_alias_section_and_dedups() {
        let content =
            "See [[Rust]] and [[Rust]] again, [[Tokio|the runtime]], [[Async#Futures]], [[]].";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["Rust", "Tokio", "Async"]);
    }

    // --- DB-backed improve pass ---

    async fn seed_memory_doc(db: &MemoryDB, source_id: &str, content: &str) {
        let doc = crate::sources::RawDocument {
            source: "memory".to_string(),
            source_id: source_id.to_string(),
            title: format!("memory-{source_id}"),
            content: content.to_string(),
            memory_type: Some("fact".to_string()),
            space: Some("default".to_string()),
            source_agent: Some("test-agent".to_string()),
            confidence: Some(0.9),
            confirmed: Some(true),
            ..Default::default()
        };
        db.upsert_documents(vec![doc]).await.unwrap();
    }

    async fn seed_page(
        db: &MemoryDB,
        title: &str,
        content: &str,
        source_memory_ids: Vec<String>,
    ) -> String {
        // A distilled page must cite >= 1 source memory whose text matches the
        // body — `create_page_with_tuning`'s quality gate rejects an empty
        // citation list and a body that diverges from its sources (cos sim <
        // 0.6). Seed each cited memory with the page's own content, and
        // synthesize one when the caller passed none, so the gate passes
        // deterministically regardless of whether the embedding model is loaded.
        let source_memory_ids = if source_memory_ids.is_empty() {
            vec![format!("auto-src-{title}")]
        } else {
            source_memory_ids
        };
        for sid in &source_memory_ids {
            seed_memory_doc(db, sid, content).await;
        }
        let result = crate::post_write::create_page_with_tuning(
            db,
            CreateConceptRequest {
                title: title.to_string(),
                content: content.to_string(),
                summary: None,
                entity_id: None,
                source_memory_ids,
                creation_kind: Some("distilled".to_string()),
                space: Some("default".to_string()),
                workspace: Some("default".to_string()),
            },
            "test",
            None,
            1,
            1.1, // high threshold forces a fresh page instead of clustering
        )
        .await
        .unwrap();
        result.id
    }

    #[tokio::test]
    async fn improve_creates_suggested_memory_nodes() {
        let (db, _tmp) = test_db().await;
        let page_id = seed_page(&db, "Improve Basics", "body", vec!["mem-x".to_string()]).await;

        let outcome = improve_page_map(&db, &page_id).await.unwrap();
        assert!(
            outcome.nodes_created >= 1,
            "expected at least the memory node"
        );

        let map = db.get_page_map(&page_id, true).await.unwrap().unwrap();
        let mem = map
            .nodes
            .iter()
            .find(|n| n.ref_kind == "memory" && n.ref_id == "mem-x")
            .expect("suggested memory node for mem-x");
        assert_eq!(mem.status, "suggested");
        assert!(
            mem.provenance
                .as_deref()
                .is_some_and(|p| p.contains("improve")),
            "suggested node must carry an improve provenance stamp, got {:?}",
            mem.provenance
        );

        // generated_at watermark was stamped.
        assert!(db.page_map_generated_at(&page_id).await.unwrap().is_some());
    }

    // Mutation-proof note: this test's teeth are on the INSERT-ONLY contract of
    // `create_suggested_map_node` / `create_suggested_map_edge` — specifically
    // the `ON CONFLICT(page_id, fingerprint) DO NOTHING`. Changing that clause
    // to `DO UPDATE SET status = 'suggested', provenance = excluded.provenance`
    // makes the improve pass overwrite the pre-existing pinned/active row, so
    // the "byte-identical" asserts below fail (status flips to 'suggested',
    // provenance stops being NULL). Applied that mutation locally, confirmed the
    // test fails, then reverted — it is NOT left in the shipped code.
    #[tokio::test]
    async fn improve_is_insert_only_never_touches_existing_rows() {
        let (db, _tmp) = test_db().await;
        let page_id = seed_page(
            &db,
            "Insert Only",
            "body",
            vec![
                "mem-active".to_string(),
                "mem-dismissed".to_string(),
                "mem-new".to_string(),
            ],
        )
        .await;

        let map = db.init_page_map(&page_id).await.unwrap();
        let root = map
            .nodes
            .iter()
            .find(|n| n.parent_id.is_none())
            .unwrap()
            .id
            .clone();

        // Pre-seed an ACTIVE, PINNED node for mem-active (fingerprint the pass
        // will re-propose).
        let rev = db
            .get_page_map(&page_id, true)
            .await
            .unwrap()
            .unwrap()
            .map
            .revision;
        let active = match db
            .create_map_node(&page_id, rev, &root, "memory", "mem-active", None, 0.0)
            .await
            .unwrap()
        {
            CreateNodeOutcome::Created(n) => n,
            other => panic!("expected Created, got {other:?}"),
        };
        let rev = db
            .get_page_map(&page_id, true)
            .await
            .unwrap()
            .unwrap()
            .map
            .revision;
        db.patch_map_node(
            &page_id,
            rev,
            &active.id,
            NodePatch {
                pinned: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Pre-seed a DISMISSED (tombstoned) node for mem-dismissed.
        let rev = db
            .get_page_map(&page_id, true)
            .await
            .unwrap()
            .unwrap()
            .map
            .revision;
        let dismissed = match db
            .create_map_node(&page_id, rev, &root, "memory", "mem-dismissed", None, 0.0)
            .await
            .unwrap()
        {
            CreateNodeOutcome::Created(n) => n,
            other => panic!("expected Created, got {other:?}"),
        };
        let rev = db
            .get_page_map(&page_id, true)
            .await
            .unwrap()
            .unwrap()
            .map
            .revision;
        db.delete_map_node(&page_id, rev, &dismissed.id)
            .await
            .unwrap();

        // Snapshot the pre-existing active row for a byte-identical comparison.
        let before = db.get_page_map(&page_id, true).await.unwrap().unwrap();
        let active_before = before
            .nodes
            .iter()
            .find(|n| n.id == active.id)
            .cloned()
            .unwrap();

        // Run the pass.
        improve_page_map(&db, &page_id).await.unwrap();

        let after = db.get_page_map(&page_id, true).await.unwrap().unwrap();

        // The pinned/active row is byte-identical (status, pinned, provenance,
        // rank, label, updated_at all unchanged) — the pass never touched it.
        let active_after = after
            .nodes
            .iter()
            .find(|n| n.id == active.id)
            .cloned()
            .expect("active node still present");
        assert_eq!(
            active_after, active_before,
            "insert-only violated: active row changed"
        );
        assert_eq!(active_after.status, "active");
        assert!(active_after.pinned);
        assert!(active_after.provenance.is_none());

        // The dismissed fingerprint was NOT resurrected: still exactly one row,
        // still dismissed.
        let dismissed_rows: Vec<_> = after
            .nodes
            .iter()
            .filter(|n| n.fingerprint == "memory:mem-dismissed@~")
            .collect();
        assert_eq!(dismissed_rows.len(), 1);
        assert_eq!(dismissed_rows[0].status, "dismissed");

        // Non-vacuous: the pass DID add a fresh suggestion for mem-new.
        let new_node = after
            .nodes
            .iter()
            .find(|n| n.ref_kind == "memory" && n.ref_id == "mem-new")
            .expect("suggested node for mem-new");
        assert_eq!(new_node.status, "suggested");
    }

    #[tokio::test]
    async fn improve_creates_wikilink_node_and_edge() {
        let (db, _tmp) = test_db().await;
        // Target page must exist for the wikilink to resolve (grounding).
        let _target = seed_page(&db, "Tokio", "runtime", vec![]).await;
        let source = seed_page(&db, "Rust Async", "See [[Tokio]] for details.", vec![]).await;

        let outcome = improve_page_map(&db, &source).await.unwrap();
        assert!(outcome.nodes_created >= 1);
        assert!(
            outcome.edges_created >= 1,
            "wikilink must yield a cross-link edge"
        );

        let map = db.get_page_map(&source, true).await.unwrap().unwrap();
        let linked = map
            .nodes
            .iter()
            .find(|n| n.ref_kind == "page" && n.ref_id == _target)
            .expect("suggested page node for the wikilink");
        assert_eq!(linked.status, "suggested");
        let edge = map
            .edges
            .iter()
            .find(|e| e.to_node == linked.id)
            .expect("suggested edge to the linked page node");
        assert_eq!(edge.status, "suggested");
        assert_eq!(edge.kind, "suggested");
        assert!(edge
            .provenance
            .as_deref()
            .is_some_and(|p| p.contains("improve")));
    }

    #[tokio::test]
    async fn improve_is_idempotent_second_pass_creates_nothing() {
        let (db, _tmp) = test_db().await;
        let page_id = seed_page(&db, "Idempotent", "body", vec!["mem-1".to_string()]).await;

        let first = improve_page_map(&db, &page_id).await.unwrap();
        assert!(first.nodes_created >= 1);

        let second = improve_page_map(&db, &page_id).await.unwrap();
        assert_eq!(second.nodes_created, 0, "re-proposals must all dedup");
        assert_eq!(second.edges_created, 0);
    }

    #[tokio::test]
    async fn improve_page_map_404s_for_unknown_page() {
        let (db, _tmp) = test_db().await;
        let err = improve_page_map(&db, "no-such-page").await.unwrap_err();
        assert!(matches!(err, WenlanError::NotFound(_)));
    }

    // --- Proactive runner ---

    #[tokio::test]
    async fn run_proactive_respects_budget() {
        let (db, _tmp) = test_db().await;
        let p1 = seed_page(&db, "Proactive One", "body", vec!["m1".to_string()]).await;
        let p2 = seed_page(&db, "Proactive Two", "body", vec!["m2".to_string()]).await;
        let p3 = seed_page(&db, "Proactive Three", "body", vec!["m3".to_string()]).await;

        let improved = run_proactive_page_maps(&db, 2).await.unwrap();
        assert_eq!(improved, 2, "budget of 2 must improve exactly 2 pages");

        // Exactly two of the three pages now carry a generated_at watermark.
        let mut stamped = 0;
        for id in [&p1, &p2, &p3] {
            if db.page_map_generated_at(id).await.unwrap().is_some() {
                stamped += 1;
            }
        }
        assert_eq!(stamped, 2);
    }

    #[tokio::test]
    async fn run_proactive_skips_already_generated_pages() {
        let (db, _tmp) = test_db().await;
        let _p = seed_page(&db, "Once Only", "body", vec!["m1".to_string()]).await;

        let first = run_proactive_page_maps(&db, 5).await.unwrap();
        assert_eq!(first, 1);

        // Nothing changed the page since; a second tick improves nothing.
        let second = run_proactive_page_maps(&db, 5).await.unwrap();
        assert_eq!(
            second, 0,
            "unchanged page must be skipped by the generated_at watermark"
        );
    }
}
