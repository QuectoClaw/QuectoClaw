// QuectoClaw — Metrics and observability.
//
// Lightweight in-process metrics for tracking token usage, response times,
// tool success rates, and request counts. Exposes a simple report API.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Global metrics collector.
#[derive(Clone)]
pub struct Metrics {
    inner: Arc<RwLock<MetricsInner>>,
    start_time: Instant,
}

#[derive(Default)]
struct MetricsInner {
    /// Total LLM API requests.
    llm_requests: u64,
    /// Total prompt tokens consumed.
    prompt_tokens: u64,
    /// Total completion tokens generated.
    completion_tokens: u64,
    /// Total LLM response time in milliseconds.
    llm_total_ms: u64,
    /// Per-tool call counts.
    tool_calls: HashMap<String, u64>,
    /// Per-tool error counts.
    tool_errors: HashMap<String, u64>,
    /// Per-tool cumulative duration in ms.
    tool_duration_ms: HashMap<String, u64>,
    /// Per-model request counts.
    model_requests: HashMap<String, u64>,
    /// Channel message counts.
    channel_messages: HashMap<String, u64>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MetricsInner::default())),
            start_time: Instant::now(),
        }
    }

    /// Record an LLM call.
    pub async fn record_llm_call(
        &self,
        model: &str,
        prompt_tokens: usize,
        completion_tokens: usize,
        duration: Duration,
    ) {
        let mut m = self.inner.write().await;
        m.llm_requests += 1;
        m.prompt_tokens += prompt_tokens as u64;
        m.completion_tokens += completion_tokens as u64;
        m.llm_total_ms += duration.as_millis() as u64;
        *m.model_requests.entry(model.to_string()).or_insert(0) += 1;
    }

    /// Record a tool execution.
    pub async fn record_tool_call(&self, name: &str, success: bool, duration: Duration) {
        let mut m = self.inner.write().await;
        *m.tool_calls.entry(name.to_string()).or_insert(0) += 1;
        *m.tool_duration_ms.entry(name.to_string()).or_insert(0) += duration.as_millis() as u64;
        if !success {
            *m.tool_errors.entry(name.to_string()).or_insert(0) += 1;
        }
    }

    /// Record a channel message.
    pub async fn record_channel_message(&self, channel: &str) {
        let mut m = self.inner.write().await;
        *m.channel_messages.entry(channel.to_string()).or_insert(0) += 1;
    }

    /// Get uptime.
    pub fn uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Generate a full metrics report as structured text.
    pub async fn report(&self) -> MetricsReport {
        let m = self.inner.read().await;
        let uptime = self.uptime();

        let total_tool_calls: u64 = m.tool_calls.values().sum();
        let total_tool_errors: u64 = m.tool_errors.values().sum();
        let avg_llm_ms = if m.llm_requests > 0 {
            m.llm_total_ms / m.llm_requests
        } else {
            0
        };

        let mut tool_stats: Vec<ToolStat> = m
            .tool_calls
            .iter()
            .map(|(name, &count)| {
                let errors = m.tool_errors.get(name).copied().unwrap_or(0);
                let total_ms = m.tool_duration_ms.get(name).copied().unwrap_or(0);
                let avg_ms = if count > 0 { total_ms / count } else { 0 };
                ToolStat {
                    name: name.clone(),
                    calls: count,
                    errors,
                    avg_ms,
                }
            })
            .collect();
        tool_stats.sort_by(|a, b| b.calls.cmp(&a.calls));

        MetricsReport {
            uptime_secs: uptime.as_secs(),
            llm_requests: m.llm_requests,
            prompt_tokens: m.prompt_tokens,
            completion_tokens: m.completion_tokens,
            total_tokens: m.prompt_tokens + m.completion_tokens,
            avg_llm_ms,
            total_tool_calls,
            total_tool_errors,
            tool_stats,
            model_requests: m.model_requests.clone(),
            channel_messages: m.channel_messages.clone(),
        }
    }

    /// Format report as a displayable string.
    pub async fn format_report(&self) -> String {
        let r = self.report().await;
        let mut out = String::new();

        let hours = r.uptime_secs / 3600;
        let mins = (r.uptime_secs % 3600) / 60;
        let secs = r.uptime_secs % 60;

        out.push_str(&format!(
            "═══ QuectoClaw Metrics ═══\n\
             Uptime:       {:02}:{:02}:{:02}\n\
             LLM Requests: {}\n\
             Tokens:       {} (prompt: {}, completion: {})\n\
             Avg Latency:  {}ms\n\
             Tool Calls:   {} ({} errors)\n",
            hours,
            mins,
            secs,
            r.llm_requests,
            r.total_tokens,
            r.prompt_tokens,
            r.completion_tokens,
            r.avg_llm_ms,
            r.total_tool_calls,
            r.total_tool_errors,
        ));

        if !r.tool_stats.is_empty() {
            out.push_str("\n─── Tool Breakdown ───\n");
            for ts in &r.tool_stats {
                out.push_str(&format!(
                    "  {:<20} {:>4} calls  {:>3} err  {:>4}ms avg\n",
                    ts.name, ts.calls, ts.errors, ts.avg_ms,
                ));
            }
        }

        if !r.model_requests.is_empty() {
            out.push_str("\n─── Models ───\n");
            for (model, count) in &r.model_requests {
                out.push_str(&format!("  {:<30} {:>4} requests\n", model, count));
            }
        }

        if !r.channel_messages.is_empty() {
            out.push_str("\n─── Channels ───\n");
            for (ch, count) in &r.channel_messages {
                out.push_str(&format!("  {:<20} {:>4} messages\n", ch, count));
            }
        }

        out
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Structured metrics report.
#[derive(Debug, Clone)]
pub struct MetricsReport {
    pub uptime_secs: u64,
    pub llm_requests: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub avg_llm_ms: u64,
    pub total_tool_calls: u64,
    pub total_tool_errors: u64,
    pub tool_stats: Vec<ToolStat>,
    pub model_requests: HashMap<String, u64>,
    pub channel_messages: HashMap<String, u64>,
}

/// Per-tool statistics.
#[derive(Debug, Clone)]
pub struct ToolStat {
    pub name: String,
    pub calls: u64,
    pub errors: u64,
    pub avg_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_recording() {
        let metrics = Metrics::new();

        metrics
            .record_llm_call("gpt-4", 100, 50, Duration::from_millis(500))
            .await;
        metrics
            .record_llm_call("gpt-4", 200, 100, Duration::from_millis(300))
            .await;
        metrics
            .record_tool_call("read_file", true, Duration::from_millis(10))
            .await;
        metrics
            .record_tool_call("exec", false, Duration::from_millis(5000))
            .await;
        metrics.record_channel_message("telegram").await;

        let report = metrics.report().await;
        assert_eq!(report.llm_requests, 2);
        assert_eq!(report.prompt_tokens, 300);
        assert_eq!(report.completion_tokens, 150);
        assert_eq!(report.total_tokens, 450);
        assert_eq!(report.avg_llm_ms, 400);
        assert_eq!(report.total_tool_calls, 2);
        assert_eq!(report.total_tool_errors, 1);
    }

    #[tokio::test]
    async fn test_metrics_report_format() {
        let metrics = Metrics::new();
        metrics
            .record_llm_call("gpt-4o", 50, 25, Duration::from_millis(200))
            .await;

        let text = metrics.format_report().await;
        assert!(text.contains("QuectoClaw Metrics"));
        assert!(text.contains("LLM Requests: 1"));
        assert!(text.contains("Tokens:       75"));
    }
}
