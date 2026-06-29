// SPDX-License-Identifier: AGPL-3.0-only
//! HTTP client for the Wenlan daemon (wenlan-server).
//!
//! Thin wrapper around `reqwest::Client` that maps each daemon endpoint
//! to a typed method. The Tauri app uses this instead of direct DB access.

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use wenlan_types::responses::HealthResponse;

/// HTTP client that proxies requests to the Wenlan daemon.
#[derive(Clone)]
pub struct WenlanClient {
    client: Client,
    base_url: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct SetupStatusResponse {
    pub setup_completed: bool,
    pub mode: String,
    pub anthropic_key_configured: bool,
    pub local_model_selected: Option<String>,
    pub local_model_loaded: Option<String>,
    pub local_model_cached: bool,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
struct CaptureStatsResponse {
    total_chunks: u64,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub(crate) struct TagInventoryResponse {
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) document_tags: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct OnDeviceModelEntry {
    pub id: String,
    pub display_name: String,
    pub param_count: String,
    pub ram_required_gb: f64,
    pub file_size_gb: f64,
    pub cached: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct OnDeviceModelResponse {
    pub loaded: Option<String>,
    pub selected: Option<String>,
    pub models: Vec<OnDeviceModelEntry>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct MoveSpaceResponse {
    pub affected: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct OnDeviceModelRequest {
    model_id: String,
}

impl Default for WenlanClient {
    fn default() -> Self {
        Self::new()
    }
}

impl WenlanClient {
    pub fn new() -> Self {
        let port = daemon_port();
        Self {
            client: Client::new(),
            base_url: format!("http://127.0.0.1:{}", port),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    // ── Generic helpers ─────────────────────────────────────────────

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        let resp = self
            .client
            .get(self.url(path))
            .send()
            .await
            .map_err(|e| format!("HTTP GET {}: {}", path, e))?;
        if !resp.status().is_success() {
            return Err(format!("HTTP GET {} returned {}", path, resp.status()));
        }
        resp.json()
            .await
            .map_err(|e| format!("Parse {}: {}", path, e))
    }

    pub async fn post_json<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp, String> {
        let resp = self
            .client
            .post(self.url(path))
            .json(body)
            .send()
            .await
            .map_err(|e| format!("HTTP POST {}: {}", path, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP POST {} returned {}: {}", path, status, text));
        }
        resp.json()
            .await
            .map_err(|e| format!("Parse {}: {}", path, e))
    }

    pub async fn put_json<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp, String> {
        let resp = self
            .client
            .put(self.url(path))
            .json(body)
            .send()
            .await
            .map_err(|e| format!("HTTP PUT {}: {}", path, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP PUT {} returned {}: {}", path, status, text));
        }
        resp.json()
            .await
            .map_err(|e| format!("Parse {}: {}", path, e))
    }

    pub async fn delete_path<Resp: DeserializeOwned>(&self, path: &str) -> Result<Resp, String> {
        let resp = self
            .client
            .delete(self.url(path))
            .send()
            .await
            .map_err(|e| format!("HTTP DELETE {}: {}", path, e))?;
        if !resp.status().is_success() {
            return Err(format!("HTTP DELETE {} returned {}", path, resp.status()));
        }
        resp.json()
            .await
            .map_err(|e| format!("Parse {}: {}", path, e))
    }

    async fn delete_empty(&self, path: &str) -> Result<(), String> {
        let resp = self
            .client
            .delete(self.url(path))
            .send()
            .await
            .map_err(|e| format!("HTTP DELETE {}: {}", path, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "HTTP DELETE {} returned {}: {}",
                path, status, text
            ));
        }
        Ok(())
    }

    pub async fn delete_json<Req: Serialize, Resp: DeserializeOwned>(
        &self,
        path: &str,
        body: &Req,
    ) -> Result<Resp, String> {
        let resp = self
            .client
            .delete(self.url(path))
            .json(body)
            .send()
            .await
            .map_err(|e| format!("HTTP DELETE {}: {}", path, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "HTTP DELETE {} returned {}: {}",
                path, status, text
            ));
        }
        resp.json()
            .await
            .map_err(|e| format!("Parse {}: {}", path, e))
    }

    pub async fn post_empty<Resp: DeserializeOwned>(&self, path: &str) -> Result<Resp, String> {
        let resp = self
            .client
            .post(self.url(path))
            .send()
            .await
            .map_err(|e| format!("HTTP POST {}: {}", path, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP POST {} returned {}: {}", path, status, text));
        }
        resp.json()
            .await
            .map_err(|e| format!("Parse {}: {}", path, e))
    }

    // ── Health ──────────────────────────────────────────────────────

    pub async fn health(&self) -> Result<HealthResponse, String> {
        self.get_json("/api/health").await
    }

    pub async fn status(&self) -> Result<wenlan_types::responses::StatusResponse, String> {
        self.get_json("/api/status").await
    }

    // ── Capture stats ────────────────────────────────────────────────

    pub async fn get_capture_stats(&self) -> Result<HashMap<String, u64>, String> {
        let stats: CaptureStatsResponse = self.get_json("/api/capture-stats").await?;
        let mut counts = HashMap::new();
        counts.insert("total".to_string(), stats.total_chunks);
        Ok(counts)
    }

    pub async fn list_tags(&self) -> Result<Vec<String>, String> {
        Ok(self.list_tag_inventory().await?.tags)
    }

    pub(crate) async fn list_tag_inventory(&self) -> Result<TagInventoryResponse, String> {
        self.get_json("/api/tags").await
    }

    // ── Ingest ───────────────────────────────────────────────────────

    pub async fn ingest_webpage(
        &self,
        req: wenlan_types::requests::IngestWebpageRequest,
    ) -> Result<wenlan_types::responses::IngestResponse, String> {
        self.post_json("/api/ingest/webpage", &req).await
    }

    pub async fn move_space(&self, from: &str, to: &str) -> Result<MoveSpaceResponse, String> {
        let path = format!(
            "/api/spaces/{}/move-to/{}",
            percent_encode_path_segment(from),
            percent_encode_path_segment(to)
        );
        self.post_empty(&path).await
    }

    // ── Chat export import ─────────────────────────────────────────

    pub async fn import_chat_export(
        &self,
        path: &str,
    ) -> Result<wenlan_types::import::ImportChatExportResponse, String> {
        let req = wenlan_types::import::ImportChatExportRequest {
            path: path.to_string(),
        };
        self.post_json("/api/import/chat-export", &req).await
    }

    pub async fn list_pending_imports(
        &self,
    ) -> Result<Vec<wenlan_types::import::PendingImport>, String> {
        self.get_json("/api/import/state").await
    }

    // ── Source registry ──────────────────────────────────────────────

    pub async fn list_sources(&self) -> Result<Vec<wenlan_types::sources::Source>, String> {
        self.get_json("/api/sources").await
    }

    pub async fn add_source(
        &self,
        source_type: String,
        path: String,
    ) -> Result<wenlan_types::sources::Source, String> {
        let req = wenlan_types::requests::AddSourceRequest { source_type, path };
        self.post_json("/api/sources", &req).await
    }

    pub async fn remove_source(&self, id: &str) -> Result<(), String> {
        let path = format!("/api/sources/{}", id);
        self.delete_empty(&path).await
    }

    pub async fn sync_source(
        &self,
        id: &str,
    ) -> Result<wenlan_types::responses::SyncStatsResponse, String> {
        let path = format!("/api/sources/{}/sync", id);
        self.post_empty(&path).await
    }

    // ── Onboarding milestones ──────────────────────────────────────

    pub async fn list_onboarding_milestones(
        &self,
    ) -> Result<Vec<wenlan_types::onboarding::MilestoneRecord>, String> {
        self.get_json("/api/onboarding/milestones").await
    }

    pub async fn acknowledge_onboarding_milestone(&self, id: &str) -> Result<(), String> {
        let path = format!("/api/onboarding/milestones/{}/acknowledge", id);
        let resp = self
            .client
            .post(self.url(&path))
            .send()
            .await
            .map_err(|e| format!("HTTP POST {}: {}", path, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP POST {} returned {}: {}", path, status, text));
        }
        Ok(())
    }

    pub async fn reset_onboarding_milestones(&self) -> Result<(), String> {
        let path = "/api/onboarding/reset";
        let resp = self
            .client
            .post(self.url(path))
            .send()
            .await
            .map_err(|e| format!("HTTP POST {}: {}", path, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP POST {} returned {}: {}", path, status, text));
        }
        Ok(())
    }

    // ── Home delta feed ────────────────────────────────────────────────

    pub async fn list_recent_retrievals(
        &self,
        limit: i64,
    ) -> Result<Vec<wenlan_types::RetrievalEvent>, String> {
        let path = format!("/api/retrievals/recent?limit={}", limit);
        self.get_json(&path).await
    }

    pub async fn list_recent_changes(
        &self,
        limit: i64,
    ) -> Result<Vec<wenlan_types::PageChange>, String> {
        let path = format!("/api/pages/recent-changes?limit={}", limit);
        self.get_json(&path).await
    }

    pub async fn list_recent_memories(
        &self,
        limit: i64,
        since_ms: Option<i64>,
    ) -> Result<Vec<wenlan_types::RecentActivityItem>, String> {
        let path = match since_ms {
            Some(ms) => format!("/api/memory/recent?limit={}&since_ms={}", limit, ms),
            None => format!("/api/memory/recent?limit={}", limit),
        };
        self.get_json(&path).await
    }

    pub async fn get_enrichment_status(
        &self,
        source_id: &str,
    ) -> Result<wenlan_types::EnrichmentStatusResponse, String> {
        let path = format!("/api/memory/{}/enrichment-status", source_id);
        self.get_json(&path).await
    }

    pub async fn get_memory_revisions(
        &self,
        source_id: &str,
    ) -> Result<wenlan_types::responses::ListMemoryRevisionsResponse, String> {
        let path = format!("/api/memory/{}/revisions", source_id);
        self.get_json(&path).await
    }

    pub async fn list_unconfirmed_memories(
        &self,
        limit: i64,
    ) -> Result<Vec<wenlan_types::RecentActivityItem>, String> {
        let path = format!("/api/memory/unconfirmed?limit={}", limit);
        self.get_json(&path).await
    }

    pub async fn list_recent_pages(
        &self,
        limit: i64,
        since_ms: Option<i64>,
    ) -> Result<Vec<wenlan_types::RecentActivityItem>, String> {
        let path = match since_ms {
            Some(ms) => format!("/api/pages/recent?limit={}&since_ms={}", limit, ms),
            None => format!("/api/pages/recent?limit={}", limit),
        };
        self.get_json(&path).await
    }

    pub async fn list_recent_relations(
        &self,
        limit: Option<usize>,
        since_ms: Option<i64>,
    ) -> Result<Vec<wenlan_types::RecentRelation>, String> {
        let mut path = format!(
            "/api/knowledge/recent-relations?limit={}",
            limit.unwrap_or(10)
        );
        if let Some(ms) = since_ms {
            path.push_str(&format!("&since_ms={}", ms));
        }
        self.get_json(&path).await
    }

    pub async fn get_page_sources(
        &self,
        page_id: &str,
    ) -> Result<Vec<wenlan_types::PageSourceWithMemory>, String> {
        let path = format!("/api/pages/{}/sources", page_id);
        self.get_json(&path).await
    }

    pub async fn get_page_links(
        &self,
        page_id: &str,
    ) -> Result<wenlan_types::responses::PageLinksResponse, String> {
        let path = format!("/api/pages/{}/links", page_id);
        self.get_json(&path).await
    }

    pub async fn get_page_revisions(
        &self,
        page_id: &str,
    ) -> Result<wenlan_types::responses::ListPageRevisionsResponse, String> {
        let path = format!("/api/pages/{}/revisions", page_id);
        self.get_json(&path).await
    }

    pub async fn list_orphan_links(
        &self,
        min_count: Option<usize>,
    ) -> Result<wenlan_types::responses::OrphanLinksResponse, String> {
        let path = match min_count {
            Some(min_count) => format!("/api/pages/orphan-links?min_count={}", min_count),
            None => "/api/pages/orphan-links".to_string(),
        };
        self.get_json(&path).await
    }

    pub async fn test_llm(
        &self,
        endpoint: String,
        model: String,
    ) -> Result<wenlan_types::requests::TestLlmResponse, String> {
        let req = wenlan_types::requests::TestLlmRequest {
            endpoint,
            model,
            prompt: None,
        };
        self.post_json("/api/llm/test", &req).await
    }

    pub async fn get_on_device_model(&self) -> Result<OnDeviceModelResponse, String> {
        self.get_json("/api/on-device-model").await
    }

    pub async fn download_on_device_model(&self, model_id: String) -> Result<(), String> {
        let req = OnDeviceModelRequest { model_id };
        let _resp: wenlan_types::responses::SuccessResponse = self
            .post_json("/api/on-device-model/download", &req)
            .await?;
        Ok(())
    }

    // ── Refinery queue ─────────────────────────────────────────────────────

    pub async fn list_refinements(
        &self,
        limit: Option<usize>,
    ) -> Result<wenlan_types::responses::ListRefinementsResponse, String> {
        let mut path = "/api/refinery/queue".to_string();
        if let Some(limit) = limit {
            path.push_str(&format!("?limit={limit}"));
        }
        self.get_json(&path).await
    }

    pub async fn accept_refinement(
        &self,
        id: &str,
    ) -> Result<wenlan_types::responses::AcceptRefinementResponse, String> {
        self.post_empty(&format!("/api/refinery/queue/{id}/accept"))
            .await
    }

    pub async fn reject_refinement(
        &self,
        id: &str,
    ) -> Result<wenlan_types::responses::RejectRefinementResponse, String> {
        self.post_empty(&format!("/api/refinery/queue/{id}/reject"))
            .await
    }

    // ── Config ─────────────────────────────────────────────────────────────

    /// GET /api/config — return the daemon's current config.
    pub async fn get_config(&self) -> Result<wenlan_types::responses::ConfigResponse, String> {
        self.get_json("/api/config").await
    }

    /// PUT /api/config — update one or more fields and return the new config.
    /// Pass `Option<T>` fields; `None` leaves a field unchanged.
    pub async fn update_config(
        &self,
        req: wenlan_types::requests::UpdateConfigRequest,
    ) -> Result<wenlan_types::responses::ConfigResponse, String> {
        let body = sparse_update_config(req)?;
        self.put_json("/api/config", &body).await
    }

    /// GET /api/setup/status — return daemon-owned setup/model/key state.
    pub async fn get_setup_status(&self) -> Result<SetupStatusResponse, String> {
        self.get_json("/api/setup/status").await
    }

    /// Mark setup complete/incomplete through the daemon config endpoint.
    pub async fn set_setup_completed(&self, completed: bool) -> Result<(), String> {
        self.update_config(empty_update().with_setup_completed(completed))
            .await
            .map(|_| ())
    }

    pub async fn get_model_choice(&self) -> Result<(Option<String>, Option<String>), String> {
        let cfg = self.get_config().await?;
        Ok((cfg.routine_model, cfg.synthesis_model))
    }

    /// Patch daemon model selection. `None` preserves the existing daemon value.
    pub async fn set_model_choice(
        &self,
        routine_model: Option<String>,
        synthesis_model: Option<String>,
    ) -> Result<(), String> {
        self.update_config(empty_update().with_model_choice(routine_model, synthesis_model))
            .await
            .map(|_| ())
    }

    pub async fn get_external_llm(&self) -> Result<(Option<String>, Option<String>), String> {
        let cfg = self.get_config().await?;
        Ok((cfg.external_llm_endpoint, cfg.external_llm_model))
    }

    /// Patch daemon external LLM config. `None` preserves the existing daemon value.
    pub async fn set_external_llm(
        &self,
        endpoint: Option<String>,
        model: Option<String>,
    ) -> Result<(), String> {
        self.update_config(empty_update().with_external_llm(endpoint, model))
            .await
            .map(|_| ())
    }

    pub async fn get_clipboard_enabled(&self) -> Result<bool, String> {
        Ok(self.get_config().await?.clipboard_enabled)
    }

    pub async fn set_clipboard_enabled(
        &self,
        enabled: bool,
    ) -> Result<wenlan_types::responses::ConfigResponse, String> {
        self.update_config(empty_update().with_clipboard_enabled(enabled))
            .await
    }

    pub async fn get_screen_capture_enabled(&self) -> Result<bool, String> {
        Ok(self.get_config().await?.screen_capture_enabled)
    }

    pub async fn set_screen_capture_enabled(
        &self,
        enabled: bool,
    ) -> Result<wenlan_types::responses::ConfigResponse, String> {
        self.update_config(empty_update().with_screen_capture_enabled(enabled))
            .await
    }

    pub async fn get_private_browsing_detection(&self) -> Result<bool, String> {
        Ok(self.get_config().await?.private_browsing_detection)
    }

    pub async fn set_private_browsing_detection(
        &self,
        enabled: bool,
    ) -> Result<wenlan_types::responses::ConfigResponse, String> {
        self.update_config(empty_update().with_private_browsing_detection(enabled))
            .await
    }

    pub async fn set_remote_access_enabled(
        &self,
        enabled: bool,
    ) -> Result<wenlan_types::responses::ConfigResponse, String> {
        self.update_config(empty_update().with_remote_access_enabled(enabled))
            .await
    }

    pub async fn get_skip_apps(&self) -> Result<Vec<String>, String> {
        Ok(self.get_config().await?.skip_apps)
    }

    pub async fn set_skip_apps(&self, apps: Vec<String>) -> Result<(), String> {
        self.update_config(empty_update().with_skip_apps(apps))
            .await
            .map(|_| ())
    }

    pub async fn get_skip_title_patterns(&self) -> Result<Vec<String>, String> {
        Ok(self.get_config().await?.skip_title_patterns)
    }

    pub async fn set_skip_title_patterns(&self, patterns: Vec<String>) -> Result<(), String> {
        self.update_config(empty_update().with_skip_title_patterns(patterns))
            .await
            .map(|_| ())
    }
}

fn daemon_port() -> u16 {
    std::env::var("WENLAN_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .or_else(|| {
            std::env::var("ORIGIN_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(7878)
}

fn percent_encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                vec![byte as char]
            }
            _ => format!("%{byte:02X}").chars().collect(),
        })
        .collect()
}

// `UpdateConfigRequest` does not derive `Default` in wenlan-types, so
// build a baseline with every field set to `None` here. When/if wenlan-types
// adds the derive, these helpers can be deleted in favor of
// `UpdateConfigRequest { skip_apps: Some(...), ..Default::default() }`.
fn empty_update() -> wenlan_types::requests::UpdateConfigRequest {
    wenlan_types::requests::UpdateConfigRequest {
        skip_apps: None,
        skip_title_patterns: None,
        private_browsing_detection: None,
        setup_completed: None,
        clipboard_enabled: None,
        screen_capture_enabled: None,
        remote_access_enabled: None,
        routine_model: None,
        synthesis_model: None,
        external_llm_endpoint: None,
        external_llm_model: None,
    }
}

fn sparse_update_config(
    req: wenlan_types::requests::UpdateConfigRequest,
) -> Result<serde_json::Value, String> {
    let mut value =
        serde_json::to_value(req).map_err(|e| format!("Serialize config update: {}", e))?;
    if let serde_json::Value::Object(ref mut map) = value {
        map.retain(|_, value| !value.is_null());
    }
    Ok(value)
}

trait UpdateConfigBuilder {
    fn with_skip_apps(self, v: Vec<String>) -> Self;
    fn with_skip_title_patterns(self, v: Vec<String>) -> Self;
    fn with_clipboard_enabled(self, v: bool) -> Self;
    fn with_screen_capture_enabled(self, v: bool) -> Self;
    fn with_private_browsing_detection(self, v: bool) -> Self;
    fn with_remote_access_enabled(self, v: bool) -> Self;
    fn with_setup_completed(self, v: bool) -> Self;
    fn with_model_choice(
        self,
        routine_model: Option<String>,
        synthesis_model: Option<String>,
    ) -> Self;
    fn with_external_llm(self, endpoint: Option<String>, model: Option<String>) -> Self;
}

impl UpdateConfigBuilder for wenlan_types::requests::UpdateConfigRequest {
    fn with_skip_apps(mut self, v: Vec<String>) -> Self {
        self.skip_apps = Some(v);
        self
    }
    fn with_skip_title_patterns(mut self, v: Vec<String>) -> Self {
        self.skip_title_patterns = Some(v);
        self
    }
    fn with_clipboard_enabled(mut self, v: bool) -> Self {
        self.clipboard_enabled = Some(v);
        self
    }
    fn with_screen_capture_enabled(mut self, v: bool) -> Self {
        self.screen_capture_enabled = Some(v);
        self
    }
    fn with_private_browsing_detection(mut self, v: bool) -> Self {
        self.private_browsing_detection = Some(v);
        self
    }
    fn with_remote_access_enabled(mut self, v: bool) -> Self {
        self.remote_access_enabled = Some(v);
        self
    }
    fn with_setup_completed(mut self, v: bool) -> Self {
        self.setup_completed = Some(v);
        self
    }
    fn with_model_choice(
        mut self,
        routine_model: Option<String>,
        synthesis_model: Option<String>,
    ) -> Self {
        self.routine_model = routine_model;
        self.synthesis_model = synthesis_model;
        self
    }
    fn with_external_llm(mut self, endpoint: Option<String>, model: Option<String>) -> Self {
        self.external_llm_endpoint = endpoint;
        self.external_llm_model = model;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    struct PortEnvGuard {
        wenlan_port: Option<std::ffi::OsString>,
        origin_port: Option<std::ffi::OsString>,
    }

    impl PortEnvGuard {
        fn capture() -> Self {
            Self {
                wenlan_port: std::env::var_os("WENLAN_PORT"),
                origin_port: std::env::var_os("ORIGIN_PORT"),
            }
        }
    }

    impl Drop for PortEnvGuard {
        fn drop(&mut self) {
            match &self.wenlan_port {
                Some(value) => std::env::set_var("WENLAN_PORT", value),
                None => std::env::remove_var("WENLAN_PORT"),
            }
            match &self.origin_port {
                Some(value) => std::env::set_var("ORIGIN_PORT", value),
                None => std::env::remove_var("ORIGIN_PORT"),
            }
        }
    }

    async fn serve_json_once(body: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 2048];
            let n = stream.read(&mut buf).await.unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            request
        });
        (format!("http://{}", addr), handle)
    }

    #[test]
    fn update_config_builder_can_set_setup_completed() {
        let req = empty_update().with_setup_completed(true);

        assert_eq!(req.setup_completed, Some(true));
        assert_eq!(req.skip_apps, None);
        assert_eq!(req.skip_title_patterns, None);
    }

    #[test]
    fn update_config_builder_patches_capture_toggles_without_touching_other_config() {
        let clipboard_req = empty_update().with_clipboard_enabled(true);
        assert_eq!(clipboard_req.clipboard_enabled, Some(true));
        assert_eq!(clipboard_req.screen_capture_enabled, None);
        assert_eq!(clipboard_req.setup_completed, None);
        assert_eq!(clipboard_req.routine_model, None);

        let screen_req = empty_update().with_screen_capture_enabled(false);
        assert_eq!(screen_req.screen_capture_enabled, Some(false));
        assert_eq!(screen_req.clipboard_enabled, None);
        assert_eq!(screen_req.setup_completed, None);
        assert_eq!(screen_req.external_llm_endpoint, None);
    }

    #[test]
    fn update_config_builder_patches_privacy_fields_without_touching_other_config() {
        let private_req = empty_update().with_private_browsing_detection(false);
        assert_eq!(private_req.private_browsing_detection, Some(false));
        assert_eq!(private_req.clipboard_enabled, None);
        assert_eq!(private_req.screen_capture_enabled, None);

        let remote_req = empty_update().with_remote_access_enabled(true);
        assert_eq!(remote_req.remote_access_enabled, Some(true));
        assert_eq!(remote_req.private_browsing_detection, None);
        assert_eq!(remote_req.skip_apps, None);
    }

    fn request_body(request: &str) -> serde_json::Value {
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    }

    #[tokio::test]
    async fn capture_toggles_use_daemon_config_endpoint() {
        let config_body = r#"{"skip_apps":[],"skip_title_patterns":[],"private_browsing_detection":true,"setup_completed":true,"clipboard_enabled":true,"screen_capture_enabled":false,"remote_access_enabled":false}"#;
        let (base_url, request) = serve_json_once(config_body).await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let config = client.set_clipboard_enabled(true).await.unwrap();

        assert!(config.clipboard_enabled);
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "PUT /api/config HTTP/1.1"
        );
        assert_eq!(
            request_body(&request),
            serde_json::json!({"clipboard_enabled": true})
        );

        let config_body = r#"{"skip_apps":[],"skip_title_patterns":[],"private_browsing_detection":true,"setup_completed":true,"clipboard_enabled":true,"screen_capture_enabled":true,"remote_access_enabled":false}"#;
        let (base_url, request) = serve_json_once(config_body).await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let config = client.set_screen_capture_enabled(true).await.unwrap();

        assert!(config.screen_capture_enabled);
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "PUT /api/config HTTP/1.1"
        );
        assert_eq!(
            request_body(&request),
            serde_json::json!({"screen_capture_enabled": true})
        );
    }

    #[tokio::test]
    async fn privacy_config_fields_use_daemon_patch_endpoint() {
        let config_body = r#"{"skip_apps":[],"skip_title_patterns":[],"private_browsing_detection":false,"setup_completed":true,"clipboard_enabled":true,"screen_capture_enabled":false,"remote_access_enabled":false}"#;
        let (base_url, request) = serve_json_once(config_body).await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let config = client.set_private_browsing_detection(false).await.unwrap();

        assert!(!config.private_browsing_detection);
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "PUT /api/config HTTP/1.1"
        );
        assert_eq!(
            request_body(&request),
            serde_json::json!({"private_browsing_detection": false})
        );

        let config_body = r#"{"skip_apps":[],"skip_title_patterns":[],"private_browsing_detection":true,"setup_completed":true,"clipboard_enabled":true,"screen_capture_enabled":false,"remote_access_enabled":true}"#;
        let (base_url, request) = serve_json_once(config_body).await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let config = client.set_remote_access_enabled(true).await.unwrap();

        assert!(config.remote_access_enabled);
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "PUT /api/config HTTP/1.1"
        );
        assert_eq!(
            request_body(&request),
            serde_json::json!({"remote_access_enabled": true})
        );
    }

    #[test]
    fn update_config_builder_patches_model_fields_without_touching_other_config() {
        let req = empty_update().with_model_choice(Some("claude-haiku-4-5-20251001".into()), None);

        assert_eq!(
            req.routine_model,
            Some("claude-haiku-4-5-20251001".to_string())
        );
        assert_eq!(req.synthesis_model, None);
        assert_eq!(req.external_llm_endpoint, None);
        assert_eq!(req.external_llm_model, None);
    }

    #[test]
    fn update_config_builder_patches_external_llm_fields_without_touching_models() {
        let req = empty_update().with_external_llm(None, Some("qwen3".into()));

        assert_eq!(req.external_llm_endpoint, None);
        assert_eq!(req.external_llm_model, Some("qwen3".to_string()));
        assert_eq!(req.routine_model, None);
        assert_eq!(req.synthesis_model, None);
    }

    #[test]
    fn setup_status_response_deserializes_daemon_payload() {
        let status: SetupStatusResponse = serde_json::from_value(serde_json::json!({
            "setup_completed": false,
            "mode": "basic-memory",
            "anthropic_key_configured": false,
            "local_model_selected": null,
            "local_model_loaded": null,
            "local_model_cached": false
        }))
        .expect("daemon setup status payload should deserialize");

        assert!(!status.setup_completed);
        assert_eq!(status.mode, "basic-memory");
        assert!(!status.anthropic_key_configured);
        assert_eq!(status.local_model_selected, None);
        assert_eq!(status.local_model_loaded, None);
        assert!(!status.local_model_cached);
    }

    #[test]
    fn wenlan_client_prefers_wenlan_port_over_legacy_origin_port() {
        let _guard = env_lock();
        let _env = PortEnvGuard::capture();
        std::env::set_var("WENLAN_PORT", "8787");
        std::env::set_var("ORIGIN_PORT", "9898");

        let client = WenlanClient::new();

        assert_eq!(
            client.url("/api/health"),
            "http://127.0.0.1:8787/api/health"
        );
    }

    #[test]
    fn wenlan_client_falls_back_to_legacy_origin_port() {
        let _guard = env_lock();
        let _env = PortEnvGuard::capture();
        std::env::remove_var("WENLAN_PORT");
        std::env::set_var("ORIGIN_PORT", "9898");

        let client = WenlanClient::new();

        assert_eq!(
            client.url("/api/health"),
            "http://127.0.0.1:9898/api/health"
        );
    }

    #[tokio::test]
    async fn capture_stats_uses_daemon_capture_stats_endpoint() {
        let (base_url, request) = serve_json_once(r#"{"total_chunks":42}"#).await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let stats = client.get_capture_stats().await.unwrap();

        assert_eq!(stats.get("total"), Some(&42));
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "GET /api/capture-stats HTTP/1.1"
        );
    }

    #[tokio::test]
    async fn list_tags_uses_daemon_tags_endpoint() {
        let (base_url, request) = serve_json_once(r#"{"tags":["ai","rust"]}"#).await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let tags = client.list_tags().await.unwrap();

        assert_eq!(tags, vec!["ai".to_string(), "rust".to_string()]);
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "GET /api/tags HTTP/1.1"
        );
    }

    #[tokio::test]
    async fn list_tag_inventory_preserves_document_tag_map() {
        let (base_url, request) = serve_json_once(
            r#"{"tags":["ai","rust"],"document_tags":{"memory::mem1":["ai"],"page::page1":["rust"]}}"#,
        )
        .await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let inventory = client.list_tag_inventory().await.unwrap();

        assert_eq!(inventory.tags, vec!["ai".to_string(), "rust".to_string()]);
        assert_eq!(
            inventory.document_tags.get("memory::mem1"),
            Some(&vec!["ai".to_string()])
        );
        assert_eq!(
            inventory.document_tags.get("page::page1"),
            Some(&vec!["rust".to_string()])
        );
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "GET /api/tags HTTP/1.1"
        );
    }

    #[tokio::test]
    async fn ingest_webpage_uses_daemon_webpage_ingest_endpoint() {
        let (base_url, request) =
            serve_json_once(r#"{"chunks_created":3,"document_id":"https://example.com/post"}"#)
                .await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };
        let mut metadata = HashMap::new();
        metadata.insert("source".to_string(), "manual-url".to_string());
        let req = wenlan_types::requests::IngestWebpageRequest {
            url: "https://example.com/post".to_string(),
            title: "Example Post".to_string(),
            content: "A durable article body.".to_string(),
            metadata: Some(metadata),
        };

        let resp = client.ingest_webpage(req).await.unwrap();

        assert_eq!(resp.chunks_created, 3);
        assert_eq!(resp.document_id, "https://example.com/post");
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "POST /api/ingest/webpage HTTP/1.1"
        );
        assert_eq!(
            request_body(&request),
            serde_json::json!({
                "url": "https://example.com/post",
                "title": "Example Post",
                "content": "A durable article body.",
                "metadata": {"source": "manual-url"}
            })
        );
    }

    #[tokio::test]
    async fn move_space_uses_daemon_space_move_endpoint() {
        let (base_url, request) = serve_json_once(r#"{"affected":7}"#).await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let resp = client.move_space("Inbox", "Archive").await.unwrap();

        assert_eq!(resp.affected, 7);
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "POST /api/spaces/Inbox/move-to/Archive HTTP/1.1"
        );
    }

    #[tokio::test]
    async fn move_space_percent_encodes_space_names_as_path_segments() {
        let (base_url, request) = serve_json_once(r#"{"affected":2}"#).await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let resp = client
            .move_space("Work/Clients?old=true", "Archive#2026")
            .await
            .unwrap();

        assert_eq!(resp.affected, 2);
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "POST /api/spaces/Work%2FClients%3Fold%3Dtrue/move-to/Archive%232026 HTTP/1.1"
        );
    }

    #[tokio::test]
    async fn status_uses_daemon_status_endpoint() {
        let (base_url, request) = serve_json_once(
            r#"{"is_running":true,"files_indexed":42,"files_total":0,"sources_connected":[],"reranker":{"state":"disabled"},"reranker_light":{"state":"active","model_id":"bge"},"reranker_mode":"lite"}"#,
        )
        .await;
        let client = WenlanClient {
            client: reqwest::Client::new(),
            base_url,
        };

        let status = client.status().await.unwrap();

        assert_eq!(status.files_indexed, 42);
        assert_eq!(status.reranker_mode, "lite");
        let request = request.await.unwrap();
        assert_eq!(
            request.lines().next().unwrap_or_default(),
            "GET /api/status HTTP/1.1"
        );
    }

    #[test]
    fn wenlan_client_exposes_enrichment_status_method() {
        let _get_enrichment_status = WenlanClient::get_enrichment_status;
    }

    #[test]
    fn enrichment_status_response_deserializes_daemon_payload() {
        let status: wenlan_types::EnrichmentStatusResponse =
            serde_json::from_value(serde_json::json!({
                "source_id": "mem-1",
                "summary": "complete",
                "steps": [
                    { "step": "classify", "status": "done", "error": null, "attempts": 1 }
                ]
            }))
            .unwrap();

        assert_eq!(status.source_id, "mem-1");
        assert_eq!(status.steps[0].step, "classify");
    }

    #[test]
    fn wenlan_client_exposes_refinery_queue_methods() {
        let _list = WenlanClient::list_refinements;
        let _accept = WenlanClient::accept_refinement;
        let _reject = WenlanClient::reject_refinement;
    }

    #[test]
    fn wenlan_client_exposes_daemon_model_config_methods() {
        let _get_model_choice = WenlanClient::get_model_choice;
        let _set_model_choice = WenlanClient::set_model_choice;
        let _get_external_llm = WenlanClient::get_external_llm;
        let _set_external_llm = WenlanClient::set_external_llm;
    }

    #[allow(dead_code)]
    async fn test_llm_uses_daemon_response_envelope(client: WenlanClient) {
        let _: Result<wenlan_types::requests::TestLlmResponse, String> =
            client.test_llm(String::new(), String::new()).await;
    }

    #[test]
    fn test_llm_response_type_is_checked() {}

    #[allow(dead_code)]
    async fn on_device_model_uses_typed_response(client: WenlanClient) {
        let _: Result<OnDeviceModelResponse, String> = client.get_on_device_model().await;
    }

    #[allow(dead_code)]
    async fn download_on_device_model_uses_typed_request(client: WenlanClient) {
        let _: Result<(), String> = client.download_on_device_model(String::new()).await;
    }

    #[test]
    fn on_device_model_response_type_is_checked() {}

    #[test]
    fn wenlan_client_exposes_source_registry_methods() {
        let _list = WenlanClient::list_sources;
        let _add = WenlanClient::add_source;
        let _remove = WenlanClient::remove_source;
        let _sync = WenlanClient::sync_source;
    }

    #[test]
    fn wenlan_client_exposes_page_link_methods() {
        let _get = WenlanClient::get_page_links;
        let _list = WenlanClient::list_orphan_links;
    }

    #[test]
    fn wenlan_client_exposes_revision_history_methods() {
        let _memory = WenlanClient::get_memory_revisions;
        let _page = WenlanClient::get_page_revisions;
    }

    #[test]
    fn revision_history_responses_deserialize_daemon_payloads() {
        let memory: wenlan_types::responses::ListMemoryRevisionsResponse =
            serde_json::from_value(serde_json::json!({
                "current_source_id": "mem-1",
                "chain_depth": 1,
                "entries": [
                    {
                        "source_id": "mem-1",
                        "depth": 0,
                        "title": "Current",
                        "content_preview": "Current version",
                        "last_modified": 10,
                        "source_agent": "claude-code",
                        "supersede_mode": "protected_revision",
                        "delta_summary": "Clarified wording"
                    }
                ]
            }))
            .unwrap();
        assert_eq!(memory.current_source_id, "mem-1");
        assert_eq!(
            memory.entries[0].delta_summary.as_deref(),
            Some("Clarified wording")
        );

        let page: wenlan_types::responses::ListPageRevisionsResponse =
            serde_json::from_value(serde_json::json!({
                "page_id": "page-1",
                "current_version": 2,
                "user_edited": false,
                "stale_reason": null,
                "entries": [
                    {
                        "version": 2,
                        "at": 1782490000000i64,
                        "edited_by": "distill",
                        "delta_summary": "Added backlinks",
                        "incoming_source_ids": ["mem-1"]
                    }
                ]
            }))
            .unwrap();
        assert_eq!(page.page_id, "page-1");
        assert_eq!(
            page.entries[0].incoming_source_ids.as_ref().unwrap(),
            &vec!["mem-1".to_string()]
        );
    }
}
