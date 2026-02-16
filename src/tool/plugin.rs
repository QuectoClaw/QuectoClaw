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
        // Substitute {{param}} in command with proper shell escaping
        let mut resolved = self.config.command.clone();
        for (key, val) in &args {
            let placeholder = format!("{{{{{}}}}}", key);
            let value_str = match val {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            // Shell-escape: wrap in single quotes, escaping any embedded single quotes
            let escaped = format!("'{}'", value_str.replace('\'', "'\\''"));
            resolved = resolved.replace(&placeholder, &escaped);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_plugin_config_deserialization() {
        let json = r#"{
            "name": "greet",
            "description": "Greet by name",
            "command": "echo Hello {{name}}",
            "parameters": [
                {
                    "name": "name",
                    "description": "Name to greet",
                    "param_type": "string",
                    "required": true
                }
            ],
            "timeout": 10
        }"#;

        let config: PluginConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "greet");
        assert_eq!(config.command, "echo Hello {{name}}");
        assert_eq!(config.parameters.len(), 1);
        assert!(config.parameters[0].required);
        assert_eq!(config.timeout, 10);
    }

    #[test]
    fn test_plugin_config_defaults() {
        let json = r#"{
            "name": "minimal",
            "description": "Minimal plugin",
            "command": "echo hi"
        }"#;

        let config: PluginConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.timeout, 30); // default
        assert!(config.parameters.is_empty());
        assert!(config.cwd.is_none());
    }

    #[test]
    fn test_plugin_param_default_type() {
        let json = r#"{
            "name": "p",
            "description": "desc",
            "required": false
        }"#;
        let param: PluginParam = serde_json::from_str(json).unwrap();
        assert_eq!(param.param_type, "string");
        assert!(!param.required);
    }

    #[tokio::test]
    async fn test_load_plugins_from_dir() {
        let tmp = TempDir::new().unwrap();

        let plugin_json = r#"{
            "name": "test-plugin",
            "description": "A test plugin",
            "command": "echo test"
        }"#;
        std::fs::write(tmp.path().join("test.json"), plugin_json).unwrap();

        // Non-JSON files should be ignored
        std::fs::write(tmp.path().join("readme.md"), "not a plugin").unwrap();

        let plugins = load_plugins(tmp.path()).await;
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "test-plugin");
    }

    #[tokio::test]
    async fn test_load_plugins_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let plugins = load_plugins(tmp.path()).await;
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_load_plugins_nonexistent_dir() {
        let plugins = load_plugins(Path::new("/nonexistent/plugins")).await;
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_load_plugins_skips_invalid_json() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("bad.json"), "not valid json").unwrap();

        let plugins = load_plugins(tmp.path()).await;
        assert!(plugins.is_empty());
    }

    #[tokio::test]
    async fn test_register_plugins_creates_tools() {
        let registry = ToolRegistry::new();

        let plugins = vec![PluginConfig {
            name: "test-tool".into(),
            description: "A test tool".into(),
            command: "echo {{msg}}".into(),
            parameters: vec![PluginParam {
                name: "msg".into(),
                description: "Message".into(),
                param_type: "string".into(),
                required: true,
            }],
            cwd: None,
            timeout: 30,
        }];

        register_plugins(&registry, plugins).await;

        let tools = registry.list().await;
        assert!(tools.contains(&"test-tool".to_string()));
    }

    #[tokio::test]
    async fn test_plugin_tool_shell_escaping() {
        let config = PluginConfig {
            name: "echo-tool".into(),
            description: "test".into(),
            command: "echo {{input}}".into(),
            parameters: vec![],
            cwd: None,
            timeout: 5,
        };

        let tool = PluginTool {
            config,
            schema: serde_json::json!({}),
        };

        // Test with a value containing single quotes
        let mut args = HashMap::new();
        args.insert(
            "input".to_string(),
            Value::String("hello 'world'".to_string()),
        );

        let result = tool.execute(args).await;
        // The command should succeed and the output should contain the escaped text
        assert!(!result.is_error);
        assert!(result.for_llm.contains("hello"));
    }
}
