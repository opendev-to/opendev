//! Agent traits defining the base contract for all agents.
//!
//! Mirrors `opendev/core/base/abstract/base_agent.py`.

use async_trait::async_trait;
use opendev_runtime::InterruptToken;
use serde_json::Value;
use std::collections::HashMap;

/// Errors that can occur during agent operations.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM call failed: {0}")]
    LlmError(String),

    #[error("Tool execution failed: {0}")]
    ToolError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Max iterations reached ({0})")]
    MaxIterations(usize),

    #[error("Interrupted by user")]
    Interrupted,

    #[error("API error {status}: {message}")]
    ApiError { status: u16, message: String },

    #[error("{0}")]
    Other(String),
}

/// Result of running an agent.
#[derive(Debug, Clone)]
pub struct AgentResult {
    /// Final response content.
    pub content: String,
    /// Whether the run completed successfully.
    pub success: bool,
    /// Whether the run was interrupted by the user.
    pub interrupted: bool,
    /// Whether the run was soft-yielded for backgrounding.
    pub backgrounded: bool,
    /// Completion status from task_complete tool (if used).
    pub completion_status: Option<String>,
    /// The full message history after the run.
    pub messages: Vec<Value>,
    /// Partial result preserved when the agent was interrupted mid-execution.
    pub partial_result: Option<crate::agent_types::PartialResult>,
}

impl AgentResult {
    /// Create a successful result.
    pub fn ok(content: impl Into<String>, messages: Vec<Value>) -> Self {
        Self {
            content: content.into(),
            success: true,
            interrupted: false,
            backgrounded: false,
            completion_status: None,
            messages,
            partial_result: None,
        }
    }

    /// Create a failed result.
    pub fn fail(content: impl Into<String>, messages: Vec<Value>) -> Self {
        Self {
            content: content.into(),
            success: false,
            interrupted: false,
            backgrounded: false,
            completion_status: None,
            messages,
            partial_result: None,
        }
    }

    /// Create an interrupted result.
    pub fn interrupted(messages: Vec<Value>) -> Self {
        Self {
            content: "Task interrupted by user".to_string(),
            success: false,
            interrupted: true,
            backgrounded: false,
            completion_status: None,
            messages,
            partial_result: None,
        }
    }

    /// Create a backgrounded result (soft yield — task continues in background).
    pub fn backgrounded(messages: Vec<Value>) -> Self {
        Self {
            content: "Task moved to background".to_string(),
            success: false,
            interrupted: false,
            backgrounded: true,
            completion_status: None,
            messages,
            partial_result: None,
        }
    }
}

/// LLM response from a single API call.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// Whether the call succeeded.
    pub success: bool,
    /// Response content text (cleaned).
    pub content: Option<String>,
    /// Tool calls requested by the model.
    pub tool_calls: Option<Vec<Value>>,
    /// The full assistant message (for appending to history).
    pub message: Option<Value>,
    /// Error message if the call failed.
    pub error: Option<String>,
    /// Whether the call was interrupted.
    pub interrupted: bool,
    /// Token usage statistics.
    pub usage: Option<Value>,
    /// Native reasoning content (from models like o1).
    pub reasoning_content: Option<String>,
    /// Finish reason from the API (e.g. "stop", "length", "tool_calls").
    pub finish_reason: Option<String>,
}

impl LlmResponse {
    /// Create a successful response.
    pub fn ok(content: Option<String>, message: Value) -> Self {
        Self {
            success: true,
            content,
            tool_calls: message
                .get("tool_calls")
                .and_then(|v| v.as_array())
                .cloned(),
            message: Some(message),
            error: None,
            interrupted: false,
            usage: None,
            reasoning_content: None,
            finish_reason: None,
        }
    }

    /// Create a failed response.
    pub fn fail(error: impl Into<String>) -> Self {
        Self {
            success: false,
            content: None,
            tool_calls: None,
            message: None,
            error: Some(error.into()),
            interrupted: false,
            usage: None,
            reasoning_content: None,
            finish_reason: None,
        }
    }

    /// Create an interrupted response.
    pub fn interrupted() -> Self {
        Self {
            success: false,
            content: None,
            tool_calls: None,
            message: None,
            error: None,
            interrupted: true,
            usage: None,
            reasoning_content: None,
            finish_reason: None,
        }
    }
}

/// Base trait for all agents.
///
/// Agents orchestrate LLM calls, tool execution, and conversation management.
/// Python's mixin-based inheritance is replaced with composition — agents hold
/// their dependencies as fields and implement this trait for the core contract.
#[async_trait]
pub trait BaseAgent: Send + Sync {
    /// Build the system prompt for downstream model calls.
    fn build_system_prompt(&self) -> String;

    /// Return tool call schemas for the LLM.
    fn build_tool_schemas(&self) -> Vec<Value>;

    /// Refresh prompt and tool metadata when registry contents change.
    fn refresh_tools(&mut self) {
        // Default implementation — subclasses may override.
    }

    /// Execute a language model call using the supplied messages.
    async fn call_llm(
        &self,
        messages: &[Value],
        task_monitor: Option<&dyn TaskMonitor>,
    ) -> LlmResponse;

    /// Run a synchronous interaction (blocking wrapper for the ReAct loop).
    async fn run(
        &self,
        message: &str,
        deps: &AgentDeps,
        message_history: Option<Vec<Value>>,
        task_monitor: Option<&dyn TaskMonitor>,
    ) -> Result<AgentResult, AgentError>;
}

/// Trait for monitoring task progress and checking for interrupts.
pub trait TaskMonitor: Send + Sync {
    /// Check if the user has requested an interrupt.
    fn should_interrupt(&self) -> bool;

    /// Check if the user has requested backgrounding (soft yield).
    fn is_background_requested(&self) -> bool {
        false
    }

    /// Update token usage counter.
    fn update_tokens(&self, _total_tokens: u64) {}
}

/// Callback for streaming agent events to the UI during the ReAct loop.
pub trait AgentEventCallback: Send + Sync {
    /// A tool execution started.
    fn on_tool_started(
        &self,
        tool_id: &str,
        tool_name: &str,
        args: &std::collections::HashMap<String, serde_json::Value>,
    );
    /// A tool execution completed.
    fn on_tool_finished(&self, tool_id: &str, success: bool);
    /// Streaming text chunk from the assistant.
    fn on_agent_chunk(&self, text: &str);
    /// Native reasoning content from the LLM response (inline thinking).
    fn on_reasoning(&self, _content: &str) {}
    /// A new reasoning/thinking block started (separator between interleaved blocks).
    fn on_reasoning_block_start(&self) {}
    /// A tool produced its final result with output content.
    fn on_tool_result(&self, _tool_id: &str, _tool_name: &str, _output: &str, _success: bool) {}
    /// Context window usage percentage updated (0.0–100.0).
    fn on_context_usage(&self, _pct: f64) {}
    /// Token usage from an LLM call.
    fn on_token_usage(&self, _input_tokens: u64, _output_tokens: u64) {}
    /// File changes detected after a query completes.
    fn on_file_changed(&self, _files: usize, _additions: u64, _deletions: u64) {}
}

/// Dependencies injected into agent runs.
///
/// Replaces Python's `AgentDependencies` — carries session state and managers
/// needed during the ReAct loop.
#[derive(Debug, Clone)]
pub struct AgentDeps {
    /// Extra context values for tool execution.
    pub context: HashMap<String, Value>,
}

impl AgentDeps {
    /// Create new agent dependencies.
    pub fn new() -> Self {
        Self {
            context: HashMap::new(),
        }
    }

    /// Add a context value.
    pub fn with_context(mut self, key: impl Into<String>, value: Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }
}

impl Default for AgentDeps {
    fn default() -> Self {
        Self::new()
    }
}

// Allow InterruptToken to be used directly as a TaskMonitor.
impl TaskMonitor for opendev_runtime::InterruptToken {
    fn should_interrupt(&self) -> bool {
        self.is_requested()
    }

    fn is_background_requested(&self) -> bool {
        InterruptToken::is_background_requested(self)
    }
}

#[cfg(test)]
#[path = "traits_tests.rs"]
mod tests;
