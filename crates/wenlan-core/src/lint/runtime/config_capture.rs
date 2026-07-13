use super::config::{ProviderRequest, RerankerRequest, RuntimeConfigSnapshot};
use super::{ProviderClass, RerankerPath};

pub(super) fn capture(config: &crate::config::Config) -> RuntimeConfigSnapshot {
    let mut providers = Vec::new();
    if config
        .anthropic_api_key
        .as_deref()
        .is_some_and(|key| !key.is_empty())
    {
        providers.push(ProviderRequest {
            class: ProviderClass::AnthropicRoutine,
            model_id: config
                .routine_model
                .clone()
                .unwrap_or_else(|| crate::llm_provider::DEFAULT_ROUTINE_MODEL.to_string()),
        });
        providers.push(ProviderRequest {
            class: ProviderClass::AnthropicSynthesis,
            model_id: config
                .synthesis_model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string()),
        });
    }
    if config
        .external_llm_endpoint
        .as_deref()
        .is_some_and(|endpoint| !endpoint.is_empty())
    {
        if let Some(model_id) = config
            .external_llm_model
            .as_deref()
            .filter(|model| !model.is_empty())
        {
            providers.push(ProviderRequest {
                class: ProviderClass::External,
                model_id: model_id.to_string(),
            });
        }
    }
    if let Some(model_id) = config
        .on_device_model
        .as_deref()
        .filter(|model| !model.is_empty())
    {
        providers.push(ProviderRequest {
            class: ProviderClass::OnDevice,
            model_id: crate::on_device_models::resolve_or_default(Some(model_id))
                .id
                .to_string(),
        });
    }
    let mode = crate::reranker::reranker_mode_resolved(config);
    let legacy = std::env::var("WENLAN_RERANKER_ENABLED").as_deref() == Ok("1");
    let plan = crate::reranker::resolve_reranker_plan(mode, legacy);
    let mut rerankers = Vec::new();
    if let Some(pick) = plan.light {
        rerankers.push(RerankerRequest {
            path: RerankerPath::Light,
            model_id: reranker_model_id(pick),
        });
    }
    if let Some(pick) = plan.deep {
        rerankers.push(RerankerRequest {
            path: RerankerPath::Deep,
            model_id: reranker_model_id(pick),
        });
    }
    RuntimeConfigSnapshot::from_requests(providers, rerankers)
}

fn reranker_model_id(pick: crate::reranker::RerankerPick) -> String {
    match pick {
        crate::reranker::RerankerPick::Turbo => "JINARerankerV1TurboEn".to_string(),
        crate::reranker::RerankerPick::BgeBase => "BGERerankerBase".to_string(),
        crate::reranker::RerankerPick::Configured => configured_reranker_model_id(),
    }
}

fn configured_reranker_model_id() -> String {
    if let Ok(directory) = std::env::var("WENLAN_RERANKER_ONNX_DIR") {
        if let Some(model_id) = std::env::var("WENLAN_RERANKER_MODEL_ID")
            .ok()
            .filter(|model_id| !model_id.is_empty())
        {
            return model_id;
        }
        return std::path::Path::new(&directory)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "user-defined".to_string());
    }
    match std::env::var("WENLAN_RERANKER_MODEL")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "turbo" | "jina-turbo" | "jina" => "JINARerankerV1TurboEn".to_string(),
        "bge-v2-m3" | "v2-m3" => "BGERerankerV2M3".to_string(),
        _ => "BGERerankerBase".to_string(),
    }
}
