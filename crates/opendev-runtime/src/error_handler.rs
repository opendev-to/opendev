//! Error handling and recovery for operations.
//!
//! Provides error classification, user action selection, and dangerous
//! operation confirmation. In a TUI/CLI context the actual prompting is
//! handled by the caller — this module provides the types and logic.
//!
//! Ported from `opendev/core/runtime/monitoring/error_handler.py`.

use serde::{Deserialize, Serialize};

/// Actions user can take on error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorAction {
    /// Retry the operation.
    Retry,
    /// Skip this operation and continue with the next.
    Skip,
    /// Cancel all remaining operations.
    Cancel,
    /// Edit parameters and retry.
    Edit,
}

impl ErrorAction {
    /// Parse from a single-character string (r/s/c/e).
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'r' => Some(Self::Retry),
            's' => Some(Self::Skip),
            'c' => Some(Self::Cancel),
            'e' => Some(Self::Edit),
            _ => None,
        }
    }

    /// Single-character representation.
    pub fn as_char(&self) -> char {
        match self {
            Self::Retry => 'r',
            Self::Skip => 's',
            Self::Cancel => 'c',
            Self::Edit => 'e',
        }
    }
}

/// Result of error handling.
#[derive(Debug, Clone)]
pub struct ErrorResult {
    pub action: ErrorAction,
    pub should_retry: bool,
    pub should_cancel: bool,
    pub edited_params: Option<serde_json::Value>,
}

impl ErrorResult {
    /// Create a retry result.
    pub fn retry() -> Self {
        Self {
            action: ErrorAction::Retry,
            should_retry: true,
            should_cancel: false,
            edited_params: None,
        }
    }

    /// Create a skip result.
    pub fn skip() -> Self {
        Self {
            action: ErrorAction::Skip,
            should_retry: false,
            should_cancel: false,
            edited_params: None,
        }
    }

    /// Create a cancel result.
    pub fn cancel() -> Self {
        Self {
            action: ErrorAction::Cancel,
            should_retry: false,
            should_cancel: true,
            edited_params: None,
        }
    }

    /// Create an edit result with new parameters.
    pub fn edit(params: serde_json::Value) -> Self {
        Self {
            action: ErrorAction::Edit,
            should_retry: true,
            should_cancel: false,
            edited_params: Some(params),
        }
    }
}

/// Information about an operation error for display/handling.
#[derive(Debug, Clone)]
pub struct OperationError {
    /// Human-readable error message.
    pub message: String,
    /// Operation type that failed (e.g. "bash_execute", "file_write").
    pub operation_type: String,
    /// Target of the operation (file path, command, etc.).
    pub target: String,
    /// Whether retry is a valid option.
    pub allow_retry: bool,
    /// Whether edit-and-retry is a valid option.
    pub allow_edit: bool,
}

/// Build the list of available options for an operation error.
pub fn available_actions(error: &OperationError) -> Vec<(ErrorAction, &'static str)> {
    let mut actions = Vec::new();
    if error.allow_retry {
        actions.push((ErrorAction::Retry, "Retry"));
    }
    if error.allow_edit {
        actions.push((ErrorAction::Edit, "Edit parameters and retry"));
    }
    actions.push((ErrorAction::Skip, "Skip this operation"));
    actions.push((ErrorAction::Cancel, "Cancel all remaining operations"));
    actions
}

/// Resolve a user's choice character into an `ErrorResult`.
///
/// Returns `None` if the choice is invalid or not allowed by the error options.
pub fn resolve_choice(choice: char, error: &OperationError) -> Option<ErrorResult> {
    match choice {
        'r' if error.allow_retry => Some(ErrorResult::retry()),
        's' => Some(ErrorResult::skip()),
        'c' => Some(ErrorResult::cancel()),
        'e' if error.allow_edit => {
            // Edit flow would be handled by the caller; return a placeholder
            None
        }
        _ => None,
    }
}

/// Classify whether an error is likely transient and worth retrying.
pub fn is_transient_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    let transient_patterns = [
        "timeout",
        "connection reset",
        "connection refused",
        "temporarily unavailable",
        "service unavailable",
        "bad gateway",
        "gateway timeout",
        "rate limit",
        "too many requests",
        "overloaded",
    ];
    transient_patterns.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
#[path = "error_handler_tests.rs"]
mod tests;
