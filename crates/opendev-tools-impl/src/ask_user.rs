//! Ask user tool — pose structured questions to the user via a channel.

use std::collections::HashMap;

use opendev_runtime::{AskUserRequest, AskUserSender};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool for asking the user a question during agent execution.
///
/// When a `ask_tx` channel is set (TUI mode), the tool blocks until
/// the user answers. When `None` (headless/pipe mode), the tool
/// formats the question and returns immediately.
#[derive(Debug)]
pub struct AskUserTool {
    /// Channel to send ask-user requests to the TUI.
    ask_tx: Option<AskUserSender>,
}

impl AskUserTool {
    /// Create an ask_user tool without a channel (headless mode).
    pub fn new() -> Self {
        Self { ask_tx: None }
    }

    /// Attach an ask-user channel for interactive (TUI) mode.
    pub fn with_ask_tx(mut self, tx: AskUserSender) -> Self {
        self.ask_tx = Some(tx);
        self
    }
}

impl Default for AskUserTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl BaseTool for AskUserTool {
    fn name(&self) -> &str {
        "AskUserQuestion"
    }

    fn description(&self) -> &str {
        "Ask the user a question and wait for their response. Use when clarification is needed."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "The question to ask the user"
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of choices for the user"
                },
                "default": {
                    "type": "string",
                    "description": "Default answer if user provides none"
                }
            },
            "required": ["question"]
        })
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Meta
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let question = match args.get("question").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::fail("question is required"),
        };

        let options: Vec<String> = args
            .get("options")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let default = args
            .get("default")
            .and_then(|v| v.as_str())
            .map(String::from);

        // --- Interactive mode: block until user answers ---
        if let Some(ref tx) = self.ask_tx {
            let (resp_tx, resp_rx) = tokio::sync::oneshot::channel();
            if tx
                .send(AskUserRequest {
                    question: question.to_string(),
                    options: options.clone(),
                    default: default.clone(),
                    response_tx: resp_tx,
                })
                .is_ok()
            {
                match resp_rx.await {
                    Ok(answer) => {
                        return ToolResult::ok(format!("User answered: {answer}"));
                    }
                    Err(_) => {
                        // Channel dropped — fall through to headless
                    }
                }
            }
        }

        // --- Headless mode: format question and return ---
        let mut output = format!("Question: {question}");
        if !options.is_empty() {
            output.push_str("\nOptions:");
            for (i, opt) in options.iter().enumerate() {
                output.push_str(&format!("\n  {}. {opt}", i + 1));
            }
        }
        if let Some(d) = &default {
            output.push_str(&format!("\nDefault: {d}"));
        }

        let mut metadata = HashMap::new();
        metadata.insert("requires_input".into(), serde_json::json!(true));
        metadata.insert("question".into(), serde_json::json!(question));
        if !options.is_empty() {
            metadata.insert("options".into(), serde_json::json!(options));
        }
        if let Some(d) = &default {
            metadata.insert("default".into(), serde_json::json!(d));
        }

        ToolResult::ok_with_metadata(output, metadata)
    }
}

#[cfg(test)]
#[path = "ask_user_tests.rs"]
mod tests;
