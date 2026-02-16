// QuectoClaw — Ultra-efficient AI assistant in Rust
// Inspired by PicoClaw: https://github.com/sipeed/picoclaw
// License: Apache-2.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("failed to read config file: {0}")]
    ReadFile(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("home directory not found")]
    NoHomeDir,
    #[error("no API key found for any provider")]
    MissingApiKey,
    #[error("workspace path is invalid or inaccessible: {0}")]
    InvalidWorkspace(String),
    #[error("{0}")]
    Other(String),
}

// ---------------------------------------------------------------------------
// Top-level Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub channels: ChannelsConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub heartbeat: HeartbeatConfig,
    #[serde(default)]
    pub devices: DevicesConfig,
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsConfig {
    #[serde(default)]
    pub defaults: AgentDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefaults {
    #[serde(default = "default_workspace")]
    pub workspace: String,
    #[serde(default = "default_true")]
    pub restrict_to_workspace: bool,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            workspace: default_workspace(),
            restrict_to_workspace: true,
            model: default_model(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            max_tool_iterations: default_max_tool_iterations(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
        }
    }
}

fn default_max_retries() -> usize {
    3
}
fn default_retry_delay_ms() -> u64 {
    1000
}

fn default_workspace() -> String {
    "~/.quectoclaw/workspace".to_string()
}
fn default_true() -> bool {
    true
}
fn default_model() -> String {
    "gpt-4o-mini".to_string()
}
fn default_max_tokens() -> usize {
    8192
}
fn default_temperature() -> f64 {
    0.7
}
fn default_max_tool_iterations() -> usize {
    20
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelsConfig {
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub discord: DiscordConfig,
    #[serde(default)]
    pub slack: SlackConfig,
    #[serde(default)]
    pub whatsapp: WhatsAppConfig,
    #[serde(default)]
    pub feishu: FeishuConfig,
    #[serde(default)]
    pub dingtalk: DingTalkConfig,
    #[serde(default)]
    pub line: LineConfig,
    #[serde(default)]
    pub onebot: OneBotConfig,
    #[serde(default)]
    pub maixcam: MaixCamConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TelegramConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub proxy: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscordConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlackConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub app_token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhatsAppConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub bridge_url: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FeishuConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub app_secret: String,
    #[serde(default)]
    pub encrypt_key: String,
    #[serde(default)]
    pub verification_token: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DingTalkConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LineConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub channel_secret: String,
    #[serde(default)]
    pub channel_access_token: String,
    #[serde(default)]
    pub webhook_host: String,
    #[serde(default = "default_line_port")]
    pub webhook_port: u16,
    #[serde(default)]
    pub webhook_path: String,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

fn default_line_port() -> u16 {
    18791
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OneBotConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub ws_url: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default = "default_reconnect")]
    pub reconnect_interval: u64,
    #[serde(default)]
    pub group_trigger_prefix: Vec<String>,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

fn default_reconnect() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MaixCamConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub host: String,
    #[serde(default = "default_maixcam_port")]
    pub port: u16,
    #[serde(default)]
    pub allow_from: Vec<String>,
}

fn default_maixcam_port() -> u16 {
    18790
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub anthropic: ProviderEntry,
    #[serde(default)]
    pub openai: ProviderEntry,
    #[serde(default)]
    pub openrouter: ProviderEntry,
    #[serde(default)]
    pub groq: ProviderEntry,
    #[serde(default)]
    pub zhipu: ProviderEntry,
    #[serde(default)]
    pub gemini: ProviderEntry,
    #[serde(default)]
    pub vllm: ProviderEntry,
    #[serde(default)]
    pub nvidia: ProviderEntryWithProxy,
    #[serde(default)]
    pub moonshot: ProviderEntry,
    /// Catch-all for unknown providers
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderEntry {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api_base: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderEntryWithProxy {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api_base: String,
    #[serde(default)]
    pub proxy: String,
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_gateway_host")]
    pub host: String,
    #[serde(default = "default_gateway_port")]
    pub port: u16,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_gateway_host(),
            port: default_gateway_port(),
        }
    }
}

fn default_gateway_host() -> String {
    "0.0.0.0".to_string()
}
fn default_gateway_port() -> u16 {
    18790
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolsConfig {
    #[serde(default)]
    pub web: WebToolsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebToolsConfig {
    #[serde(default)]
    pub search: WebSearchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebSearchConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
}

fn default_max_results() -> usize {
    5
}

// ---------------------------------------------------------------------------
// Heartbeat
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_heartbeat_interval")]
    pub interval: u64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: default_heartbeat_interval(),
        }
    }
}

fn default_heartbeat_interval() -> u64 {
    30
}

// ---------------------------------------------------------------------------
// Devices
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DevicesConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub monitor_usb: bool,
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

impl Config {
    /// Load configuration from a JSON file, falling back to defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            tracing::warn!("Config file not found at {:?}, using defaults", path);
            return Ok(Config::default());
        }

        let contents = std::fs::read_to_string(path)?;
        let mut config: Config = serde_json::from_str(&contents)?;
        config.apply_env_overrides();
        Ok(config)
    }

    /// Apply environment variable overrides (prefix: QUECTOCLAW_)
    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("QUECTOCLAW_AGENTS_DEFAULTS_WORKSPACE") {
            self.agents.defaults.workspace = v;
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_AGENTS_DEFAULTS_MODEL") {
            self.agents.defaults.model = v;
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_AGENTS_DEFAULTS_RESTRICT_TO_WORKSPACE") {
            self.agents.defaults.restrict_to_workspace = v.parse().unwrap_or(true);
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_AGENTS_DEFAULTS_MAX_TOKENS") {
            if let Ok(n) = v.parse() {
                self.agents.defaults.max_tokens = n;
            }
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_AGENTS_DEFAULTS_TEMPERATURE") {
            if let Ok(n) = v.parse() {
                self.agents.defaults.temperature = n;
            }
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_AGENTS_DEFAULTS_MAX_TOOL_ITERATIONS") {
            if let Ok(n) = v.parse() {
                self.agents.defaults.max_tool_iterations = n;
            }
        }
        // Provider overrides
        if let Ok(v) = std::env::var("QUECTOCLAW_PROVIDERS_OPENAI_API_KEY") {
            self.providers.openai.api_key = v;
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_PROVIDERS_OPENAI_API_BASE") {
            self.providers.openai.api_base = v;
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_PROVIDERS_ANTHROPIC_API_KEY") {
            self.providers.anthropic.api_key = v;
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_PROVIDERS_OPENROUTER_API_KEY") {
            self.providers.openrouter.api_key = v;
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_PROVIDERS_GEMINI_API_KEY") {
            self.providers.gemini.api_key = v;
        }
        // Channel overrides
        if let Ok(v) = std::env::var("QUECTOCLAW_CHANNELS_TELEGRAM_TOKEN") {
            self.channels.telegram.token = v;
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_CHANNELS_TELEGRAM_ENABLED") {
            self.channels.telegram.enabled = v.parse().unwrap_or(false);
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_CHANNELS_DISCORD_TOKEN") {
            self.channels.discord.token = v;
        }
        if let Ok(v) = std::env::var("QUECTOCLAW_CHANNELS_DISCORD_ENABLED") {
            self.channels.discord.enabled = v.parse().unwrap_or(false);
        }
    }

    /// Resolve the workspace path, expanding `~` to home directory.
    pub fn workspace_path(&self) -> Result<PathBuf, ConfigError> {
        let ws = &self.agents.defaults.workspace;
        if let Some(stripped) = ws.strip_prefix('~') {
            let home = dirs::home_dir().ok_or(ConfigError::NoHomeDir)?;
            Ok(home.join(ws.strip_prefix("~/").unwrap_or(stripped)))
        } else {
            Ok(PathBuf::from(ws))
        }
    }

    /// Get the default config file path: ~/.quectoclaw/config.json
    pub fn default_path() -> Result<PathBuf, ConfigError> {
        let home = dirs::home_dir().ok_or(ConfigError::NoHomeDir)?;
        Ok(home.join(".quectoclaw").join("config.json"))
    }

    /// Find the API key and base URL for the configured model.
    /// Returns (api_key, api_base, provider_name).
    pub fn resolve_provider(&self) -> Option<(String, String, String)> {
        let model = &self.agents.defaults.model;

        // Auto-detect provider from model name
        let entries: Vec<(&str, &str, &str)> = vec![
            (
                "claude",
                &self.providers.anthropic.api_key,
                &self.providers.anthropic.api_base,
            ),
            (
                "gpt",
                &self.providers.openai.api_key,
                &self.providers.openai.api_base,
            ),
            (
                "o1",
                &self.providers.openai.api_key,
                &self.providers.openai.api_base,
            ),
            (
                "o3",
                &self.providers.openai.api_key,
                &self.providers.openai.api_base,
            ),
            (
                "o4",
                &self.providers.openai.api_key,
                &self.providers.openai.api_base,
            ),
            (
                "gemini",
                &self.providers.gemini.api_key,
                &self.providers.gemini.api_base,
            ),
            (
                "glm",
                &self.providers.zhipu.api_key,
                &self.providers.zhipu.api_base,
            ),
            (
                "llama",
                &self.providers.groq.api_key,
                &self.providers.groq.api_base,
            ),
            (
                "mixtral",
                &self.providers.groq.api_key,
                &self.providers.groq.api_base,
            ),
            (
                "moonshot",
                &self.providers.moonshot.api_key,
                &self.providers.moonshot.api_base,
            ),
        ];

        // Try model-name matching first
        for (prefix, key, base) in &entries {
            if model.to_lowercase().contains(prefix) && !key.is_empty() {
                return Some((key.to_string(), base.to_string(), prefix.to_string()));
            }
        }

        // Fall back: try OpenRouter (works with most models)
        if !self.providers.openrouter.api_key.is_empty() {
            let base = if self.providers.openrouter.api_base.is_empty() {
                "https://openrouter.ai/api/v1".to_string()
            } else {
                self.providers.openrouter.api_base.clone()
            };
            return Some((
                self.providers.openrouter.api_key.clone(),
                base,
                "openrouter".to_string(),
            ));
        }

        // Fall back: first non-empty key
        let all_providers: Vec<(&str, &ProviderEntry)> = vec![
            ("openai", &self.providers.openai),
            ("anthropic", &self.providers.anthropic),
            ("groq", &self.providers.groq),
            ("zhipu", &self.providers.zhipu),
            ("gemini", &self.providers.gemini),
            ("vllm", &self.providers.vllm),
            ("moonshot", &self.providers.moonshot),
        ];

        for (name, entry) in all_providers {
            if !entry.api_key.is_empty() {
                return Some((
                    entry.api_key.clone(),
                    entry.api_base.clone(),
                    name.to_string(),
                ));
            }
        }

        None
    }

    /// Validate configuration for basic correctness.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // 1. Ensure at least one API key is present
        if self.resolve_provider().is_none() {
            return Err(ConfigError::Other(
                "No API key found for any provider. Please add an API key (e.g. 'openai') to your config.json or set QUECTOCLAW_PROVIDERS_OPENAI_API_KEY environment variable.".to_string()
            ));
        }

        // 2. Validate workspace path (expand tilde if needed and check)
        let ws = self.workspace_path().map_err(|_| ConfigError::NoHomeDir)?;
        if let Some(_parent) = ws.parent() {
            // We just need it to be a valid path string for now
        } else {
            return Err(ConfigError::InvalidWorkspace(ws.to_string_lossy().to_string()));
        }

        // 3. Channel validation if enabled
        if self.channels.telegram.enabled && self.channels.telegram.token.is_empty() {
             tracing::warn!("Telegram channel is enabled but token is missing");
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.agents.defaults.workspace, "~/.quectoclaw/workspace");
        assert!(cfg.agents.defaults.restrict_to_workspace);
        assert_eq!(cfg.agents.defaults.max_tool_iterations, 20);
    }

    #[test]
    fn test_parse_minimal_json() {
        let json = r#"{"agents": {"defaults": {"model": "claude-3-opus"}}}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.agents.defaults.model, "claude-3-opus");
        assert_eq!(cfg.agents.defaults.workspace, "~/.quectoclaw/workspace");
    }

    #[test]
    fn test_parse_full_config() {
        let json = r#"{
            "agents": {"defaults": {"workspace": "/tmp/ws", "model": "gpt-4o", "max_tokens": 4096}},
            "providers": {"openai": {"api_key": "sk-test", "api_base": ""}},
            "channels": {"telegram": {"enabled": true, "token": "abc"}}
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.agents.defaults.workspace, "/tmp/ws");
        assert_eq!(cfg.providers.openai.api_key, "sk-test");
        assert!(cfg.channels.telegram.enabled);
    }

    #[test]
    fn test_workspace_path_tilde() {
        let cfg = Config::default();
        let path = cfg.workspace_path().unwrap();
        assert!(path.to_str().unwrap().contains("quectoclaw"));
        assert!(!path.to_str().unwrap().starts_with('~'));
    }

    #[test]
    fn test_resolve_provider_openai() {
        let json = r#"{
            "agents": {"defaults": {"model": "gpt-4o"}},
            "providers": {"openai": {"api_key": "sk-test"}}
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        let (key, _base, name) = cfg.resolve_provider().unwrap();
        assert_eq!(key, "sk-test");
        assert_eq!(name, "gpt");
    }
}
