use wenlan_types::lint::{LintConfigSelection, LintConfigSetting, LintConfigValue};

#[derive(Debug, Clone, Copy)]
pub(crate) struct KgRunConfig {
    pub(crate) serving_enabled: bool,
    pub(crate) sweep_enabled: bool,
    pub(crate) provider_ready: bool,
    pub(crate) hub_cap: u64,
}

impl KgRunConfig {
    pub(crate) fn capture() -> Self {
        Self {
            serving_enabled: crate::db::graph_memory_stream_enabled(),
            sweep_enabled: crate::db::entity_sweep_enabled(),
            provider_ready: crate::llm_provider::llm_provider_ready(),
            hub_cap: u64::try_from(crate::db::graph_hub_cap()).unwrap_or(u64::MAX),
        }
    }

    #[cfg(test)]
    pub(crate) const fn for_test(
        serving_enabled: bool,
        sweep_enabled: bool,
        provider_ready: bool,
        hub_cap: u64,
    ) -> Self {
        Self {
            serving_enabled,
            sweep_enabled,
            provider_ready,
            hub_cap,
        }
    }

    pub(crate) fn fingerprint_selections(self) -> [LintConfigSelection; 4] {
        [
            selection(
                LintConfigSetting::KnowledgeGraphServingEnabled,
                self.serving_enabled,
            ),
            selection(
                LintConfigSetting::KnowledgeGraphSweepEnabled,
                self.sweep_enabled,
            ),
            selection(
                LintConfigSetting::KnowledgeGraphProviderReady,
                self.provider_ready,
            ),
            LintConfigSelection::count(LintConfigSetting::KnowledgeGraphHubCap, self.hub_cap),
        ]
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
