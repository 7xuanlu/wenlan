use super::{ProviderClass, RerankerPath, RuntimeObservation};
use sha2::{Digest, Sha256};
use wenlan_types::lint::{
    LintCommitReceipt, LintConfigSelection, LintConfigSetting, LintConfigValue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProviderRequest {
    pub(super) class: ProviderClass,
    pub(super) model_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RerankerRequest {
    pub(super) path: RerankerPath,
    pub(super) model_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeConfigSnapshot {
    pub(super) providers: Vec<ProviderRequest>,
    pub(super) rerankers: Vec<RerankerRequest>,
    model_identity: [u8; 32],
}

impl RuntimeConfigSnapshot {
    #[cfg(test)]
    pub(crate) fn disabled() -> Self {
        Self::from_requests(Vec::new(), Vec::new())
    }

    #[cfg(test)]
    pub(crate) fn with_provider_request(
        mut self,
        class: ProviderClass,
        model_id: impl Into<String>,
    ) -> Self {
        self.providers.push(ProviderRequest {
            class,
            model_id: model_id.into(),
        });
        self.refresh_identity();
        self
    }

    #[cfg(test)]
    pub(crate) fn with_reranker_request(
        mut self,
        path: RerankerPath,
        model_id: impl Into<String>,
    ) -> Self {
        self.rerankers.push(RerankerRequest {
            path,
            model_id: model_id.into(),
        });
        self.refresh_identity();
        self
    }

    pub(super) fn from_requests(
        providers: Vec<ProviderRequest>,
        rerankers: Vec<RerankerRequest>,
    ) -> Self {
        let mut snapshot = Self {
            providers,
            rerankers,
            model_identity: [0; 32],
        };
        snapshot.refresh_identity();
        snapshot
    }

    fn refresh_identity(&mut self) {
        let mut digest = Sha256::new();
        for request in &self.providers {
            digest.update([request.class as u8]);
            digest_value(&mut digest, &request.model_id);
        }
        for request in &self.rerankers {
            digest.update([request.path as u8]);
            digest_value(&mut digest, &request.model_id);
        }
        self.model_identity = digest.finalize().into();
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RuntimeRunConfig {
    pub(super) snapshot: RuntimeConfigSnapshot,
    pub(super) observation: RuntimeObservation,
    pub(super) clock_epoch_seconds: i64,
    runtime_commit: Option<LintCommitReceipt>,
    #[cfg(test)]
    pub(super) force_query_failure: bool,
}

impl RuntimeRunConfig {
    pub(crate) fn capture() -> Self {
        Self {
            snapshot: super::config_capture::capture(&crate::config::load_config()),
            observation: RuntimeObservation::unavailable(),
            clock_epoch_seconds: 0,
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
            clock_epoch_seconds: 0,
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
        let providers = u64::try_from(self.snapshot.providers.len()).unwrap_or(u64::MAX);
        let rerankers = u64::try_from(self.snapshot.rerankers.len()).unwrap_or(u64::MAX);
        [
            LintConfigSelection::count(LintConfigSetting::ProviderSlotsRequested, providers),
            LintConfigSelection::count(LintConfigSetting::ModelSlotsConfigured, providers),
            LintConfigSelection::count(LintConfigSetting::RerankerPathsRequested, rerankers),
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

    pub(crate) const fn with_clock_epoch_seconds(mut self, value: i64) -> Self {
        self.clock_epoch_seconds = value;
        self
    }
}

fn digest_value(digest: &mut Sha256, value: &str) {
    digest.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_le_bytes());
    digest.update(value.as_bytes());
}
