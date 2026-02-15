// QuectoClaw — Filesystem tools (read_file, write_file, list_dir, edit_file, append_file)

use super::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Path validation
// ---------------------------------------------------------------------------

fn validate_path(path: &str, workspace: &str, restrict: bool) -> Result<PathBuf, String> {
    if workspace.is_empty() {
        return Ok(PathBuf::from(path));
    }

    let abs_workspace = std::fs::canonicalize(workspace)
        .or_else(|_| Ok::<PathBuf, String>(PathBuf::from(workspace)))
        .unwrap();

    let abs_path = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        abs_workspace.join(path)
    };

    // Normalize the path (resolve . and ..)
    let abs_path = abs_path.canonicalize().unwrap_or_else(|_| abs_path.clone());

    if restrict && !abs_path.starts_with(&abs_workspace) {
        return Err("access denied: path is outside the workspace".into());
    }

    Ok(abs_path)
}

// ---------------------------------------------------------------------------
// ReadFileTool
// ---------------------------------------------------------------------------

pub struct ReadFileTool {
    workspace: String,
    restrict: bool,
}

impl ReadFileTool {
    pub fn new(workspace: String, restrict: bool) -> Self {
        Self {
            workspace,
            restrict,
        }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }
    fn description(&self) -> &str {
        "Read the contents of a file"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to read" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("path is required"),
        };

        let resolved = match validate_path(path, &self.workspace, self.restrict) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };

        match tokio::fs::read_to_string(&resolved).await {
            Ok(content) => ToolResult::success(content),
            Err(e) => ToolResult::error(format!("failed to read file: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// WriteFileTool
// ---------------------------------------------------------------------------

pub struct WriteFileTool {
    workspace: String,
    restrict: bool,
}

impl WriteFileTool {
    pub fn new(workspace: String, restrict: bool) -> Self {
        Self {
            workspace,
            restrict,
        }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }
    fn description(&self) -> &str {
        "Write content to a file (creates parent directories)"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to write" },
                "content": { "type": "string", "description": "Content to write to the file" }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("path is required"),
        };
        let content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("content is required"),
        };

        let resolved = match validate_path(path, &self.workspace, self.restrict) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };

        // Create parent directories
        if let Some(parent) = resolved.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolResult::error(format!("failed to create directory: {}", e));
            }
        }

        match tokio::fs::write(&resolved, content).await {
            Ok(_) => ToolResult::silent(format!("File written: {}", path)),
            Err(e) => ToolResult::error(format!("failed to write file: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// ListDirTool
// ---------------------------------------------------------------------------

pub struct ListDirTool {
    workspace: String,
    restrict: bool,
}

impl ListDirTool {
    pub fn new(workspace: String, restrict: bool) -> Self {
        Self {
            workspace,
            restrict,
        }
    }
}

#[async_trait]
impl Tool for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }
    fn description(&self) -> &str {
        "List files and directories in a path"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to list" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let resolved = match validate_path(path, &self.workspace, self.restrict) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };

        let mut entries = match tokio::fs::read_dir(&resolved).await {
            Ok(e) => e,
            Err(e) => return ToolResult::error(format!("failed to read directory: {}", e)),
        };

        let mut result = String::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_type = entry.file_type().await.ok();
            let prefix = if file_type.as_ref().is_some_and(|ft| ft.is_dir()) {
                "DIR:  "
            } else {
                "FILE: "
            };
            result.push_str(prefix);
            result.push_str(&entry.file_name().to_string_lossy());
            result.push('\n');
        }

        ToolResult::success(result)
    }
}

// ---------------------------------------------------------------------------
// EditFileTool
// ---------------------------------------------------------------------------

pub struct EditFileTool {
    workspace: String,
    restrict: bool,
}

impl EditFileTool {
    pub fn new(workspace: String, restrict: bool) -> Self {
        Self {
            workspace,
            restrict,
        }
    }
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }
    fn description(&self) -> &str {
        "Edit a file by replacing old_text with new_text"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to edit" },
                "old_text": { "type": "string", "description": "Text to search for (must match exactly)" },
                "new_text": { "type": "string", "description": "Text to replace with" }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("path is required"),
        };
        let old_text = match args.get("old_text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("old_text is required"),
        };
        let new_text = match args.get("new_text").and_then(|v| v.as_str()) {
            Some(t) => t,
            None => return ToolResult::error("new_text is required"),
        };

        let resolved = match validate_path(path, &self.workspace, self.restrict) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };

        let content = match tokio::fs::read_to_string(&resolved).await {
            Ok(c) => c,
            Err(e) => return ToolResult::error(format!("failed to read file: {}", e)),
        };

        if !content.contains(old_text) {
            return ToolResult::error("old_text not found in file");
        }

        let new_content = content.replacen(old_text, new_text, 1);

        match tokio::fs::write(&resolved, new_content).await {
            Ok(_) => ToolResult::silent(format!("File edited: {}", path)),
            Err(e) => ToolResult::error(format!("failed to write file: {}", e)),
        }
    }
}

// ---------------------------------------------------------------------------
// AppendFileTool
// ---------------------------------------------------------------------------

pub struct AppendFileTool {
    workspace: String,
    restrict: bool,
}

impl AppendFileTool {
    pub fn new(workspace: String, restrict: bool) -> Self {
        Self {
            workspace,
            restrict,
        }
    }
}

#[async_trait]
impl Tool for AppendFileTool {
    fn name(&self) -> &str {
        "append_file"
    }
    fn description(&self) -> &str {
        "Append content to the end of a file"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" },
                "content": { "type": "string", "description": "Content to append" }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => return ToolResult::error("path is required"),
        };
        let content = match args.get("content").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("content is required"),
        };

        let resolved = match validate_path(path, &self.workspace, self.restrict) {
            Ok(p) => p,
            Err(e) => return ToolResult::error(e),
        };

        use tokio::io::AsyncWriteExt;
        let mut file = match tokio::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&resolved)
            .await
        {
            Ok(f) => f,
            Err(e) => return ToolResult::error(format!("failed to open file: {}", e)),
        };

        match file.write_all(content.as_bytes()).await {
            Ok(_) => ToolResult::silent(format!("Content appended to: {}", path)),
            Err(e) => ToolResult::error(format!("failed to append: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_absolute() {
        let result = validate_path("/tmp/test.txt", "/tmp", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_outside_workspace() {
        let result = validate_path("/etc/passwd", "/tmp", true);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_unrestricted() {
        let result = validate_path("/etc/passwd", "/tmp", false);
        assert!(result.is_ok());
    }
}
