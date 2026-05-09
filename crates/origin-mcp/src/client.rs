use std::time::Duration;

use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};

const DEFAULT_HTTP_URL: &str = "http://127.0.0.1:7878";

/// Discover the Origin server URL.
/// Priority: CLI flag > HTTP default.
/// Note: UDS discovery disabled — reqwest doesn't support unix:// URLs natively.
/// Origin always binds HTTP on 127.0.0.1:7878 alongside UDS, so HTTP is reliable.
pub fn discover_origin_url(cli_url: Option<String>) -> String {
    if let Some(url) = cli_url {
        return url;
    }

    DEFAULT_HTTP_URL.to_string()
}

/// HTTP client for the Origin REST API.
#[derive(Clone)]
pub struct OriginClient {
    client: Client,
    base_url: String,
}

/// Max retries on connection errors (daemon restarting).
const MAX_RETRIES: u32 = 3;
/// Backoff per retry: attempt 1 = 1s, attempt 2 = 2s, attempt 3 = 3s.
/// Total worst-case wait: ~6s, covering a typical daemon restart.
const BACKOFF_BASE: Duration = Duration::from_secs(1);

impl OriginClient {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    /// Retry a request on connection errors (daemon restarting).
    /// Only retries on connect failures; non-connect errors and HTTP responses
    /// are returned immediately.
    async fn send_with_retry(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, OriginError> {
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
                Err(e) => return Err(OriginError::Unreachable(e.to_string())),
            }
        }
        Err(OriginError::Unreachable(last_err.map_or_else(
            || "connection failed".into(),
            |e| e.to_string(),
        )))
    }

    /// Parse a successful response body as JSON.
    fn parse_response<R: DeserializeOwned>(bytes: &[u8]) -> Result<R, OriginError> {
        serde_json::from_slice::<R>(bytes).map_err(|e| {
            let preview = std::str::from_utf8(bytes)
                .unwrap_or("<non-utf8>")
                .chars()
                .take(512)
                .collect::<String>();
            OriginError::Deserialize(format!("{e} (body preview: {preview})"))
        })
    }

    /// Read response body, checking status first.
    async fn read_body(resp: reqwest::Response) -> Result<Vec<u8>, OriginError> {
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(OriginError::Api { status, body });
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| OriginError::Deserialize(format!("failed to read response body: {e:#}")))
    }

    /// GET request, deserialize JSON response.
    pub async fn get<R: DeserializeOwned>(&self, path: &str) -> Result<R, OriginError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.send_with_retry(|| self.client.get(&url)).await?;
        let bytes = Self::read_body(resp).await?;
        Self::parse_response(&bytes)
    }

    /// POST request with JSON body, deserialize JSON response.
    pub async fn post<B: Serialize, R: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<R, OriginError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .send_with_retry(|| self.client.post(&url).json(body))
            .await?;
        let bytes = Self::read_body(resp).await?;
        Self::parse_response(&bytes)
    }

    /// DELETE request, deserialize JSON response.
    pub async fn delete<R: DeserializeOwned>(&self, path: &str) -> Result<R, OriginError> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.send_with_retry(|| self.client.delete(&url)).await?;
        let bytes = Self::read_body(resp).await?;
        Self::parse_response(&bytes)
    }

    /// Query the daemon's /api/health, compare versions, and return a
    /// human-readable warning if origin-mcp is older than the daemon's minor.
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
                "Your origin-mcp v{mcp} is older than the daemon v{daemon}. \
                 Run `brew upgrade origin-mcp` (or `npm update -g origin-mcp`)."
            )),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OriginError {
    #[error("Origin is not reachable: {0}")]
    Unreachable(String),

    #[error("Origin API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },

    #[error("Failed to parse Origin response: {0}")]
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
}
