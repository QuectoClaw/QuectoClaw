// QuectoClaw — Tool system

pub mod exec;
pub mod filesystem;
pub mod plugin;
pub mod subagent;
pub mod vectordb_index;
pub mod vectordb_search;
pub mod web;

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Tool result
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ToolResult {
    /// Content shown to the LLM
    pub for_llm: String,
    /// Content shown to the user (may differ from LLM view)
    pub for_user: String,
    /// Whether this result represents an error
    pub is_error: bool,
    /// Whether this tool runs asynchronously (result comes later)
    pub is_async: bool,
}

impl ToolResult {
    pub fn success(content: impl Into<String>) -> Self {
        let c = content.into();
        Self {
            for_llm: c.clone(),
            for_user: c,
            is_error: false,
            is_async: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        let c = content.into();
        Self {
            for_llm: c.clone(),
            for_user: c,
            is_error: true,
            is_async: false,
        }
    }

    /// Result shown to LLM but silent to user
    pub fn silent(content: impl Into<String>) -> Self {
        let c = content.into();
        Self {
            for_llm: c,
            for_user: String::new(),
            is_error: false,
            is_async: false,
        }
    }

    pub fn async_started(content: impl Into<String>) -> Self {
        let c = content.into();
        Self {
            for_llm: c.clone(),
            for_user: c,
            is_error: false,
            is_async: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Tool trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult;
}

// ---------------------------------------------------------------------------
// Tool registry
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ToolRegistry {
    tools: Arc<RwLock<HashMap<String, Arc<dyn Tool>>>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.write().await.insert(name, tool);
    }

    pub async fn execute(&self, name: &str, args: HashMap<String, Value>) -> ToolResult {
        let tools = self.tools.read().await;
        match tools.get(name) {
            Some(tool) => {
                tracing::info!(tool = %name, "Executing tool");
                let start = std::time::Instant::now();
                let result = tool.execute(args).await;
                let duration = start.elapsed();

                if result.is_error {
                    tracing::error!(tool = %name, duration_ms = %duration.as_millis(), error = %result.for_llm, "Tool failed");
                } else {
                    tracing::info!(tool = %name, duration_ms = %duration.as_millis(), result_len = result.for_llm.len(), "Tool completed");
                }

                result
            }
            None => {
                tracing::error!(tool = %name, "Tool not found");
                ToolResult::error(format!("tool '{}' not found", name))
            }
        }
    }

    /// Get tool definitions for the LLM API in provider-compatible format.
    pub async fn get_definitions(&self) -> Vec<crate::provider::ToolDefinition> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|tool| crate::provider::ToolDefinition {
                def_type: "function".to_string(),
                function: crate::provider::ToolFunctionDefinition {
                    name: tool.name().to_string(),
                    description: tool.description().to_string(),
                    parameters: tool.parameters(),
                },
            })
            .collect()
    }

    pub async fn list(&self) -> Vec<String> {
        self.tools.read().await.keys().cloned().collect()
    }

    pub async fn count(&self) -> usize {
        self.tools.read().await.len()
    }

    pub async fn get_summaries(&self) -> Vec<String> {
        let tools = self.tools.read().await;
        tools
            .values()
            .map(|t| format!("- `{}` - {}", t.name(), t.description()))
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
