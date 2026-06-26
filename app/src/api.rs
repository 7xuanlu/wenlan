// SPDX-License-Identifier: AGPL-3.0-only
//! HTTP client for the Origin daemon (origin-server).
//!
//! Thin wrapper around `reqwest::Client` that maps each daemon endpoint
//! to a typed method. The Tauri app uses this instead of direct DB access.

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use wenlan_types::responses::HealthResponse;

/// HTTP client that proxies requests to the origin-server daemon.
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

impl Default for WenlanClient {
    fn default() -> Self {
        Self::new()
    }
}

impl WenlanClient {
    pub fn new() -> Self {
        let port: u16 = std::env::var("ORIGIN_PORT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(7878);
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

    pub async fn test_llm(&self, endpoint: String, model: String) -> Result<String, String> {
        let req = wenlan_types::requests::TestLlmRequest {
            endpoint,
            model,
            prompt: None,
        };
        let resp: wenlan_types::requests::TestLlmResponse =
            self.post_json("/api/llm/test", &req).await?;
        Ok(resp.response)
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
        self.put_json("/api/config", &req).await
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

trait UpdateConfigBuilder {
    fn with_skip_apps(self, v: Vec<String>) -> Self;
    fn with_skip_title_patterns(self, v: Vec<String>) -> Self;
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

    #[test]
    fn update_config_builder_can_set_setup_completed() {
        let req = empty_update().with_setup_completed(true);

        assert_eq!(req.setup_completed, Some(true));
        assert_eq!(req.skip_apps, None);
        assert_eq!(req.skip_title_patterns, None);
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

    #[test]
    fn wenlan_client_exposes_source_registry_methods() {
        let _list = WenlanClient::list_sources;
        let _add = WenlanClient::add_source;
        let _remove = WenlanClient::remove_source;
        let _sync = WenlanClient::sync_source;
    }
}
