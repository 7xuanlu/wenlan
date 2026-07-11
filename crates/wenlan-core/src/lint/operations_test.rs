#[path = "operations_test/config_queue.rs"]
mod config_queue;
#[path = "operations_test/nonmutation.rs"]
mod nonmutation;
#[path = "operations_test/review_maintenance.rs"]
mod review_maintenance;

use crate::lint::context::{CancellationToken, LintClock};
use crate::lint::runner::LintRunner;
use crate::lint::snapshot::LintReadSnapshot;
use crate::lint::test_support::DbSemanticFingerprint;
use std::path::PathBuf;
use wenlan_types::lint::{LintMetricCode, LintMetricValue, LintQuery};
use wenlan_types::sources::{Source, SourceType, SyncStatus};

const NOW: i64 = 1_700_000_000;
const SOURCE_CONFIG: &str = "operations.source_configuration";
const IMPORTS: &str = "operations.import_checkpoints";
const QUEUE: &str = "operations.document_queue";
const REFINEMENTS: &str = "operations.refinement_inventory";
const REJECTIONS: &str = "operations.rejection_inventory";
const MAINTENANCE: &str = "operations.maintenance_backlogs";

async fn run(db: &crate::db::MemoryDB, sources: &[Source]) -> wenlan_types::lint::LintReport {
    LintRunner::new(LintClock::fixed_at(NOW), CancellationToken::new())
        .with_sources(sources)
        .run(db, &LintQuery { space: None }, None, false)
        .await
        .unwrap()
}

fn source(id: &str, path: &str, status: SyncStatus) -> Source {
    Source {
        id: id.into(),
        source_type: SourceType::Directory,
        path: PathBuf::from(path),
        status,
        last_sync: None,
        file_count: 0,
        memory_count: 0,
        last_sync_errors: 0,
        last_sync_error_detail: None,
    }
}

fn check<'a>(
    report: &'a wenlan_types::lint::LintReport,
    id: &str,
) -> &'a wenlan_types::lint::LintCheckResult {
    report
        .checks()
        .iter()
        .find(|result| result.check_id() == id)
        .unwrap()
}

fn metric(result: &wenlan_types::lint::LintCheckResult, code: LintMetricCode) -> u64 {
    result
        .metrics()
        .iter()
        .find_map(|metric| {
            (metric.code() == code).then(|| match metric.value() {
                LintMetricValue::Count { value } => *value,
                _ => 0,
            })
        })
        .unwrap()
}

async fn fingerprint(db: &crate::db::MemoryDB) -> DbSemanticFingerprint {
    let snapshot = LintReadSnapshot::open(&db._db).await.unwrap();
    let fingerprint = DbSemanticFingerprint::capture(&snapshot).await.unwrap();
    snapshot.finish().await.unwrap();
    fingerprint
}
