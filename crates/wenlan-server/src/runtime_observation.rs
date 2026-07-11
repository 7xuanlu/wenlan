use crate::state::ServerState;
use std::sync::Arc;
use wenlan_core::lint::runtime::{
    ProviderClass, RerankerPath, RuntimeObservation, RuntimeReadiness, StatusFilesObservation,
};
use wenlan_core::llm_provider::LlmProvider;
use wenlan_types::responses::RerankerStatus;

pub async fn from_server_state(state: &ServerState) -> RuntimeObservation {
    let mut observation = RuntimeObservation::unavailable();
    observation = observe_provider(
        observation,
        ProviderClass::AnthropicRoutine,
        state.api_llm.as_ref(),
    );
    observation = observe_provider(
        observation,
        ProviderClass::AnthropicSynthesis,
        state.synthesis_llm.as_ref(),
    );
    observation = observe_provider(
        observation,
        ProviderClass::External,
        state.external_llm.as_ref(),
    );
    if let Some(provider) = &state.llm {
        let model_id = state
            .loaded_on_device_model
            .clone()
            .unwrap_or_else(|| provider.model_id());
        let readiness = if provider.is_available() && state.loaded_on_device_model.is_some() {
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
            status: &state.reranker_light_status,
            runtime: state.reranker_light.as_ref(),
        },
    );
    observation = observe_reranker(
        observation,
        RerankerRuntime {
            path: RerankerPath::Deep,
            status: &state.reranker_status,
            runtime: state.reranker.as_ref(),
        },
    );
    if let Some(batcher) = &state.ingest_batcher {
        observation = observation.with_ingest_worker_closed(batcher.is_closed());
    }
    let status = match &state.db {
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
