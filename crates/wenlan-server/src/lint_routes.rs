use crate::error::ServerError;
use crate::route_registry::{get, TrackedRouter};
use crate::runtime_observation::RuntimeObservationInput;
use crate::state::SharedState;
use axum::extract::{Query, State};
use axum::Json;
use std::sync::Arc;
use wenlan_core::lint::context::CancellationToken;
use wenlan_core::lint::runner::{LintRunError, LintRunner};
use wenlan_types::lint::{LintAgentSubmission, LintQuery, LintReport, LintRequestQuery};

pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router.route("/api/lint", get(handle_lint).post(handle_lint_submission))
}

async fn handle_lint(
    State(state): State<SharedState>,
    Query(request): Query<LintRequestQuery>,
) -> Result<Json<LintReport>, ServerError> {
    run_lint(state, request, None).await.map(Json)
}

async fn handle_lint_submission(
    State(state): State<SharedState>,
    Query(request): Query<LintRequestQuery>,
    Json(submission): Json<LintAgentSubmission>,
) -> Result<Json<LintReport>, ServerError> {
    if !request.agent_assist() {
        return Err(ServerError::ValidationError(
            "agent_assist_required_for_submission".to_string(),
        ));
    }
    run_lint(state, request, Some(submission)).await.map(Json)
}

async fn run_lint(
    state: SharedState,
    request: LintRequestQuery,
    submission: Option<LintAgentSubmission>,
) -> Result<LintReport, ServerError> {
    let query = request.lint();
    let deep = query.applied_profile() == wenlan_types::lint::LintProfile::Deep;
    if request.external_egress() && !deep {
        return Err(ServerError::ValidationError(
            "external_egress_requires_deep".to_string(),
        ));
    }
    if request.agent_assist() && !deep {
        return Err(ServerError::ValidationError(
            "agent_assist_requires_deep".to_string(),
        ));
    }
    let (db, config, runtime, lint_observer, semantic_provider) = {
        let state = state.read().await;
        let db = state.db.clone().ok_or(ServerError::DbNotInitialized)?;
        (
            db,
            state.lint_config.clone(),
            RuntimeObservationInput::capture(&state),
            Arc::clone(&state.lint_observer),
            select_semantic_provider(&state, query, request.external_egress()),
        )
    };
    let observation = runtime.observe().await;
    let runner = LintRunner::new(config.clock(), CancellationToken::new())
        .with_observer(lint_observer)
        .with_sources(config.sources())
        .with_runtime_observation(observation)
        .with_semantic_external_egress_enabled(request.external_egress())
        .with_semantic_provider(semantic_provider);
    let runner = match submission {
        Some(submission) => runner.with_semantic_agent_submission(submission),
        None if request.agent_assist() => runner.with_semantic_agent_assist(),
        None => runner,
    };
    let report = runner
        .run(&db, query, config.page_root(), config.page_root().is_some())
        .await
        .map_err(map_lint_error)?;
    Ok(report)
}

fn select_semantic_provider(
    state: &crate::state::ServerState,
    query: &LintQuery,
    external_egress: bool,
) -> Option<Arc<dyn wenlan_core::llm_provider::LlmProvider>> {
    if query.applied_profile() != wenlan_types::lint::LintProfile::Deep {
        return None;
    }
    if external_egress {
        for provider in [
            &state.synthesis_llm,
            &state.api_llm,
            &state.external_llm,
            &state.llm,
        ] {
            if provider
                .as_deref()
                .is_some_and(|provider| provider.is_available())
            {
                return provider.clone();
            }
        }
        None
    } else {
        state.llm.clone()
    }
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
