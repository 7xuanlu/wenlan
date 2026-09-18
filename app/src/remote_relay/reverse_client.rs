// SPDX-License-Identifier: AGPL-3.0-only
//! Native control-plane calls for a reverse connector.

use super::{decode, now_ms, valid_id, valid_space, DeviceCredential, RelayClient, RelayError};
use reqwest::{Method, StatusCode};
use serde::{de::Deserializer, Deserialize, Serialize};

const MAX_JS_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReverseCandidate {
    pub backend_token: String,
    pub space: String,
}

impl std::fmt::Debug for ReverseCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ReverseCandidate([redacted])")
    }
}

impl ReverseCandidate {
    fn validate(&self) -> Result<(), RelayError> {
        if valid_id(&self.backend_token, 32) && valid_space(&self.space) {
            Ok(())
        } else {
            Err(RelayError::InvalidInput)
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedReverse {
    #[serde(flatten)]
    pub credential: DeviceCredential,
    pub pending_until: u64,
}

impl std::fmt::Debug for PreparedReverse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PreparedReverse([redacted])")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReverseStatus {
    connected: bool,
    #[serde(default, deserialize_with = "deserialize_generation")]
    generation: Option<u64>,
}

fn deserialize_generation<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    u64::deserialize(deserializer).map(Some)
}

fn valid_connection_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

impl RelayClient {
    pub async fn prepare_reverse(
        &self,
        candidate: &ReverseCandidate,
    ) -> Result<PreparedReverse, RelayError> {
        candidate.validate()?;
        let response = Self::send(
            self.request(Method::POST, "/devices/reverse", None)?
                .json(candidate),
        )
        .await?;
        if response.status() != StatusCode::CREATED {
            return Err(RelayError::InvalidResponse);
        }
        let prepared: PreparedReverse = decode(response).await?;
        prepared
            .credential
            .validate()
            .map_err(|_| RelayError::InvalidResponse)?;
        if prepared.pending_until <= now_ms()
            || prepared.pending_until > prepared.credential.expires_at
        {
            return Err(RelayError::InvalidResponse);
        }
        Ok(prepared)
    }

    pub async fn reverse_status(
        &self,
        credential: &DeviceCredential,
        connection_id: &str,
    ) -> Result<Option<u64>, RelayError> {
        if !valid_connection_id(connection_id) {
            return Err(RelayError::InvalidInput);
        }
        let response = Self::send(
            self.request(Method::GET, "/devices/reverse/status", Some(credential))?
                .header("x-wenlan-connection-id", connection_id),
        )
        .await?;
        if response.status() != StatusCode::OK {
            return Err(RelayError::InvalidResponse);
        }
        let status: ReverseStatus = decode(response).await?;
        match (status.connected, status.generation) {
            (false, None) => Ok(None),
            (true, Some(generation)) if generation <= MAX_JS_SAFE_INTEGER => Ok(Some(generation)),
            _ => Err(RelayError::InvalidResponse),
        }
    }
}
