// QuectoClaw — WASM Plugin System
// This module provides a way to run sandboxed WebAssembly plugins as tools.
// It uses wasmtime and wasmtime-wasi for secure execution.

use crate::tool::{Tool, ToolRegistry, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::WasiCtxBuilder;

/// A WASM plugin definition loaded from a manifest.json file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WasmPluginConfig {
    /// Plugin name (becomes the tool name).
    pub name: String,
    /// Description for the LLM.
    pub description: String,
    /// Path to the .wasm file (relative to manifest).
    pub wasm_file: String,
    /// Parameters the tool accepts.
    #[serde(default)]
    pub parameters: Vec<WasmPluginParam>,
    /// Fuel limit for code execution (default: 1,000,000).
    #[serde(default = "default_fuel")]
    pub fuel: u64,
}

fn default_fuel() -> u64 {
    1_000_000
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WasmPluginParam {
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

/// Dynamic tool backed by a WASM module.
pub struct WasmPluginTool {
    config: WasmPluginConfig,
    wasm_path: PathBuf,
    schema: Value,
    engine: Engine,
}

impl WasmPluginTool {
    pub fn new(config: WasmPluginConfig, manifest_dir: &Path) -> anyhow::Result<Self> {
        let wasm_path = manifest_dir.join(&config.wasm_file);
        
        // Build JSON schema for parameters
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &config.parameters {
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

        // Initialize Wasmtime engine with fuel enabled
        let mut wasm_cfg = Config::new();
        wasm_cfg.consume_fuel(true);
        let engine = Engine::new(&wasm_cfg)?;

        Ok(Self {
            config,
            wasm_path,
            schema,
            engine,
        })
    }
}

#[async_trait]
impl Tool for WasmPluginTool {
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
        let input_json = serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_string());
        
        let engine = self.engine.clone();
        let wasm_path = self.wasm_path.clone();
        let fuel = self.config.fuel;

        let result: Result<(Vec<u8>, Vec<u8>), anyhow::Error> = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, Vec<u8>), anyhow::Error> {
            let module = Module::from_file(&engine, &wasm_path)?;
            let mut linker = Linker::new(&engine);
            wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |t| t)?;

            let stdout_file = wasmtime_wasi::pipe::MemoryOutputPipe::new(1024 * 1024);
            let stderr_file = wasmtime_wasi::pipe::MemoryOutputPipe::new(1024 * 1024);
            let stdin_file = wasmtime_wasi::pipe::MemoryInputPipe::new(input_json.into_bytes());

            let mut wasi_builder = WasiCtxBuilder::new();
            wasi_builder.stdin(stdin_file);
            wasi_builder.stdout(stdout_file.clone());
            wasi_builder.stderr(stderr_file.clone());
            
            let wasi = wasi_builder.build_p1();

            let mut store = Store::new(&engine, wasi);
            store.set_fuel(fuel)?;

            let instance = linker.instantiate(&mut store, &module)?;
            let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;

            match start.call(&mut store, ()) {
                Ok(_) => {
                    let out = stdout_file.contents().to_vec();
                    let err = stderr_file.contents().to_vec();
                    Ok((out, err))
                },
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("fuel") || msg.contains("exhausted") {
                        Err(anyhow::anyhow!("WASM plugin exhausted fuel limit of {}", fuel))
                    } else {
                        Err(e)
                    }
                }
            }
        }).await.unwrap_or_else(|e| Err(anyhow::anyhow!("Task panic: {}", e)));

        match result {
            Ok((stdout_bytes, _stderr_bytes)) => {
                let out = String::from_utf8_lossy(&stdout_bytes).to_string();
                ToolResult::success(out)
            }
            Err(e) => {
                ToolResult::error(format!("WASM Error: {}", e))
            }
        }
    }
}

/// Load WASM plugin configs from a directory.
/// Each plugin should be in its own subdirectory with a manifest.json.
pub async fn load_wasm_plugins(dir: &Path) -> Vec<(WasmPluginConfig, PathBuf)> {
    let mut plugins = Vec::new();

    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return plugins;
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }

        match tokio::fs::read_to_string(&manifest_path).await {
            Ok(content) => match serde_json::from_str::<WasmPluginConfig>(&content) {
                Ok(plugin) => {
                    tracing::info!(name = %plugin.name, "Loaded WASM plugin: {}", manifest_path.display());
                    plugins.push((plugin, path));
                }
                Err(e) => {
                    tracing::warn!("Failed to parse WASM plugin {}: {}", manifest_path.display(), e);
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read WASM plugin file {}: {}", manifest_path.display(), e);
            }
        }
    }

    plugins
}

/// Register loaded WASM plugins as tools in the registry.
pub async fn register_wasm_plugins(registry: &ToolRegistry, plugins: Vec<(WasmPluginConfig, PathBuf)>) {
    for (config, manifest_dir) in plugins {
        match WasmPluginTool::new(config, &manifest_dir) {
            Ok(tool) => {
                let name = tool.name().to_string();
                registry.register(Arc::new(tool)).await;
                tracing::info!(name = %name, "Registered WASM plugin tool");
            }
            Err(e) => {
                tracing::error!("Failed to initialize WASM plugin: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_wasm_plugin_execution() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wasm_plugins/hello");
        let config = WasmPluginConfig {
            name: "wasm_hello".into(),
            description: "test".into(),
            wasm_file: "wasm_hello.wasm".into(),
            parameters: vec![],
            fuel: 1_000_000,
        };

        let tool = WasmPluginTool::new(config, &manifest_dir).unwrap();
        let mut args = HashMap::new();
        args.insert("name".into(), serde_json::Value::String("Antigravity".into()));

        let result = tool.execute(args).await;
        assert!(!result.is_error, "Execution error: {}", result.for_llm);
        assert!(result.for_llm.contains("Hello, Antigravity!"), "Actual output: {}", result.for_llm);
    }

    #[tokio::test]
    async fn test_wasm_plugin_fuel_exhaustion() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/wasm_plugins/hello");
        let config = WasmPluginConfig {
            name: "wasm_hello".into(),
            description: "test".into(),
            wasm_file: "wasm_hello.wasm".into(),
            parameters: vec![],
            fuel: 100, // Very low fuel
        };

        let tool = WasmPluginTool::new(config, &manifest_dir).unwrap();
        let result = tool.execute(HashMap::new()).await;
        assert!(result.is_error, "Expected fuel exhaustion error, got success: {}", result.for_llm);
        assert!(result.for_llm.contains("exhausted fuel limit") || result.for_llm.contains("WASM Error"), 
                "Actual error: {}", result.for_llm);
    }
}
