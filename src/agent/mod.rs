// QuectoClaw — Agent loop (core orchestrator)

pub mod context;
pub mod gateway;
pub mod memory;

use crate::bus::{InboundMessage, MessageBus, OutboundMessage};
use crate::config::Config;
use crate::provider::{LLMProvider, Message, ToolCall};
use crate::session::SessionManager;
use crate::tool::ToolRegistry;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub struct AgentLoop {
    config: Config,
    provider: Arc<dyn LLMProvider>,
    tools: ToolRegistry,
    sessions: SessionManager,
    bus: Arc<MessageBus>,
    workspace: String,
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

        Self {
            config,
            provider,
            tools,
            sessions,
            bus,
            workspace,
        }
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
    async fn run_agent_loop(
        &self,
        user_message: &str,
        session_key: &str,
        use_history: bool,
        stream_tx: Option<tokio::sync::mpsc::Sender<crate::provider::StreamEvent>>,
    ) -> anyhow::Result<String> {
        let max_iterations = self.config.agents.defaults.max_tool_iterations;
        let model = &self.config.agents.defaults.model;

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

        let mut final_content = String::new();

        for iteration in 0..max_iterations {
            tracing::info!(
                iteration = iteration,
                messages = messages.len(),
                model = %model,
                "Running LLM iteration"
            );

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

            // Log usage
            if let Some(ref usage) = response.usage {
                tracing::info!(
                    prompt_tokens = usage.prompt_tokens,
                    completion_tokens = usage.completion_tokens,
                    total_tokens = usage.total_tokens,
                    "Token usage"
                );
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
            for (tc, result) in tool_calls.iter().zip(tool_results.iter()) {
                let tool_msg = Message::tool_result(&tc.id, &result.for_llm);
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
    async fn execute_tools(&self, tool_calls: &[ToolCall]) -> Vec<crate::tool::ToolResult> {
        use tokio::task::JoinSet;

        let mut set = JoinSet::new();
        let registry = self.tools.clone();

        for tc in tool_calls {
            let name = tc.function_name().to_string();
            let args = tc.parsed_arguments();
            let reg = registry.clone();

            set.spawn(async move {
                let args_converted: HashMap<String, serde_json::Value> = args;
                reg.execute(&name, args_converted).await
            });
        }

        let mut results = Vec::with_capacity(tool_calls.len());
        while let Some(result) = set.join_next().await {
            match result {
                Ok(tool_result) => results.push(tool_result),
                Err(e) => results.push(crate::tool::ToolResult::error(format!(
                    "Tool execution panicked: {}",
                    e
                ))),
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
}
