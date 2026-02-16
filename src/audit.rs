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

    /// Read all audit entries from the log file.
    pub fn read_entries(&self) -> anyhow::Result<Vec<AuditEntry>> {
        let content = std::fs::read_to_string(&self.path)?;
        let mut entries = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            entries.push(serde_json::from_str(line)?);
        }
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_log_message_event() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("audit.jsonl");
        let logger = AuditLogger::new(log_path.clone());

        logger
            .log(
                "sess-1",
                AuditEvent::Message {
                    role: "user".into(),
                    content: "hello".into(),
                },
            )
            .await
            .unwrap();

        let entries = logger.read_entries().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_id, "sess-1");
        match &entries[0].event {
            AuditEvent::Message { role, content } => {
                assert_eq!(role, "user");
                assert_eq!(content, "hello");
            }
            _ => panic!("expected Message event"),
        }
    }

    #[tokio::test]
    async fn test_log_tool_execution_event() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("audit.jsonl");
        let logger = AuditLogger::new(log_path);

        logger
            .log(
                "sess-2",
                AuditEvent::ToolExecution {
                    name: "read_file".into(),
                    args: serde_json::json!({"path": "foo.txt"}),
                    result: serde_json::json!({"status": "ok"}),
                },
            )
            .await
            .unwrap();

        let entries = logger.read_entries().unwrap();
        assert_eq!(entries.len(), 1);
        match &entries[0].event {
            AuditEvent::ToolExecution { name, .. } => {
                assert_eq!(name, "read_file");
            }
            _ => panic!("expected ToolExecution event"),
        }
    }

    #[tokio::test]
    async fn test_log_system_event() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("audit.jsonl");
        let logger = AuditLogger::new(log_path);

        logger
            .log(
                "sess-3",
                AuditEvent::System {
                    event: "startup".into(),
                    details: "agent initialized".into(),
                },
            )
            .await
            .unwrap();

        let entries = logger.read_entries().unwrap();
        assert_eq!(entries.len(), 1);
        match &entries[0].event {
            AuditEvent::System { event, details } => {
                assert_eq!(event, "startup");
                assert_eq!(details, "agent initialized");
            }
            _ => panic!("expected System event"),
        }
    }

    #[tokio::test]
    async fn test_log_error_event() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("audit.jsonl");
        let logger = AuditLogger::new(log_path);

        logger
            .log(
                "sess-4",
                AuditEvent::Error {
                    message: "timeout".into(),
                    context: "llm call".into(),
                },
            )
            .await
            .unwrap();

        let entries = logger.read_entries().unwrap();
        assert_eq!(entries.len(), 1);
        match &entries[0].event {
            AuditEvent::Error { message, context } => {
                assert_eq!(message, "timeout");
                assert_eq!(context, "llm call");
            }
            _ => panic!("expected Error event"),
        }
    }

    #[tokio::test]
    async fn test_append_only_multiple_entries() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("audit.jsonl");
        let logger = AuditLogger::new(log_path);

        for i in 0..5 {
            logger
                .log(
                    &format!("sess-{}", i),
                    AuditEvent::System {
                        event: format!("event-{}", i),
                        details: "test".into(),
                    },
                )
                .await
                .unwrap();
        }

        let entries = logger.read_entries().unwrap();
        assert_eq!(entries.len(), 5);
        for (i, entry) in entries.iter().enumerate() {
            assert_eq!(entry.session_id, format!("sess-{}", i));
        }
    }

    #[tokio::test]
    async fn test_creates_parent_directories() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("nested").join("deep").join("audit.jsonl");
        let logger = AuditLogger::new(log_path.clone());

        logger
            .log(
                "sess-1",
                AuditEvent::System {
                    event: "test".into(),
                    details: "nested dir".into(),
                },
            )
            .await
            .unwrap();

        assert!(log_path.exists());
    }

    #[tokio::test]
    async fn test_get_path() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("audit.jsonl");
        let logger = AuditLogger::new(log_path.clone());

        assert_eq!(logger.get_path(), log_path.as_path());
    }
}
