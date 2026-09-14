use super::{from_server_state, RuntimeObservationInput};
use crate::ingest_batcher::{BatchProcessFn, BatcherConfig, IngestBatcher};
use crate::state::ServerState;
use async_trait::async_trait;
use std::sync::Arc;
use wenlan_core::lint::runtime::{
    ProviderClass, RerankerPath, RuntimeReadiness, StatusFilesObservation,
};
use wenlan_core::llm_provider::{LlmBackend, LlmError, LlmProvider, LlmRequest};
use wenlan_types::responses::RerankerStatus;

struct TestOnDeviceProvider {
    available: bool,
    model_id: &'static str,
}

#[async_trait]
impl LlmProvider for TestOnDeviceProvider {
    async fn generate(&self, _request: LlmRequest) -> Result<String, LlmError> {
        Ok(String::new())
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn name(&self) -> &str {
        "test-on-device"
    }

    fn backend(&self) -> LlmBackend {
        LlmBackend::OnDevice
    }

    fn kind(&self) -> &'static str {
        "on-device"
    }

    fn model_id(&self) -> String {
        self.model_id.to_string()
    }
}

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

#[tokio::test]
async fn server_state_marks_repair_suspended_optional_workers() {
    let mut state = ServerState::new();
    assert!(!from_server_state(&state).await.optional_workers_suspended());

    state.optional_runtime_workers_suspended = true;

    assert!(from_server_state(&state).await.optional_workers_suspended());
}

#[tokio::test]
async fn server_marks_only_the_loaded_canonical_ready_ondevice_model() {
    let provider: Arc<dyn LlmProvider> = Arc::new(TestOnDeviceProvider {
        available: true,
        model_id: "model-a",
    });
    let observation =
        RuntimeObservationInput::for_test(Some(provider), Some("model-a"), Some("model-a"), true)
            .observe()
            .await;

    assert_eq!(
        observation.provider_readiness(ProviderClass::OnDevice, "model-a"),
        Some(RuntimeReadiness::Ready)
    );
    assert_eq!(observation.repair_verification_model(), Some("model-a"));
}

#[tokio::test]
async fn server_does_not_mark_wrong_failed_or_unsuspended_ondevice_models() {
    let cases = [
        (true, Some("model-b"), Some("model-a"), true, None),
        (false, Some("model-a"), Some("model-a"), true, None),
        (true, Some("model-a"), Some("model-a"), false, None),
    ];
    for (available, loaded, configured, suspended, expected_marker) in cases {
        let provider: Arc<dyn LlmProvider> = Arc::new(TestOnDeviceProvider {
            available,
            model_id: "model-a",
        });
        let observation =
            RuntimeObservationInput::for_test(Some(provider), loaded, configured, suspended)
                .observe()
                .await;
        assert_eq!(observation.repair_verification_model(), expected_marker);
    }
}
