// QuectoClaw — Web UI handlers

use super::WebState;
use axum::extract::{Query, State};
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse, Json};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Escape HTML special characters to prevent XSS.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Dashboard HTML page (full page with HTMX).
pub async fn dashboard(State(_state): State<Arc<WebState>>) -> Html<String> {
    Html(super::templates::DASHBOARD_HTML.to_string())
}

/// JSON metrics API endpoint.
pub async fn api_metrics(State(state): State<Arc<WebState>>) -> Json<serde_json::Value> {
    let report = state.metrics.report().await;

    Json(serde_json::json!({
        "uptime_secs": report.uptime_secs,
        "llm_requests": report.llm_requests,
        "prompt_tokens": report.prompt_tokens,
        "completion_tokens": report.completion_tokens,
        "total_tokens": report.total_tokens,
        "avg_llm_ms": report.avg_llm_ms,
        "total_tool_calls": report.total_tool_calls,
        "total_tool_errors": report.total_tool_errors,
        "total_cost": report.total_cost,
        "model_requests": report.model_requests,
        "model_costs": report.model_costs,
        "tool_stats": report.tool_stats.iter().map(|ts| {
            serde_json::json!({
                "name": ts.name,
                "calls": ts.calls,
                "errors": ts.errors,
                "avg_ms": ts.avg_ms,
            })
        }).collect::<Vec<_>>(),
        "channel_messages": report.channel_messages,
    }))
}

/// JSON status endpoint.
pub async fn api_status(State(state): State<Arc<WebState>>) -> Json<serde_json::Value> {
    let provider = state.config.resolve_provider();
    let model = &state.config.agents.defaults.model;

    Json(serde_json::json!({
        "status": "online",
        "version": crate::VERSION,
        "model": model,
        "provider": provider.as_ref().map(|(_, _, n)| n.as_str()).unwrap_or("none"),
        "workspace": state.config.agents.defaults.workspace,
        "routing_enabled": state.config.routing.enabled,
        "cost_tracking_enabled": state.config.cost.enabled,
        "budget_limit": state.config.cost.budget_limit,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub session: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatParams {
    pub compact: Option<bool>,
}

/// Streaming chat API (SSE).
pub async fn api_chat_stream(
    State(state): State<Arc<WebState>>,
    Query(_params): Query<ChatParams>,
    Json(payload): Json<ChatRequest>,
) -> impl IntoResponse {
    let agent = state.agent.clone();
    let message = payload.message;
    let session = payload.session.unwrap_or_else(|| "watch-default".to_string());

    let (tx, rx) = mpsc::channel(100);

    // Run agent in background task
    let agent_bg = agent.clone();
    let message_bg = message.clone();
    let session_bg = session.clone();
    let tx_bg = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = agent_bg.process_direct_streaming(&message_bg, &session_bg, tx_bg.clone()).await {
            let _ = tx_bg.send(crate::provider::StreamEvent::Error(e.to_string())).await;
        }
    });

    // Convert mpsc receiver to SSE stream
    let stream_session = session.clone();
    let stream = futures_util::stream::unfold((rx, stream_session), |(mut rx, session)| async move {
        while let Some(event) = rx.recv().await {
            match event {
                crate::provider::StreamEvent::Token(token) => {
                    let data = serde_json::json!({ "token": token }).to_string();
                    return Some((Ok::<Event, std::convert::Infallible>(Event::default().data(data)), (rx, session)));
                }
                crate::provider::StreamEvent::ToolCallDelta { name: Some(name), .. } => {
                    let data = serde_json::json!({ "tool": name }).to_string();
                    return Some((Ok::<Event, std::convert::Infallible>(Event::default().data(data)), (rx, session)));
                }
                crate::provider::StreamEvent::Done(resp) => {
                    let data = serde_json::json!({
                        "done": true,
                        "response": resp.content,
                        "session": session,
                        "model": resp.finish_reason,
                        "tokens_used": resp.usage.map(|u| u.total_tokens).unwrap_or(0)
                    }).to_string();
                    return Some((Ok::<Event, std::convert::Infallible>(Event::default().data(data)), (rx, session)));
                }
                crate::provider::StreamEvent::Error(err) => {
                    let data = serde_json::json!({ "error": err }).to_string();
                    return Some((Ok::<Event, std::convert::Infallible>(Event::default().data(data)), (rx, session)));
                }
                _ => {}
            }
        }
        None
    });

    Sse::new(stream)
}

/// HTMX partial fragment for live-updating metrics panel.
pub async fn fragment_metrics(State(state): State<Arc<WebState>>) -> Html<String> {
    let report = state.metrics.report().await;

    let hours = report.uptime_secs / 3600;
    let mins = (report.uptime_secs % 3600) / 60;
    let secs = report.uptime_secs % 60;

    let mut tool_rows = String::new();
    for ts in &report.tool_stats {
        tool_rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}ms</td></tr>",
            html_escape(&ts.name),
            ts.calls,
            ts.errors,
            ts.avg_ms
        ));
    }

    let mut model_rows = String::new();
    for (model, count) in &report.model_requests {
        let cost = report.model_costs.get(model).copied().unwrap_or(0.0);
        model_rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>${:.4}</td></tr>",
            html_escape(model),
            count,
            cost
        ));
    }

    let html = format!(
        r#"<div class="metrics-grid">
  <div class="metric-card">
    <span class="metric-value">{:02}:{:02}:{:02}</span>
    <span class="metric-label">Uptime</span>
  </div>
  <div class="metric-card">
    <span class="metric-value">{}</span>
    <span class="metric-label">LLM Requests</span>
  </div>
  <div class="metric-card">
    <span class="metric-value">{}</span>
    <span class="metric-label">Total Tokens</span>
  </div>
  <div class="metric-card">
    <span class="metric-value">{}ms</span>
    <span class="metric-label">Avg Latency</span>
  </div>
  <div class="metric-card">
    <span class="metric-value">{}</span>
    <span class="metric-label">Tool Calls</span>
  </div>
  <div class="metric-card">
    <span class="metric-value">${:.4}</span>
    <span class="metric-label">Total Cost</span>
  </div>
</div>

<div class="tables-row">
  <div class="table-section">
    <h3>Tool Breakdown</h3>
    <table>
      <thead><tr><th>Tool</th><th>Calls</th><th>Errors</th><th>Avg</th></tr></thead>
      <tbody>{}</tbody>
    </table>
  </div>

  <div class="table-section">
    <h3>Models</h3>
    <table>
      <thead><tr><th>Model</th><th>Requests</th><th>Cost</th></tr></thead>
      <tbody>{}</tbody>
    </table>
  </div>
</div>"#,
        hours,
        mins,
        secs,
        report.llm_requests,
        report.total_tokens,
        report.avg_llm_ms,
        report.total_tool_calls,
        report.total_cost,
        tool_rows,
        model_rows,
    );

    Html(html)
}
