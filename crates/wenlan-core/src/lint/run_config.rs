use super::kg::KgRunConfig;
use super::memories::MemoryFeatureConfig;
use super::operations::OperationsRunConfig;
use super::runtime::RuntimeRunConfig;
use super::serving::ServingRunConfig;
use wenlan_types::lint::{
    LintConfigFingerprint, LintConfigSelection, LintConfigSetting, LintConfigValue,
};

#[derive(Debug, Clone)]
pub(super) struct EffectiveLintConfig {
    pub(super) page_projection_enabled: bool,
    pub(super) memory: MemoryFeatureConfig,
    pub(super) kg: KgRunConfig,
    pub(super) operations: OperationsRunConfig,
    pub(super) serving: ServingRunConfig,
    pub(super) runtime: RuntimeRunConfig,
    semantic_provider_ready: bool,
    semantic_provider_on_device: bool,
    semantic_external_egress_enabled: bool,
    semantic_calling_agent_enabled: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SemanticProviderConfig {
    ready: bool,
    on_device: bool,
    external_egress_enabled: bool,
    calling_agent_enabled: bool,
}

impl SemanticProviderConfig {
    pub(super) const fn new(
        ready: bool,
        on_device: bool,
        external_egress_enabled: bool,
        calling_agent_enabled: bool,
    ) -> Self {
        Self {
            ready,
            on_device,
            external_egress_enabled,
            calling_agent_enabled,
        }
    }
}

impl EffectiveLintConfig {
    pub(super) const fn new(
        page_projection_enabled: bool,
        memory: MemoryFeatureConfig,
        kg: KgRunConfig,
        operations: OperationsRunConfig,
        serving: ServingRunConfig,
        runtime: RuntimeRunConfig,
        semantic_provider: SemanticProviderConfig,
    ) -> Self {
        Self {
            page_projection_enabled,
            memory,
            kg,
            operations,
            serving,
            runtime,
            semantic_provider_ready: semantic_provider.ready,
            semantic_provider_on_device: semantic_provider.on_device,
            semantic_external_egress_enabled: semantic_provider.external_egress_enabled,
            semantic_calling_agent_enabled: semantic_provider.calling_agent_enabled,
        }
    }

    pub(super) fn fingerprint(&self) -> LintConfigFingerprint {
        let mut selections = vec![
            selection(
                LintConfigSetting::PageProjectionEnabled,
                self.page_projection_enabled,
            ),
            selection(
                LintConfigSetting::PageRetrievalChannelEnabled,
                self.serving.page,
            ),
            LintConfigSelection::count(
                LintConfigSetting::FactChannelLimit,
                u64::try_from(self.serving.fact_limit).unwrap_or(u64::MAX),
            ),
            selection(LintConfigSetting::RerankerEnabled, self.serving.reranker),
            selection(
                LintConfigSetting::RerankerLightConfigured,
                self.serving.reranker_light,
            ),
            selection(
                LintConfigSetting::RerankerDeepConfigured,
                self.serving.reranker_deep,
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
            selection(
                LintConfigSetting::SemanticProviderReady,
                self.semantic_provider_ready,
            ),
            selection(
                LintConfigSetting::SemanticProviderOnDevice,
                self.semantic_provider_on_device,
            ),
            selection(
                LintConfigSetting::SemanticExternalEgressEnabled,
                self.semantic_external_egress_enabled,
            ),
            selection(
                LintConfigSetting::SemanticCallingAgentEnabled,
                self.semantic_calling_agent_enabled,
            ),
        ];
        selections.extend(self.kg.fingerprint_selections());
        selections.extend(self.operations.fingerprint_selections());
        selections.extend(self.runtime.fingerprint_selections());
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
