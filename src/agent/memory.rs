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
