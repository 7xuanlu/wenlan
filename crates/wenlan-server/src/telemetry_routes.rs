// SPDX-License-Identifier: Apache-2.0
//! Local-only opt-in telemetry consent/status endpoints.

use crate::error::ServerError;
use crate::route_registry::{get, TrackedRouter};
use crate::state::SharedState;
use crate::telemetry::TelemetryStatus;
use axum::{extract::State, response::Json};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateTelemetryRequest {
    pub enabled: bool,
}

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router.route(
        "/api/telemetry",
        get(handle_get_telemetry).put(handle_update_telemetry),
    )
}

/// GET /api/telemetry — report local consent, availability, and in-memory
/// operation count. No database or user data is read.
pub async fn handle_get_telemetry(
    State(state): State<SharedState>,
) -> Result<Json<TelemetryStatus>, ServerError> {
    let telemetry = { state.read().await.telemetry.clone() };
    Ok(Json(telemetry.status()))
}

/// PUT /api/telemetry — persist strict `{ "enabled": bool }` consent before
/// changing the in-memory send gate.
pub async fn handle_update_telemetry(
    State(state): State<SharedState>,
    Json(request): Json<UpdateTelemetryRequest>,
) -> Result<Json<TelemetryStatus>, ServerError> {
    let telemetry = { state.read().await.telemetry.clone() };
    let writer = telemetry.clone();
    tokio::task::spawn_blocking(move || writer.set_enabled(request.enabled))
        .await
        .map_err(|_| ServerError::Internal("persist telemetry consent failed".into()))?
        .map_err(|_| ServerError::Internal("persist telemetry consent failed".into()))?;
    Ok(Json(telemetry.status()))
}
