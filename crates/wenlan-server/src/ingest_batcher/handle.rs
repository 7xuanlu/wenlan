use super::{CoalescedRequest, IngestBatcher, RawDocument, StoreOutcome};
use tokio::sync::oneshot;

impl IngestBatcher {
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }

    /// Submit a document for coalesced processing. Returns the per-doc
    /// outcome — [`StoreOutcome::Stored`] on success, `GateRejected` /
    /// `UpsertFailed` for the specific failure modes. `Err(String)` only
    /// when the batcher itself is unreachable (worker panicked, channel
    /// closed).
    pub async fn submit(
        &self,
        doc: RawDocument,
        chunks_predicted: usize,
    ) -> Result<StoreOutcome, String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        if self
            .tx
            .send(CoalescedRequest {
                doc,
                chunks_predicted,
                response: resp_tx,
            })
            .await
            .is_err()
        {
            return Err("ingest batcher is shut down".to_string());
        }
        resp_rx
            .await
            .map_err(|_| "ingest batcher dropped response channel".to_string())
    }
}
