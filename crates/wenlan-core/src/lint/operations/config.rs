use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use wenlan_types::lint::{LintConfigSelection, LintConfigSetting, LintConfigValue};
use wenlan_types::sources::{Source, SyncStatus};

#[derive(Debug, Clone)]
pub(crate) struct OperationsRunConfig {
    pub(crate) captured: bool,
    pub(super) source_count: u64,
    pub(super) invalid_positions: Vec<usize>,
    pub(super) terminal_positions: Vec<usize>,
    pub(crate) configured_ids: BTreeSet<String>,
    snapshot_identity: [u8; 32],
}

impl OperationsRunConfig {
    pub(crate) fn unavailable() -> Self {
        Self {
            captured: false,
            source_count: 0,
            invalid_positions: Vec::new(),
            terminal_positions: Vec::new(),
            configured_ids: BTreeSet::new(),
            snapshot_identity: digest_records(&[]),
        }
    }

    pub(crate) fn from_sources(sources: &[Source]) -> Self {
        let mut id_counts = BTreeMap::<&str, usize>::new();
        for source in sources {
            *id_counts.entry(source.id.as_str()).or_default() += 1;
        }
        let mut records = sources
            .iter()
            .map(|source| SourceRecord::new(source, &id_counts))
            .collect::<Vec<_>>();
        records.sort_unstable();
        let invalid_positions = records
            .iter()
            .enumerate()
            .filter_map(|(position, record)| record.invalid.then_some(position))
            .collect();
        let terminal_positions = records
            .iter()
            .enumerate()
            .filter_map(|(position, record)| record.terminal.then_some(position))
            .collect();
        Self {
            captured: true,
            source_count: u64::try_from(sources.len()).unwrap_or(u64::MAX),
            invalid_positions,
            terminal_positions,
            configured_ids: sources.iter().map(|source| source.id.clone()).collect(),
            snapshot_identity: digest_records(&records),
        }
    }

    pub(crate) fn fingerprint_selections(&self) -> [LintConfigSelection; 3] {
        [
            LintConfigSelection::new(
                LintConfigSetting::SourceConfigurationCaptured,
                if self.captured {
                    LintConfigValue::Enabled
                } else {
                    LintConfigValue::Disabled
                },
            ),
            LintConfigSelection::count(
                LintConfigSetting::SourceConfigurationCount,
                self.source_count,
            ),
            LintConfigSelection::digest(
                LintConfigSetting::SourceSnapshotIdentity,
                self.snapshot_identity,
            ),
        ]
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, PartialEq)]
struct SourceRecord {
    id_digest: [u8; 32],
    path_present: bool,
    status: u8,
    terminal: bool,
    invalid: bool,
}

impl SourceRecord {
    fn new(source: &Source, id_counts: &BTreeMap<&str, usize>) -> Self {
        let terminal = matches!(
            source.status,
            SyncStatus::Error(_) | SyncStatus::Unavailable(_)
        ) || source.last_sync_errors > 0;
        Self {
            id_digest: Sha256::digest(source.id.as_bytes()).into(),
            path_present: !source.path.as_os_str().is_empty(),
            status: match source.status {
                SyncStatus::Active => 1,
                SyncStatus::Paused => 2,
                SyncStatus::Error(_) => 3,
                SyncStatus::Unavailable(_) => 4,
            },
            terminal,
            invalid: source.id.trim().is_empty()
                || source.path.as_os_str().is_empty()
                || id_counts.get(source.id.as_str()).copied().unwrap_or(0) > 1,
        }
    }
}

fn digest_records(records: &[SourceRecord]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"wenlan-lint-source-snapshot-v1\0");
    digest.update(records.len().to_le_bytes());
    for record in records {
        digest.update(record.id_digest);
        digest.update([
            u8::from(record.path_present),
            record.status,
            u8::from(record.terminal),
            u8::from(record.invalid),
        ]);
    }
    digest.finalize().into()
}
