use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::Request;
use axum::http::{HeaderName, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use rmcp::model::{ClientJsonRpcMessage, ClientRequest};
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tower_http::cors::CorsLayer;

use crate::auth;
use crate::client::WenlanClient;
use crate::lock_state;
use crate::tools::{ToolProfile, TransportMode, WenlanMcpServer};

pub const QUERY_ONLY_AUTH_ERROR: &str =
    "Query-only tool profile requires bearer token authentication; --no-auth is not allowed.";

pub const QUERY_ONLY_SPACE_ERROR: &str =
    "Query-only tool profile requires a strict WENLAN_SPACE pin; WENLAN_DEFAULT_SPACE alone does not satisfy it.";

#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub port: u16,
    pub host: String,
    pub origin_url: String,
    pub token: Option<String>,
    pub agent_name: String,
    pub user_id: Option<String>,
    pub allowed_origins: Vec<String>,
}

async fn health() -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "status": "ok",
        "server": "wenlan-mcp",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

pub async fn run_serve(config: ServeConfig) -> anyhow::Result<()> {
    run_serve_with_profile(config, ToolProfile::Standard).await
}

pub async fn run_serve_with_profile(
    config: ServeConfig,
    tool_profile: ToolProfile,
) -> anyhow::Result<()> {
    if tool_profile == ToolProfile::QueryOnly
        && !config
            .token
            .as_deref()
            .is_some_and(|token| !token.trim().is_empty())
    {
        anyhow::bail!(QUERY_ONLY_AUTH_ERROR);
    }
    // A Space pin scopes retrieval to one Space; it is a data-scoping control,
    // not authentication or per-caller tenant isolation.
    if tool_profile == ToolProfile::QueryOnly && lock_state::locked_space().is_none() {
        anyhow::bail!(QUERY_ONLY_SPACE_ERROR);
    }

    let client =
        WenlanClient::new(config.origin_url.clone()).with_agent_name(config.agent_name.clone());
    let agent_name = config.agent_name.clone();
    let user_id = config.user_id.clone();
    let token = config.token.clone();
    let allowed_origins = config.allowed_origins.clone();

    // rmcp's default allowed_hosts = [localhost, 127.0.0.1, ::1] is
    // DNS-rebinding protection for loopback deployments. Every serve
    // deployment we support is reached through a public tunnel
    // (cloudflared, ngrok) that forwards the tunnel hostname in Host,
    // with a separate backend bearer token. Leaving the default in
    // place rejects every tunneled request with a plain-text 403 the
    // upstream MCP proxy cannot parse, surfacing to users as a bogus
    // "-32600 Invalid Request". MCP's custom-header requirement
    // already triggers CORS preflight, so the Origin allowlist catches
    // browser-driven DNS rebinding; a local non-browser attacker
    // bypasses Host checking anyway by hitting 127.0.0.1 directly.
    let mut mcp_config = StreamableHttpServerConfig::default().disable_allowed_hosts();
    if tool_profile == ToolProfile::QueryOnly {
        // Let the SDK frame heartbeats between complete SSE events. Frequent
        // writes also expose a disconnected HTTP reader to the relay runtime.
        mcp_config = mcp_config.with_sse_keep_alive(Some(std::time::Duration::from_secs(1)));
    }

    let mcp_service = StreamableHttpService::new(
        move || {
            Ok(WenlanMcpServer::new(
                client.clone(),
                TransportMode::Http,
                agent_name.clone(),
                user_id.clone(),
            )
            .with_tool_profile(tool_profile))
        },
        Arc::new(LocalSessionManager::default()),
        mcp_config,
    );

    let cors = build_cors_layer(&config.allowed_origins);

    let mut router = Router::new()
        .nest_service("/mcp", mcp_service)
        .route("/health", get(health));

    if tool_profile == ToolProfile::QueryOnly {
        // Enrollment reads this only through the same bearer gate as MCP.
        // Never put the Space pin in the public health response.
        let space = lock_state::locked_space().expect("query-only Space checked above");
        router = router.route(
            "/connector-info",
            get(move || {
                let space = space.clone();
                async move {
                    (
                        [(http::header::CACHE_CONTROL, "no-store")],
                        axum::Json(serde_json::json!({
                            "contract_version": 1,
                            "server": "wenlan-mcp",
                            "tool_profile": "query-only",
                            "space": space,
                            "authentication": "bearer",
                        })),
                    )
                }
            }),
        );
        router = router.layer(middleware::from_fn(validate_initialization));
    }
    router = router.layer(cors);

    if let Some(ref expected_token) = token {
        let token_for_middleware = expected_token.clone();
        let origins_for_middleware = allowed_origins.clone();
        router = router.layer(middleware::from_fn(move |req: Request, next: Next| {
            let token = token_for_middleware.clone();
            let origins = origins_for_middleware.clone();
            async move { auth_and_origin_middleware(req, next, &token, &origins).await }
        }));
    }

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("wenlan-mcp HTTP server listening on {}", addr);

    if token.is_some() {
        tracing::info!("Bearer token authentication enabled");
    } else {
        tracing::warn!("Running without authentication — only safe on loopback");
    }

    let shutdown = async {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let ctrl_c = tokio::signal::ctrl_c();
            let mut sigterm =
                signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
            tokio::select! {
                _ = ctrl_c => {},
                _ = sigterm.recv() => {},
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
        }
        tracing::info!("Shutting down wenlan-mcp HTTP server");
    };

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await?;

    Ok(())
}

// rmcp 1.5 can allocate a session before rejecting a non-initialize message,
// without spawning the service task that normally removes that session.
// Reuse its typed parser before allocation on the public query-only surface.
async fn validate_initialization(req: Request, next: Next) -> axum::response::Response {
    let is_mcp = req.uri().path() == "/mcp" || req.uri().path().starts_with("/mcp/");
    let has_session = req
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .is_some();
    if !is_mcp || req.method() != Method::POST || has_session {
        return next.run(req).await;
    }
    let (parts, body) = req.into_parts();
    let bytes = match tokio::time::timeout(Duration::from_secs(5), to_bytes(body, 64 * 1024)).await
    {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                "MCP initialization too large",
            )
                .into_response()
        }
        Err(_) => {
            return (StatusCode::REQUEST_TIMEOUT, "MCP initialization timed out").into_response()
        }
    };
    let valid = match serde_json::from_slice::<ClientJsonRpcMessage>(&bytes) {
        Ok(ClientJsonRpcMessage::Request(rpc)) => {
            matches!(rpc.request, ClientRequest::InitializeRequest(_))
        }
        _ => false,
    };
    if !valid {
        return (StatusCode::BAD_REQUEST, "Initialize request required").into_response();
    }
    next.run(Request::from_parts(parts, Body::from(bytes)))
        .await
}

#[cfg(test)]
mod tests;

fn build_cors_layer(allowed_origins: &[String]) -> CorsLayer {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            http::header::AUTHORIZATION,
            http::header::CONTENT_TYPE,
            http::header::ACCEPT,
            HeaderName::from_static("mcp-session-id"),
            HeaderName::from_static("mcp-protocol-version"),
        ]);

    if allowed_origins.iter().any(|o| o == "*") {
        cors.allow_origin(tower_http::cors::Any)
    } else {
        let origins: Vec<http::HeaderValue> = allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        cors.allow_origin(origins)
    }
}

/// Auth middleware: bearer token first (401), then Origin header (403).
async fn auth_and_origin_middleware(
    req: Request,
    next: Next,
    expected_token: &str,
    allowed_origins: &[String],
) -> axum::response::Response {
    let is_preflight = req.method() == Method::OPTIONS;
    let is_health = req.uri().path() == "/health";
    if is_preflight || is_health {
        return next.run(req).await;
    }

    // 1. Validate bearer token FIRST
    let auth_header = req.headers().get(http::header::AUTHORIZATION);
    match auth_header {
        Some(value) => {
            let value_str = value.to_str().unwrap_or("");
            match value_str.strip_prefix("Bearer ") {
                Some(provided) if auth::verify_token(provided, expected_token) => {}
                _ => return (StatusCode::UNAUTHORIZED, "Invalid bearer token").into_response(),
            }
        }
        None => return (StatusCode::UNAUTHORIZED, "Authorization header required").into_response(),
    }

    // 2. Validate Origin header AFTER auth
    if let Some(origin) = req.headers().get(http::header::ORIGIN) {
        if let Ok(origin_str) = origin.to_str() {
            if !auth::is_origin_allowed(origin_str, allowed_origins) {
                return (StatusCode::FORBIDDEN, "Origin not allowed").into_response();
            }
        }
    }

    next.run(req).await
}
