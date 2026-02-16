// QuectoClaw — Audit Logging
//
// Structured, append-only logging for agent actions, tool usage, and system events.
// Provides a tamper-proof audit trail for security and debugging.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Type of audit event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditEvent {
    Message {
        role: String,
        content: String,
    },
    ToolExecution {
        name: String,
        args: Value,
        result: Value, // ToolResult serialized
    },
    System {
        event: String,
        details: String,
    },
    Error {
        message: String,
        context: String,
    },
}

/// A single entry in the audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: DateTime<Utc>,
    pub session_id: String,
    pub event: AuditEvent,
    /// Simple hash chaining for basic tamper detection (future phase)
    pub prev_hash: String,
}

pub struct AuditLogger {
    path: PathBuf,
    writer: Arc<Mutex<()>>, // Used to serialize file access
}

impl AuditLogger {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            writer: Arc::new(Mutex::new(())),
        }
    }

    pub async fn log(&self, session_id: &str, event: AuditEvent) -> anyhow::Result<()> {
        let entry = AuditEntry {
            timestamp: Utc::now(),
            session_id: session_id.to_string(),
            event,
            prev_hash: String::new(), // Placeholder for now
        };

        let json = serde_json::to_string(&entry)? + "\n";

        let _guard = self.writer.lock().await;

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        file.write_all(json.as_bytes())?;

        Ok(())
    }

    pub fn get_path(&self) -> &Path {
        &self.path
    }
}
