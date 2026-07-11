use super::from_server_state;
use crate::ingest_batcher::{BatchProcessFn, BatcherConfig, IngestBatcher};
use crate::state::ServerState;
use std::sync::Arc;

#[tokio::test]
async fn server_state_observes_worker_without_submitting() {
    let mut state = ServerState::new();
    assert_eq!(from_server_state(&state).ingest_worker_closed(), None);

    let process: BatchProcessFn = Arc::new(|_| Box::pin(async { Vec::new() }));
    state.ingest_batcher = Some(IngestBatcher::spawn(process, BatcherConfig::default()));
    assert_eq!(
        from_server_state(&state).ingest_worker_closed(),
        Some(false)
    );
}
