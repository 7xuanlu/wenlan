use std::collections::{BTreeMap, BTreeSet};
use wenlan_types::lint::{LintConfigSelection, LintConfigSetting, LintConfigValue};
use wenlan_types::sources::{Source, SyncStatus};

#[derive(Debug, Clone)]
pub(crate) struct OperationsRunConfig {
    pub(super) captured: bool,
    pub(super) source_count: u64,
    pub(super) invalid_positions: Vec<usize>,
    pub(super) terminal_positions: Vec<usize>,
    pub(super) configured_ids: BTreeSet<String>,
}

impl OperationsRunConfig {
    pub(crate) fn unavailable() -> Self {
        Self {
            captured: false,
            source_count: 0,
            invalid_positions: Vec::new(),
            terminal_positions: Vec::new(),
            configured_ids: BTreeSet::new(),
        }
    }

    pub(crate) fn from_sources(sources: &[Source]) -> Self {
        let mut id_counts = BTreeMap::<&str, usize>::new();
        for source in sources {
            *id_counts.entry(source.id.as_str()).or_default() += 1;
        }
        let invalid_positions = sources
            .iter()
            .enumerate()
            .filter_map(|(position, source)| {
                (source.id.trim().is_empty()
                    || source.path.as_os_str().is_empty()
                    || id_counts.get(source.id.as_str()).copied().unwrap_or(0) > 1)
                    .then_some(position)
            })
            .collect();
        let terminal_positions = sources
            .iter()
            .enumerate()
            .filter_map(|(position, source)| {
                (matches!(
                    source.status,
                    SyncStatus::Error(_) | SyncStatus::Unavailable(_)
                ) || source.last_sync_errors > 0)
                    .then_some(position)
            })
            .collect();
        Self {
            captured: true,
            source_count: u64::try_from(sources.len()).unwrap_or(u64::MAX),
            invalid_positions,
            terminal_positions,
            configured_ids: sources.iter().map(|source| source.id.clone()).collect(),
        }
    }

    pub(crate) fn fingerprint_selections(&self) -> [LintConfigSelection; 2] {
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
        ]
    }
}
