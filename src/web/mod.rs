// QuectoClaw — Web UI module (Axum + HTMX)
//
// Lightweight web dashboard alternative to the TUI.
// Serves an HTML page with HTMX for live-updating metrics.

pub mod handlers;
pub mod templates;

use crate::config::Config;
use crate::metrics::Metrics;
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;

/// Shared state for web handlers.
#[derive(Clone)]
pub struct WebState {
    pub metrics: Metrics,
    pub config: Config,
}

/// Start the web dashboard server.
pub async fn start_web_server(
    addr: SocketAddr,
    metrics: Metrics,
    config: Config,
) -> anyhow::Result<()> {
    let state = Arc::new(WebState { metrics, config });

    let app = Router::new()
        .route("/", axum::routing::get(handlers::dashboard))
        .route("/api/metrics", axum::routing::get(handlers::api_metrics))
        .route("/api/status", axum::routing::get(handlers::api_status))
        .route(
            "/fragments/metrics",
            axum::routing::get(handlers::fragment_metrics),
        )
        .with_state(state);

    tracing::info!(addr = %addr, "Starting Web UI server");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
