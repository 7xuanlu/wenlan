use super::kg::KgRunConfig;
use super::memories::MemoryFeatureConfig;
use super::operations::OperationsRunConfig;
use wenlan_types::lint::{
    LintConfigFingerprint, LintConfigSelection, LintConfigSetting, LintConfigValue,
};

#[derive(Debug, Clone)]
pub(super) struct EffectiveLintConfig {
    pub(super) page_projection_enabled: bool,
    pub(super) memory: MemoryFeatureConfig,
    pub(super) kg: KgRunConfig,
    pub(super) operations: OperationsRunConfig,
}

impl EffectiveLintConfig {
    pub(super) const fn new(
        page_projection_enabled: bool,
        memory: MemoryFeatureConfig,
        kg: KgRunConfig,
        operations: OperationsRunConfig,
    ) -> Self {
        Self {
            page_projection_enabled,
            memory,
            kg,
            operations,
        }
    }

    pub(super) fn fingerprint(&self) -> LintConfigFingerprint {
        let mut selections = vec![
            selection(
                LintConfigSetting::PageProjectionEnabled,
                self.page_projection_enabled,
            ),
            selection(
                LintConfigSetting::EpisodeChannelEnabled,
                self.memory.episode,
            ),
            selection(LintConfigSetting::FactChannelEnabled, self.memory.fact),
            selection(
                LintConfigSetting::SummaryPreludeEnabled,
                self.memory.summary,
            ),
            selection(
                LintConfigSetting::TemporalGroundingEnabled,
                self.memory.temporal,
            ),
        ];
        selections.extend(self.kg.fingerprint_selections());
        selections.extend(self.operations.fingerprint_selections());
        LintConfigFingerprint::from_effective_config(&selections)
    }
}

const fn selection(setting: LintConfigSetting, enabled: bool) -> LintConfigSelection {
    LintConfigSelection::new(
        setting,
        if enabled {
            LintConfigValue::Enabled
        } else {
            LintConfigValue::Disabled
        },
    )
}
