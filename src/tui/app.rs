// QuectoClaw — TUI application state and event loop.

use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Maximum log entries kept in memory.
const MAX_LOG_ENTRIES: usize = 200;

/// Events that drive the TUI state.
#[derive(Debug, Clone)]
pub enum TuiEvent {
    /// A tool was called.
    ToolCall {
        tool: String,
        session: String,
        timestamp: String,
    },
    /// A tool finished executing.
    ToolResult {
        tool: String,
        success: bool,
        duration_ms: u64,
    },
    /// An LLM request was made.
    LlmRequest { model: String, messages: usize },
    /// An LLM response was received.
    LlmResponse { tokens: usize, duration_ms: u64 },
    /// An inbound message arrived on a channel.
    ChannelMessage { channel: String, sender: String },
    /// A generic log line.
    Log(String),
    /// Agent iteration count update.
    Iteration(usize),
}

/// Shared application state for the TUI.
#[derive(Clone)]
pub struct TuiState {
    inner: Arc<RwLock<TuiStateInner>>,
}

struct TuiStateInner {
    logs: VecDeque<LogEntry>,
    active_sessions: Vec<SessionInfo>,
    stats: DashboardStats,
    is_running: bool,
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
    Tool,
    Llm,
}

impl LogLevel {
    pub fn symbol(&self) -> &str {
        match self {
            LogLevel::Info => "ℹ",
            LogLevel::Warn => "⚠",
            LogLevel::Error => "✖",
            LogLevel::Tool => "⚙",
            LogLevel::Llm => "🤖",
            LogLevel::Debug => "🔍",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    pub key: String,
    pub channel: String,
    pub messages: usize,
    pub last_activity: String,
}

#[derive(Debug, Clone, Default)]
pub struct DashboardStats {
    pub total_requests: u64,
    pub total_tokens: u64,
    pub total_tool_calls: u64,
    pub tool_errors: u64,
    pub uptime_secs: u64,
    pub active_channels: usize,
}

impl TuiState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(TuiStateInner {
                logs: VecDeque::with_capacity(MAX_LOG_ENTRIES),
                active_sessions: Vec::new(),
                stats: DashboardStats::default(),
                is_running: true,
            })),
        }
    }

    /// Synchronous version for use in tracing layers
    pub fn push_log_sync(&self, level: LogLevel, message: String) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut inner = inner.write().await;
            if inner.logs.len() >= MAX_LOG_ENTRIES {
                inner.logs.pop_front();
            }
            inner.logs.push_back(LogEntry {
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                level,
                message,
            });
        });
    }

    pub async fn push_log(&self, level: LogLevel, message: impl Into<String>) {
        let mut inner = self.inner.write().await;
        if inner.logs.len() >= MAX_LOG_ENTRIES {
            inner.logs.pop_front();
        }
        inner.logs.push_back(LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            level,
            message: message.into(),
        });
    }

    pub async fn get_logs(&self) -> Vec<LogEntry> {
        self.inner.read().await.logs.iter().cloned().collect()
    }

    pub async fn get_stats(&self) -> DashboardStats {
        self.inner.read().await.stats.clone()
    }

    pub async fn get_sessions(&self) -> Vec<SessionInfo> {
        self.inner.read().await.active_sessions.clone()
    }

    pub async fn is_running(&self) -> bool {
        self.inner.read().await.is_running
    }

    pub async fn stop(&self) {
        self.inner.write().await.is_running = false;
    }

    /// Process a TUI event and update state accordingly.
    pub async fn handle_event(&self, event: TuiEvent) {
        let mut inner = self.inner.write().await;

        match event {
            TuiEvent::ToolCall {
                tool,
                session,
                timestamp,
            } => {
                inner.stats.total_tool_calls += 1;
                // Update or add session
                if let Some(s) = inner.active_sessions.iter_mut().find(|s| s.key == session) {
                    s.last_activity = timestamp.clone();
                    s.messages += 1;
                } else {
                    inner.active_sessions.push(SessionInfo {
                        key: session.clone(),
                        channel: "cli".into(),
                        messages: 1,
                        last_activity: timestamp.clone(),
                    });
                }
                // Log
                if inner.logs.len() >= MAX_LOG_ENTRIES {
                    inner.logs.pop_front();
                }
                inner.logs.push_back(LogEntry {
                    timestamp,
                    level: LogLevel::Tool,
                    message: format!("⚙ {} ({})", tool, session),
                });
            }
            TuiEvent::ToolResult {
                tool,
                success,
                duration_ms,
            } => {
                if !success {
                    inner.stats.tool_errors += 1;
                }
                if inner.logs.len() >= MAX_LOG_ENTRIES {
                    inner.logs.pop_front();
                }
                let status = if success { "✓" } else { "✗" };
                inner.logs.push_back(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: if success {
                        LogLevel::Info
                    } else {
                        LogLevel::Error
                    },
                    message: format!("{} {} ({}ms)", status, tool, duration_ms),
                });
            }
            TuiEvent::LlmRequest { model, messages } => {
                inner.stats.total_requests += 1;
                if inner.logs.len() >= MAX_LOG_ENTRIES {
                    inner.logs.pop_front();
                }
                inner.logs.push_back(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Llm,
                    message: format!("→ {} ({} msgs)", model, messages),
                });
            }
            TuiEvent::LlmResponse {
                tokens,
                duration_ms,
            } => {
                inner.stats.total_tokens += tokens as u64;
                if inner.logs.len() >= MAX_LOG_ENTRIES {
                    inner.logs.pop_front();
                }
                inner.logs.push_back(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Llm,
                    message: format!("← {} tokens ({}ms)", tokens, duration_ms),
                });
            }
            TuiEvent::ChannelMessage { channel, sender } => {
                if inner.logs.len() >= MAX_LOG_ENTRIES {
                    inner.logs.pop_front();
                }
                inner.logs.push_back(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Info,
                    message: format!("📨 {} from {}", channel, sender),
                });
            }
            TuiEvent::Log(msg) => {
                if inner.logs.len() >= MAX_LOG_ENTRIES {
                    inner.logs.pop_front();
                }
                inner.logs.push_back(LogEntry {
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    level: LogLevel::Info,
                    message: msg,
                });
            }
            TuiEvent::Iteration(_) => {
                // Just a heartbeat, ignore for now
            }
        }
    }

    pub async fn update_uptime(&self, secs: u64) {
        self.inner.write().await.stats.uptime_secs = secs;
    }

    pub async fn set_active_channels(&self, count: usize) {
        self.inner.write().await.stats.active_channels = count;
    }
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tracing Integration
// ---------------------------------------------------------------------------

pub struct TuiLayer {
    state: TuiState,
}

impl TuiLayer {
    pub fn new(state: TuiState) -> Self {
        Self { state }
    }
}

impl<S> tracing_subscriber::Layer<S> for TuiLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = LogVisitor::default();
        event.record(&mut visitor);

        let level = match *event.metadata().level() {
            tracing::Level::ERROR => LogLevel::Error,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::INFO => LogLevel::Info,
            _ => LogLevel::Debug,
        };

        if !visitor.message.is_empty() {
            self.state.push_log_sync(level, visitor.message);
        }
    }
}

#[derive(Default)]
struct LogVisitor {
    message: String,
}

impl tracing::field::Visit for LogVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}
