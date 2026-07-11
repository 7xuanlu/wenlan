// SPDX-License-Identifier: Apache-2.0
use super::{WenlanClient, DEFAULT_HOST};
use anyhow::{bail, Context, Result};
use wenlan_types::lint::{LintErrorResponse, LintQuery, LintReport};

const MAX_LINT_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

pub fn origin_host_from_env() -> String {
    std::env::var("WENLAN_HOST")
        .unwrap_or_else(|_| DEFAULT_HOST.to_string())
        .trim_end_matches('/')
        .to_string()
}

impl WenlanClient {
    pub async fn lint(&self, space: Option<String>) -> Result<LintReport> {
        let url = format!("{}/api/lint", self.base_url);
        let response = self
            .http
            .get(&url)
            .query(&LintQuery { space })
            .send()
            .await
            .with_context(|| format!("GET {url} failed"))?;
        let status = response.status();
        let body = read_lint_body(response, &url).await?;
        if !status.is_success() {
            if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
                if let Ok(error) = serde_json::from_slice::<LintErrorResponse>(&body) {
                    if error.error() == "invalid_scope" {
                        bail!(error.error().to_string());
                    }
                }
            }
            bail!("daemon returned HTTP {status} for {url}");
        }
        serde_json::from_slice(&body).context("parsing /api/lint response")
    }
}

async fn read_lint_body(mut response: reqwest::Response, url: &str) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_LINT_RESPONSE_BYTES as u64)
    {
        bail!("lint response exceeds {MAX_LINT_RESPONSE_BYTES} bytes");
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("reading daemon response for {url}"))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_LINT_RESPONSE_BYTES {
            bail!("lint response exceeds {MAX_LINT_RESPONSE_BYTES} bytes");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}
