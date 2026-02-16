// QuectoClaw — Ultra-efficient AI assistant in Rust
// Inspired by PicoClaw: https://github.com/sipeed/picoclaw
// License: Apache-2.0

pub mod agent;
pub mod audit;
pub mod bus;
pub mod channel;
pub mod config;
pub mod logger;
pub mod market;
pub mod mcp;
pub mod metrics;
pub mod provider;
pub mod session;
pub mod tool;
pub mod tui;
pub mod vectordb;
pub mod web;
pub mod workflow;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
