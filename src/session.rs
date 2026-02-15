// QuectoClaw — Session manager (file-based conversation persistence)

use crate::provider::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionData {
    messages: Vec<Message>,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

impl Default for SessionData {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            messages: Vec::new(),
            summary: String::new(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

pub struct SessionManager {
    sessions_dir: PathBuf,
    cache: Arc<RwLock<HashMap<String, SessionData>>>,
}

impl SessionManager {
    pub fn new(workspace: &Path) -> Self {
        let sessions_dir = workspace.join("sessions");
        Self {
            sessions_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a message to a session.
    pub async fn add_message(&self, session_key: &str, message: Message) {
        let mut cache = self.cache.write().await;
        let session = cache.entry(session_key.to_string()).or_default();
        session.messages.push(message);
        session.updated_at = chrono::Utc::now().to_rfc3339();

        // Persist in background
        let data = session.clone();
        let path = self.session_path(session_key);
        let session_key_owned = session_key.to_string();
        tokio::spawn(async move {
            if let Err(e) = save_session(&path, &data).await {
                tracing::error!(session = %session_key_owned, "Failed to save session: {}", e);
            }
        });
    }

    /// Get all messages for a session.
    pub async fn get_messages(&self, session_key: &str) -> Vec<Message> {
        // Try cache first
        {
            let cache = self.cache.read().await;
            if let Some(session) = cache.get(session_key) {
                return session.messages.clone();
            }
        }

        // Try loading from disk
        let path = self.session_path(session_key);
        if path.exists() {
            match load_session(&path).await {
                Ok(data) => {
                    let messages = data.messages.clone();
                    self.cache.write().await.insert(session_key.to_string(), data);
                    return messages;
                }
                Err(e) => {
                    tracing::warn!(session = %session_key, "Failed to load session: {}", e);
                }
            }
        }

        Vec::new()
    }

    /// Get the summary for a session.
    pub async fn get_summary(&self, session_key: &str) -> String {
        let cache = self.cache.read().await;
        cache.get(session_key).map(|s| s.summary.clone()).unwrap_or_default()
    }

    /// Set the summary for a session.
    pub async fn set_summary(&self, session_key: &str, summary: String) {
        let mut cache = self.cache.write().await;
        let session = cache.entry(session_key.to_string()).or_default();
        session.summary = summary;

        let data = session.clone();
        let path = self.session_path(session_key);
        let _session_key_owned = session_key.to_string();
        tokio::spawn(async move {
            if let Err(e) = save_session(&path, &data).await {
                tracing::error!(session = %_session_key_owned, "Failed to save session summary: {}", e);
            }
        });
    }

    /// Get the number of messages in a session.
    pub async fn message_count(&self, session_key: &str) -> usize {
        self.get_messages(session_key).await.len()
    }

    /// Clear all messages from a session.
    pub async fn clear(&self, session_key: &str) {
        self.cache.write().await.remove(session_key);
        let path = self.session_path(session_key);
        let _ = tokio::fs::remove_file(path).await;
    }

    /// List all session keys.
    pub async fn list_sessions(&self) -> Vec<String> {
        let mut sessions = Vec::new();

        if let Ok(mut entries) = tokio::fs::read_dir(&self.sessions_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".json") {
                        sessions.push(name.trim_end_matches(".json").to_string());
                    }
                }
            }
        }

        sessions
    }

    fn session_path(&self, session_key: &str) -> PathBuf {
        // Sanitize session key for filesystem
        let safe_name = session_key.replace(['/', '\\', ':', '|'], "_");
        self.sessions_dir.join(format!("{}.json", safe_name))
    }
}

async fn save_session(path: &Path, data: &SessionData) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Atomic write: write to temp file then rename
    let tmp_path = path.with_extension("json.tmp");
    let content = serde_json::to_string_pretty(data)?;
    tokio::fs::write(&tmp_path, &content).await?;
    tokio::fs::rename(&tmp_path, path).await?;

    Ok(())
}

async fn load_session(path: &Path) -> anyhow::Result<SessionData> {
    let content = tokio::fs::read_to_string(path).await?;
    let data: SessionData = serde_json::from_str(&content)?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_add_and_get_messages() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path());

        mgr.add_message("test", Message::user("Hello")).await;
        mgr.add_message("test", Message::assistant("Hi there")).await;

        // Small delay for async persistence
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let messages = mgr.get_messages("test").await;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }

    #[tokio::test]
    async fn test_clear_session() {
        let tmp = TempDir::new().unwrap();
        let mgr = SessionManager::new(tmp.path());

        mgr.add_message("test", Message::user("Hello")).await;
        mgr.clear("test").await;

        let messages = mgr.get_messages("test").await;
        assert!(messages.is_empty());
    }
}
