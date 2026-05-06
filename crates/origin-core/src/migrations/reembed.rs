// SPDX-License-Identifier: Apache-2.0
//! Re-embedding migration phase. Re-runs the embedding pipeline on chunks
//! whose stored embedding metadata indicates a stale model/version.

use crate::db::MemoryDB;
use crate::error::OriginError;

/// Run one batch of re-embedding. Pulls up to `batch_size` candidates from
/// the DB, re-embeds them, and persists the new vectors. Returns the count
/// of memories that were re-embedded successfully.
pub async fn run(db: &MemoryDB, batch_size: usize) -> Result<usize, OriginError> {
    let candidates = db.get_reembed_candidates(batch_size).await?;
    let mut reembedded = 0usize;
    for (chunk_id, content) in &candidates {
        if let Err(e) = db.reembed_memory(chunk_id, content).await {
            log::warn!("[reembed] failed for {}: {}", chunk_id, e);
        } else {
            reembedded += 1;
        }
    }
    Ok(reembedded)
}
