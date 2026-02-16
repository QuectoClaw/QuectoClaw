// QuectoClaw — Agent loop (core orchestrator)

pub mod context;
pub mod gateway;
pub mod memory;

use crate::bus::{InboundMessage, MessageBus, OutboundMessage};
use crate::config::Config;
use crate::metrics::Metrics;
use crate::provider::router::ModelRouter;
use crate::provider::{LLMProvider, Message, ToolCall};
use crate::session::SessionManager;
use crate::tool::ToolRegistry;
use crate::tui::app::{TuiEvent, TuiState};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

struct InternalToolResult {
    tool_call_id: String,
    tool_name: String,
    output: String,
    success: bool,
    duration: std::time::Duration,
}

pub struct AgentLoop {
    config: Config,
    provider: Arc<dyn LLMProvider>,
    tools: ToolRegistry,
    sessions: SessionManager,
    bus: Arc<MessageBus>,
    workspace: String,
    metrics: Metrics,
    tui_state: Option<TuiState>,
    rate_limiter: Arc<RateLimiter>,
    router: ModelRouter,
}

struct RateLimiter {
    // Key: sender_id or session_key
    history: tokio::sync::Mutex<HashMap<String, Vec<std::time::Instant>>>,
    max_requests: u64,
    window: std::time::Duration,
}

impl RateLimiter {
    pub fn new(max_requests: u64, window_secs: u64) -> Self {
        Self {
            history: tokio::sync::Mutex::new(HashMap::new()),
            max_requests,
            window: std::time::Duration::from_secs(window_secs),
        }
    }

    pub async fn check_rate_limit(&self, key: &str) -> bool {
        if self.max_requests == 0 {
            return true; // No limit
        }

        let mut history = self.history.lock().await;
        let now = std::time::Instant::now();
        let entries = history.entry(key.to_string()).or_default();

        // 1. Remove old entries outside the window
        let window = self.window;
        entries.retain(|&t| now.duration_since(t) < window);

        // 2. Check if we're over the limit
        if entries.len() as u64 >= self.max_requests {
            return false;
        }

        // 3. Record this request
        entries.push(now);
        true
    }
}

impl AgentLoop {
    pub fn new(
        config: Config,
        provider: Arc<dyn LLMProvider>,
        tools: ToolRegistry,
        bus: Arc<MessageBus>,
    ) -> Self {
        let workspace = config
            .workspace_path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| "~/.quectoclaw/workspace".to_string());

        let sessions = SessionManager::new(Path::new(&workspace));

        let rate_limit_requests = config.gateway.rate_limit_requests;
        let rate_limit_seconds = config.gateway.rate_limit_seconds;

        let router = ModelRouter::new(
            if config.routing.enabled {
                config.routing.routes.clone()
            } else {
                vec![]
            },
            config.agents.defaults.model.clone(),
        );

        Self {
            config,
            provider,
            tools,
            sessions,
            bus,
            workspace,
            metrics: Metrics::new(),
            tui_state: None,
            rate_limiter: Arc::new(RateLimiter::new(rate_limit_requests, rate_limit_seconds)),
            router,
        }
    }

    pub fn set_tui_state(&mut self, state: TuiState) {
        self.tui_state = Some(state);
    }

    /// Process a direct one-shot message (CLI agent mode).
    pub async fn process_direct(&self, content: &str, session_key: &str) -> anyhow::Result<String> {
        self.run_agent_loop(content, session_key, true, None).await
    }

    /// Process a direct message with streaming output.
    /// The callback receives each token as it arrives.
    pub async fn process_direct_streaming(
        &self,
        content: &str,
        session_key: &str,
        token_tx: tokio::sync::mpsc::Sender<crate::provider::StreamEvent>,
    ) -> anyhow::Result<String> {
        self.run_agent_loop(content, session_key, true, Some(token_tx))
            .await
    }

    /// Process an inbound message from a channel.
    pub async fn process_message(&self, msg: InboundMessage) -> anyhow::Result<String> {
        let response = self
            .run_agent_loop(&msg.content, &msg.session_key, true, None)
            .await?;

        // Send response back via bus
        self.bus
            .publish_outbound(OutboundMessage {
                channel: msg.channel,
                chat_id: msg.chat_id,
                content: response.clone(),
                metadata: HashMap::new(),
            })
            .await;

        Ok(response)
    }

    /// Core agent loop: build context → call LLM → execute tools → repeat.
    pub async fn run_agent_loop(
        &self,
        user_message: &str,
        session_key: &str,
        use_history: bool,
        stream_tx: Option<tokio::sync::mpsc::Sender<crate::provider::StreamEvent>>,
    ) -> anyhow::Result<String> {
        let max_iterations = self.config.agents.defaults.max_tool_iterations;
        // Use router to select model based on message content
        let routed_model = self.router.resolve_model(user_message).to_string();
        let model = &routed_model;
        if self.router.has_routes() && *model != self.config.agents.defaults.model {
            tracing::info!(routed_model = %model, "Multi-model routing selected alternate model");
        }

        // 1. Build system prompt
        let system_prompt = context::build_system_prompt(&self.workspace, &self.tools).await;

        // 2. Build message history
        let mut messages = Vec::new();
        messages.push(Message::system(system_prompt));

        // Add existing summary if available
        let summary = self.sessions.get_summary(session_key).await;
        if !summary.is_empty() {
            messages.push(Message::system(format!(
                "Summary of earlier conversation:\n{}",
                summary
            )));
        }

        // Add session history
        if use_history {
            let history = self.sessions.get_messages(session_key).await;
            messages.extend(history);
        }

        // Add current user message
        let user_msg = Message::user(user_message);
        messages.push(user_msg.clone());
        self.sessions.add_message(session_key, user_msg).await;

        // 3. Get tool definitions
        let tool_defs = self.tools.get_definitions().await;

        // 4. LLM iteration loop
        let mut options = HashMap::new();
        options.insert(
            "temperature".to_string(),
            serde_json::Value::from(self.config.agents.defaults.temperature),
        );
        options.insert(
            "max_tokens".to_string(),
            serde_json::Value::from(self.config.agents.defaults.max_tokens),
        );
        options.insert(
            "max_retries".to_string(),
            serde_json::Value::from(self.config.agents.defaults.max_retries),
        );
        options.insert(
            "retry_delay_ms".to_string(),
            serde_json::Value::from(self.config.agents.defaults.retry_delay_ms),
        );

        let mut final_content = String::new();

        for iteration in 0..max_iterations {
            tracing::info!(
                iteration = iteration,
                messages = messages.len(),
                model = %model,
                "Running LLM iteration"
            );

            if let Some(tui) = &self.tui_state {
                tui.handle_event(TuiEvent::LlmRequest {
                    model: model.clone(),
                    messages: messages.len(),
                })
                .await;
            }

            let llm_start = std::time::Instant::now();

            // Call LLM (streaming on last iteration or when no tool calls expected)
            let response = if let Some(ref tx) = stream_tx {
                // Use streaming — tokens will be sent via tx
                let (done_tx, mut done_rx) = tokio::sync::mpsc::channel(256);
                let tx_clone = tx.clone();

                // Forward events and capture the Done response
                let fwd_tx = done_tx.clone();
                let provider = self.provider.clone();
                let msgs = messages.clone();
                let defs = tool_defs.clone();
                let mdl = model.to_string();
                let opts = options.clone();

                tokio::spawn(async move {
                    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel(256);
                    let _ = provider
                        .chat_stream(&msgs, &defs, &mdl, &opts, event_tx)
                        .await;

                    while let Some(event) = event_rx.recv().await {
                        match &event {
                            crate::provider::StreamEvent::Done(resp) => {
                                let _ = fwd_tx.send(resp.clone()).await;
                                let _ = tx_clone.send(event).await;
                                break;
                            }
                            _ => {
                                let _ = tx_clone.send(event).await;
                            }
                        }
                    }
                });

                done_rx
                    .recv()
                    .await
                    .ok_or_else(|| anyhow::anyhow!("Stream ended without response"))?
            } else {
                self.provider
                    .chat(&messages, &tool_defs, model, &options)
                    .await?
            };

            let llm_duration = llm_start.elapsed();

            if let Some(tui) = &self.tui_state {
                tui.handle_event(TuiEvent::LlmResponse {
                    tokens: response.usage.as_ref().map_or(0, |u| u.total_tokens),
                    duration_ms: llm_duration.as_millis() as u64,
                })
                .await;
            }

            // Log usage and record metrics
            if let Some(ref usage) = response.usage {
                tracing::info!(
                    prompt_tokens = usage.prompt_tokens,
                    completion_tokens = usage.completion_tokens,
                    total_tokens = usage.total_tokens,
                    "Token usage"
                );
                self.metrics
                    .record_llm_call(
                        model,
                        usage.prompt_tokens,
                        usage.completion_tokens,
                        llm_duration,
                    )
                    .await;

                // Record cost if pricing is available
                if self.config.cost.enabled {
                    // Try exact match first, then prefix match
                    let pricing = self.config.cost.pricing.get(model).or_else(|| {
                        self.config
                            .cost
                            .pricing
                            .iter()
                            .find(|(k, _)| model.starts_with(k.as_str()))
                            .map(|(_, v)| v)
                    });
                    if let Some(p) = pricing {
                        self.metrics
                            .record_cost(model, usage.prompt_tokens, usage.completion_tokens, p)
                            .await;
                        // Check budget alert
                        if let Some(alert) = self
                            .metrics
                            .check_budget(
                                self.config.cost.budget_limit,
                                self.config.cost.alert_threshold,
                            )
                            .await
                        {
                            tracing::warn!(
                                total_cost = alert.total_cost,
                                budget_limit = alert.budget_limit,
                                percentage = format!("{:.1}%", alert.percentage_used),
                                "⚠️ Budget alert: {:.1}% of ${:.2} budget used (${:.4} spent)",
                                alert.percentage_used,
                                alert.budget_limit,
                                alert.total_cost
                            );
                        }
                    }
                }
            } else {
                self.metrics
                    .record_llm_call(model, 0, 0, llm_duration)
                    .await;
            }

            // If no tool calls, we're done
            if !response.has_tool_calls() {
                final_content = response.content.clone();

                // Save assistant response to session
                let assistant_msg = Message::assistant(&response.content);
                self.sessions.add_message(session_key, assistant_msg).await;
                break;
            }

            // Process tool calls
            let tool_calls = response.tool_calls.clone().unwrap_or_default();

            if let Some(tui) = &self.tui_state {
                for tc in &tool_calls {
                    tui.handle_event(TuiEvent::ToolCall {
                        tool: tc.function_name().to_string(),
                        session: session_key.to_string(),
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    })
                    .await;
                }
            }

            tracing::info!(
                count = tool_calls.len(),
                names = %tool_calls.iter().map(|tc| tc.function_name()).collect::<Vec<_>>().join(", "),
                "Executing tool calls"
            );

            // Save assistant message with tool calls
            let assistant_msg =
                Message::assistant_with_tool_calls(&response.content, tool_calls.clone());
            messages.push(assistant_msg.clone());
            self.sessions.add_message(session_key, assistant_msg).await;

            // Execute tools (parallel when multiple)
            let tool_results = self.execute_tools(&tool_calls).await;

            // Add tool results as messages
            for result in tool_results {
                if let Some(tui) = &self.tui_state {
                    tui.handle_event(TuiEvent::ToolResult {
                        tool: result.tool_name.clone(),
                        success: result.success,
                        duration_ms: result.duration.as_millis() as u64,
                    })
                    .await;
                }
                let tool_msg = Message::tool_result(&result.tool_call_id, &result.output);
                messages.push(tool_msg.clone());
                self.sessions.add_message(session_key, tool_msg).await;
            }
        }

        // 5. Check if we should summarize
        self.maybe_summarize(session_key).await;

        if final_content.is_empty() {
            final_content =
                "(Agent reached maximum tool iterations without a final response)".into();
        }

        Ok(final_content)
    }

    /// Execute tool calls in parallel using tokio::JoinSet.
    async fn execute_tools(&self, tool_calls: &[ToolCall]) -> Vec<InternalToolResult> {
        use tokio::task::JoinSet;

        let mut set = JoinSet::new();
        let registry = self.tools.clone();
        let metrics = self.metrics.clone();

        for tc in tool_calls {
            let name = tc.function_name().to_string();
            let args = tc.parsed_arguments();
            let reg = registry.clone();
            let m = metrics.clone();
            let id = tc.id.clone();

            set.spawn(async move {
                let start = std::time::Instant::now();
                let args_converted: HashMap<String, serde_json::Value> = args;
                let result = reg.execute(&name, args_converted).await;
                let duration = start.elapsed();
                m.record_tool_call(&name, !result.is_error, duration).await;

                InternalToolResult {
                    tool_call_id: id,
                    tool_name: name,
                    output: result.for_llm,
                    success: !result.is_error,
                    duration,
                }
            });
        }

        let mut results = Vec::with_capacity(tool_calls.len());
        while let Some(result) = set.join_next().await {
            match result {
                Ok(res) => results.push(res),
                Err(e) => {
                    // This shouldn't really happen unless there's a serious bug in an async tool
                    results.push(InternalToolResult {
                        tool_call_id: "unknown".to_string(),
                        tool_name: "panic".to_string(),
                        output: format!("Tool execution panicked: {}", e),
                        success: false,
                        duration: std::time::Duration::from_secs(0),
                    });
                }
            }
        }

        results
    }

    /// Trigger session summarization if history is too long.
    async fn maybe_summarize(&self, session_key: &str) {
        let count = self.sessions.message_count(session_key).await;
        // Summarize every 20 messages
        if count > 20 && count % 20 == 0 {
            tracing::info!(session = %session_key, messages = count, "Triggering session summarization");
            if let Err(e) = self.summarize_session(session_key).await {
                tracing::warn!(session = %session_key, "Summarization failed: {}", e);
            }
        }
    }

    /// Summarize session history using the LLM.
    async fn summarize_session(&self, session_key: &str) -> anyhow::Result<()> {
        let messages = self.sessions.get_messages(session_key).await;
        if messages.is_empty() {
            return Ok(());
        }

        // Build a summary request
        let existing = self.sessions.get_summary(session_key).await;
        let mut content = String::from("Summarize the following conversation concisely, preserving key information and decisions:\n\n");

        if !existing.is_empty() {
            content.push_str(&format!("Previous summary:\n{}\n\n", existing));
        }

        for msg in &messages {
            content.push_str(&format!("[{}]: {}\n", msg.role, msg.content));
        }

        let summary_messages = vec![
            Message::system("You are a conversation summarizer. Create a concise summary."),
            Message::user(content),
        ];

        let response = self
            .provider
            .chat(
                &summary_messages,
                &[],
                &self.config.agents.defaults.model,
                &HashMap::new(),
            )
            .await?;

        self.sessions
            .set_summary(session_key, response.content)
            .await;

        Ok(())
    }

    /// Run the agent loop listening for inbound messages (gateway mode).
    pub async fn run(&self) -> anyhow::Result<()> {
        tracing::info!("Agent loop started, waiting for messages...");

        loop {
            match self.bus.consume_inbound().await {
                Some(msg) => {
                    tracing::info!(
                        channel = %msg.channel,
                        sender = %msg.sender_id,
                        "Processing inbound message"
                    );

                    // Rate limiting
                    if !self.rate_limiter.check_rate_limit(&msg.sender_id).await {
                        tracing::warn!(sender = %msg.sender_id, "Rate limit exceeded");
                        self.bus
                            .publish_outbound(OutboundMessage {
                                channel: msg.channel,
                                chat_id: msg.chat_id,
                                content: "⚠️ Rate limit exceeded. Please wait a moment."
                                    .to_string(),
                                metadata: HashMap::new(),
                            })
                            .await;
                        continue;
                    }

                    match self.process_message(msg).await {
                        Ok(response) => {
                            tracing::debug!(response_len = response.len(), "Message processed");
                        }
                        Err(e) => {
                            tracing::error!("Failed to process message: {}", e);
                        }
                    }
                }
                None => {
                    tracing::info!("Message bus closed, shutting down agent loop");
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    /// Fork a session into a new session key.
    pub async fn fork_session(&self, source_key: &str, target_key: &str) -> bool {
        self.sessions.fork_session(source_key, target_key).await
    }

    /// Get metrics collector reference.
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
}
