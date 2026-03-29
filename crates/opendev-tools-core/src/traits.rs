//! Core tool traits and types.
//!
//! Defines the `BaseTool` async trait that all tools implement, along with
//! `ToolResult` (execution outcome) and `ToolContext` (session state passed
//! to tool handlers).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
    /// Shared mutable state across tool executions within a react loop.
    /// Used for cross-iteration state like planning phase transitions.
    pub shared_state: Option<Arc<Mutex<HashMap<String, serde_json::Value>>>>,
}

impl ToolContext {
    /// Create a new tool context with a working directory.
    ///
    /// Relative paths (including `.`) are resolved to absolute paths using
    /// `std::env::current_dir()` and then canonicalized. Absolute paths
    /// are stored as-is to avoid changing paths in tests.
    pub fn new(working_dir: impl Into<PathBuf>) -> Self {
        let raw: PathBuf = working_dir.into();
        let resolved = if raw.is_relative() {
            // Resolve relative paths (including ".") to absolute
            if let Ok(cwd) = std::env::current_dir() {
                let joined = cwd.join(&raw);
                joined.canonicalize().unwrap_or(joined)
            } else {
                raw.canonicalize().unwrap_or(raw)
            }
        } else {
            raw
        };
        Self {
            working_dir: resolved,
            is_subagent: false,
            session_id: None,
            values: HashMap::new(),
            timeout_config: None,
            cancel_token: None,
            diagnostic_provider: None,
            shared_state: None,
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

    /// Set shared mutable state for cross-iteration communication.
    pub fn with_shared_state(
        mut self,
        state: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    ) -> Self {
        self.shared_state = Some(state);
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
            shared_state: None,
        }
    }
}

/// Metadata describing how a tool should appear in the TUI.
///
/// Tools return this from `display_meta()` so the display registry can
/// auto-discover formatting without manual static entries.
#[derive(Debug, Clone, Copy)]
pub struct ToolDisplayMeta {
    /// Display verb shown in TUI, e.g. "Read", "Bash".
    pub verb: &'static str,
    /// Fallback noun when no arg is available, e.g. "file", "command".
    pub label: &'static str,
    /// Category name (matches `ToolCategory` variant names).
    pub category: &'static str,
    /// Ordered keys to try when extracting the primary arg for display.
    pub primary_arg_keys: &'static [&'static str],
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

    /// Return TUI display metadata for this tool.
    ///
    /// Tools that override this allow the display registry to auto-discover
    /// their formatting. Returns `None` by default (falls back to static registry).
    fn display_meta(&self) -> Option<ToolDisplayMeta> {
        None
    }
}

#[cfg(test)]
#[path = "traits_tests.rs"]
mod tests;
