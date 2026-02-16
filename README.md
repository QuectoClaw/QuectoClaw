<p align="center">
  <img src="assets/logo.png" alt="QuectoClaw Logo" width="200" />
</p>

<h1 align="center">QuectoClaw</h1>

<p align="center">
  <strong>Ultra-efficient AI coding assistant — built in Rust 🦀</strong>
</p>

<p align="center">
  <a href="#features"><img src="https://img.shields.io/badge/tools-9%20built--in-cyan" alt="Tools" /></a>
  <a href="#installation"><img src="https://img.shields.io/badge/binary-<5MB-orange" alt="Size" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="License" /></a>
  <a href="#testing"><img src="https://img.shields.io/badge/tests-23%20passing-green" alt="Tests" /></a>
</p>

---

## What is QuectoClaw?

QuectoClaw is a **self-contained AI assistant** that connects to any OpenAI-compatible API and equips the model with powerful tools — file editing, shell execution, web search, sub-agents, and more. It features **real-time streaming**, **conversation branching**, a **TUI dashboard**, and a **plugin system** for extensibility.

```
You: Build a REST API with health check and user endpoints

🤖 ← streaming tokens in real-time...

I'll set up the project structure and implement the endpoints.

⚙ Calling exec: cargo init --name api
⚙ Calling write_file: src/main.rs
⚙ Calling exec: cargo build

✓ Done! The API is ready at src/main.rs with:
  - GET /health — returns status
  - GET /users — lists users
  - POST /users — creates user
```

---

## Features

### 🧠 Intelligent Agent Loop
- Multi-iteration tool-use with configurable depth
- Automatic conversation summarization for long sessions
- Hierarchical sub-agents for complex tasks

### ⚡ Real-Time Streaming
- Server-Sent Events (SSE) for token-by-token output
- Live tool call indicators in the terminal
- Non-blocking async architecture

### 🛠️ 9 Built-in Tools
| Tool | Description |
|------|-------------|
| `exec` | Safe shell execution with command guards |
| `read_file` | Read files with line-range support |
| `write_file` | Create or overwrite files |
| `edit_file` | Surgical find-and-replace |
| `append_file` | Append content to files |
| `list_dir` | Recursive directory listing |
| `web_search` | Search the web via API |
| `web_fetch` | Fetch and parse web pages |
| `subagent` | Spawn focused sub-agents |

### 🔌 Plugin System
Extend QuectoClaw with custom tools — just drop a JSON file in `plugins/`:

```json
{
  "name": "docker_ps",
  "description": "List running Docker containers",
  "command": "docker ps --format '{{.Names}}: {{.Status}}'",
  "parameters": [],
  "timeout": 10
}
```

### ✂️ Conversation Branching
```
> /fork experiment-branch
✂️  Forked session to: experiment-branch
   (Restart with --session experiment-branch to use it)

> /metrics
═══ QuectoClaw Metrics ═══
Uptime:       00:12:34
LLM Requests: 8
Tokens:       12,450 (prompt: 9,200, completion: 3,250)
Tool Calls:   15 (0 errors)
```

### 📊 TUI Monitoring Dashboard
```bash
quectoclaw dashboard
```
Real-time terminal dashboard with:
- Live stats (requests, tokens, tool calls, errors)
- Active session tracking
- Scrolling activity log with color-coded levels
- Keyboard shortcuts (`q` quit, `c` clear logs)

### 📡 Multi-Channel Gateway
Deploy as a service that connects to messaging platforms:
- **Telegram** (via teloxide)
- **Discord** (via serenity)
- **Slack** (webhook stub)

```bash
quectoclaw gateway
```

---

## Installation

### From Source

```bash
git clone https://github.com/your-username/QuectoClaw.git
cd QuectoClaw
cargo build --release
```

The optimized binary will be at `target/release/quectoclaw` (< 5 MB).

### With Channel Support

```bash
# Telegram support
cargo build --release --features telegram

# Discord support
cargo build --release --features discord

# Both
cargo build --release --features "telegram,discord"
```

---

## Quick Start

### 1. Initialize

```bash
quectoclaw onboard
```

This creates `~/.quectoclaw/config.json` — add your API key there.

### 2. Chat

```bash
# Interactive mode (default)
quectoclaw

# One-shot mode
quectoclaw agent -m "Explain this Rust project's architecture"

# With a specific session
quectoclaw agent -s my-project -m "Add unit tests"
```

### 3. Monitor

```bash
# Check status
quectoclaw status

# Launch TUI dashboard
quectoclaw dashboard

# Run the gateway
quectoclaw gateway
```

---

## Configuration

Create or edit `~/.quectoclaw/config.json`:

```json
{
  "provider": {
    "name": "openai",
    "api_key": "sk-your-key-here",
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
  }
}
```

> **Compatible providers**: OpenAI, Anthropic (via proxy), Ollama, LM Studio, OpenRouter, Groq — any OpenAI-compatible endpoint.

---

## Architecture

```
CLI / TUI ─── AgentLoop ─── Provider (HTTP + SSE)
                  │
          ┌───────┼───────┐
        Tools   Sessions  Metrics
          │
   ┌──────┼──────┐
 exec   fs/web  plugins
```

See [plan.md](plan.md) for the full architecture documentation, module map, and future roadmap.

---

## Testing

```bash
cargo test            # 23 tests
cargo clippy -- -D warnings   # Zero-warning policy
cargo fmt             # Consistent formatting
```

---

## Roadmap

| Phase | Focus | Status |
|-------|-------|--------|
| 1-5 | Core foundation, tools, agent loop, channels, CLI | ✅ Complete |
| 6 | Streaming, branching, plugins, TUI, metrics | ✅ Complete |
| 7 | Integration tests, error resilience, rate limiting | 🔜 Next |
| 8 | Vector DB, MCP, Web UI, multi-model routing | 📋 Planned |
| 9 | Plugin marketplace, workflow engine, audit logs | 📋 Planned |
| 10 | Cross-platform CI/CD, Homebrew, Docker, crates.io | 📋 Planned |

See [plan.md](plan.md) for detailed roadmap.

---

## License

[Apache License 2.0](LICENSE)

---

<p align="center">
  <sub>Built with 🦀 Rust • Inspired by <a href="https://github.com/sipeed/picoclaw">PicoClaw</a></sub>
</p>
