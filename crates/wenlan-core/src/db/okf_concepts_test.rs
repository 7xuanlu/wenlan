// SPDX-License-Identifier: Apache-2.0

use super::super::tests::test_db;
use super::merge_okf_links;
use crate::db::MemoryDB;
use crate::synthesis::wikilinks::Wikilink;

const NOW: &str = "2026-09-17T00:00:00Z";

async fn page(db: &MemoryDB, id: &str, title: &str, content: &str, space: Option<&str>) {
    db.insert_page(id, title, None, content, None, space, &[], NOW)
        .await
        .unwrap();
}

async fn concept(db: &MemoryDB, page_id: &str, source_id: &str, concept_id: &str, links: &[&str]) {
    let links: Vec<String> = links.iter().map(|l| (*l).to_string()).collect();
    db.upsert_okf_concept(
        page_id,
        source_id,
        concept_id,
        &serde_json::json!({ "type": "concept", "title": concept_id }),
        &links,
    )
    .await
    .unwrap();
}

async fn scalar(db: &MemoryDB, sql: &str, id: &str) -> i64 {
    let conn = db.conn.lock().await;
    let mut rows = conn.query(sql, libsql::params![id]).await.unwrap();
    rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
}

async fn active_link_edges(db: &MemoryDB, src: &str) -> Vec<String> {
    let conn = db.conn.lock().await;
    let mut rows = conn
        .query(
            "SELECT dst_id FROM edges
             WHERE edge_type = 'links' AND src_id = ?1 AND valid_until IS NULL
             ORDER BY dst_id",
            libsql::params![src],
        )
        .await
        .unwrap();
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        out.push(row.get::<String>(0).unwrap());
    }
    out
}

async fn orphan_labels(db: &MemoryDB, src: &str) -> Vec<String> {
    let conn = db.conn.lock().await;
    let mut rows = conn
        .query(
            "SELECT label FROM page_links
             WHERE source_page_id = ?1 AND target_page_id IS NULL ORDER BY label",
            libsql::params![src],
        )
        .await
        .unwrap();
    let mut out = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        out.push(row.get::<String>(0).unwrap());
    }
    out
}

fn resolved(links: &[Wikilink], label: &str) -> Option<Option<String>> {
    links
        .iter()
        .find(|l| l.label == label)
        .map(|l| l.target_page_id.clone())
}

#[tokio::test]
async fn migration_creates_both_tables_at_schema_version() {
    let (db, _tmp) = test_db().await;
    assert_eq!(
        scalar(
            &db,
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name IN ('okf_concepts', ?1)",
            "okf_concept_links",
        )
        .await,
        2
    );
    let conn = db.conn.lock().await;
    let mut rows = conn.query("PRAGMA user_version", ()).await.unwrap();
    let version = rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap();
    assert_eq!(version, i64::from(crate::db::SCHEMA_VERSION));
}

#[tokio::test]
async fn upsert_round_trips_and_replaces_the_link_set() {
    let (db, _tmp) = test_db().await;
    concept(
        &db,
        "p-a",
        "okf-wiki",
        "concepts/a",
        &["concepts/b", "concepts/c"],
    )
    .await;

    let record = db.get_okf_concept("p-a").await.unwrap().unwrap();
    assert_eq!(record.source_id, "okf-wiki");
    assert_eq!(record.concept_id, "concepts/a");
    assert_eq!(record.frontmatter["type"], "concept");
    assert_eq!(
        db.okf_concept_link_targets("p-a").await.unwrap(),
        vec!["concepts/b", "concepts/c"]
    );

    concept(&db, "p-a", "okf-wiki", "concepts/a", &["concepts/d"]).await;
    assert_eq!(
        db.okf_concept_link_targets("p-a").await.unwrap(),
        vec!["concepts/d"]
    );
    assert!(db.get_okf_concept("missing").await.unwrap().is_none());
}

#[tokio::test]
async fn upsert_under_a_new_page_id_replaces_the_old_row() {
    let (db, _tmp) = test_db().await;
    concept(&db, "p-old", "okf-wiki", "concepts/a", &["concepts/b"]).await;
    concept(&db, "p-new", "okf-wiki", "concepts/a", &[]).await;

    assert!(db.get_okf_concept("p-old").await.unwrap().is_none());
    assert!(db
        .okf_concept_link_targets("p-old")
        .await
        .unwrap()
        .is_empty());
    assert!(db.get_okf_concept("p-new").await.unwrap().is_some());
}

#[tokio::test]
async fn links_resolve_exact_then_unique_case_fold_within_source_and_space() {
    let (db, _tmp) = test_db().await;
    page(&db, "p-a", "A", "digest a", None).await;
    page(&db, "p-b", "B", "digest b", None).await;
    page(&db, "p-c1", "C1", "digest c1", None).await;
    page(&db, "p-c2", "C2", "digest c2", None).await;
    page(&db, "p-other", "Other", "digest other", None).await;
    page(&db, "p-far", "Far", "digest far", Some("space-far")).await;
    concept(
        &db,
        "p-a",
        "okf-wiki",
        "concepts/a",
        &[
            "concepts/b",
            "Concepts/B",
            "concepts/dup",
            "concepts/elsewhere",
            "concepts/far",
            "concepts/missing",
            "concepts/a",
        ],
    )
    .await;
    concept(&db, "p-b", "okf-wiki", "concepts/b", &[]).await;
    concept(&db, "p-c1", "okf-wiki", "concepts/Dup", &[]).await;
    concept(&db, "p-c2", "okf-wiki", "concepts/DUP", &[]).await;
    concept(&db, "p-other", "okf-other", "concepts/elsewhere", &[]).await;
    concept(&db, "p-far", "okf-wiki", "concepts/far", &[]).await;

    let links = db.okf_links_for_page("p-a", None).await.unwrap();

    assert_eq!(resolved(&links, "concepts/b"), Some(Some("p-b".into())));
    assert_eq!(
        resolved(&links, "Concepts/B"),
        Some(Some("p-b".into())),
        "a unique case-insensitive match resolves"
    );
    assert_eq!(
        resolved(&links, "concepts/dup"),
        Some(None),
        "two case-folded matches are ambiguous"
    );
    assert_eq!(
        resolved(&links, "concepts/elsewhere"),
        Some(None),
        "another source's concept never resolves"
    );
    assert_eq!(
        resolved(&links, "concepts/far"),
        Some(None),
        "a concept in another Space never resolves"
    );
    assert_eq!(resolved(&links, "concepts/missing"), Some(None));
    assert_eq!(
        resolved(&links, "concepts/a"),
        None,
        "a link to the page itself is dropped"
    );
    assert!(db.okf_links_for_page("p-b", None).await.unwrap().is_empty());
    assert!(db
        .okf_links_for_page("not-a-concept", None)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_returning_concept_resolves_even_while_its_page_is_marked_source_removed() {
    let (db, _tmp) = test_db().await;
    page(&db, "p-a", "A", "digest a", None).await;
    page(&db, "p-b", "B", "digest b", None).await;
    concept(&db, "p-a", "okf-wiki", "concepts/a", &["concepts/b"]).await;
    {
        let conn = db.conn.lock().await;
        conn.execute(
            "UPDATE pages SET stale_reason = 'source_removed' WHERE id = 'p-b'",
            (),
        )
        .await
        .unwrap();
    }
    assert_eq!(
        resolved(
            &db.okf_links_for_page("p-a", None).await.unwrap(),
            "concepts/b"
        ),
        Some(None),
        "without a concept row the removed page does not resolve"
    );

    concept(&db, "p-b", "okf-wiki", "concepts/b", &[]).await;

    assert_eq!(
        resolved(
            &db.okf_links_for_page("p-a", None).await.unwrap(),
            "concepts/b"
        ),
        Some(Some("p-b".into()))
    );
}

#[tokio::test]
async fn refresh_merges_concept_links_and_every_later_refresh_keeps_them() {
    let (db, _tmp) = test_db().await;
    page(&db, "p-b", "Beta", "digest b", None).await;
    concept(&db, "p-b", "okf-wiki", "concepts/b", &[]).await;
    concept(
        &db,
        "p-a",
        "okf-wiki",
        "concepts/a",
        &["concepts/b", "concepts/later"],
    )
    .await;
    // Inserting the page runs its own link refresh.
    page(&db, "p-a", "Alpha", "digest a mentions [[Beta]]", None).await;

    assert_eq!(active_link_edges(&db, "p-a").await, vec!["p-b"]);
    assert_eq!(orphan_labels(&db, "p-a").await, vec!["concepts/later"]);

    // A non-worker write path (lint repair, rename, drafts) refreshes from the
    // stored content only; the concept links must survive it.
    db.refresh_page_wikilinks("p-a", "rewritten digest, no wikilinks")
        .await
        .unwrap();
    assert_eq!(active_link_edges(&db, "p-a").await, vec!["p-b"]);
    assert_eq!(orphan_labels(&db, "p-a").await, vec!["concepts/later"]);

    // The forward link resolves once its target arrives and the page is
    // refreshed.
    page(&db, "p-later", "Later", "digest later", None).await;
    concept(&db, "p-later", "okf-wiki", "concepts/later", &[]).await;
    db.refresh_page_wikilinks("p-a", "digest a").await.unwrap();
    assert_eq!(active_link_edges(&db, "p-a").await, vec!["p-b", "p-later"]);
    assert!(orphan_labels(&db, "p-a").await.is_empty());
}

#[tokio::test]
async fn refresh_retires_the_edge_to_a_removed_concept() {
    let (db, _tmp) = test_db().await;
    page(&db, "p-b", "Beta", "digest b", None).await;
    concept(&db, "p-b", "okf-wiki", "concepts/b", &[]).await;
    concept(&db, "p-a", "okf-wiki", "concepts/a", &["concepts/b"]).await;
    page(&db, "p-a", "Alpha", "digest a", None).await;
    assert_eq!(active_link_edges(&db, "p-a").await, vec!["p-b"]);

    assert!(db.delete_okf_concept("p-b").await.unwrap());
    db.refresh_page_wikilinks("p-a", "digest a").await.unwrap();

    assert!(active_link_edges(&db, "p-a").await.is_empty());
    assert_eq!(orphan_labels(&db, "p-a").await, vec!["concepts/b"]);
}

#[tokio::test]
async fn a_page_without_a_concept_row_keeps_plain_wikilink_behavior() {
    let (db, _tmp) = test_db().await;
    page(&db, "p-b", "Beta", "digest b", None).await;
    page(&db, "p-a", "Alpha", "see [[Beta]] and [[Gamma]]", None).await;

    assert_eq!(active_link_edges(&db, "p-a").await, vec!["p-b"]);
    assert_eq!(orphan_labels(&db, "p-a").await, vec!["Gamma"]);
}

#[tokio::test]
async fn rekey_moves_the_row_and_its_links() {
    let (db, _tmp) = test_db().await;
    concept(&db, "p-old", "okf-wiki", "concepts/old", &["concepts/b"]).await;

    assert!(db
        .rekey_okf_concept("p-old", "p-new", "concepts/new")
        .await
        .unwrap());

    assert!(db.get_okf_concept("p-old").await.unwrap().is_none());
    let record = db.get_okf_concept("p-new").await.unwrap().unwrap();
    assert_eq!(record.concept_id, "concepts/new");
    assert_eq!(
        db.okf_concept_link_targets("p-new").await.unwrap(),
        vec!["concepts/b"]
    );
    assert!(!db
        .rekey_okf_concept("p-absent", "p-x", "concepts/x")
        .await
        .unwrap());
}

#[tokio::test]
async fn delete_for_source_removes_only_that_source() {
    let (db, _tmp) = test_db().await;
    concept(&db, "p-a", "okf-wiki", "concepts/a", &["concepts/b"]).await;
    concept(&db, "p-b", "okf-wiki", "concepts/b", &[]).await;
    concept(&db, "p-x", "okf-other", "concepts/x", &["concepts/y"]).await;

    assert_eq!(
        db.delete_okf_concepts_for_source("okf-wiki").await.unwrap(),
        2
    );

    assert!(db.get_okf_concept("p-a").await.unwrap().is_none());
    assert!(db.okf_concept_link_targets("p-a").await.unwrap().is_empty());
    assert!(db.get_okf_concept("p-x").await.unwrap().is_some());
    assert_eq!(
        db.okf_concept_link_targets("p-x").await.unwrap(),
        vec!["concepts/y"]
    );
}

#[tokio::test]
async fn linking_pages_match_case_folded_targets_within_the_source() {
    let (db, _tmp) = test_db().await;
    concept(&db, "p-a", "okf-wiki", "concepts/a", &["Concepts/Target"]).await;
    concept(
        &db,
        "p-b",
        "okf-wiki",
        "concepts/b",
        &["concepts/unrelated"],
    )
    .await;
    concept(&db, "p-x", "okf-other", "concepts/x", &["concepts/target"]).await;

    let pages = db
        .okf_pages_linking_to("okf-wiki", &["concepts/target".to_string()])
        .await
        .unwrap();

    // p-a links to the concept (case-folded); p-b links elsewhere; p-x is
    // another source.
    assert_eq!(pages, vec!["p-a"]);
}

#[tokio::test]
async fn a_forward_link_resolves_when_its_linkers_are_refreshed_after_the_target_arrives() {
    let (db, _tmp) = test_db().await;
    concept(&db, "p-a", "okf-wiki", "concepts/a", &["concepts/b"]).await;
    page(&db, "p-a", "Alpha", "digest a", None).await;
    assert_eq!(orphan_labels(&db, "p-a").await, vec!["concepts/b"]);

    // The target arrives: its row first, then its page (the worker's order).
    concept(&db, "p-b", "okf-wiki", "concepts/b", &[]).await;
    page(&db, "p-b", "Beta", "digest b", None).await;
    assert_eq!(orphan_labels(&db, "p-a").await, vec!["concepts/b"]);

    let refreshed = db
        .refresh_okf_concept_linkers("okf-wiki", &["concepts/b".to_string()], &[])
        .await
        .unwrap();

    assert_eq!(refreshed, 1);
    assert!(orphan_labels(&db, "p-a").await.is_empty());
    assert_eq!(active_link_edges(&db, "p-a").await, vec!["p-b"]);
}

#[test]
fn merge_skips_a_label_or_target_already_present() {
    let mut links = vec![
        Wikilink {
            label: "Beta".into(),
            target_page_id: Some("p-b".into()),
        },
        Wikilink {
            label: "concepts/gone".into(),
            target_page_id: None,
        },
    ];

    merge_okf_links(
        &mut links,
        vec![
            Wikilink {
                label: "concepts/b".into(),
                target_page_id: Some("p-b".into()),
            },
            Wikilink {
                label: "CONCEPTS/GONE".into(),
                target_page_id: None,
            },
            Wikilink {
                label: "concepts/c".into(),
                target_page_id: None,
            },
            Wikilink {
                label: "concepts/d".into(),
                target_page_id: None,
            },
        ],
    );

    let labels: Vec<&str> = links.iter().map(|l| l.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["Beta", "concepts/gone", "concepts/c", "concepts/d"]
    );
}
