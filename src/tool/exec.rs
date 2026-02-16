// QuectoClaw — Shell command execution tool

use super::{Tool, ToolResult};
use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;

use std::time::Duration;
use tokio::process::Command;

pub struct ExecTool {
    working_dir: String,
    timeout: Duration,
    deny_patterns: Vec<Regex>,
    restrict_to_workspace: bool,
    allowed_commands: Vec<String>,
    forbidden_paths: Vec<String>,
}

impl ExecTool {
    pub fn new(
        working_dir: String,
        restrict: bool,
        allowed_commands: Vec<String>,
        forbidden_paths: Vec<String>,
    ) -> Self {
        let deny_patterns = vec![
            Regex::new(r"\brm\s+-[rf]{1,2}\b").unwrap(),
            Regex::new(r"\bdel\s+/[fq]\b").unwrap(),
            Regex::new(r"\brmdir\s+/s\b").unwrap(),
            Regex::new(r"\b(format|mkfs|diskpart)\b\s").unwrap(),
            Regex::new(r"\bdd\s+if=").unwrap(),
            Regex::new(r">\s*/dev/sd[a-z]\b").unwrap(),
            Regex::new(r"\b(shutdown|reboot|poweroff)\b").unwrap(),
            Regex::new(r":\(\)\s*\{.*\};\s*:").unwrap(),
        ];

        Self {
            working_dir,
            timeout: Duration::from_secs(60),
            deny_patterns,
            restrict_to_workspace: restrict,
            allowed_commands,
            forbidden_paths,
        }
    }

    /// Resolve forbidden path entries (expand ~ to home dir).
    fn resolve_forbidden_paths(&self) -> Vec<PathBuf> {
        let home = dirs::home_dir();
        self.forbidden_paths
            .iter()
            .map(|p| {
                if let Some(stripped) = p.strip_prefix("~/") {
                    if let Some(ref h) = home {
                        return h.join(stripped);
                    }
                }
                PathBuf::from(p)
            })
            .collect()
    }

    fn guard_command(&self, command: &str, cwd: &str) -> Option<String> {
        // Reject null bytes (injection vector)
        if command.contains('\0') || cwd.contains('\0') {
            return Some("Command blocked: null byte detected".into());
        }

        let lower = command.to_lowercase();

        // --- Allowlist gate (primary) ---
        if !self.allowed_commands.is_empty() {
            // Extract the first token (the program name)
            let first_token = command
                .split(|c: char| c.is_whitespace() || c == ';' || c == '|' || c == '&')
                .find(|s| !s.is_empty())
                .unwrap_or("");

            // Also check after any env-var assignments (e.g. FOO=bar cmd)
            let program = if first_token.contains('=') {
                command
                    .split_whitespace()
                    .find(|t| !t.contains('='))
                    .unwrap_or(first_token)
            } else {
                first_token
            };

            // Extract basename (strip path)
            let basename = program.rsplit('/').next().unwrap_or(program);

            if !self
                .allowed_commands
                .iter()
                .any(|a| a == basename || a == program)
            {
                return Some(format!(
                    "Command blocked: '{}' is not in the allowed commands list. Allowed: {:?}",
                    basename, self.allowed_commands
                ));
            }
        }

        // --- Deny-list gate (secondary defense-in-depth) ---
        for pattern in &self.deny_patterns {
            if pattern.is_match(&lower) {
                return Some("Command blocked by safety guard (dangerous pattern detected)".into());
            }
        }

        // --- Workspace restriction ---
        if self.restrict_to_workspace {
            if command.contains("../") || command.contains("..\\") {
                return Some("Command blocked by safety guard (path traversal detected)".into());
            }

            // Check for absolute paths outside workspace
            let cwd_path = match std::path::Path::new(cwd).canonicalize() {
                Ok(p) => p,
                Err(_) => return None,
            };

            let path_re = Regex::new(r"[A-Za-z]:\\[^\\\x22']+|/[^\s\x22']+").unwrap();
            for raw in path_re.find_iter(command) {
                if let Ok(abs) = std::path::Path::new(raw.as_str()).canonicalize() {
                    if !abs.starts_with(&cwd_path) {
                        return Some(
                            "Command blocked by safety guard (path outside working dir)".into(),
                        );
                    }
                }
            }
        }

        // --- Forbidden paths check ---
        let forbidden = self.resolve_forbidden_paths();
        let path_re = Regex::new(r"[A-Za-z]:\\[^\\\x22']+|/[^\s\x22']+").unwrap();
        for raw in path_re.find_iter(command) {
            let raw_path = std::path::Path::new(raw.as_str());
            let resolved = raw_path.canonicalize().unwrap_or_else(|_| raw_path.to_path_buf());
            for fp in &forbidden {
                if resolved.starts_with(fp) {
                    return Some(format!(
                        "Command blocked: accesses forbidden path '{}'",
                        fp.display()
                    ));
                }
            }
        }

        None
    }
}

use std::path::PathBuf;

#[async_trait]
impl Tool for ExecTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Use with caution."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory for the command"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: HashMap<String, Value>) -> ToolResult {
        let command = match args.get("command").and_then(|v| v.as_str()) {
            Some(c) => c,
            None => return ToolResult::error("command is required"),
        };

        let cwd = args
            .get("working_dir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.working_dir);

        // Validate working_dir against workspace boundary
        if self.restrict_to_workspace {
            if cwd.contains('\0') {
                return ToolResult::error("working_dir blocked: null byte detected");
            }
            if let Ok(workspace_abs) = std::path::Path::new(&self.working_dir).canonicalize() {
                let cwd_path = std::path::Path::new(cwd);
                let cwd_resolved = cwd_path
                    .canonicalize()
                    .unwrap_or_else(|_| cwd_path.to_path_buf());
                if !cwd_resolved.starts_with(&workspace_abs) {
                    return ToolResult::error(
                        "working_dir blocked: path is outside the workspace",
                    );
                }
            }
        }

        if let Some(err) = self.guard_command(command, cwd) {
            return ToolResult::error(err);
        }

        let result = tokio::time::timeout(self.timeout, async {
            let output = Command::new("sh")
                .arg("-c")
                .arg(command)
                .current_dir(cwd)
                .output()
                .await;
            output
        })
        .await;

        match result {
            Ok(Ok(output)) => {
                let mut text = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    text.push_str("\nSTDERR:\n");
                    text.push_str(&stderr);
                }
                if !output.status.success() {
                    text.push_str(&format!("\nExit code: {}", output.status));
                }
                if text.is_empty() {
                    text = "(no output)".into();
                }
                // Truncate very large outputs
                let max_len = 10000;
                if text.len() > max_len {
                    let extra = text.len() - max_len;
                    text.truncate(max_len);
                    text.push_str(&format!("\n... (truncated, {} more chars)", extra));
                }
                if output.status.success() {
                    ToolResult::success(text)
                } else {
                    ToolResult::error(text)
                }
            }
            Ok(Err(e)) => ToolResult::error(format!("Failed to execute command: {}", e)),
            Err(_) => ToolResult::error(format!("Command timed out after {:?}", self.timeout)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool(restrict: bool) -> ExecTool {
        ExecTool::new("/tmp".into(), restrict, vec![], vec![])
    }

    fn make_tool_with_allowlist(cmds: Vec<&str>) -> ExecTool {
        ExecTool::new(
            "/tmp".into(),
            false,
            cmds.into_iter().map(String::from).collect(),
            vec![],
        )
    }

    #[test]
    fn test_guard_dangerous_commands() {
        let tool = make_tool(false);
        assert!(tool.guard_command("rm -rf /", "/tmp").is_some());
        assert!(tool.guard_command("shutdown now", "/tmp").is_some());
        assert!(tool.guard_command("dd if=/dev/zero", "/tmp").is_some());
        assert!(tool.guard_command(":(){ :|:& };:", "/tmp").is_some());
    }

    #[test]
    fn test_guard_safe_commands() {
        let tool = make_tool(false);
        assert!(tool.guard_command("ls -la", "/tmp").is_none());
        assert!(tool.guard_command("cat file.txt", "/tmp").is_none());
        assert!(tool.guard_command("echo hello", "/tmp").is_none());
    }

    #[test]
    fn test_guard_path_traversal() {
        let tool = make_tool(true);
        assert!(tool
            .guard_command("cat ../../../etc/passwd", "/tmp")
            .is_some());
    }

    #[test]
    fn test_guard_null_byte() {
        let tool = make_tool(false);
        assert!(tool.guard_command("cat file\0.txt", "/tmp").is_some());
    }

    #[test]
    fn test_allowlist_blocks_unlisted() {
        let tool = make_tool_with_allowlist(vec!["ls", "cat", "echo"]);
        assert!(tool.guard_command("curl http://evil.com", "/tmp").is_some());
        assert!(tool.guard_command("wget http://evil.com", "/tmp").is_some());
    }

    #[test]
    fn test_allowlist_permits_listed() {
        let tool = make_tool_with_allowlist(vec!["ls", "cat", "echo"]);
        assert!(tool.guard_command("ls -la", "/tmp").is_none());
        assert!(tool.guard_command("cat file.txt", "/tmp").is_none());
        assert!(tool.guard_command("echo hello", "/tmp").is_none());
    }

    #[test]
    fn test_forbidden_paths() {
        let tool = ExecTool::new(
            "/tmp".into(),
            false,
            vec![],
            vec!["/etc".into(), "/root".into()],
        );
        assert!(tool.guard_command("cat /etc/passwd", "/tmp").is_some());
    }

    #[tokio::test]
    async fn test_exec_echo() {
        let tool = make_tool(false);
        let mut args = HashMap::new();
        args.insert("command".into(), Value::String("echo hello".into()));
        let result = tool.execute(args).await;
        assert!(!result.is_error);
        assert!(result.for_llm.contains("hello"));
    }
}
