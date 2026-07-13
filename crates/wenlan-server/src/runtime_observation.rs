use crate::state::ServerState;
use std::sync::Arc;
use wenlan_core::lint::runtime::{
    ProviderClass, RerankerPath, RuntimeObservation, RuntimeReadiness, StatusFilesObservation,
};
use wenlan_core::llm_provider::LlmProvider;
use wenlan_types::responses::RerankerStatus;

pub async fn from_server_state(state: &ServerState) -> RuntimeObservation {
    RuntimeObservationInput::capture(state).observe().await
}

pub(crate) struct RuntimeObservationInput {
    api_llm: Option<Arc<dyn LlmProvider>>,
    synthesis_llm: Option<Arc<dyn LlmProvider>>,
    external_llm: Option<Arc<dyn LlmProvider>>,
    llm: Option<Arc<dyn LlmProvider>>,
    loaded_on_device_model: Option<String>,
    reranker: Option<Arc<dyn wenlan_core::reranker::Reranker>>,
    reranker_status: RerankerStatus,
    reranker_light: Option<Arc<dyn wenlan_core::reranker::Reranker>>,
    reranker_light_status: RerankerStatus,
    ingest_worker_closed: Option<bool>,
    db: Option<Arc<wenlan_core::db::MemoryDB>>,
}

impl RuntimeObservationInput {
    pub(crate) fn capture(state: &ServerState) -> Self {
        Self {
            api_llm: state.api_llm.clone(),
            synthesis_llm: state.synthesis_llm.clone(),
            external_llm: state.external_llm.clone(),
            llm: state.llm.clone(),
            loaded_on_device_model: state.loaded_on_device_model.clone(),
            reranker: state.reranker.clone(),
            reranker_status: state.reranker_status.clone(),
            reranker_light: state.reranker_light.clone(),
            reranker_light_status: state.reranker_light_status.clone(),
            ingest_worker_closed: state
                .ingest_batcher
                .as_ref()
                .map(|batcher| batcher.is_closed()),
            db: state.db.clone(),
        }
    }

    pub(crate) async fn observe(self) -> RuntimeObservation {
        let mut observation = RuntimeObservation::unavailable();
        observation = observe_provider(
            observation,
            ProviderClass::AnthropicRoutine,
            self.api_llm.as_ref(),
        );
        observation = observe_provider(
            observation,
            ProviderClass::AnthropicSynthesis,
            self.synthesis_llm.as_ref(),
        );
        observation = observe_provider(
            observation,
            ProviderClass::External,
            self.external_llm.as_ref(),
        );
        if let Some(provider) = &self.llm {
            let model_id = self
                .loaded_on_device_model
                .clone()
                .unwrap_or_else(|| provider.model_id());
            let readiness = if provider.is_available() && self.loaded_on_device_model.is_some() {
                RuntimeReadiness::Ready
            } else {
                RuntimeReadiness::Failed
            };
            observation = observation.with_provider(ProviderClass::OnDevice, model_id, readiness);
        }
        observation = observe_reranker(
            observation,
            RerankerRuntime {
                path: RerankerPath::Light,
                status: &self.reranker_light_status,
                runtime: self.reranker_light.as_ref(),
            },
        );
        observation = observe_reranker(
            observation,
            RerankerRuntime {
                path: RerankerPath::Deep,
                status: &self.reranker_status,
                runtime: self.reranker.as_ref(),
            },
        );
        if let Some(closed) = self.ingest_worker_closed {
            observation = observation.with_ingest_worker_closed(closed);
        }
        let status = match &self.db {
            Some(db) => match db.count_direct().await {
                Ok(files_indexed) => StatusFilesObservation::Direct(files_indexed),
                Err(_) => StatusFilesObservation::DirectError {
                    fallback_files_indexed: 0,
                },
            },
            None => StatusFilesObservation::Unavailable,
        };
        observation.with_status_files(status)
    }
}

fn observe_provider(
    observation: RuntimeObservation,
    class: ProviderClass,
    provider: Option<&Arc<dyn LlmProvider>>,
) -> RuntimeObservation {
    match provider {
        Some(provider) => observation.with_provider(
            class,
            provider.model_id(),
            if provider.is_available() {
                RuntimeReadiness::Ready
            } else {
                RuntimeReadiness::Failed
            },
        ),
        None => observation,
    }
}

struct RerankerRuntime<'a> {
    path: RerankerPath,
    status: &'a RerankerStatus,
    runtime: Option<&'a Arc<dyn wenlan_core::reranker::Reranker>>,
}

fn observe_reranker(
    observation: RuntimeObservation,
    state: RerankerRuntime<'_>,
) -> RuntimeObservation {
    match state.status {
        RerankerStatus::Active { model_id } => observation.with_reranker(
            state.path,
            model_id,
            if state
                .runtime
                .is_some_and(|reranker| reranker.model_id() == model_id)
            {
                RuntimeReadiness::Ready
            } else {
                RuntimeReadiness::Failed
            },
        ),
        RerankerStatus::Failed { .. } => {
            observation.with_reranker(state.path, "", RuntimeReadiness::Failed)
        }
        RerankerStatus::Disabled => {
            observation.with_reranker(state.path, "", RuntimeReadiness::Unavailable)
        }
    }
}

#[cfg(test)]
#[path = "runtime_observation_test.rs"]
mod tests;
