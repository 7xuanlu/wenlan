// SPDX-License-Identifier: AGPL-3.0-only
//! Native startup boundary shared by initial connection and process recovery.
use super::store::{Profile, Store, StoreError};
use super::{ConnectorCandidate, RelayClient, RelayError};
use serde::Deserialize;

pub const TOKEN_ENV: &str = "WENLAN_REMOTE_MCP_TOKEN";

pub async fn storage<T: Send + 'static>(
    operation: impl FnOnce(Store) -> Result<T, StoreError> + Send + 'static,
) -> Result<T, String> {
    // Resolve the identity path before moving into the blocking pool.
    let store = Store::current();
    tokio::task::spawn_blocking(move || operation(store))
        .await
        .map_err(|_| "Remote access storage task failed".to_string())?
        .map_err(|error| error.to_string())
}

pub async fn enabled_profile() -> Result<Profile, String> {
    storage(|store| require_enabled(store.load()?)).await
}

pub async fn consent_profile(expected_revision: String) -> Result<Profile, String> {
    consent_profile_at(Store::current(), expected_revision).await
}

async fn consent_profile_at(store: Store, expected_revision: String) -> Result<Profile, String> {
    tokio::task::spawn_blocking(move || {
        let profile = require_enabled(store.load()?)?;
        if profile.revision() != expected_revision || profile.device().is_none() {
            return Err(StoreError::Stale);
        }
        Ok(profile)
    })
    .await
    .map_err(|_| "Remote access storage task failed".to_string())?
    .map_err(|error| error.to_string())
}

/// Re-read both local consent and the server intent immediately before approval.
/// A frontend-provided label or a stale inspection cannot authorize another client.
pub async fn approve_inspected(
    expected_revision: String,
    inspected: super::PairingView,
) -> Result<(), String> {
    let client = RelayClient::new().map_err(|e| e.to_string())?;
    approve_inspected_at(Store::current(), client, expected_revision, inspected).await
}

async fn approve_inspected_at(
    store: Store,
    client: RelayClient,
    expected_revision: String,
    inspected: super::PairingView,
) -> Result<(), String> {
    let profile = consent_profile_at(store.clone(), expected_revision.clone()).await?;
    let device = profile
        .device()
        .ok_or_else(|| StoreError::Stale.to_string())?;
    let current = client
        .inspect_pairing(device, &inspected.pairing_id)
        .await
        .map_err(|e| e.to_string())?;
    if current != inspected {
        return Err(StoreError::Stale.to_string());
    }
    consent_profile_at(store, expected_revision).await?;
    client
        .approve_pairing(device, &current, profile.space())
        .await
        .map_err(|e| e.to_string())
}

fn require_enabled(profile: Option<Profile>) -> Result<Profile, StoreError> {
    profile
        .filter(Profile::enabled)
        .ok_or(StoreError::NotConfigured)
}

pub fn mcp_args(origin_url: &str, port: u16) -> Vec<String> {
    [
        "--origin-url".to_string(),
        origin_url.to_string(),
        "serve".to_string(),
        "--host".to_string(),
        "127.0.0.1".to_string(),
        "--port".to_string(),
        port.to_string(),
        "--tool-profile".to_string(),
        "query-only".to_string(),
        "--token-env".to_string(),
        TOKEN_ENV.to_string(),
        "--agent-name".to_string(),
        "remote-mcp".to_string(),
    ]
    .into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConnectorInfo {
    contract_version: u32,
    server: String,
    tool_profile: String,
    space: String,
    authentication: String,
}

/// Probe the protected contract, not only a public /health response. Never
/// follow a redirect with the backend credential or report raw response text.
pub async fn verify_backend(port: u16, profile: &Profile) -> Result<(), RelayError> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|_| RelayError::Unavailable)?;
    let url = format!("http://127.0.0.1:{port}/connector-info");
    let mut response = client
        .get(&url)
        .bearer_auth(profile.backend_token())
        .send()
        .await
        .map_err(|_| RelayError::Unavailable)?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(RelayError::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| RelayError::Unavailable)?
    {
        if bytes.len() + chunk.len() > 4096 {
            return Err(RelayError::InvalidResponse);
        }
        bytes.extend(chunk);
    }
    let info: ConnectorInfo =
        serde_json::from_slice(&bytes).map_err(|_| RelayError::InvalidResponse)?;
    if info.contract_version != 1
        || info.server != "wenlan-mcp"
        || info.tool_profile != "query-only"
        || info.space != profile.space()
        || info.authentication != "bearer"
    {
        return Err(RelayError::InvalidResponse);
    }
    let unauthenticated = client
        .get(url)
        .send()
        .await
        .map_err(|_| RelayError::Unavailable)?;
    if unauthenticated.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Err(RelayError::InvalidResponse);
    }
    Ok(())
}

/// A failed or stale registration never returns a direct-tunnel fallback.
/// Keep this future alive through enrollment persistence/cleanup even if the
/// UI cancels startup. The caller owns stopping the transport on cancellation.
pub async fn register(profile: Profile, tunnel_url: String) -> Result<(), String> {
    register_at(
        Store::current(),
        RelayClient::new().map_err(|e| e.to_string())?,
        profile,
        tunnel_url,
    )
    .await
}

#[derive(Debug, thiserror::Error)]
pub enum RenewalError {
    #[error("{0}")]
    Profile(String),
    #[error(transparent)]
    Relay(#[from] RelayError),
}

/// Renew only an already enrolled device. Recovery must never silently enroll
/// a replacement device or rotate credentials and invalidate existing consent.
pub async fn renew(tunnel_url: String) -> Result<(), RenewalError> {
    let profile = enabled_profile().await.map_err(RenewalError::Profile)?;
    renew_at(Store::current(), RelayClient::new()?, profile, tunnel_url).await
}

async fn renew_at(
    store: Store,
    client: RelayClient,
    profile: Profile,
    tunnel_url: String,
) -> Result<(), RenewalError> {
    same_profile(&store, &profile)
        .await
        .map_err(RenewalError::Profile)?;
    let device = profile
        .device()
        .ok_or_else(|| RenewalError::Profile(StoreError::Stale.to_string()))?;
    client
        .refresh(
            device,
            &ConnectorCandidate {
                tunnel_origin: tunnel_url,
                backend_token: profile.backend_token().into(),
                space: profile.space().into(),
            },
        )
        .await?;
    same_profile(&store, &profile)
        .await
        .map_err(RenewalError::Profile)
}

async fn same_profile(store: &Store, expected: &Profile) -> Result<(), String> {
    let store = store.clone();
    let revision = expected.revision().to_string();
    tokio::task::spawn_blocking(move || {
        let current = require_enabled(store.load()?)?;
        if current.revision() != revision {
            return Err(StoreError::Stale);
        }
        Ok(())
    })
    .await
    .map_err(|_| "Remote access storage task failed".to_string())?
    .map_err(|e| e.to_string())
}

async fn register_at(
    store: Store,
    client: RelayClient,
    profile: Profile,
    tunnel_url: String,
) -> Result<(), String> {
    same_profile(&store, &profile).await?;
    let candidate = ConnectorCandidate {
        tunnel_origin: tunnel_url,
        backend_token: profile.backend_token().into(),
        space: profile.space().into(),
    };
    if let Some(device) = profile.device() {
        client
            .refresh(device, &candidate)
            .await
            .map_err(|e| e.to_string())?;
        same_profile(&store, &profile).await?;
    } else {
        let device = client.enroll(&candidate).await.map_err(|e| e.to_string())?;
        let stored_device = device.clone();
        let revision = profile.revision().to_string();
        let saved =
            tokio::task::spawn_blocking(move || store.attach_device(&revision, stored_device))
                .await
                .map_err(|_| "Remote access storage task failed".to_string())
                .and_then(|result| result.map_err(|e| e.to_string()));
        if let Err(error) = saved {
            // A stale completion must not enable the new profile. The caller
            // closes the protected backend even if the network revoke fails.
            let _ = client.revoke_device(&device).await;
            return Err(error);
        }
    }
    Ok(())
}

/// Prepare durable stop intent without discarding credentials on write failure.
/// The caller must stop transport before finishing, even if preparation fails.
pub(crate) async fn prepare_disconnect(
    expected_revision: Option<String>,
) -> Result<super::disconnect::StopPlan, String> {
    let store = Store::current();
    tokio::task::spawn_blocking(move || {
        super::disconnect::prepare(&store, expected_revision.as_deref())
    })
    .await
    .map_err(|_| {
        "Local transport stop requested, but saved settings and server revocation are unconfirmed. Retry disconnect before restarting the App.".to_string()
    })
}

pub(crate) async fn finish_prepared_disconnect(
    plan: super::disconnect::StopPlan,
) -> Result<(), String> {
    super::disconnect::finish(
        Store::current(),
        RelayClient::new().map_err(|e| e.to_string())?,
        plan,
    )
    .await
}

#[cfg(test)]
mod tests;
