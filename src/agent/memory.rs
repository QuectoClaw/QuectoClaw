// QuectoClaw — Long-term memory (MEMORY.md)

use std::path::Path;

/// Append an entry to the agent's long-term memory.
pub async fn append_memory(workspace: &str, entry: &str) -> anyhow::Result<()> {
    let memory_dir = Path::new(workspace).join("memory");
    tokio::fs::create_dir_all(&memory_dir).await?;

    let path = memory_dir.join("MEMORY.md");
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC");

    let line = format!("\n- [{}] {}\n", timestamp, entry);

    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .await?;
    file.write_all(line.as_bytes()).await?;

    tracing::debug!(entry = %entry, "Memory appended");
    Ok(())
}

/// Read the agent's long-term memory.
pub async fn read_memory(workspace: &str) -> String {
    let path = Path::new(workspace).join("memory").join("MEMORY.md");
    tokio::fs::read_to_string(&path).await.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_append_and_read_memory() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_str().unwrap();

        append_memory(ws, "first entry").await.unwrap();
        append_memory(ws, "second entry").await.unwrap();

        let content = read_memory(ws).await;
        assert!(content.contains("first entry"));
        assert!(content.contains("second entry"));
    }

    #[tokio::test]
    async fn test_read_memory_empty_workspace() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_str().unwrap();

        let content = read_memory(ws).await;
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn test_append_memory_creates_directory() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_str().unwrap();

        append_memory(ws, "test").await.unwrap();

        let memory_dir = tmp.path().join("memory");
        assert!(memory_dir.exists());
        assert!(memory_dir.join("MEMORY.md").exists());
    }

    #[tokio::test]
    async fn test_memory_entries_have_timestamps() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().to_str().unwrap();

        append_memory(ws, "timestamped entry").await.unwrap();

        let content = read_memory(ws).await;
        // Format: "- [YYYY-MM-DD HH:MM UTC] entry"
        assert!(content.contains("UTC"));
        assert!(content.contains("timestamped entry"));
    }
}
