//! Core tool traits and types.
//!
//! Defines the `BaseTool` async trait that all tools implement, along with
//! `ToolResult` (execution outcome) and `ToolContext` (session state passed
//! to tool handlers).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// A structured validation error with field path and message.
///
/// Used by `BaseTool::format_validation_error` to provide context about
/// each validation failure.
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// Dot-separated path to the invalid field (e.g. `"tool_calls.0.tool"`).
    pub path: String,
    /// Human-readable description of what went wrong.
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() || self.path == "root" {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// A diagnostic reported by a language server for a specific file location.
#[derive(Debug, Clone)]
pub struct FileDiagnostic {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
    /// Severity: 1 = Error, 2 = Warning, 3 = Info, 4 = Hint.
    pub severity: u32,
    /// Diagnostic message.
    pub message: String,
}

impl FileDiagnostic {
    /// Format this diagnostic as a human-readable line.
    pub fn pretty(&self) -> String {
        let level = match self.severity {
            1 => "ERROR",
            2 => "WARN",
            3 => "INFO",
            _ => "HINT",
        };
        format!("{level} [{}:{}] {}", self.line, self.column, self.message)
    }
}

/// Provider of LSP diagnostics for files after edits.
///
/// Implementors connect to language servers and return diagnostics
/// for modified files. The file tools call this after successful writes
/// to give the LLM immediate feedback about introduced errors.
#[async_trait::async_trait]
pub trait DiagnosticProvider: Send + Sync + std::fmt::Debug {
    /// Notify the provider that a file was modified and retrieve diagnostics.
    ///
    /// Returns diagnostics for the specified file, filtering to the given
    /// severity threshold (1 = errors only, 2 = errors + warnings, etc.).
    /// The `max_count` parameter limits how many diagnostics to return.
    ///
    /// Returns an empty vec if no diagnostics are available or if the
    /// language server doesn't support the file type.
    async fn diagnostics_for_file(
        &self,
        file_path: &Path,
        max_severity: u32,
        max_count: usize,
    ) -> Vec<FileDiagnostic>;
}

/// Errors that can occur during tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool execution failed: {0}")]
    Execution(String),

    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    #[error("Tool not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Interrupted by user")]
    Interrupted,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Whether the tool executed successfully.
    pub success: bool,
    /// Tool output text (for successful results).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Error message (for failed results).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Additional metadata (tool-specific).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
    /// Execution duration in milliseconds (populated by the registry).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Hidden suffix appended to the tool result for the LLM but not shown in the UI.
    /// Used to silently guide LLM behavior on errors (e.g., retry hints).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_suffix: Option<String>,
}

impl ToolResult {
    /// Create a successful result.
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: Some(output.into()),
            error: None,
            metadata: HashMap::new(),
            duration_ms: None,
            llm_suffix: None,
        }
    }

    /// Create a successful result with metadata.
    pub fn ok_with_metadata(
        output: impl Into<String>,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Self {
        Self {
            success: true,
            output: Some(output.into()),
            error: None,
            metadata,
            duration_ms: None,
            llm_suffix: None,
        }
    }

    /// Create a failed result.
    pub fn fail(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: None,
            error: Some(error.into()),
            metadata: HashMap::new(),
            duration_ms: None,
            llm_suffix: None,
        }
    }

    /// Attach an LLM-only suffix to this result.
    pub fn with_llm_suffix(mut self, suffix: impl Into<String>) -> Self {
        self.llm_suffix = Some(suffix.into());
        self
    }

    /// Create a result from a ToolError.
    pub fn from_error(err: ToolError) -> Self {
        Self::fail(err.to_string())
    }
}

/// Per-tool timeout configuration.
///
/// Allows overriding the default idle and maximum timeouts for tools
/// that execute external processes (e.g., bash).
#[derive(Debug, Clone)]
pub struct ToolTimeoutConfig {
    /// Idle timeout in seconds: kill when no stdout/stderr activity for this long.
    /// Defaults to 60 seconds.
    pub idle_timeout_secs: u64,
    /// Absolute maximum runtime in seconds (safety cap).
    /// Defaults to 600 seconds.
    pub max_timeout_secs: u64,
}

impl Default for ToolTimeoutConfig {
    fn default() -> Self {
        Self {
            idle_timeout_secs: 60,
            max_timeout_secs: 600,
        }
    }
}

/// Execution context passed to tool handlers.
///
/// Carries session state, configuration, and working directory so tools
/// can resolve paths, check permissions, and access shared resources.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Working directory for path resolution.
    pub working_dir: PathBuf,
    /// Whether the caller is a subagent (may restrict some operations).
    pub is_subagent: bool,
    /// Optional session ID for session-scoped operations.
    pub session_id: Option<String>,
    /// Arbitrary context values for tool-specific needs.
    pub values: HashMap<String, serde_json::Value>,
    /// Optional per-tool timeout overrides.
    pub timeout_config: Option<ToolTimeoutConfig>,
    /// Cancellation token for cooperative interrupt from the UI.
    pub cancel_token: Option<CancellationToken>,
    /// Optional LSP diagnostic provider for post-edit feedback.
    pub diagnostic_provider: Option<Arc<dyn DiagnosticProvider>>,
}

impl ToolContext {
    /// Create a new tool context with a working directory.
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        Self {
            working_dir: working_dir.into(),
            is_subagent: false,
            session_id: None,
            values: HashMap::new(),
            timeout_config: None,
            cancel_token: None,
            diagnostic_provider: None,
        }
    }

    /// Set a cancellation token for cooperative interrupt.
    pub fn with_cancel_token(mut self, token: CancellationToken) -> Self {
        self.cancel_token = Some(token);
        self
    }

    /// Set the subagent flag.
    pub fn with_subagent(mut self, is_subagent: bool) -> Self {
        self.is_subagent = is_subagent;
        self
    }

    /// Set the session ID.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Insert a context value.
    pub fn with_value(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.values.insert(key.into(), value);
        self
    }

    /// Set a diagnostic provider for post-edit LSP feedback.
    pub fn with_diagnostic_provider(mut self, provider: Arc<dyn DiagnosticProvider>) -> Self {
        self.diagnostic_provider = Some(provider);
        self
    }

    /// Set timeout configuration.
    pub fn with_timeout_config(mut self, config: ToolTimeoutConfig) -> Self {
        self.timeout_config = Some(config);
        self
    }
}

impl Default for ToolContext {
    fn default() -> Self {
        Self {
            working_dir: std::env::current_dir().unwrap_or_default(),
            is_subagent: false,
            session_id: None,
            values: HashMap::new(),
            timeout_config: None,
            cancel_token: None,
            diagnostic_provider: None,
        }
    }
}

/// Base trait for all tools.
///
/// Tools implement this trait to provide:
/// - Identity (name, description)
/// - Parameter schema (JSON Schema for LLM tool-use)
/// - Async execution
#[async_trait::async_trait]
pub trait BaseTool: Send + Sync + std::fmt::Debug {
    /// Unique tool name used for dispatch.
    fn name(&self) -> &str;

    /// Human-readable description shown to the LLM.
    fn description(&self) -> &str;

    /// JSON Schema describing the tool's parameters.
    ///
    /// Returns a JSON object with `type`, `properties`, and `required` fields
    /// following the JSON Schema specification.
    fn parameter_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments and context.
    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult;

    /// Format a validation error into a tool-specific, LLM-friendly message.
    ///
    /// When validation fails, the registry calls this method to produce a
    /// structured error that helps the LLM understand exactly what went wrong
    /// and how to fix the call. Tools that don't override this get the default
    /// generic validation error message.
    ///
    /// The `errors` slice contains `(field_path, message)` pairs extracted
    /// from the JSON Schema validation.
    fn format_validation_error(&self, errors: &[ValidationError]) -> Option<String> {
        let _ = errors;
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_ok() {
        let result = ToolResult::ok("file contents here");
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("file contents here"));
        assert!(result.error.is_none());
        assert!(result.metadata.is_empty());
    }

    #[test]
    fn test_tool_result_ok_with_metadata() {
        let mut meta = HashMap::new();
        meta.insert("lines".into(), serde_json::json!(42));
        let result = ToolResult::ok_with_metadata("output", meta);
        assert!(result.success);
        assert_eq!(result.metadata.get("lines"), Some(&serde_json::json!(42)));
    }

    #[test]
    fn test_tool_result_fail() {
        let result = ToolResult::fail("file not found");
        assert!(!result.success);
        assert!(result.output.is_none());
        assert_eq!(result.error.as_deref(), Some("file not found"));
    }

    #[test]
    fn test_tool_result_from_error() {
        let err = ToolError::NotFound("read_file".into());
        let result = ToolResult::from_error(err);
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("read_file"));
    }

    #[test]
    fn test_tool_result_serde_roundtrip() {
        let result = ToolResult::ok("hello");
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ToolResult = serde_json::from_str(&json).unwrap();
        assert!(deserialized.success);
        assert_eq!(deserialized.output.as_deref(), Some("hello"));
    }

    #[test]
    fn test_tool_context_builder() {
        let ctx = ToolContext::new("/tmp/project")
            .with_subagent(true)
            .with_session_id("sess-123")
            .with_value("key", serde_json::json!("value"));

        assert_eq!(ctx.working_dir, PathBuf::from("/tmp/project"));
        assert!(ctx.is_subagent);
        assert_eq!(ctx.session_id.as_deref(), Some("sess-123"));
        assert_eq!(ctx.values.get("key"), Some(&serde_json::json!("value")));
    }

    #[test]
    fn test_tool_context_default() {
        let ctx = ToolContext::default();
        assert!(!ctx.is_subagent);
        assert!(ctx.session_id.is_none());
    }

    #[test]
    fn test_tool_error_display() {
        let err = ToolError::InvalidParams("missing file_path".into());
        assert_eq!(err.to_string(), "Invalid parameters: missing file_path");
    }

    #[test]
    fn test_tool_result_duration_ms_default_none() {
        let result = ToolResult::ok("output");
        assert!(result.duration_ms.is_none());

        let result = ToolResult::fail("error");
        assert!(result.duration_ms.is_none());
    }

    #[test]
    fn test_tool_result_duration_ms_serde() {
        let mut result = ToolResult::ok("output");
        result.duration_ms = Some(42);
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"duration_ms\":42"));
        let deserialized: ToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.duration_ms, Some(42));
    }

    #[test]
    fn test_tool_result_duration_ms_skipped_when_none() {
        let result = ToolResult::ok("output");
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("duration_ms"));
    }

    #[test]
    fn test_tool_timeout_config_default() {
        let config = ToolTimeoutConfig::default();
        assert_eq!(config.idle_timeout_secs, 60);
        assert_eq!(config.max_timeout_secs, 600);
    }

    #[test]
    fn test_tool_context_with_timeout_config() {
        let config = ToolTimeoutConfig {
            idle_timeout_secs: 30,
            max_timeout_secs: 300,
        };
        let ctx = ToolContext::new("/tmp/project").with_timeout_config(config);
        assert!(ctx.timeout_config.is_some());
        let tc = ctx.timeout_config.unwrap();
        assert_eq!(tc.idle_timeout_secs, 30);
        assert_eq!(tc.max_timeout_secs, 300);
    }

    #[test]
    fn test_tool_context_default_no_timeout_config() {
        let ctx = ToolContext::default();
        assert!(ctx.timeout_config.is_none());
    }

    // --- ValidationError tests ---

    #[test]
    fn test_validation_error_display_with_path() {
        let err = ValidationError {
            path: "file_path".to_string(),
            message: "Missing required parameter: 'file_path'".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "file_path: Missing required parameter: 'file_path'"
        );
    }

    #[test]
    fn test_validation_error_display_root_path() {
        let err = ValidationError {
            path: "root".to_string(),
            message: "Invalid object".to_string(),
        };
        assert_eq!(err.to_string(), "Invalid object");
    }

    #[test]
    fn test_validation_error_display_empty_path() {
        let err = ValidationError {
            path: String::new(),
            message: "Something is wrong".to_string(),
        };
        assert_eq!(err.to_string(), "Something is wrong");
    }

    #[test]
    fn test_validation_error_display_nested_path() {
        let err = ValidationError {
            path: "invocations.0.tool".to_string(),
            message: "expected type 'string', got number".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "invocations.0.tool: expected type 'string', got number"
        );
    }
}
