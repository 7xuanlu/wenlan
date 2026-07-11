use crate::state::ServerState;

pub fn from_server_state(state: &ServerState) -> wenlan_core::lint::runtime::RuntimeObservation {
    let provider_slots = u64::from(state.llm.is_some())
        + u64::from(state.api_llm.is_some())
        + u64::from(state.synthesis_llm.is_some())
        + u64::from(state.external_llm.is_some());
    let reranker_paths =
        u64::from(state.reranker.is_some()) + u64::from(state.reranker_light.is_some());
    let observation = wenlan_core::lint::runtime::RuntimeObservation::unavailable()
        .with_provider_slots_available(provider_slots)
        .with_reranker_paths_available(reranker_paths);
    match &state.ingest_batcher {
        Some(batcher) => observation.with_ingest_worker_closed(batcher.is_closed()),
        None => observation,
    }
}

#[cfg(test)]
#[path = "runtime_observation_test.rs"]
mod tests;
