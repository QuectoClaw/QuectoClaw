# QuectoClaw — Project Plan & Architecture

> **Version**: 0.1.0  
> **License**: Apache-2.0  
> **Language**: Rust 2021 Edition  
> **Inspired by**: [PicoClaw](https://github.com/sipeed/picoclaw)

---

## 1. Vision

QuectoClaw is an **ultra-efficient, self-contained AI coding assistant** built in Rust. It
connects to any OpenAI-compatible LLM API and equips the model with filesystem, execution,
web search, and sub-agent tools — all orchestrated through a streaming agent loop with
built-in conversation persistence, multi-channel delivery, and real-time monitoring.

Key design principles:
- **Minimal footprint** — release binary is < 5 MB (LTO, strip, panic=abort).
- **Zero external runtime** — no Python, no Node, no Docker required.
- **Pluggable** — JSON-defined tool plugins, provider-agnostic LLM integration.
- **Observable** — integrated metrics, structured logging, TUI dashboard.

---

## 2. Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                        CLI / TUI                         │
│   interactive_mode · one-shot · dashboard · gateway      │
├──────────────────────────────────────────────────────────┤
│                      AgentLoop                           │
│   system prompt · tool dispatch · memory · streaming     │
├────────────┬──────────────┬──────────────┬───────────────┤
│  Provider  │   ToolReg    │  Sessions    │   Metrics     │
│  (HTTP)    │   + Plugins  │  (file-JSON) │   (in-proc)   │
├────────────┼──────────────┼──────────────┼───────────────┤
│  SSE/REST  │ exec, fs,    │ fork, clear  │ LLM tokens,   │
│  streaming │ web, subagent│ summarize    │ tool stats     │
├────────────┴──────────────┴──────────────┴───────────────┤
│                     MessageBus                           │
│            InboundMessage ↔ OutboundMessage               │
├──────────────────────────────────────────────────────────┤
│                      Channels                            │
│           Telegram · Discord · Slack (optional)          │
└──────────────────────────────────────────────────────────┘
```

---

## 3. Module Map

```
src/
├── main.rs                 # CLI entry point, command routing
├── lib.rs                  # Public module declarations
├── agent/
│   ├── mod.rs              # AgentLoop — core orchestrator
│   ├── context.rs          # System prompt builder
│   ├── gateway.rs          # Multi-channel gateway service
│   └── memory.rs           # Conversation summarization
├── bus.rs                  # Async message bus (mpsc-based)
├── channel/
│   ├── mod.rs              # Channel trait
│   ├── telegram.rs         # Telegram adapter (teloxide)
│   ├── discord.rs          # Discord adapter (serenity)
│   └── slack.rs            # Slack adapter (webhook)
├── config/
│   └── mod.rs              # Hierarchical JSON config
├── logger.rs               # Structured tracing setup
├── metrics.rs              # In-process observability
├── provider/
│   ├── mod.rs              # LLMProvider trait, types
│   ├── http.rs             # OpenAI-compatible HTTP client + SSE
│   └── factory.rs          # Provider factory
├── session.rs              # File-based session persistence
├── tool/
│   ├── mod.rs              # Tool trait, ToolRegistry
│   ├── exec.rs             # Shell command execution
│   ├── filesystem.rs       # read, write, edit, list, append
│   ├── web.rs              # Web search + fetch
│   ├── subagent.rs         # Hierarchical sub-agent tool
│   └── plugin.rs           # Dynamic plugin loader
└── tui/
    ├── mod.rs              # TUI module root
    ├── app.rs              # Shared state, events, log buffer
    └── ui.rs               # Ratatui layout and rendering
```

---

## 4. Features (Current — v0.1.0)

### Core
| Feature              | Description                                                      |
|----------------------|------------------------------------------------------------------|
| Agent Loop           | Multi-iteration tool-use loop with configurable max iterations   |
| SSE Streaming        | Real-time token output in interactive mode                       |
| System Prompt        | Auto-generated from workspace context and tool registry          |
| Session Persistence  | JSON-backed conversation history with summarization              |
| Conversation Fork    | `/fork [name]` — clone session into a new branch                 |

### Tools (Built-in)
| Tool         | Description                                    |
|--------------|------------------------------------------------|
| `exec`       | Shell command execution with safety guards     |
| `read_file`  | Read file contents with line-range support     |
| `write_file` | Create or overwrite files                      |
| `edit_file`  | Surgical find-and-replace editing              |
| `append_file`| Append content to existing files               |
| `list_dir`   | Recursive directory listing                    |
| `web_search` | Web search via configurable API                |
| `web_fetch`  | HTTP GET with content truncation               |
| `subagent`   | Spawn sub-agents for complex subtasks          |

### Plugin System
- Drop a JSON file into `workspace/plugins/` to register a custom tool.
- Each plugin defines a name, description, shell command template, and parameters.
- Template syntax: `{{param_name}}` for argument substitution.
- Configurable timeout per plugin (default: 30s).

### Channels
| Channel   | Status   | Library    |
|-----------|----------|------------|
| Telegram  | Optional | teloxide   |
| Discord   | Optional | serenity   |
| Slack     | Stub     | webhook    |

### Observability
| Component   | Description                                                |
|-------------|------------------------------------------------------------|
| Metrics     | Auto-instrumented LLM token/latency and tool stats         |
| TUI         | `quectoclaw dashboard` — live stats, sessions, activity log |
| Logging     | Structured `tracing` with env-filter and colored output    |
| `/metrics`  | CLI command to print performance report in-session         |

### CLI Commands
| Command               | Description                              |
|-----------------------|------------------------------------------|
| `quectoclaw`          | Interactive agent mode (default)         |
| `quectoclaw agent -m` | One-shot mode with piped message         |
| `quectoclaw gateway`  | Launch multi-channel gateway service     |
| `quectoclaw dashboard`| TUI monitoring dashboard                 |
| `quectoclaw onboard`  | Initialize workspace and config          |
| `quectoclaw status`   | Show configuration summary               |
| `quectoclaw version`  | Show version info                        |

### Interactive Slash Commands
| Command         | Description                           |
|-----------------|---------------------------------------|
| `/fork [name]`  | Branch current conversation           |
| `/clear`        | Reset session (auto-backup)           |
| `/metrics`      | Show performance report               |
| `/help`         | List available commands               |
| `exit` / `quit` | Exit interactive mode                 |

---

## 5. Configuration

QuectoClaw uses a single JSON config file at `~/.quectoclaw/config.json`:

```json
{
  "provider": {
    "name": "openai",
    "api_key": "sk-...",
    "base_url": "https://api.openai.com/v1",
    "default_model": "gpt-4o"
  },
  "workspace": "~/.quectoclaw/workspace",
  "agents": {
    "defaults": {
      "model": "gpt-4o",
      "max_tokens": 4096,
      "temperature": 0.7,
      "max_tool_iterations": 25,
      "restrict_to_workspace": true
    }
  },
  "channels": {
    "telegram": { "enabled": false, "token": "" },
    "discord": { "enabled": false, "token": "" },
    "slack": { "enabled": false, "webhook_url": "" }
  },
  "tools": {
    "web": {
      "search": { "api_key": "", "max_results": 5 }
    }
  }
}
```

---

## 6. Build & Release

```bash
# Development
cargo build
cargo test
cargo clippy -- -D warnings

# Optimized release (< 5 MB binary)
cargo build --release

# With Telegram support
cargo build --release --features telegram

# With Discord support
cargo build --release --features discord
```

### Release Profile
```toml
[profile.release]
opt-level = "z"    # Size optimization
lto = true         # Link-time optimization
codegen-units = 1  # Single codegen unit
strip = true       # Strip debug symbols
panic = "abort"    # Abort on panic
```

---

## 7. Testing

```bash
# Run all tests
cargo test

# Current test coverage (23 tests):
# - Config: loading, defaults, env expansion
# - Provider: response parsing, tool call parsing
# - Session: add/get messages, clearing
# - Tools: exec guards, path validation, filesystem restrictions
# - Metrics: recording accuracy, report formatting
```

---

## 8. Future Roadmap

### Phase 7 — Hardening (Next)
- [ ] **Integration tests** — End-to-end test with mock LLM server
- [ ] **Error resilience** — Retry logic for transient LLM/network failures
- [ ] **Rate limiting** — Per-channel and per-user rate limits in gateway
- [ ] **Config validation** — Startup-time validation with actionable error messages

### Phase 8 — Advanced Features
- [ ] **Local vector database** — RAG-powered context retrieval using `qdrant` or `lancedb`
- [ ] **MCP (Model Context Protocol)** — Support for MCP tool servers
- [ ] **Web UI** — Lightweight web dashboard alternative to TUI (Axum + HTMX)
- [ ] **Multi-model routing** — Route different tasks to different models by capability
- [ ] **Cost tracking** — Per-model token cost estimation with budget alerts

### Phase 9 — Ecosystem
- [ ] **Plugin marketplace** — Community-contributed tool plugins
- [ ] **Workflow engine** — Multi-step task automation with YAML definitions
- [ ] **Agent memory** — Long-term knowledge base with vector similarity search
- [ ] **Audit logging** — Tamper-proof audit trail for enterprise deployments
- [ ] **WASM plugin support** — Run sandboxed plugins via WebAssembly

### Phase 10 — Distribution
- [ ] **Cross-platform binaries** — CI/CD pipeline for Linux, macOS, Windows
- [ ] **Homebrew formula** — `brew install quectoclaw`
- [ ] **Docker image** — Minimal container for gateway deployment
- [ ] **Crates.io publish** — Library crate for embedding in other Rust projects

---

## 9. Contributing

1. Fork and clone the repository.
2. Create a feature branch: `git checkout -b feat/my-feature`.
3. Follow the coding standards:
   - `cargo fmt` — format all code.
   - `cargo clippy -- -D warnings` — zero warnings policy.
   - `cargo test` — all tests must pass.
4. Commit with conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`.
5. Open a pull request.

---

## 10. License

Apache License 2.0 — See [LICENSE](LICENSE) for details.
