use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("wenlan-core must live under <repo>/crates")
        .to_path_buf()
}

#[tokio::test]
async fn probe_enumerates_libsql_vector_indexed_tables_and_gates_count_drift() {
    let temp = tempfile::tempdir().expect("create probe fixture root");
    let db_path = temp.path().join("origin_memory.db");
    let pages_root = temp.path().join("pages");
    let output_root = temp.path().join("output");
    std::fs::create_dir_all(&pages_root).expect("create Page fixture");
    std::fs::create_dir_all(&output_root).expect("create output fixture");
    std::fs::write(
        pages_root.join("fixture.md"),
        "---\norigin_id: page-dangling\n---\n# Private fixture\n",
    )
    .expect("write Page fixture");

    let database = libsql::Builder::new_local(&db_path)
        .build()
        .await
        .expect("create libSQL fixture");
    let conn = database.connect().expect("connect libSQL fixture");
    conn.execute_batch(
        "
        CREATE TABLE memories (
            id TEXT PRIMARY KEY,
            content TEXT NOT NULL,
            source TEXT NOT NULL,
            source_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            entity_id TEXT,
            enrichment_status TEXT,
            content_hash TEXT,
            pending_revision INTEGER NOT NULL DEFAULT 0,
            supersedes TEXT,
            episode_of TEXT,
            embedding F32_BLOB(2)
        );
        CREATE TABLE entities (id TEXT PRIMARY KEY, embedding F32_BLOB(2));
        CREATE TABLE relations (id TEXT PRIMARY KEY, from_entity TEXT, to_entity TEXT);
        CREATE TABLE pages (
            id TEXT PRIMARY KEY,
            entity_id TEXT,
            source_memory_ids TEXT,
            embedding F32_BLOB(2)
        );
        CREATE TABLE page_sources (page_id TEXT, memory_source_id TEXT);
        CREATE TABLE page_evidence (page_id TEXT, source_kind TEXT, locator TEXT);
        CREATE TABLE page_links (source_page_id TEXT, target_page_id TEXT);
        CREATE TABLE enrichment_steps (source_id TEXT, step_name TEXT, status TEXT);
        CREATE TABLE source_sync_state (source_id TEXT, file_path TEXT, content_hash TEXT);
        CREATE TABLE document_enrichment_queue (source_id TEXT, file_path TEXT, status TEXT);

        INSERT INTO memories
            (id, content, source, source_id, chunk_index, entity_id,
             enrichment_status, content_hash, pending_revision, supersedes,
             episode_of, embedding)
        VALUES
            ('memory-row', 'private fixture', 'memory', 'memory-logical', 0,
             'entity-valid', 'enriched', 'hash-valid', 0, NULL, NULL,
             vector32('[1,0]'));
        INSERT INTO entities VALUES ('entity-valid', vector32('[1,0]'));
        INSERT INTO pages VALUES
            ('page-dangling', 'entity-missing', '[\"memory-logical\"]',
             vector32('[1,0]'));
        ",
    )
    .await
    .expect("seed libSQL fixture");
    conn.execute_batch(
        "
        CREATE INDEX memories_probe_vec_idx
            ON memories (libsql_vector_idx(embedding));
        CREATE INDEX entities_probe_vec_idx
            ON entities (libsql_vector_idx(embedding));
        CREATE INDEX pages_probe_vec_idx
            ON pages (libsql_vector_idx(embedding));
        ",
    )
    .await
    .expect("create libSQL vector indexes");
    drop(conn);
    drop(database);

    let probe = repo_root().join("scripts/lint-maintenance-probe.sh");
    let output = Command::new(&probe)
        .arg("--db")
        .arg(&db_path)
        .arg("--pages-root")
        .arg(&pages_root)
        .arg("--output-root")
        .arg(&output_root)
        .output()
        .expect("run lint maintenance probe");
    assert!(
        output.status.success(),
        "probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("probe stdout is UTF-8");
    let manifest_path = stdout
        .lines()
        .find_map(|line| line.strip_prefix("manifest="))
        .map(PathBuf::from)
        .expect("probe prints manifest path");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).expect("read probe manifest"))
            .expect("parse probe manifest");
    let aggregates: serde_json::Value = serde_json::from_slice(
        &std::fs::read(manifest_path.parent().unwrap().join("aggregates.json"))
            .expect("read aggregate receipt"),
    )
    .expect("parse aggregate receipt");
    let oracle = &manifest["count_oracle"];
    let oracle_status = oracle["status"].as_str().expect("oracle status");
    let oracle_receipt: serde_json::Value = serde_json::from_slice(
        &std::fs::read(manifest_path.parent().unwrap().join("count-oracle.json"))
            .expect("read count oracle receipt"),
    )
    .expect("parse count oracle receipt");
    assert_eq!(
        aggregates["pages_dangling_entity"], 1,
        "manifest={manifest:#}\naggregates={aggregates:#}\noracle={oracle_receipt:#}"
    );
    assert_eq!(oracle_receipt["tables"]["memories"]["enumerated_count"], 1);
    assert_eq!(oracle_receipt["tables"]["entities"]["enumerated_count"], 1);
    assert_eq!(oracle_receipt["tables"]["pages"]["enumerated_count"], 1);

    if oracle_status == "mismatch" {
        assert_eq!(manifest["complete"], false);
        assert_eq!(manifest["reason"], "count_oracle_mismatch");
    } else {
        assert_eq!(oracle_status, "match");
        assert_eq!(manifest["complete"], true);
        assert_eq!(manifest["reason"], "stable_snapshot");
    }
}
