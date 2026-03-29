//! Task complete tool — signal explicit task completion.
//!
//! Instead of relying on implicit termination (no tool calls = done),
//! agents call this tool to end the ReAct loop. This provides:
//! - Explicit completion signal (no ambiguity)
//! - Required summary of what was accomplished
//! - Natural error recovery (agent keeps trying until this is called)
//! - Clean conversation history

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Valid completion statuses.
const VALID_STATUSES: &[&str] = &["success", "partial", "failed"];

/// Tool that signals explicit task completion.
#[derive(Debug)]
pub struct TaskCompleteTool;

#[async_trait::async_trait]
impl BaseTool for TaskCompleteTool {
    fn name(&self) -> &str {
        "task_complete"
    }

    fn description(&self) -> &str {
        "Call this tool when you have completed the user's request. \
         You MUST call this tool to end the conversation. \
         Provide your response to the user in the result parameter."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "result": {
                    "type": "string",
                    "description": "Your response to the user — what was accomplished or your conversational reply"
                },
                "status": {
                    "type": "string",
                    "description": "Completion status: 'success', 'partial', or 'failed'",
                    "enum": VALID_STATUSES,
                    "default": "success"
                }
            },
            "required": ["result"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let summary = match args
            .get("result")
            .or_else(|| args.get("summary"))
            .and_then(|v| v.as_str())
        {
            Some(s) if !s.trim().is_empty() => s.trim(),
            _ => return ToolResult::fail("Result is required for task_complete"),
        };

        let status = args
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("success");

        if !VALID_STATUSES.contains(&status) {
            return ToolResult::fail(format!(
                "Invalid status '{status}'. Must be one of: {}",
                VALID_STATUSES.join(", ")
            ));
        }

        let output = format!("Task completed ({status}): {summary}");

        let mut metadata = HashMap::new();
        metadata.insert("_completion".into(), serde_json::json!(true));
        metadata.insert("summary".into(), serde_json::json!(summary));
        metadata.insert("status".into(), serde_json::json!(status));

        ToolResult::ok_with_metadata(output, metadata)
    }
}

#[cfg(test)]
#[path = "task_complete_tests.rs"]
mod tests;
