---
description: how to create and run WASM plugins
---

# Creating and Running WASM Plugins

QuectoClaw supports sandboxed plugins via WebAssembly. Follow these steps to create and run your own.

## 1. Prerequisites

Ensure you have the WASM target installed for Rust:
```bash
rustup target add wasm32-wasip1
```

## 2. Create a Plugin

1. Create a new Rust project:
```bash
cargo new my_plugin
cd my_plugin
```

2. Add `serde` and `serde_json` to `Cargo.toml`:
```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

3. Implement your logic in `src/main.rs`. Use `stdin` for input and `stdout` for output:
```rust
use std::io::{self, Read};
use serde::Deserialize;

#[derive(Deserialize)]
struct Input {
    name: String,
}

fn main() {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).unwrap();
    let input: Input = serde_json::from_str(&buffer).unwrap();
    println!("Hello, {}! From WASM.", input.name);
}
```

## 3. Build the Plugin

Compile to WASM:
// turbo
```bash
cargo build --target wasm32-wasip1 --release
```

## 4. Install the Plugin

1. Find your workspace directory (usually `~/.quectoclaw/workspace` or your current project root).
2. Create a folder: `mkdir -p workspace/wasm_plugins/my_plugin`.
3. Copy the `.wasm` file:
```bash
cp target/wasm32-wasip1/release/my_plugin.wasm workspace/wasm_plugins/my_plugin/
```
4. Create a `manifest.json` in the same folder:
```json
{
  "name": "my_plugin",
  "description": "My first WASM plugin",
  "wasm_file": "my_plugin.wasm",
  "parameters": [
    {
      "name": "name",
      "description": "Name to greet",
      "param_type": "string",
      "required": true
    }
  ],
  "fuel": 1000000
}
```

## 5. Enable WASM in Config

In `~/.quectoclaw/config.json`, ensure WASM is enabled:
```json
{
  "wasm": {
    "enabled": true,
    "fuel_limit": 1000000
  }
}
```

## 6. Run QuectoClaw

Run with the `wasm` feature enabled:
// turbo
```bash
cargo run --features wasm
```

The tool `my_plugin` will now be available for the LLM to use.
