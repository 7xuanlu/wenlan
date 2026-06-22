use std::time::Duration;

use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

const DEFAULT_HTTP_URL: &str = "http://127.0.0.1:7878";

/// Single source of truth for the space-lock header name.
/// Mirrors the daemon's `X-Origin-Space` constant (HTTP normalises to lowercase).
pub fn space_header_name() -> &'static str {
    "x-origin-space"
}

/// Discover the Wenlan server URL.
/// Priority: CLI flag > HTTP default.
/// Note: UDS discovery disabled — reqwest doesn't support unix:// URLs natively.
/// Wenlan always binds HTTP on 127.0.0.1:7878 alongside UDS, so HTTP is reliable.
pub fn discover_origin_url(cli_url: Option<String>) -> String {
    if let Some(url) = cli_url {
        return url;
    }

    DEFAULT_HTTP_URL.to_string()
}

/// HTTP client for the Wenlan REST API.
#[derive(Clone)]
pub struct WenlanClient {
    client: Client,
    base_url: String,
    agent_name: Option<String>,
}

/// Max retries on connection errors (daemon restarting).
const MAX_RETRIES: u32 = 3;
/// Backoff per retry: attempt 1 = 1s, attempt 2 = 2s, attempt 3 = 3s.
/// Total worst-case wait: ~6s, covering a typical daemon restart.
const BACKOFF_BASE: Duration = Duration::from_secs(1);

impl WenlanClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
            agent_name: None,
        }
    }

    /// Set the agent name to be sent as `x-agent-name` header on every request.
    pub fn with_agent_name(mut self, name: String) -> Self {
        self.agent_name = Some(name);
        self
    }

    /// Retry a request on connection errors (daemon restarting).
    /// Only retries on connect failures; non-connect errors and HTTP responses
    /// are returned immediately.
    async fn send_with_retry(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, WenlanError> {
        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                tokio::time::sleep(BACKOFF_BASE * attempt).await;
            }
            match build().send().await {
                Ok(resp) => return Ok(resp),
                Err(e) if e.is_connect() => {
                    tracing::debug!(attempt, "daemon unreachable, retrying");
                    last_err = Some(e);
                }
                Err(e) => return Err(WenlanError::Unreachable(e.to_string())),
            }
        }
        Err(WenlanError::Unreachable(last_err.map_or_else(
            || "connection failed".into(),
            |e| e.to_string(),
        )))
    }

    /// Parse a successful response body as JSON.
    fn parse_response<R: DeserializeOwned>(bytes: &[u8]) -> Result<R, WenlanError> {
        serde_json::from_slice::<R>(bytes).map_err(|e| {
            let preview = std::str::from_utf8(bytes)
                .unwrap_or("<non-utf8>")
                .chars()
                .take(512)
                .collect::<String>();
            WenlanError::Deserialize(format!("{e} (body preview: {preview})"))
        })
    }

    /// Read response body, checking status first.
    async fn read_body(resp: reqwest::Response) -> Result<Vec<u8>, WenlanError> {
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(WenlanError::Api { status, body });
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| WenlanError::Deserialize(format!("failed to read response body: {e:#}")))
    }

    /// Attach per-request headers common to all daemon calls:
    /// `x-agent-name` (when set) and `x-origin-space` (when space is locked).
    fn attach_common_headers(
        mut req: reqwest::RequestBuilder,
        agent: Option<&str>,
    ) -> reqwest::RequestBuilder {
        if let Some(a) = agent {
            req = req.header("x-agent-name", a);
        }
        if let Some(space) = crate::lock_state::locked_space() {
            req = req.header(space_header_name(), space);
        }
        req
    }

    /// GET request, deserialize JSON response.
    pub async fn get<R: DeserializeOwned>(&self, path: &str) -> Result<R, WenlanError> {
        let url = format!("{}{}", self.base_url, path);
        let agent = self.agent_name.clone();
        let resp = self
            .send_with_retry(|| {
                let req = self.client.get(&url);
                Self::attach_common_headers(req, agent.as_deref())
            })
            .await?;
        let bytes = Self::read_body(resp).await?;
        Self::parse_response(&bytes)
    }

    /// POST request with JSON body, deserialize JSON response.
    pub async fn post<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, WenlanError> {
        let url = format!("{}{}", self.base_url, path);
        let agent = self.agent_name.clone();
        let resp = self
            .send_with_retry(|| {
                let req = self.client.post(&url).json(body);
                Self::attach_common_headers(req, agent.as_deref())
            })
            .await?;
        let bytes = Self::read_body(resp).await?;
        Self::parse_response(&bytes)
    }

    /// POST request with empty body, deserialize JSON response.
    /// Used for mutate endpoints where the id is in the path and no body is needed.
    pub async fn post_empty<R: DeserializeOwned>(&self, path: &str) -> Result<R, WenlanError> {
        let url = format!("{}{}", self.base_url, path);
        let agent = self.agent_name.clone();
        let resp = self
            .send_with_retry(|| {
                let req = self.client.post(&url);
                Self::attach_common_headers(req, agent.as_deref())
            })
            .await?;
        let bytes = Self::read_body(resp).await?;
        Self::parse_response(&bytes)
    }

    /// DELETE request, deserialize JSON response.
    pub async fn delete<R: DeserializeOwned>(&self, path: &str) -> Result<R, WenlanError> {
        let url = format!("{}{}", self.base_url, path);
        let agent = self.agent_name.clone();
        let resp = self
            .send_with_retry(|| {
                let req = self.client.delete(&url);
                Self::attach_common_headers(req, agent.as_deref())
            })
            .await?;
        let bytes = Self::read_body(resp).await?;
        Self::parse_response(&bytes)
    }

    /// PUT request with JSON body, deserialize JSON response.
    pub async fn put<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, WenlanError> {
        let url = format!("{}{}", self.base_url, path);
        let agent = self.agent_name.clone();
        let resp = self
            .send_with_retry(|| {
                let req = self.client.put(&url).json(body);
                Self::attach_common_headers(req, agent.as_deref())
            })
            .await?;
        let bytes = Self::read_body(resp).await?;
        Self::parse_response(&bytes)
    }

    /// Query the daemon's /api/health, compare versions, and return a
    /// human-readable warning if wenlan-mcp is older than the daemon's minor.
    /// Returns None if compatible OR if the daemon is unreachable / response
    /// can't be parsed (handshake never blocks startup).
    pub async fn version_handshake(&self) -> Option<String> {
        use crate::version_check::{compare, VersionStatus};

        let url = format!("{}/api/health", self.base_url);
        // Bypass send_with_retry: a 6s retry loop at startup against a missing
        // or hung daemon would be worse UX than a silent skip. 2s timeout bounds
        // the worst case where the daemon socket accepts but the handler stalls.
        let resp = self
            .client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .ok()?;
        let body: serde_json::Value = resp.json().await.ok()?;
        let daemon_version = body["version"].as_str()?;
        let mcp_version = env!("CARGO_PKG_VERSION");

        match compare(mcp_version, daemon_version) {
            VersionStatus::Compatible => None,
            VersionStatus::McpOutdated { mcp, daemon } => Some(format!(
                "Your wenlan-mcp v{mcp} is older than the daemon v{daemon}. \
                 Run `brew upgrade wenlan-mcp` (or `npm update -g wenlan-mcp`)."
            )),
            VersionStatus::DaemonOutdated { mcp, daemon } => Some(format!(
                "The Wenlan daemon is running v{daemon} but wenlan-mcp v{mcp} is installed. \
                 The daemon was not restarted after an upgrade. Run `origin restart` to load it."
            )),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WenlanError {
    #[error("Wenlan is not reachable: {0}")]
    Unreachable(String),

    #[error("Wenlan API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },

    #[error("Failed to parse Wenlan response: {0}")]
    Deserialize(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_url_prefers_cli_flag() {
        let url = discover_origin_url(Some("http://localhost:9999".into()));
        assert_eq!(url, "http://localhost:9999");
    }

    #[test]
    fn test_discover_url_falls_back_to_http() {
        // With no CLI flag and no socket, should fall back to default HTTP
        let url = discover_origin_url(None);
        assert_eq!(url, "http://127.0.0.1:7878");
    }

    #[test]
    fn space_header_attached_when_locked() {
        // Share ENV_LOCK with lock_state::tests to prevent env var races.
        let _guard = crate::lock_state::ENV_LOCK.lock().unwrap();
        std::env::set_var("WENLAN_SPACE", "career");
        crate::lock_state::init_from_env();

        let client = Client::new();
        let builder = WenlanClient::attach_common_headers(
            client.get("http://127.0.0.1:7878/api/health"),
            None,
        );
        let req = builder.build().unwrap();
        let header = req.headers().get(space_header_name()).unwrap();
        assert_eq!(header.to_str().unwrap(), "career");

        // Clean up.
        std::env::remove_var("WENLAN_SPACE");
        crate::lock_state::init_from_env();
    }

    #[test]
    fn space_header_absent_when_unlocked() {
        // Share ENV_LOCK with lock_state::tests to prevent env var races.
        let _guard = crate::lock_state::ENV_LOCK.lock().unwrap();
        std::env::remove_var("WENLAN_SPACE");
        crate::lock_state::init_from_env();

        let client = Client::new();
        let builder = WenlanClient::attach_common_headers(
            client.get("http://127.0.0.1:7878/api/health"),
            None,
        );
        let req = builder.build().unwrap();
        assert!(req.headers().get(space_header_name()).is_none());
    }
}
