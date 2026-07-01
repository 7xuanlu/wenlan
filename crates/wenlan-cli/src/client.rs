// SPDX-License-Identifier: Apache-2.0
//! HTTP client for talking to the origin daemon.
//!
//! Mirrors the relevant slice of `app/src/api.rs::WenlanClient` but uses
//! `anyhow::Result` (this is a CLI binary, not a Tauri command surface),
//! and reads `WENLAN_HOST` (full URL) instead of `WENLAN_PORT` so users can
//! point the CLI at a remote daemon over a tunnel.
//!
//! The methods exposed here are the subset the CLI subcommands need:
//! status, ping, search, context, store, list, agents.

use anyhow::{Context, Result};
use wenlan_types::{
    requests::{ListMemoriesRequest, SearchRequest, StoreMemoryRequest, UpdateAgentRequest},
    responses::{
        AgentResponse, HealthResponse, KnowledgeContext, ListMemoriesResponse,
        MemoryDetailResponse, PendingRevisionItem, RevisionAcceptResponse, RevisionDismissResponse,
        SearchResponse, StoreMemoryResponse,
    },
};

const DEFAULT_HOST: &str = "http://127.0.0.1:7878";

pub fn origin_host_from_env() -> String {
    std::env::var("WENLAN_HOST")
        .unwrap_or_else(|_| DEFAULT_HOST.to_string())
        .trim_end_matches('/')
        .to_string()
}

pub struct WenlanClient {
    base_url: String,
    http: reqwest::Client,
}

impl WenlanClient {
    /// Create a client using `WENLAN_HOST` env var, or default to `http://127.0.0.1:7878`.
    pub fn from_env() -> Self {
        let base_url = origin_host_from_env();
        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }

    /// GET /api/health — daemon liveness + version.
    pub async fn health(&self) -> Result<HealthResponse> {
        let url = format!("{}/api/health", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed (is the daemon running?)", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json().await.context("parsing /api/health response")
    }

    /// POST /api/search — hybrid memory search.
    pub async fn search(&self, query: String, limit: usize) -> Result<SearchResponse> {
        let url = format!("{}/api/search", self.base_url);
        let req = SearchRequest {
            query,
            limit,
            source_filter: None,
            space: None,
        };
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json().await.context("parsing /api/search response")
    }

    /// POST /api/context — contextual knowledge bundle for a query.
    ///
    /// Note: `/api/context` (the older endpoint) takes `current_file` +
    /// `cursor_prefix` and is hidden from the public API. The CLI uses
    /// `/api/context` which accepts a free-form query and returns
    /// the same `KnowledgeContext` shape inside its response.
    pub async fn context(&self, query: String) -> Result<KnowledgeContext> {
        let url = format!("{}/api/context", self.base_url);
        let req = serde_json::json!({ "query": query });
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        let full: serde_json::Value = resp.json().await.context("parsing /api/context response")?;
        let knowledge = full
            .get("knowledge")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("/api/context response missing `knowledge`"))?;
        serde_json::from_value(knowledge).context("parsing knowledge subobject")
    }

    /// POST /api/memory/store — write a memory.
    pub async fn store(
        &self,
        content: String,
        memory_type: Option<String>,
    ) -> Result<StoreMemoryResponse> {
        let url = format!("{}/api/memory/store", self.base_url);
        let req = StoreMemoryRequest {
            content,
            memory_type,
            space: None,
            source_agent: None,
            title: None,
            confidence: None,
            supersedes: None,
            entity: None,
            entity_id: None,
            structured_fields: None,
            retrieval_cue: None,
        };
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json()
            .await
            .context("parsing /api/memory/store response")
    }

    /// POST /api/memory/list — list memories with optional filters.
    pub async fn list(
        &self,
        limit: Option<usize>,
        memory_type: Option<String>,
    ) -> Result<ListMemoriesResponse> {
        let url = format!("{}/api/memory/list", self.base_url);
        let req = ListMemoriesRequest {
            memory_type,
            space: None,
            limit: limit.unwrap_or(100),
            confirmed: None,
        };
        let resp = self
            .http
            .post(&url)
            .json(&req)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json()
            .await
            .context("parsing /api/memory/list response")
    }

    /// GET /api/agents — list registered agents.
    pub async fn list_agents(&self) -> Result<Vec<AgentResponse>> {
        let url = format!("{}/api/agents", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json().await.context("parsing /api/agents response")
    }

    /// GET /api/agents/{name} — fetch a single agent.
    pub async fn get_agent(&self, name: &str) -> Result<AgentResponse> {
        let url = format!("{}/api/agents/{}", self.base_url, name);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json()
            .await
            .context("parsing /api/agents/{name} response")
    }

    /// POST /api/spaces — register a new space.
    pub async fn create_space(&self, name: &str) -> Result<()> {
        let url = format!("{}/api/spaces", self.base_url);
        self.http
            .post(&url)
            .json(&serde_json::json!({"name": name}))
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        Ok(())
    }

    /// POST /api/spaces/{from}/move-to/{to} — bulk reassign memories from one space to another.
    pub async fn move_space(&self, from: &str, to: &str) -> Result<usize> {
        let url = format!("{}/api/spaces/{}/move-to/{}", self.base_url, from, to);
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        let json: serde_json::Value = resp.json().await.context("parsing move-to response")?;
        Ok(json["affected"].as_u64().unwrap_or(0) as usize)
    }

    /// GET /api/spaces — fetch a single space by name (filters from list).
    pub async fn get_space(&self, name: &str) -> Result<wenlan_types::Space> {
        let spaces = self.list_spaces().await?;
        spaces
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow::anyhow!("space '{}' not found", name))
    }

    /// GET /api/spaces — list all spaces.
    pub async fn list_spaces(&self) -> Result<Vec<wenlan_types::Space>> {
        let url = format!("{}/api/spaces", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json().await.context("parsing /api/spaces response")
    }

    /// PUT /api/agents/{name} — update an agent's metadata.
    pub async fn update_agent(&self, name: &str, req: UpdateAgentRequest) -> Result<AgentResponse> {
        let url = format!("{}/api/agents/{}", self.base_url, name);
        let resp = self
            .http
            .put(&url)
            .json(&req)
            .send()
            .await
            .with_context(|| format!("PUT {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json()
            .await
            .context("parsing /api/agents/{name} update response")
    }

    /// GET /api/memory/pending-revisions — staged revisions awaiting human accept/dismiss.
    pub async fn list_pending_revisions(&self, limit: usize) -> Result<Vec<PendingRevisionItem>> {
        let url = format!(
            "{}/api/memory/pending-revisions?limit={}",
            self.base_url, limit
        );
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed (is the daemon running?)", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json()
            .await
            .context("parsing /api/memory/pending-revisions response")
    }

    /// POST /api/memory/revision/{id}/accept — replace the original with the revision.
    /// `id` is the revision's own `source_id` (the daemon also accepts a target id, legacy).
    pub async fn accept_revision(&self, id: &str) -> Result<RevisionAcceptResponse> {
        let url = format!("{}/api/memory/revision/{}/accept", self.base_url, id);
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json()
            .await
            .context("parsing accept-revision response")
    }

    /// POST /api/memory/revision/{id}/dismiss — drop the revision, keep the original.
    /// `id` is the revision's own `source_id` (the daemon also accepts a target id, legacy).
    pub async fn dismiss_revision(&self, id: &str) -> Result<RevisionDismissResponse> {
        let url = format!("{}/api/memory/revision/{}/dismiss", self.base_url, id);
        let resp = self
            .http
            .post(&url)
            .send()
            .await
            .with_context(|| format!("POST {} failed", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json()
            .await
            .context("parsing dismiss-revision response")
    }

    /// GET /api/memory/{id}/detail — the assembled (chunks-joined) memory by source_id.
    /// Used by `wenlan curate` to fetch the ORIGINAL a revision would replace, so the
    /// card can show an original->revision diff.
    pub async fn get_memory_detail(&self, source_id: &str) -> Result<MemoryDetailResponse> {
        let url = format!("{}/api/memory/{}/detail", self.base_url, source_id);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed (is the daemon running?)", url))?;
        let resp = resp
            .error_for_status()
            .with_context(|| format!("daemon returned error for {}", url))?;
        resp.json()
            .await
            .context("parsing /api/memory/{id}/detail response")
    }
}
