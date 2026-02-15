// QuectoClaw — Subagent tool (allows an agent to spawn another agent task)

use super::{Tool, ToolResult};
use crate::agent::AgentLoop;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

pub struct SubagentTool {
    agent: Arc<AgentLoop>,
}

impl SubagentTool {
    pub fn new(agent: Arc<AgentLoop>) -> Self {
        Self { agent }
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

        tracing::info!(task = %task, "Spawning subagent");

        // Create a unique session key for this subagent task
        let subagent_session = format!("subagent:{}", uuid::Uuid::new_v4());

        match self.agent.process_direct(task, &subagent_session).await {
            Ok(response) => {
                tracing::info!(session = %subagent_session, "Subagent task completed");
                ToolResult::success(response)
            }
            Err(e) => {
                tracing::error!(session = %subagent_session, error = %e, "Subagent task failed");
                ToolResult::error(format!("Subagent failed: {}", e))
            }
        }
    }
}
