// QuectoClaw — Subagent tool (allows an agent to spawn another agent task)

use super::{Tool, ToolResult};
use crate::agent::AgentLoop;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Maximum allowed subagent recursion depth to prevent resource exhaustion.
const MAX_SUBAGENT_DEPTH: u32 = 3;

pub struct SubagentTool {
    agent: Arc<AgentLoop>,
    current_depth: Arc<AtomicU32>,
}

impl SubagentTool {
    pub fn new(agent: Arc<AgentLoop>) -> Self {
        Self {
            agent,
            current_depth: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Create a subagent tool with an inherited depth counter.
    pub fn with_depth(agent: Arc<AgentLoop>, depth: Arc<AtomicU32>) -> Self {
        Self {
            agent,
            current_depth: depth,
        }
    }
}

#[async_trait]
impl Tool for SubagentTool {
    fn name(&self) -> &str {
        "subagent"
    }

    fn description(&self) -> &str {
        "Spawn a subagent to solve a specific sub-task. Returns the subagent's final answer."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": {
                    "type": "string",
                    "description": "The specific task for the subagent to solve"
                }
            },
            "required": ["task"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let task = match args.get("task").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("task is required"),
        };

        // Check recursion depth
        let depth = self.current_depth.fetch_add(1, Ordering::SeqCst);
        if depth >= MAX_SUBAGENT_DEPTH {
            self.current_depth.fetch_sub(1, Ordering::SeqCst);
            return ToolResult::error(format!(
                "Subagent recursion limit reached (max depth: {}). Break the task into smaller pieces or solve directly.",
                MAX_SUBAGENT_DEPTH
            ));
        }

        tracing::info!(task = %task, depth = depth + 1, "Spawning subagent");

        // Create a unique session key for this subagent task
        let subagent_session = format!("subagent:{}", uuid::Uuid::new_v4());

        let result = match self.agent.process_direct(task, &subagent_session).await {
            Ok(response) => {
                tracing::info!(session = %subagent_session, "Subagent task completed");
                ToolResult::success(response)
            }
            Err(e) => {
                tracing::error!(session = %subagent_session, error = %e, "Subagent task failed");
                ToolResult::error(format!("Subagent failed: {}", e))
            }
        };

        self.current_depth.fetch_sub(1, Ordering::SeqCst);
        result
    }
}
