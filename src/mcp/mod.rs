// QuectoClaw — Model Context Protocol (MCP) support
// Spec: https://modelcontextprotocol.io/

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, Mutex};

use crate::tool::{Tool, ToolResult};

// ---------------------------------------------------------------------------
// JSON-RPC Types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

// ---------------------------------------------------------------------------
// MCP Client
// ---------------------------------------------------------------------------

pub struct MCPClient {
    #[allow(dead_code)]
    child: Child,
    tx: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<Result<Value>>>>>,
    next_id: Arc<Mutex<u64>>,
}

impl MCPClient {
    pub async fn spawn(
        name: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to open stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to open stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to open stderr"))?;

        let (tx, mut rx) = mpsc::channel::<String>(32);
        let pending: Arc<Mutex<HashMap<String, mpsc::Sender<Result<Value>>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();

        // Stdin writer task
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(msg) = rx.recv().await {
                if let Err(e) = stdin.write_all(format!("{}\n", msg).as_bytes()).await {
                    tracing::error!("MCP stdin error: {}", e);
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        // Stdout reader task
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(resp) = serde_json::from_str::<JsonRpcResponse>(&line) {
                    let id_str = match &resp.id {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        _ => continue,
                    };

                    let mut pending = pending_clone.lock().await;
                    if let Some(chan) = pending.remove(&id_str) {
                        if let Some(err) = resp.error {
                            let _ = chan
                                .send(Err(anyhow!("MCP Error ({}): {}", err.code, err.message)))
                                .await;
                        } else {
                            let _ = chan.send(Ok(resp.result.unwrap_or(Value::Null))).await;
                        }
                    }
                }
            }
        });

        // Stderr logger task
        let name_str = name.to_string();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                tracing::debug!("MCP [{}] stderr: {}", name_str, line);
            }
        });

        Ok(Self {
            child,
            tx,
            pending,
            next_id: Arc::new(Mutex::new(1)),
        })
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = {
            let mut next_id = self.next_id.lock().await;
            let id = *next_id;
            *next_id += 1;
            id.to_string()
        };

        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!(id),
            method: method.to_string(),
            params,
        };

        let (resp_tx, mut resp_rx) = mpsc::channel(1);
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, resp_tx);
        }

        let msg = serde_json::to_string(&req)?;
        self.tx
            .send(msg)
            .await
            .map_err(|e| anyhow!("Failed to send to MCP: {}", e))?;

        match tokio::time::timeout(tokio::time::Duration::from_secs(10), resp_rx.recv()).await {
            Ok(Some(res)) => res,
            Ok(None) => Err(anyhow!("MCP connection closed")),
            Err(_) => Err(anyhow!("MCP request timed out")),
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let req = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };

        let msg = serde_json::to_string(&req)?;
        self.tx
            .send(msg)
            .await
            .map_err(|e| anyhow!("Failed to send to MCP: {}", e))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// MCP Tool Adapter
// ---------------------------------------------------------------------------

pub struct MCPTool {
    client: Arc<MCPClient>,
    mcp_name: String,
    full_name: String, // e.g. "sqlite_query"
    description: String,
    parameters: Value,
}

impl MCPTool {
    pub fn new(
        client: Arc<MCPClient>,
        server_name: &str,
        mcp_name: &str,
        desc: &str,
        params: Value,
    ) -> Self {
        Self {
            client,
            mcp_name: mcp_name.to_string(),
            full_name: format!("{}_{}", server_name, mcp_name),
            description: desc.to_string(),
            parameters: params,
        }
    }
}

#[async_trait]
impl Tool for MCPTool {
    fn name(&self) -> &str {
        &self.full_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let params = json!({
            "name": self.mcp_name,
            "arguments": args,
        });

        match self.client.call("tools/call", params).await {
            Ok(result) => {
                // MCP tool results are usually { content: [{ type: "text", text: "..." }] }
                if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
                    let mut combined = String::new();
                    for item in content {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            combined.push_str(text);
                        }
                    }

                    let is_error = result
                        .get("isError")
                        .and_then(|e| e.as_bool())
                        .unwrap_or(false);
                    if is_error {
                        ToolResult::error(combined)
                    } else {
                        ToolResult::success(combined)
                    }
                } else {
                    ToolResult::success(serde_json::to_string_pretty(&result).unwrap_or_default())
                }
            }
            Err(e) => ToolResult::error(format!("MCP execution failed: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

pub async fn init_mcp_servers(
    config: &crate::config::Config,
    registry: &crate::tool::ToolRegistry,
) -> Result<()> {
    for (name, server_cfg) in &config.mcp.servers {
        tracing::info!(server = %name, command = %server_cfg.command, "Spawning MCP server");
        let client =
            MCPClient::spawn(name, &server_cfg.command, &server_cfg.args, &server_cfg.env).await?;
        let client_arc = Arc::new(client);

        // 1. Initialize
        let init_params = json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "QuectoClaw", "version": crate::VERSION }
        });

        match client_arc.call("initialize", init_params).await {
            Ok(_) => {
                let _ = client_arc
                    .notify("notifications/initialized", json!({}))
                    .await;
                tracing::info!(server = %name, "MCP server initialized");

                // 2. List tools
                match client_arc.call("tools/list", json!({})).await {
                    Ok(tools_list) => {
                        if let Some(tools) = tools_list.get("tools").and_then(|t| t.as_array()) {
                            for tool_def in tools {
                                let mcp_name =
                                    tool_def.get("name").and_then(|n| n.as_str()).unwrap_or("");
                                let desc = tool_def
                                    .get("description")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("");
                                let params = tool_def
                                    .get("inputSchema")
                                    .cloned()
                                    .unwrap_or(json!({"type": "object"}));

                                let adapter =
                                    MCPTool::new(client_arc.clone(), name, mcp_name, desc, params);
                                registry.register(Arc::new(adapter)).await;
                                tracing::debug!(server = %name, tool = %mcp_name, "Registered MCP tool");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(server = %name, error = %e, "Failed to list MCP tools");
                    }
                }
            }
            Err(e) => {
                tracing::error!(server = %name, error = %e, "Failed to initialize MCP server");
            }
        }
    }
    Ok(())
}
