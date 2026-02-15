// QuectoClaw — Plugin system for loading custom tools from JSON config files.

use crate::tool::{Tool, ToolRegistry, ToolResult};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

/// A plugin definition loaded from a JSON file in the plugins directory.
#[derive(Debug, Clone, Deserialize)]
pub struct PluginConfig {
    /// Plugin name (becomes the tool name).
    pub name: String,
    /// Description for the LLM.
    pub description: String,
    /// Shell command template (supports `{{param}}` substitution).
    pub command: String,
    /// Parameters the tool accepts.
    #[serde(default)]
    pub parameters: Vec<PluginParam>,
    /// Optional working directory.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Timeout in seconds (default 30).
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    30
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginParam {
    pub name: String,
    pub description: String,
    #[serde(default = "default_type")]
    pub param_type: String,
    #[serde(default)]
    pub required: bool,
}

fn default_type() -> String {
    "string".into()
}

/// Dynamic tool backed by a shell command template.
struct PluginTool {
    config: PluginConfig,
    schema: Value,
}

#[async_trait]
impl Tool for PluginTool {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn description(&self) -> &str {
        &self.config.description
    }

    fn parameters(&self) -> Value {
        self.schema.clone()
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        // Substitute {{param}} in command
        let mut resolved = self.config.command.clone();
        for (key, val) in &args {
            let placeholder = format!("{{{{{}}}}}", key);
            let value_str = match val {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            resolved = resolved.replace(&placeholder, &value_str);
        }

        // Execute shell command
        let mut command = tokio::process::Command::new("sh");
        command
            .arg("-c")
            .arg(&resolved)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(ref dir) = self.config.cwd {
            command.current_dir(dir);
        }

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(self.config.timeout),
            command.output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                if output.status.success() {
                    ToolResult::success(stdout.to_string())
                } else {
                    ToolResult::error(format!("Exit {}: {}", output.status, stderr))
                }
            }
            Ok(Err(e)) => ToolResult::error(format!("Failed to run: {}", e)),
            Err(_) => ToolResult::error(format!("Plugin timed out after {}s", self.config.timeout)),
        }
    }
}

/// Load plugin configs from a directory of JSON files.
pub async fn load_plugins(dir: &Path) -> Vec<PluginConfig> {
    let mut plugins = Vec::new();

    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return plugins;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => match serde_json::from_str::<PluginConfig>(&content) {
                Ok(plugin) => {
                    tracing::info!(name = %plugin.name, "Loaded plugin: {}", path.display());
                    plugins.push(plugin);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse plugin {}: {}", path.display(), e);
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read plugin file {}: {}", path.display(), e);
            }
        }
    }

    plugins
}

/// Register loaded plugins as tools in the registry.
pub async fn register_plugins(registry: &ToolRegistry, plugins: Vec<PluginConfig>) {
    for plugin in plugins {
        // Build JSON schema for parameters
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &plugin.parameters {
            let mut prop = serde_json::Map::new();
            prop.insert("type".into(), Value::String(param.param_type.clone()));
            prop.insert(
                "description".into(),
                Value::String(param.description.clone()),
            );
            properties.insert(param.name.clone(), Value::Object(prop));
            if param.required {
                required.push(Value::String(param.name.clone()));
            }
        }

        let schema = serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
        });

        let name = plugin.name.clone();
        let tool = PluginTool {
            config: plugin,
            schema,
        };

        registry.register(Arc::new(tool)).await;
        tracing::info!(name = %name, "Registered plugin tool");
    }
}
