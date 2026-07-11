use super::from_server_state;
use crate::ingest_batcher::{BatchProcessFn, BatcherConfig, IngestBatcher};
use crate::state::ServerState;
use std::sync::Arc;
use wenlan_core::lint::runtime::{
    ProviderClass, RerankerPath, RuntimeReadiness, StatusFilesObservation,
};
use wenlan_types::responses::RerankerStatus;

#[tokio::test]
async fn server_state_observes_worker_without_submitting() {
    let mut state = ServerState::new();
    assert_eq!(from_server_state(&state).await.ingest_worker_closed(), None);

    let process: BatchProcessFn = Arc::new(|_| Box::pin(async { Vec::new() }));
    state.ingest_batcher = Some(IngestBatcher::spawn(process, BatcherConfig::default()));
    assert_eq!(
        from_server_state(&state).await.ingest_worker_closed(),
        Some(false)
    );
}

#[tokio::test]
async fn server_state_preserves_typed_readiness_and_unavailable_status() {
    let mut state = ServerState::new();
    state.api_llm = Some(Arc::new(wenlan_core::llm_provider::ApiProvider::new(
        "test-key".to_string(),
        "routine-model".to_string(),
    )));
    state.reranker_light = Some(Arc::new(wenlan_core::reranker::NoopReranker));
    state.reranker_light_status = RerankerStatus::Active {
        model_id: "noop".to_string(),
    };

    let observation = from_server_state(&state).await;
    assert_eq!(
        observation.provider_readiness(ProviderClass::AnthropicRoutine, "routine-model"),
        Some(RuntimeReadiness::Ready)
    );
    assert_eq!(
        observation.reranker_readiness(RerankerPath::Light, "noop"),
        Some(RuntimeReadiness::Ready)
    );
    assert_eq!(
        observation.status_files(),
        StatusFilesObservation::Unavailable
    );
}
