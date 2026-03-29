//! Process user queries, enhance with context, delegate to ReAct loop.
//!
//! Mirrors `opendev/repl/query_processor.py`.

use chrono::Utc;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{debug, info};

use crate::file_injector::FileContentInjector;

use opendev_history::SessionManager;
use opendev_models::{ChatMessage, Role};
use opendev_tools_core::ToolRegistry;

use crate::error::ReplError;

/// Result of processing a query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// The assistant's response content.
    pub content: String,
    /// Summary of the last operation (for status display).
    pub operation_summary: String,
    /// Error message if something went wrong.
    pub error: Option<String>,
    /// LLM call latency in milliseconds.
    pub latency_ms: Option<u64>,
}

impl Default for QueryResult {
    fn default() -> Self {
        Self {
            content: String::new(),
            operation_summary: String::from("—"),
            error: None,
            latency_ms: None,
        }
    }
}

/// Processes user queries using the ReAct pattern.
///
/// Coordinates:
/// - Query enhancement (@ file references)
/// - Message preparation
/// - LLM calls with progress display
/// - Tool execution
pub struct QueryProcessor {
    /// Number of queries processed in this session.
    execution_count: u64,
    /// Working directory for resolving @ file references.
    working_dir: PathBuf,
}

impl QueryProcessor {
    /// Create a new query processor.
    pub fn new() -> Self {
        Self {
            execution_count: 0,
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    /// Create a new query processor with an explicit working directory.
    pub fn with_working_dir(working_dir: PathBuf) -> Self {
        Self {
            execution_count: 0,
            working_dir,
        }
    }

    /// Process a user query.
    ///
    /// Adds the user message to the session, enhances the query with file
    /// references, and delegates to the ReAct loop for execution.
    pub async fn process(
        &mut self,
        query: &str,
        session_manager: &mut SessionManager,
        tool_registry: &ToolRegistry,
        plan_requested: bool,
    ) -> Result<QueryResult, ReplError> {
        self.execution_count += 1;
        info!(query_num = self.execution_count, "Processing query");

        // Add user message to session
        let user_msg = ChatMessage {
            role: Role::User,
            content: query.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
            tool_calls: vec![],
            tokens: None,
            thinking_trace: None,
            reasoning_content: None,
            token_usage: None,
            provenance: None,
        };
        if let Some(session) = session_manager.current_session_mut() {
            session.messages.push(user_msg);
        }

        // Enhance query with @ file references
        let enhanced = self.enhance_query(query);

        // Build messages for the LLM
        let mut messages = self.build_messages(&enhanced, session_manager);

        // Inject plan reminder if plan mode is active
        if plan_requested {
            let plans_dir = dirs::home_dir()
                .map(|h| h.join(".opendev").join("plans"))
                .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
            let plan_name = opendev_runtime::generate_plan_name(Some(&plans_dir), 50);
            let plan_path = format!("~/.opendev/plans/{}.md", plan_name);
            let reminder = opendev_agents::prompts::reminders::get_reminder(
                "plan_subagent_request",
                &[("plan_file_path", &plan_path)],
            );
            if !reminder.is_empty() {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": format!("<system-reminder>{}</system-reminder>", reminder)
                }));
            }
        }

        // ReactLoop integration is deferred to the integration wiring phase (Step 9).
        // The messages are prepared above; a caller holding an AgentRuntime will
        // invoke ReactLoop::run() with these messages.
        debug!(
            tool_count = tool_registry.len(),
            msg_count = messages.len(),
            plan_requested,
            "Ready for ReAct execution"
        );

        let mode_label = if plan_requested { "plan" } else { "normal" };
        let result = QueryResult {
            content: format!(
                "[{} mode] Received query #{} ({} message(s) in context, {} tool(s) available):\n{}",
                mode_label,
                self.execution_count,
                messages.len(),
                tool_registry.len(),
                enhanced,
            ),
            operation_summary: format!("Query #{}", self.execution_count),
            error: None,
            latency_ms: None,
        };

        Ok(result)
    }

    /// Enhance a query by resolving @ file references.
    ///
    /// Looks for patterns like `@path/to/file` and injects the file contents
    /// into the query text.
    fn enhance_query(&self, query: &str) -> String {
        let injector = FileContentInjector::new(self.working_dir.clone());
        let result = injector.inject_content(query);

        if result.text_content.is_empty() {
            return query.to_string();
        }

        // Append the injected file content after the original query.
        format!("{}\n\n{}", query, result.text_content)
    }

    /// Build the message list for the LLM API call.
    fn build_messages(&self, _query: &str, session_manager: &SessionManager) -> Vec<Value> {
        let mut messages = Vec::new();

        // Add conversation history from session
        if let Some(session) = session_manager.current_session() {
            for msg in &session.messages {
                messages.push(serde_json::json!({
                    "role": msg.role.to_string(),
                    "content": &msg.content,
                }));
            }
        }

        messages
    }
}

impl Default for QueryProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "query_processor_tests.rs"]
mod tests;
