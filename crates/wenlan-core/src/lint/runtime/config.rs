use crate::config::Config;
use sha2::{Digest, Sha256};
use wenlan_types::lint::{
    LintCommitReceipt, LintConfigSelection, LintConfigSetting, LintConfigValue,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeObservation {
    pub(super) provider_slots_available: Option<u64>,
    pub(super) reranker_paths_available: Option<u64>,
    pub(super) ingest_worker_closed: Option<bool>,
    pub(super) status_files_indexed: Option<u64>,
    pub(super) working_memory_entries: Option<u64>,
}

impl RuntimeObservation {
    pub const fn unavailable() -> Self {
        Self {
            provider_slots_available: None,
            reranker_paths_available: None,
            ingest_worker_closed: None,
            status_files_indexed: None,
            working_memory_entries: None,
        }
    }

    pub const fn with_provider_slots_available(mut self, value: u64) -> Self {
        self.provider_slots_available = Some(value);
        self
    }

    pub const fn with_reranker_paths_available(mut self, value: u64) -> Self {
        self.reranker_paths_available = Some(value);
        self
    }

    pub const fn with_ingest_worker_closed(mut self, value: bool) -> Self {
        self.ingest_worker_closed = Some(value);
        self
    }

    pub const fn with_status_files_indexed(mut self, value: u64) -> Self {
        self.status_files_indexed = Some(value);
        self
    }

    pub const fn with_working_memory_entries(mut self, value: u64) -> Self {
        self.working_memory_entries = Some(value);
        self
    }

    pub const fn ingest_worker_closed(self) -> Option<bool> {
        self.ingest_worker_closed
    }

    #[cfg(test)]
    pub(crate) const fn open(status_files_indexed: u64) -> Self {
        Self::unavailable()
            .with_provider_slots_available(0)
            .with_reranker_paths_available(0)
            .with_ingest_worker_closed(false)
            .with_status_files_indexed(status_files_indexed)
    }

    #[cfg(test)]
    pub(crate) const fn closed(status_files_indexed: u64) -> Self {
        Self::open(status_files_indexed).with_ingest_worker_closed(true)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeConfigSnapshot {
    pub(super) provider_slots_requested: u64,
    pub(super) model_slots_configured: u64,
    pub(super) reranker_paths_requested: u64,
    model_identity: [u8; 32],
}

impl RuntimeConfigSnapshot {
    #[cfg(test)]
    pub(crate) fn new(
        provider_slots_requested: u64,
        model_slots_configured: u64,
        reranker_paths_requested: u64,
    ) -> Self {
        let mut digest = Sha256::new();
        digest.update(provider_slots_requested.to_le_bytes());
        digest.update(model_slots_configured.to_le_bytes());
        digest.update(reranker_paths_requested.to_le_bytes());
        Self {
            provider_slots_requested,
            model_slots_configured,
            reranker_paths_requested,
            model_identity: digest.finalize().into(),
        }
    }

    #[cfg(test)]
    pub(crate) fn disabled() -> Self {
        Self::new(0, 0, 0)
    }

    fn capture(config: &Config) -> Self {
        let api = u64::from(
            config
                .anthropic_api_key
                .as_deref()
                .is_some_and(|key| !key.is_empty()),
        );
        let external = u64::from(
            config
                .external_llm_endpoint
                .as_deref()
                .is_some_and(|value| !value.is_empty())
                && config
                    .external_llm_model
                    .as_deref()
                    .is_some_and(|value| !value.is_empty()),
        );
        let on_device = u64::from(
            config
                .on_device_model
                .as_deref()
                .is_some_and(|value| !value.is_empty()),
        );
        let mode = crate::reranker::reranker_mode_resolved(config);
        let legacy = std::env::var("WENLAN_RERANKER_ENABLED").as_deref() == Ok("1");
        let plan = crate::reranker::resolve_reranker_plan(mode, legacy);
        let reranker_paths_requested =
            u64::from(plan.light.is_some()) + u64::from(plan.deep.is_some());
        let model_slots_configured = api
            .saturating_mul(2)
            .saturating_add(external)
            .saturating_add(on_device);
        let mut digest = Sha256::new();
        for value in [
            config.routine_model.as_deref(),
            config.synthesis_model.as_deref(),
            config.on_device_model.as_deref(),
            config.external_llm_model.as_deref(),
            config.reranker_mode.as_deref(),
        ] {
            let value = value.unwrap_or_default().as_bytes();
            digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
            digest.update(value);
        }
        Self {
            provider_slots_requested: api
                .saturating_mul(2)
                .saturating_add(external)
                .saturating_add(on_device),
            model_slots_configured,
            reranker_paths_requested,
            model_identity: digest.finalize().into(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeRunConfig {
    pub(super) snapshot: RuntimeConfigSnapshot,
    pub(super) observation: RuntimeObservation,
    runtime_commit: Option<LintCommitReceipt>,
    #[cfg(test)]
    pub(super) force_query_failure: bool,
}

impl RuntimeRunConfig {
    pub(crate) fn capture() -> Self {
        Self {
            snapshot: RuntimeConfigSnapshot::capture(&crate::config::load_config()),
            observation: RuntimeObservation::unavailable(),
            runtime_commit: option_env!("WENLAN_GIT_SHA")
                .and_then(|value| LintCommitReceipt::new(value).ok()),
            #[cfg(test)]
            force_query_failure: false,
        }
    }

    pub(crate) fn with_observation(mut self, observation: RuntimeObservation) -> Self {
        self.observation = observation;
        self
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        snapshot: RuntimeConfigSnapshot,
        observation: RuntimeObservation,
        commit: Option<&str>,
    ) -> Self {
        Self {
            snapshot,
            observation,
            runtime_commit: commit.and_then(|value| LintCommitReceipt::new(value).ok()),
            force_query_failure: false,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_query_failure(mut self) -> Self {
        self.force_query_failure = true;
        self
    }

    pub(crate) fn fingerprint_selections(&self) -> [LintConfigSelection; 5] {
        [
            LintConfigSelection::count(
                LintConfigSetting::ProviderSlotsRequested,
                self.snapshot.provider_slots_requested,
            ),
            LintConfigSelection::count(
                LintConfigSetting::ModelSlotsConfigured,
                self.snapshot.model_slots_configured,
            ),
            LintConfigSelection::count(
                LintConfigSetting::RerankerPathsRequested,
                self.snapshot.reranker_paths_requested,
            ),
            LintConfigSelection::digest(
                LintConfigSetting::ModelConfigurationIdentity,
                self.snapshot.model_identity,
            ),
            LintConfigSelection::new(
                LintConfigSetting::RuntimeObservationCaptured,
                if self.observation.ingest_worker_closed.is_some() {
                    LintConfigValue::Enabled
                } else {
                    LintConfigValue::Disabled
                },
            ),
        ]
    }

    pub(crate) fn runtime_commit(&self) -> Option<LintCommitReceipt> {
        self.runtime_commit.clone()
    }
}
