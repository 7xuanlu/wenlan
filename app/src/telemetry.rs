// SPDX-License-Identifier: AGPL-3.0-only
//! Tauri commands for the daemon-owned, opt-in product telemetry setting.
//!
//! The daemon is the source of truth for consent and the local outbox. The
//! app only proxies the typed status and mutation through `WenlanClient`.

use crate::api::WenlanClient;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// The daemon's telemetry consent/status response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryStatus {
    pub enabled: bool,
    pub available: bool,
    pub pending_operations: u64,
}

#[derive(Debug, Clone, Serialize)]
struct SetTelemetryEnabledRequest {
    enabled: bool,
}

impl WenlanClient {
    /// Read the daemon-owned telemetry consent and queue status.
    pub async fn get_telemetry_status(&self) -> Result<TelemetryStatus, String> {
        self.get_json("/api/telemetry").await
    }

    /// Update telemetry consent and return the daemon's resulting status.
    pub async fn set_telemetry_enabled(&self, enabled: bool) -> Result<TelemetryStatus, String> {
        self.put_json("/api/telemetry", &SetTelemetryEnabledRequest { enabled })
            .await
    }
}

type State = Arc<RwLock<AppState>>;

/// Read telemetry status from the selected daemon.
#[tauri::command]
pub async fn get_telemetry_status(
    state: tauri::State<'_, State>,
) -> Result<TelemetryStatus, String> {
    let client = { state.read().await.client.clone() };
    client.get_telemetry_status().await
}

/// Persist telemetry consent through the selected daemon.
#[tauri::command]
pub async fn set_telemetry_enabled(
    state: tauri::State<'_, State>,
    enabled: bool,
) -> Result<TelemetryStatus, String> {
    let client = { state.read().await.client.clone() };
    client.set_telemetry_enabled(enabled).await
}
