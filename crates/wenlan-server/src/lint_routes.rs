use crate::error::ServerError;
use crate::route_registry::{get, TrackedRouter};
use crate::runtime_observation::RuntimeObservationInput;
use crate::state::SharedState;
use axum::extract::{Query, State};
use axum::Json;
use std::sync::Arc;
use wenlan_core::lint::context::CancellationToken;
use wenlan_core::lint::runner::{LintRunError, LintRunner};
use wenlan_types::lint::{LintQuery, LintReport};

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router.route("/api/lint", get(handle_lint))
}

async fn handle_lint(
    State(state): State<SharedState>,
    Query(query): Query<LintQuery>,
) -> Result<Json<LintReport>, ServerError> {
    let (db, config, runtime, lint_observer) = {
        let state = state.read().await;
        let db = state.db.clone().ok_or(ServerError::DbNotInitialized)?;
        (
            db,
            state.lint_config.clone(),
            RuntimeObservationInput::capture(&state),
            Arc::clone(&state.lint_observer),
        )
    };
    let observation = runtime.observe().await;
    let runner = LintRunner::new(config.clock(), CancellationToken::new())
        .with_observer(lint_observer)
        .with_sources(config.sources())
        .with_runtime_observation(observation);
    let report = runner
        .run(
            &db,
            &query,
            config.page_root(),
            config.page_root().is_some(),
        )
        .await
        .map_err(map_lint_error)?;
    Ok(Json(report))
}

fn map_lint_error(error: LintRunError) -> ServerError {
    match error {
        LintRunError::InvalidScope => ServerError::ValidationError(error.to_string()),
        LintRunError::CatalogMismatch
        | LintRunError::Snapshot(_)
        | LintRunError::PageScan(_)
        | LintRunError::Contract(_) => ServerError::Internal(error.to_string()),
    }
}
