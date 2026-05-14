// SPDX-License-Identifier: Apache-2.0
//! HTTP client for talking to the origin daemon.
//!
//! Mirrors the relevant slice of `app/src/api.rs::OriginClient` but uses
//! `anyhow::Result` (this is a CLI binary, not a Tauri command surface),
//! and reads `ORIGIN_HOST` (full URL) instead of `ORIGIN_PORT` so users can
//! point the CLI at a remote daemon over a tunnel.
//!
//! The methods exposed here are the subset the CLI subcommands need:
//! status, ping, search, context, store, list, agents.

use anyhow::{Context, Result};
use origin_types::{
    requests::{ListMemoriesRequest, SearchRequest, StoreMemoryRequest, UpdateAgentRequest},
    responses::{
        AgentResponse, HealthResponse, KnowledgeContext, ListMemoriesResponse, SearchResponse,
        StoreMemoryResponse,
    },
};

const DEFAULT_HOST: &str = "http://127.0.0.1:7878";

pub struct OriginClient {
    base_url: String,
    http: reqwest::Client,
}

impl OriginClient {
    /// Create a client using `ORIGIN_HOST` env var, or default to `http://127.0.0.1:7878`.
    pub fn from_env() -> Self {
        let base_url = std::env::var("ORIGIN_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
        // Strip trailing slash so URL joins are predictable.
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
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

    /// POST /api/chat-context — contextual knowledge bundle for a query.
    ///
    /// Note: `/api/context` (the older endpoint) takes `current_file` +
    /// `cursor_prefix` and is hidden from the public API. The CLI uses
    /// `/api/chat-context` which accepts a free-form query and returns
    /// the same `KnowledgeContext` shape inside its response.
    pub async fn context(&self, query: String) -> Result<KnowledgeContext> {
        let url = format!("{}/api/chat-context", self.base_url);
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
        let full: serde_json::Value = resp
            .json()
            .await
            .context("parsing /api/chat-context response")?;
        let knowledge = full
            .get("knowledge")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("/api/chat-context response missing `knowledge`"))?;
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
}
