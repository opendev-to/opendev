//! Session tool — list and inspect conversation sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use opendev_runtime::redact_secrets;
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool for listing and reading conversation sessions.
#[derive(Debug)]
pub struct SessionTool;

impl SessionTool {
    fn sessions_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".opendev").join("sessions"))
    }
}

#[async_trait::async_trait]
impl BaseTool for SessionTool {
    fn name(&self) -> &str {
        "session"
    }

    fn description(&self) -> &str {
        "List conversation sessions or read a session's history."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "read"],
                    "description": "Action: list sessions or read a specific session"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID to read (for read action)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max sessions to list (default: 20)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::fail("action is required"),
        };

        let sessions_dir = match Self::sessions_dir() {
            Some(d) => d,
            None => return ToolResult::fail("Cannot determine home directory"),
        };

        match action {
            "list" => {
                let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
                session_list(&sessions_dir, limit)
            }
            "read" => {
                let session_id = match args.get("session_id").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => return ToolResult::fail("session_id is required for read"),
                };
                session_read(&sessions_dir, session_id)
            }
            _ => ToolResult::fail(format!("Unknown action: {action}. Available: list, read")),
        }
    }
}

fn session_list(dir: &Path, limit: usize) -> ToolResult {
    if !dir.exists() {
        return ToolResult::ok("No sessions found (directory does not exist)".to_string());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => return ToolResult::fail(format!("Failed to read sessions directory: {e}")),
    };

    let mut sessions: Vec<(String, std::time::SystemTime)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let name = path
                .file_stem()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let mtime = path
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            sessions.push((name, mtime));
        }
    }

    // Sort by most recent first
    sessions.sort_by(|a, b| b.1.cmp(&a.1));

    if sessions.is_empty() {
        return ToolResult::ok("No sessions found".to_string());
    }

    let total = sessions.len();
    let shown = sessions.len().min(limit);

    let mut output = format!("Sessions ({total} total, showing {shown}):\n");
    for (name, _) in sessions.iter().take(limit) {
        output.push_str(&format!("  {name}\n"));
    }

    let mut metadata = HashMap::new();
    metadata.insert("total".into(), serde_json::json!(total));

    ToolResult::ok_with_metadata(output, metadata)
}

fn session_read(dir: &Path, session_id: &str) -> ToolResult {
    // Prevent path traversal
    if session_id.contains("..") || session_id.contains('/') || session_id.contains('\\') {
        return ToolResult::fail("Invalid session ID");
    }

    let path = dir.join(format!("{session_id}.json"));
    if !path.exists() {
        return ToolResult::fail(format!("Session not found: {session_id}"));
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return ToolResult::fail(format!("Failed to read session: {e}")),
    };

    // Redact sensitive data using the comprehensive secrets detector
    // (catches Anthropic, OpenAI, Groq, Google, GitHub, Bearer, password, base64 secrets)
    let redacted = redact_secrets(&content);

    ToolResult::ok(redacted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_session_list_empty() {
        let tmp = TempDir::new().unwrap();
        let result = session_list(tmp.path(), 20);
        assert!(result.success);
        assert!(result.output.unwrap().contains("No sessions"));
    }

    #[test]
    fn test_session_list_with_files() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("abc123.json"), "{}").unwrap();
        std::fs::write(tmp.path().join("def456.json"), "{}").unwrap();

        let result = session_list(tmp.path(), 20);
        assert!(result.success);
        let out = result.output.unwrap();
        assert!(out.contains("abc123") || out.contains("def456"));
    }

    #[test]
    fn test_session_read() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("test123.json"), r#"{"messages":[]}"#).unwrap();

        let result = session_read(tmp.path(), "test123");
        assert!(result.success);
        assert!(result.output.unwrap().contains("messages"));
    }

    #[test]
    fn test_session_read_path_traversal() {
        let tmp = TempDir::new().unwrap();
        let result = session_read(tmp.path(), "../etc/passwd");
        assert!(!result.success);
    }

    #[test]
    fn test_redact_secrets_in_session() {
        // Anthropic API key pattern
        let text = "key: sk-ant-api03-abcdefghij1234567890abcdefghij1234567890abcdefghij normal text";
        let redacted = redact_secrets(text);
        assert!(!redacted.contains("abcdefghij1234567890"));
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("normal text"));
    }
}
