// QuectoClaw — Ultra-efficient AI assistant in Rust
// Inspired by PicoClaw: https://github.com/sipeed/picoclaw
// License: Apache-2.0

use clap::{Parser, Subcommand};
use quectoclaw::agent::AgentLoop;
use quectoclaw::bus::MessageBus;
use quectoclaw::config::Config;
use quectoclaw::provider::factory::create_provider;
use quectoclaw::tool::exec::ExecTool;
use quectoclaw::tool::filesystem::*;
use quectoclaw::tool::subagent::SubagentTool;
use quectoclaw::tool::vectordb_index::VectorIndexTool;
use quectoclaw::tool::vectordb_search::VectorSearchTool;
use quectoclaw::tool::web::{WebFetchTool, WebSearchTool};
use quectoclaw::tool::ToolRegistry;
use quectoclaw::vectordb::VectorStore;
use std::path::PathBuf;
use std::sync::Arc;

const LOGO: &str = "🦀";

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(
    name = "quectoclaw",
    about = "QuectoClaw — Ultra-efficient AI assistant in Rust",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the agent (one-shot or interactive)
    Agent {
        /// One-shot message to process
        #[arg(short, long)]
        message: Option<String>,
        /// Session key for conversation continuity
        #[arg(short, long, default_value = "default")]
        session: String,
        /// Config file path
        #[arg(short, long)]
        config: Option<String>,
    },
    /// Run the multi-channel gateway service
    Gateway {
        /// Config file path
        #[arg(short, long)]
        config: Option<String>,
        /// Open the monitoring dashboard
        #[arg(short, long)]
        dashboard: bool,
    },
    /// Initialize workspace and config
    Onboard,
    /// Show version information
    Version,
    /// Show status of configuration and workspace
    Status {
        /// Config file path
        #[arg(short, long)]
        config: Option<String>,
    },
    /// Launch the TUI monitoring dashboard
    Dashboard {
        /// Config file path
        #[arg(short, long)]
        config: Option<String>,
    },
    /// Launch the web-based monitoring dashboard
    #[command(name = "webui")]
    WebUI {
        /// Config file path
        #[arg(short, long)]
        config: Option<String>,
        /// Port to listen on (default: 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    quectoclaw::logger::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Agent {
            message,
            session,
            config,
        }) => {
            agent_cmd(message, session, config).await;
        }
        Some(Commands::Gateway { config, dashboard }) => {
            gateway_cmd(config, dashboard).await;
        }
        Some(Commands::Onboard) => {
            onboard_cmd().await;
        }
        Some(Commands::Version) => {
            version_cmd();
        }
        Some(Commands::Status { config }) => {
            status_cmd(config).await;
        }
        Some(Commands::Dashboard { config }) => {
            dashboard_cmd(config).await;
        }
        Some(Commands::WebUI { config, port }) => {
            webui_cmd(config, port).await;
        }
        None => {
            // Default: run in interactive agent mode
            agent_cmd(None, "default".into(), None).await;
        }
    }
}

// ---------------------------------------------------------------------------
// Agent command
// ---------------------------------------------------------------------------

async fn agent_cmd(message: Option<String>, session: String, config_path: Option<String>) {
    let cfg = load_config(config_path.as_deref());

    if let Err(e) = cfg.validate() {
        eprintln!("{} Configuration Error: {}", LOGO, e);
        eprintln!("\nRun `quectoclaw onboard` to set up your configuration.");
        std::process::exit(1);
    }

    let provider = match create_provider(&cfg) {
        Ok(p) => Arc::from(p),
        Err(e) => {
            eprintln!("{} Error: {}", LOGO, e);
            eprintln!("\nRun `quectoclaw onboard` to set up your configuration.");
            std::process::exit(1);
        }
    };

    let workspace = cfg
        .workspace_path()
        .unwrap_or_else(|_| PathBuf::from("/tmp/quectoclaw"));

    // Ensure workspace exists
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        eprintln!("Failed to create workspace: {}", e);
        std::process::exit(1);
    }

    let ws_str = workspace.to_string_lossy().to_string();
    let restrict = cfg.agents.defaults.restrict_to_workspace;

    // Build tool registry
    let tools = create_tool_registry(&ws_str, restrict, &cfg).await;

    let bus = Arc::new(MessageBus::new());
    let agent = Arc::new(AgentLoop::new(cfg, provider, tools.clone(), bus));

    // Register subagent tool (circular dependency handled via Arc)
    let agent_clone = agent.clone();
    tools
        .register(Arc::new(SubagentTool::new(agent_clone)))
        .await;

    tracing::info!(
        workspace = %ws_str,
        "QuectoClaw agent starting"
    );

    match message {
        Some(msg) => {
            // One-shot mode
            match agent.process_direct(&msg, &session).await {
                Ok(response) => {
                    println!("{}", response);
                }
                Err(e) => {
                    eprintln!("{} Error: {}", LOGO, e);
                    std::process::exit(1);
                }
            }
        }
        None => {
            // Interactive mode
            interactive_mode(agent, &session).await;
        }
    }
}

/// Interactive readline-based chat mode with streaming output.
async fn interactive_mode(agent: Arc<AgentLoop>, session: &str) {
    use std::io::Write;

    println!(
        "{} QuectoClaw v{} — Ultra-efficient AI Assistant",
        LOGO,
        quectoclaw::VERSION
    );
    println!("Type your message and press Enter. Type 'exit' or Ctrl+D to quit.\n");

    let mut rl = match rustyline::DefaultEditor::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to initialize readline: {}", e);
            simple_interactive_mode(agent, session).await;
            return;
        }
    };

    loop {
        match rl.readline(&format!("{} > ", LOGO)) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    println!("Goodbye! 👋");
                    break;
                }

                let _ = rl.add_history_entry(trimmed);

                // Handle slash commands
                if trimmed.starts_with('/') {
                    let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
                    match parts[0] {
                        "/fork" => {
                            let new_name =
                                parts.get(1).map(|s| s.to_string()).unwrap_or_else(|| {
                                    format!("{}-fork-{}", session, chrono::Utc::now().timestamp())
                                });
                            if agent.fork_session(session, &new_name).await {
                                println!("✂️  Forked session to: {}", new_name);
                                println!("   (Restart with --session {} to use it)\n", new_name);
                            } else {
                                println!("⚠️  Nothing to fork — session is empty.\n");
                            }
                            continue;
                        }
                        "/clear" => {
                            agent
                                .fork_session(
                                    session,
                                    &format!(
                                        "{}-backup-{}",
                                        session,
                                        chrono::Utc::now().timestamp()
                                    ),
                                )
                                .await;
                            println!("🗑️  Session cleared. (Backup saved)\n");
                            continue;
                        }
                        "/help" => {
                            println!("Commands:");
                            println!("  /fork [name]  — Branch this conversation");
                            println!("  /clear        — Clear session (auto-backs up)");
                            println!("  /metrics      — Show performance metrics");
                            println!("  /cost         — Show cost breakdown");
                            println!("  /help         — Show this help");
                            println!("  exit          — Quit\n");
                            continue;
                        }
                        "/metrics" => {
                            let report = agent.metrics().format_report().await;
                            println!("{}\n", report);
                            continue;
                        }
                        "/cost" => {
                            let report = agent.metrics().format_cost_report().await;
                            println!("{}\n", report);
                            continue;
                        }
                        _ => {
                            println!("Unknown command: {} (try /help)\n", parts[0]);
                            continue;
                        }
                    }
                }

                // Stream response tokens
                let (tx, mut rx) =
                    tokio::sync::mpsc::channel::<quectoclaw::provider::StreamEvent>(256);

                let agent_clone = agent.clone();
                let msg = trimmed.to_string();
                let sess = session.to_string();

                let handle = tokio::spawn(async move {
                    agent_clone.process_direct_streaming(&msg, &sess, tx).await
                });

                println!();
                let mut printed_content = false;
                while let Some(event) = rx.recv().await {
                    match event {
                        quectoclaw::provider::StreamEvent::Token(token) => {
                            print!("{}", token);
                            std::io::stdout().flush().ok();
                            printed_content = true;
                        }
                        quectoclaw::provider::StreamEvent::ToolCallDelta { name, .. } => {
                            if let Some(n) = name {
                                print!("\n⚙️  Calling {}...", n);
                                std::io::stdout().flush().ok();
                            }
                        }
                        quectoclaw::provider::StreamEvent::Done(_) => {
                            break;
                        }
                        quectoclaw::provider::StreamEvent::Error(e) => {
                            eprintln!("\n{} Stream error: {}", LOGO, e);
                            break;
                        }
                    }
                }
                if printed_content {
                    println!("\n");
                }

                // Wait for the agent loop to finish (handles tool iterations)
                match handle.await {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => eprintln!("{} Error: {}\n", LOGO, e),
                    Err(e) => eprintln!("{} Task error: {}\n", LOGO, e),
                }
            }
            Err(rustyline::error::ReadlineError::Eof) => {
                println!("\nGoodbye! 👋");
                break;
            }
            Err(rustyline::error::ReadlineError::Interrupted) => {
                println!("\nUse 'exit' to quit or Ctrl+D to exit.");
            }
            Err(e) => {
                eprintln!("Readline error: {}", e);
                break;
            }
        }
    }
}

/// Simple fallback interactive mode using stdin.
async fn simple_interactive_mode(agent: Arc<AgentLoop>, session: &str) {
    use std::io::BufRead;

    println!("(Simple mode — type your message and press Enter)\n");

    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        match line {
            Ok(input) => {
                let trimmed = input.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    break;
                }

                match agent.process_direct(&trimmed, session).await {
                    Ok(response) => println!("\n{}\n", response),
                    Err(e) => eprintln!("\n{} Error: {}\n", LOGO, e),
                }
            }
            Err(_) => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Onboard command
// ---------------------------------------------------------------------------

async fn onboard_cmd() {
    println!(
        "{} QuectoClaw Onboard — Setting up your AI assistant\n",
        LOGO
    );

    let home = dirs::home_dir().expect("Could not find home directory");
    let config_dir = home.join(".quectoclaw");
    let workspace_dir = config_dir.join("workspace");
    let config_path = config_dir.join("config.json");

    // Create directories
    for dir in &[
        &config_dir,
        &workspace_dir,
        &workspace_dir.join("sessions"),
        &workspace_dir.join("memory"),
        &workspace_dir.join("state"),
        &workspace_dir.join("skills"),
    ] {
        std::fs::create_dir_all(dir).expect("Failed to create directory");
    }

    // Write default config if it doesn't exist
    if !config_path.exists() {
        let default_config = serde_json::json!({
            "agents": {
                "defaults": {
                    "workspace": "~/.quectoclaw/workspace",
                    "restrict_to_workspace": true,
                    "model": "gpt-4o-mini",
                    "max_tokens": 8192,
                    "temperature": 0.7,
                    "max_tool_iterations": 20
                }
            },
            "providers": {
                "openai": { "api_key": "", "api_base": "" },
                "openrouter": { "api_key": "", "api_base": "" },
                "anthropic": { "api_key": "", "api_base": "" },
                "gemini": { "api_key": "", "api_base": "" }
            },
            "tools": {
                "web": { "search": { "api_key": "", "max_results": 5 } }
            },
            "heartbeat": { "enabled": true, "interval": 30 }
        });

        let content = serde_json::to_string_pretty(&default_config).unwrap();
        std::fs::write(&config_path, content).expect("Failed to write config");
        println!("  ✅ Config created at {}", config_path.display());
    } else {
        println!("  ⏭️  Config already exists at {}", config_path.display());
    }

    // Write workspace templates
    let templates = [
        ("IDENTITY.md", "# Identity\nYou are QuectoClaw, an ultra-efficient AI assistant built in Rust.\nYou are fast, precise, and helpful.\n"),
        ("SOUL.md", "# Soul\nYou communicate clearly and concisely.\nYou prefer practical solutions over theoretical ones.\nYou are honest about what you don't know.\n"),
        ("AGENTS.md", "# Agent Behavior\n- Think step by step\n- Use tools when needed\n- Ask for clarification when uncertain\n- Keep responses concise\n"),
        ("TOOLS.md", "# Tool Usage\n- Use `exec` for shell commands\n- Use `read_file` / `write_file` / `edit_file` for file operations\n- Use `web_search` to find information online\n- Use `list_dir` to explore directory structure\n"),
        ("USER.md", "# User Preferences\n(Add your preferences here)\n"),
        ("HEARTBEAT.md", "# Heartbeat Tasks\n(Add periodic tasks here — the agent will check this file every 30 minutes)\n"),
    ];

    for (filename, content) in &templates {
        let path = workspace_dir.join(filename);
        if !path.exists() {
            std::fs::write(&path, content).expect("Failed to write template");
            println!("  ✅ Created {}", filename);
        }
    }

    println!("\n{} Setup complete!", LOGO);
    println!("\nNext steps:");
    println!("  1. Edit ~/.quectoclaw/config.json and add your API key");
    println!("  2. Run: quectoclaw agent -m \"Hello!\"");
    println!("  3. Or start interactive mode: quectoclaw agent");
}

// ---------------------------------------------------------------------------
// Other commands
// ---------------------------------------------------------------------------

fn version_cmd() {
    println!("{} QuectoClaw v{}", LOGO, quectoclaw::VERSION);
    println!("  Built with Rust 🦀");
    println!("  Ultra-efficient AI assistant");
}

async fn status_cmd(config_path: Option<String>) {
    println!("{} QuectoClaw Status\n", LOGO);

    let cfg = load_config(config_path.as_deref());

    // Config status
    let config_path = Config::default_path().unwrap_or_default();
    if config_path.exists() {
        println!("  Config:    ✅ {}", config_path.display());
    } else {
        println!("  Config:    ❌ Not found (run 'quectoclaw onboard')");
    }

    // Workspace status
    match cfg.workspace_path() {
        Ok(ws) if ws.exists() => println!("  Workspace: ✅ {}", ws.display()),
        Ok(ws) => println!("  Workspace: ❌ {} (not created)", ws.display()),
        Err(_) => println!("  Workspace: ❌ Could not resolve path"),
    }

    // Model
    println!("  Model:     {}", cfg.agents.defaults.model);

    // Provider status
    match cfg.resolve_provider() {
        Some((_, _, name)) => println!("  Provider:  ✅ {} (key configured)", name),
        None => println!("  Provider:  ❌ No API key found"),
    }

    // Build tool registry for current config to check tools
    let workspace = cfg
        .workspace_path()
        .unwrap_or_else(|_| PathBuf::from("workspace"));
    let tools = create_tool_registry(
        &workspace.to_string_lossy(),
        cfg.agents.defaults.restrict_to_workspace,
        &cfg,
    )
    .await;

    // Tool status
    let count = tools.count().await;
    println!("  Tools:     {} registered", count);
    for summary in tools.get_summaries().await {
        println!("             {}", summary);
    }

    // Channels
    let mut channels = Vec::new();
    if cfg.channels.telegram.enabled {
        channels.push("telegram");
    }
    if cfg.channels.discord.enabled {
        channels.push("discord");
    }
    if cfg.channels.slack.enabled {
        channels.push("slack");
    }
    if channels.is_empty() {
        println!("  Channels:  None enabled");
    } else {
        println!("  Channels:  {}", channels.join(", "));
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_config(path: Option<&str>) -> Config {
    let config_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        Config::default_path().unwrap_or_else(|_| PathBuf::from("config.json"))
    };

    Config::load(&config_path).unwrap_or_else(|e| {
        tracing::warn!("Failed to load config: {}, using defaults", e);
        Config::default()
    })
}

async fn create_tool_registry(workspace: &str, restrict: bool, cfg: &Config) -> ToolRegistry {
    let registry = ToolRegistry::new();

    // We can't register tools inside the registry synchronously, so we build them here.
    let tools: Vec<Arc<dyn quectoclaw::tool::Tool>> = vec![
        Arc::new(ExecTool::new(workspace.to_string(), restrict)),
        Arc::new(ReadFileTool::new(workspace.to_string(), restrict)),
        Arc::new(WriteFileTool::new(workspace.to_string(), restrict)),
        Arc::new(ListDirTool::new(workspace.to_string(), restrict)),
        Arc::new(EditFileTool::new(workspace.to_string(), restrict)),
        Arc::new(AppendFileTool::new(workspace.to_string(), restrict)),
        Arc::new(WebSearchTool::new(
            Some(cfg.tools.web.search.api_key.clone()),
            cfg.tools.web.search.max_results,
        )),
        Arc::new(WebFetchTool::new(50_000)),
    ];

    for tool in tools {
        registry.register(tool).await;
    }

    // Register vector DB tools
    let vectordb_path = std::path::Path::new(workspace).join("memory/vectordb.json");
    let vector_store = if vectordb_path.exists() {
        match VectorStore::load(&vectordb_path) {
            Ok(store) => {
                tracing::info!(docs = store.len(), "Loaded vector store");
                store
            }
            Err(e) => {
                tracing::warn!("Failed to load vector store: {}, creating new", e);
                VectorStore::new()
            }
        }
    } else {
        VectorStore::new()
    };
    let store = Arc::new(tokio::sync::RwLock::new(vector_store));
    registry
        .register(Arc::new(VectorSearchTool::new(store.clone())))
        .await;
    registry
        .register(Arc::new(VectorIndexTool::new(store, workspace.to_string())))
        .await;

    // Load plugins from workspace/plugins/ directory
    let plugins_dir = std::path::Path::new(workspace).join("plugins");
    let plugins = quectoclaw::tool::plugin::load_plugins(&plugins_dir).await;
    if !plugins.is_empty() {
        tracing::info!(count = plugins.len(), "Loading plugins");
        quectoclaw::tool::plugin::register_plugins(&registry, plugins).await;
    }

    // Load WASM plugins from workspace/wasm_plugins/ directory
    #[cfg(feature = "wasm")]
    if cfg.wasm.enabled {
        let wasm_plugins_dir = std::path::Path::new(workspace).join("wasm_plugins");
        let wasm_plugins = quectoclaw::tool::wasm_plugin::load_wasm_plugins(&wasm_plugins_dir).await;
        if !wasm_plugins.is_empty() {
            tracing::info!(count = wasm_plugins.len(), "Loading WASM plugins");
            quectoclaw::tool::wasm_plugin::register_wasm_plugins(&registry, wasm_plugins).await;
        }
    }

    // Initialize MCP servers
    if let Err(e) = quectoclaw::mcp::init_mcp_servers(cfg, &registry).await {
        tracing::error!("Failed to initialize MCP servers: {}", e);
    }

    registry
}

// ---------------------------------------------------------------------------
// Gateway command
// ---------------------------------------------------------------------------

async fn gateway_cmd(config_path: Option<String>, use_dashboard: bool) {
    let cfg = load_config(config_path.as_deref());

    if let Err(e) = cfg.validate() {
        eprintln!("{} Configuration Error: {}", LOGO, e);
        std::process::exit(1);
    }

    let provider = match create_provider(&cfg) {
        Ok(p) => Arc::from(p),
        Err(e) => {
            eprintln!("{} Error: {}", LOGO, e);
            std::process::exit(1);
        }
    };

    let workspace = cfg
        .workspace_path()
        .unwrap_or_else(|_| PathBuf::from("/tmp/quectoclaw"));

    let ws_str = workspace.to_string_lossy().to_string();
    let restrict = cfg.agents.defaults.restrict_to_workspace;

    let tools = create_tool_registry(&ws_str, restrict, &cfg).await;
    let bus = Arc::new(MessageBus::new());

    let mut agent_loop = AgentLoop::new(cfg.clone(), provider, tools.clone(), bus.clone());

    // Setup TUI if requested
    let mut tui_state = None;
    if use_dashboard {
        use quectoclaw::tui::app::TuiState;
        let state = TuiState::new();
        quectoclaw::logger::attach_tui(state.clone());
        agent_loop.set_tui_state(state.clone());
        tui_state = Some(state);
    }

    let agent_arc = Arc::new(agent_loop);

    // Register subagent tool
    let agent_clone = agent_arc.clone();
    tools
        .register(Arc::new(SubagentTool::new(agent_clone)))
        .await;

    let gateway = quectoclaw::agent::gateway::Gateway::new(cfg.clone(), agent_arc, bus);

    if let Some(state) = tui_state {
        // Run Gateway in background
        println!("{} Starting Gateway with integrated dashboard...", LOGO);
        tokio::spawn(async move {
            if let Err(e) = gateway.run().await {
                tracing::error!("Gateway error: {}", e);
            }
        });

        // Run TUI in foreground (blocking)
        if let Err(e) = quectoclaw::tui::run(state, cfg).await {
            eprintln!("{} Dashboard error: {}", LOGO, e);
        }
    } else {
        // Run Gateway normally in foreground
        println!("{} QuectoClaw Gateway starting...", LOGO);
        if let Err(e) = gateway.run().await {
            eprintln!("{} Gateway error: {}", LOGO, e);
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Dashboard command (TUI)
// ---------------------------------------------------------------------------

async fn dashboard_cmd(config_path: Option<String>) {
    let cfg = load_config(config_path.as_deref());
    let state = quectoclaw::tui::app::TuiState::new();
    quectoclaw::logger::attach_tui(state.clone());

    if let Err(e) = quectoclaw::tui::run(state, cfg).await {
        eprintln!("{} Dashboard error: {}", LOGO, e);
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// WebUI command (Axum + HTMX)
// ---------------------------------------------------------------------------

async fn webui_cmd(config_path: Option<String>, port: u16) {
    let cfg = load_config(config_path.as_deref());
    let metrics = quectoclaw::metrics::Metrics::new();

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    println!(
        "{} QuectoClaw Web Dashboard starting at http://localhost:{}",
        LOGO, port
    );

    if let Err(e) = quectoclaw::web::start_web_server(addr, metrics, cfg).await {
        eprintln!("{} Web UI error: {}", LOGO, e);
        std::process::exit(1);
    }
}
