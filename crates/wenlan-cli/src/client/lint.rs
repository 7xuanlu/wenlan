// SPDX-License-Identifier: Apache-2.0
use super::{WenlanClient, DEFAULT_HOST};
use anyhow::{Context, Result};
use wenlan_types::lint::{LintQuery, LintReport};

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
            .with_context(|| format!("GET {url} failed"))?
            .error_for_status()
            .with_context(|| format!("daemon returned error for {url}"))?;
        response.json().await.context("parsing /api/lint response")
    }
}
