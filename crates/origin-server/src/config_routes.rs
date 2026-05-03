// SPDX-License-Identifier: Apache-2.0
//! Config endpoints — read/write the daemon's persistent Config.

use crate::error::ServerError;
use crate::state::SharedState;
use axum::extract::State;
use axum::response::Json;
use origin_core::config;
use origin_core::on_device_models::{self, OnDeviceModel};
use origin_types::requests::UpdateConfigRequest;
use origin_types::responses::ConfigResponse;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

fn config_to_response(cfg: &config::Config) -> ConfigResponse {
    ConfigResponse {
        skip_apps: cfg.skip_apps.clone(),
        skip_title_patterns: cfg.skip_title_patterns.clone(),
        private_browsing_detection: cfg.private_browsing_detection,
        setup_completed: cfg.setup_completed,
        clipboard_enabled: cfg.clipboard_enabled,
        screen_capture_enabled: cfg.screen_capture_enabled,
        selection_capture_enabled: cfg.selection_capture_enabled,
        remote_access_enabled: cfg.remote_access_enabled,
    }
}

/// GET /api/config — return current config.
pub async fn handle_get_config() -> Result<Json<ConfigResponse>, ServerError> {
    let cfg = config::load_config();
    Ok(Json(config_to_response(&cfg)))
}

/// PUT /api/config — update config fields (partial update).
pub async fn handle_update_config(
    Json(req): Json<UpdateConfigRequest>,
) -> Result<Json<ConfigResponse>, ServerError> {
    let mut cfg = config::load_config();
    if let Some(v) = req.skip_apps {
        cfg.skip_apps = v;
    }
    if let Some(v) = req.skip_title_patterns {
        cfg.skip_title_patterns = v;
    }
    if let Some(v) = req.private_browsing_detection {
        cfg.private_browsing_detection = v;
    }
    if let Some(v) = req.setup_completed {
        cfg.setup_completed = v;
    }
    if let Some(v) = req.clipboard_enabled {
        cfg.clipboard_enabled = v;
    }
    if let Some(v) = req.screen_capture_enabled {
        cfg.screen_capture_enabled = v;
    }
    if let Some(v) = req.selection_capture_enabled {
        cfg.selection_capture_enabled = v;
    }
    if let Some(v) = req.remote_access_enabled {
        cfg.remote_access_enabled = v;
    }
    config::save_config(&cfg).map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(config_to_response(&cfg)))
}

/// GET /api/config/skip-apps — return skip-apps list.
pub async fn handle_get_skip_apps() -> Result<Json<Vec<String>>, ServerError> {
    let cfg = config::load_config();
    Ok(Json(cfg.skip_apps))
}

#[derive(serde::Deserialize)]
pub struct SkipAppsRequest {
    pub apps: Vec<String>,
}

/// PUT /api/config/skip-apps — update skip-apps list.
pub async fn handle_update_skip_apps(
    Json(req): Json<SkipAppsRequest>,
) -> Result<Json<SuccessResponse>, ServerError> {
    let mut cfg = config::load_config();
    cfg.skip_apps = req.apps;
    config::save_config(&cfg).map_err(|e| ServerError::Internal(e.to_string()))?;
    Ok(Json(SuccessResponse { ok: true }))
}

#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub ok: bool,
}

// ── Setup/status endpoints ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SetupStatusResponse {
    pub setup_completed: bool,
    pub mode: String,
    pub anthropic_key_configured: bool,
    pub local_model_selected: Option<String>,
    pub local_model_loaded: Option<String>,
    pub local_model_cached: bool,
}

#[derive(Debug, Deserialize)]
pub struct AnthropicKeyRequest {
    pub api_key: String,
}

fn has_anthropic_key(cfg: &config::Config) -> bool {
    cfg.anthropic_api_key
        .as_deref()
        .map(|key| !key.trim().is_empty())
        .unwrap_or(false)
}

fn apply_anthropic_provider(state: &mut crate::state::ServerState, cfg: &config::Config) {
    if let Some(ref key) = cfg.anthropic_api_key {
        if !key.trim().is_empty() {
            let routine_model = cfg
                .routine_model
                .clone()
                .unwrap_or_else(|| origin_core::llm_provider::DEFAULT_ROUTINE_MODEL.to_string());
            let synthesis_model = cfg
                .synthesis_model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            state.api_llm = Some(Arc::new(origin_core::llm_provider::ApiProvider::new(
                key.clone(),
                routine_model,
            )));
            state.synthesis_llm = Some(Arc::new(origin_core::llm_provider::ApiProvider::new(
                key.clone(),
                synthesis_model,
            )));
            return;
        }
    }
    state.api_llm = None;
    state.synthesis_llm = None;
}

/// GET /api/setup/status — return setup + model/key status for every client.
pub async fn handle_get_setup_status(
    State(state): State<SharedState>,
) -> Result<Json<SetupStatusResponse>, ServerError> {
    let cfg = config::load_config();
    let selected_model = cfg
        .on_device_model
        .as_deref()
        .map(|id| on_device_models::resolve_or_default(Some(id)));
    let local_model_cached = selected_model
        .map(on_device_models::is_cached)
        .unwrap_or(false);
    let local_model_loaded = {
        let s = state.read().await;
        s.loaded_on_device_model.clone()
    };
    let anthropic_key_configured = has_anthropic_key(&cfg);
    let mode = if anthropic_key_configured {
        "anthropic-key"
    } else if selected_model.is_some() {
        "local-model"
    } else {
        "basic-memory"
    };

    Ok(Json(SetupStatusResponse {
        setup_completed: cfg.setup_completed,
        mode: mode.to_string(),
        anthropic_key_configured,
        local_model_selected: selected_model.map(|model| model.id.to_string()),
        local_model_loaded,
        local_model_cached,
    }))
}

/// PUT /api/setup/anthropic-key — save and hot-load the Anthropic provider.
pub async fn handle_set_anthropic_key(
    State(state): State<SharedState>,
    Json(req): Json<AnthropicKeyRequest>,
) -> Result<Json<SuccessResponse>, ServerError> {
    let key = req.api_key.trim().to_string();
    if key.is_empty() {
        return Err(ServerError::ValidationError(
            "api_key cannot be empty".into(),
        ));
    }
    let mut cfg = config::load_config();
    cfg.anthropic_api_key = Some(key);
    config::save_config(&cfg).map_err(|e| ServerError::Internal(e.to_string()))?;
    {
        let mut s = state.write().await;
        apply_anthropic_provider(&mut s, &cfg);
    }
    Ok(Json(SuccessResponse { ok: true }))
}

/// DELETE /api/setup/anthropic-key — clear the Anthropic provider.
pub async fn handle_clear_anthropic_key(
    State(state): State<SharedState>,
) -> Result<Json<SuccessResponse>, ServerError> {
    let mut cfg = config::load_config();
    cfg.anthropic_api_key = None;
    config::save_config(&cfg).map_err(|e| ServerError::Internal(e.to_string()))?;
    {
        let mut s = state.write().await;
        apply_anthropic_provider(&mut s, &cfg);
    }
    Ok(Json(SuccessResponse { ok: true }))
}

// ── On-device model endpoints ──────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct OnDeviceModelEntry {
    pub id: String,
    pub display_name: String,
    pub param_count: String,
    pub ram_required_gb: f64,
    pub file_size_gb: f64,
    pub cached: bool,
}

#[derive(Debug, Serialize)]
pub struct OnDeviceModelResponse {
    /// ID of the model currently loaded in the daemon (if any).
    pub loaded: Option<String>,
    /// ID the user has selected in config (may differ from loaded if a
    /// download is pending or a restart is needed).
    pub selected: Option<String>,
    /// All available models with per-model cache/download state.
    pub models: Vec<OnDeviceModelEntry>,
}

fn model_entry(model: &OnDeviceModel) -> OnDeviceModelEntry {
    OnDeviceModelEntry {
        id: model.id.to_string(),
        display_name: model.display_name.to_string(),
        param_count: model.param_count.to_string(),
        ram_required_gb: model.ram_required_gb,
        file_size_gb: model.file_size_gb,
        cached: on_device_models::is_cached(model),
    }
}

/// GET /api/on-device-model — returns the list of models with cache/load state.
pub async fn handle_get_on_device_model(
    State(state): State<SharedState>,
) -> Result<Json<OnDeviceModelResponse>, ServerError> {
    let cfg = config::load_config();
    let loaded = {
        let s = state.read().await;
        s.loaded_on_device_model.clone()
    };
    let models: Vec<OnDeviceModelEntry> =
        on_device_models::MODELS.iter().map(model_entry).collect();
    // Resolve selected against registry so stale config values map to the default,
    // but keep Basic Memory distinct from "default local model selected".
    let selected = cfg
        .on_device_model
        .as_deref()
        .map(|id| on_device_models::resolve_or_default(Some(id)))
        .map(|model| model.id.to_string());
    Ok(Json(OnDeviceModelResponse {
        loaded,
        selected,
        models,
    }))
}

#[derive(Debug, Deserialize)]
pub struct OnDeviceModelRequest {
    pub model_id: String,
}

/// POST /api/on-device-model/download — download (if needed) and hot-load a model.
///
/// This is a long-running endpoint: the HTTP request stays open until the
/// download + engine init completes. For a 2.7GB model on a fresh laptop this
/// can take minutes. The client should set a generous timeout.
pub async fn handle_download_on_device_model(
    State(state): State<SharedState>,
    Json(req): Json<OnDeviceModelRequest>,
) -> Result<Json<SuccessResponse>, ServerError> {
    // Validate the id against the registry.
    let Some(model) = on_device_models::get_model(&req.model_id) else {
        return Err(ServerError::ValidationError(format!(
            "unknown on-device model id: {}",
            req.model_id
        )));
    };
    let model_id = model.id.to_string();

    // Run the blocking download + engine init on a dedicated thread so the
    // async runtime stays responsive.
    let provider: Arc<dyn origin_core::llm_provider::LlmProvider> =
        tokio::task::spawn_blocking(move || {
            let provider =
                origin_core::llm_provider::OnDeviceProvider::new_with_model(Some(&model_id))?;
            Ok::<_, origin_core::error::OriginError>(
                Arc::new(provider) as Arc<dyn origin_core::llm_provider::LlmProvider>
            )
        })
        .await
        .map_err(|e| ServerError::Internal(format!("download task panicked: {}", e)))?
        .map_err(|e| ServerError::Internal(format!("download failed: {}", e)))?;

    // Persist the selection.
    let mut cfg = config::load_config();
    cfg.on_device_model = Some(req.model_id.clone());
    config::save_config(&cfg).map_err(|e| ServerError::Internal(e.to_string()))?;

    // Hot-swap the provider in ServerState. The old provider (if any) is
    // dropped here; its worker thread exits when the channel closes.
    {
        let mut s = state.write().await;
        s.llm = Some(provider);
        s.loaded_on_device_model = Some(req.model_id.clone());
    }

    tracing::info!("[on-device] model {} downloaded and loaded", req.model_id);
    Ok(Json(SuccessResponse { ok: true }))
}
