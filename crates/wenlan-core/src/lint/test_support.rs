#[path = "test_support_db.rs"]
mod db;
#[path = "test_support_fs.rs"]
mod fs;
#[path = "test_support_privacy.rs"]
mod privacy;

pub(crate) use db::DbSemanticFingerprint;
pub(crate) use fs::{DbBytesFingerprint, PageBytesFingerprint};
pub(crate) use privacy::{
    assert_no_privacy_canaries, normalize_json, population_after_sample_cap, validate_population,
    LintClock, PrivacyCanaries,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum TestOracleError {
    #[error("lint test oracle snapshot failed")]
    Snapshot(#[from] super::snapshot::SnapshotError),
    #[error("lint test oracle database read failed")]
    Database(#[from] libsql::Error),
    #[error("lint test oracle filesystem read failed")]
    Io(#[from] std::io::Error),
    #[error("lint test oracle Page scan failed")]
    Page(#[from] super::pages::fs::PageFsError),
    #[error("lint test oracle table has too many columns")]
    ColumnCount,
}

#[cfg(test)]
#[path = "test_support_tests.rs"]
mod tests;
