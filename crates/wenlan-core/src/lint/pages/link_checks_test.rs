use super::*;
use crate::db::tests::test_db;
use crate::lint::context::{
    AppliedScope, CancellationToken, ExecutionGate, LintClock, LintContext,
};
use crate::lint::pages::fs::scan_page_root;
use crate::lint::runner::LintRunner;
use crate::lint::snapshot::LintReadSnapshot;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use wenlan_types::lint::{
    LintApplicability, LintMetricCode, LintMetricValue, LintOpaqueId, LintOutcome, LintQuery,
    LintSeverity,
};

#[path = "link_checks_test/artifacts.rs"]
mod artifacts;
#[path = "link_checks_test/manifest.rs"]
mod manifest;
#[path = "link_checks_test/orphans.rs"]
mod orphans;

async fn insert_page(conn: &libsql::Connection, id: &str, workspace: Option<&str>, status: &str) {
    conn.execute(
        "INSERT INTO pages (id, title, content, source_memory_ids, version, status, created_at, last_compiled, last_modified, workspace, creation_kind, review_status) \
         VALUES (?1, ?1, 'body', '[]', 1, ?3, 'now', 'now', 'now', ?2, 'distilled', 'confirmed')",
        libsql::params![id, workspace, status],
    )
    .await
    .unwrap();
}

async fn link_row_count(conn: &libsql::Connection) -> i64 {
    let mut rows = conn
        .query("SELECT COUNT(*) FROM page_links", ())
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

fn metric_value(result: &LintCheckResult, code: LintMetricCode) -> u64 {
    result
        .metrics()
        .iter()
        .find_map(|metric| {
            if metric.code() != code {
                return None;
            }
            match metric.value() {
                LintMetricValue::Count { value } => Some(*value),
                _ => None,
            }
        })
        .unwrap()
}

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}
