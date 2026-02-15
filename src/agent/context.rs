// QuectoClaw — System prompt builder (reads workspace .md files and tool descriptions)

use crate::tool::ToolRegistry;
use std::path::Path;

/// Build the system prompt from workspace template files and tool descriptions.
pub async fn build_system_prompt(workspace: &str, tools: &ToolRegistry) -> String {
    let mut parts = Vec::new();

    // Core identity
    parts.push("You are QuectoClaw, an ultra-efficient AI assistant. You are helpful, precise, and concise.".to_string());

    // Read workspace files in order
    let files = [
        ("IDENTITY.md", "Identity"),
        ("SOUL.md", "Personality"),
        ("AGENTS.md", "Agent Behavior"),
        ("USER.md", "User Preferences"),
    ];

    for (filename, label) in &files {
        let path = Path::new(workspace).join(filename);
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                parts.push(format!("\n## {}\n{}", label, trimmed));
            }
        }
    }

    // Tool descriptions
    let summaries = tools.get_summaries().await;
    if !summaries.is_empty() {
        parts.push(format!(
            "\n## Available Tools\nYou have access to the following tools:\n{}",
            summaries.join("\n")
        ));
    }

    // Read TOOLS.md for additional tool instructions
    let tools_md = Path::new(workspace).join("TOOLS.md");
    if let Ok(content) = tokio::fs::read_to_string(&tools_md).await {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            parts.push(format!("\n## Tool Usage Guide\n{}", trimmed));
        }
    }

    // Memory
    let memory_path = Path::new(workspace).join("memory").join("MEMORY.md");
    if let Ok(content) = tokio::fs::read_to_string(&memory_path).await {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            parts.push(format!("\n## Long-Term Memory\n{}", trimmed));
        }
    }

    parts.join("\n")
}
