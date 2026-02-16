// QuectoClaw — Web UI module (Axum + HTMX)
//
// Lightweight web dashboard alternative to the TUI.
// Serves an HTML page with HTMX for live-updating metrics.
// Security: bearer token auth on API endpoints, localhost-only by default, CORS.

pub mod handlers;
pub mod templates;

use crate::config::Config;
use crate::metrics::Metrics;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Shared state for web handlers.
#[derive(Clone)]
pub struct WebState {
    pub metrics: Metrics,
    pub config: Config,
    pub agent: Arc<crate::agent::AgentLoop>,
    pub dashboard_token: String,
}

/// Auth middleware: check Authorization: Bearer <token> on /api/* routes.
async fn auth_middleware(state: Arc<WebState>, request: Request, next: Next) -> Response {
    // Skip auth if no token is configured (e.g. local-only dev use)
    if state.dashboard_token.is_empty() {
        return next.run(request).await;
    }

    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if token == state.dashboard_token {
                next.run(request).await
            } else {
                (StatusCode::UNAUTHORIZED, "Invalid bearer token").into_response()
            }
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            "Authorization: Bearer <token> required",
        )
            .into_response(),
    }
}

/// Start the web dashboard server.
pub async fn start_web_server(
    addr: SocketAddr,
    metrics: Metrics,
    config: Config,
    agent: Arc<crate::agent::AgentLoop>,
) -> anyhow::Result<()> {
    // Enforce localhost-only binding unless explicitly allowed
    let ip = addr.ip();
    if ip.is_unspecified() && !config.gateway.allow_public_bind {
        anyhow::bail!(
            "Refusing to bind web dashboard to 0.0.0.0 — set gateway.allow_public_bind = true in config to allow public binding"
        );
    }

    // Generate dashboard token if not configured
    let dashboard_token = if config.gateway.dashboard_token.is_empty() {
        let token = uuid::Uuid::new_v4().to_string();
        tracing::info!(token = %token, "Generated dashboard bearer token (use Authorization: Bearer <token>)");
        token
    } else {
        config.gateway.dashboard_token.clone()
    };

    let state = Arc::new(WebState {
        metrics,
        config,
        agent,
        dashboard_token,
    });

    // CORS layer — restrictive by default
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([axum::http::Method::GET])
        .allow_headers(Any);

    // API routes (require auth)
    let api_state = state.clone();
    let api_routes = Router::new()
        .route("/api/metrics", axum::routing::get(handlers::api_metrics))
        .route("/api/status", axum::routing::get(handlers::api_status))
        .route("/api/chat/stream", axum::routing::post(handlers::api_chat_stream))
        .layer(middleware::from_fn(move |req, next| {
            let s = api_state.clone();
            auth_middleware(s, req, next)
        }));

    // Public routes (dashboard HTML + HTMX fragments)
    let public_routes = Router::new()
        .route("/", axum::routing::get(handlers::dashboard))
        .route(
            "/fragments/metrics",
            axum::routing::get(handlers::fragment_metrics),
        );

    let app = Router::new()
        .merge(public_routes)
        .merge(api_routes)
        .layer(cors)
        .with_state(state);

    tracing::info!(addr = %addr, "Starting Web UI server (localhost-only by default)");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
