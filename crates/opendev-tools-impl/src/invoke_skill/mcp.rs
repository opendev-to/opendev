//! MCP prompt integration for invoke_skill.

use std::collections::HashMap;

use opendev_mcp::McpManager;
use opendev_tools_core::ToolResult;

use super::InvokeSkillTool;

impl InvokeSkillTool {
    /// Try to resolve a skill name as an MCP prompt (`server:prompt` pattern).
    ///
    /// Returns `Some(ToolResult)` if matched, `None` if not an MCP prompt.
    pub(super) async fn try_mcp_prompt(
        &self,
        mgr: &McpManager,
        skill_name: &str,
        args: &HashMap<String, serde_json::Value>,
    ) -> Option<ToolResult> {
        let (server_name, prompt_name) = skill_name.split_once(':')?;
        if server_name.is_empty() || prompt_name.is_empty() {
            return None;
        }

        let prompt_args = args
            .get("arguments")
            .and_then(|v| v.as_str())
            .and_then(|s| {
                let s = s.trim();
                if s.is_empty() {
                    return None;
                }
                let mut map = HashMap::new();
                for pair in s.split_whitespace() {
                    if let Some((k, v)) = pair.split_once('=') {
                        map.insert(k.to_string(), v.to_string());
                    }
                }
                if map.is_empty() { None } else { Some(map) }
            });

        match mgr.get_prompt(server_name, prompt_name, prompt_args).await {
            Ok(result) => {
                let mut output = format!(
                    "MCP prompt: {server_name}:{prompt_name}\n\n<mcp_prompt name=\"{prompt_name}\">\n"
                );
                for msg in &result.messages {
                    output.push_str(&format!("[{}]\n", msg.role));
                    match &msg.content {
                        opendev_mcp::models::McpPromptContent::Text(text) => {
                            output.push_str(text);
                        }
                        opendev_mcp::models::McpPromptContent::Structured { text } => {
                            output.push_str(text);
                        }
                        opendev_mcp::models::McpPromptContent::Multiple(blocks) => {
                            for block in blocks {
                                if let opendev_mcp::models::McpContent::Text { text } = block {
                                    output.push_str(text);
                                }
                            }
                        }
                    }
                    output.push('\n');
                }
                output.push_str("</mcp_prompt>");

                if let Ok(mut invoked) = self.invoked_skills.lock() {
                    invoked.insert(skill_name.to_string());
                }

                let mut meta = HashMap::new();
                meta.insert("mcp_server".to_string(), serde_json::json!(server_name));
                meta.insert("mcp_prompt".to_string(), serde_json::json!(prompt_name));

                Some(ToolResult::ok_with_metadata(output, meta))
            }
            Err(e) => Some(ToolResult::fail(format!(
                "MCP prompt '{server_name}:{prompt_name}' failed: {e}"
            ))),
        }
    }
}
