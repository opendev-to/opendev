//! Operation models for tracking actions and results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use strum::{Display, EnumString};

/// Type of operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum OperationType {
    FileWrite,
    FileEdit,
    FileDelete,
    BashExecute,
}

/// Status of operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum OperationStatus {
    Pending,
    Approved,
    Executing,
    Success,
    Failed,
    Cancelled,
}

/// Represents a single operation to be performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    #[serde(rename = "type")]
    pub op_type: OperationType,
    pub status: OperationStatus,
    /// File path, command, etc.
    pub target: String,
    #[serde(default)]
    pub parameters: HashMap<String, serde_json::Value>,
    #[serde(default = "Utc::now", with = "crate::datetime_compat")]
    pub created_at: DateTime<Utc>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::datetime_compat::option"
    )]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::datetime_compat::option"
    )]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Operation {
    /// Create a new pending operation.
    pub fn new(op_type: OperationType, target: String) -> Self {
        Self {
            id: Utc::now().format("%Y%m%d%H%M%S%f").to_string(),
            op_type,
            status: OperationStatus::Pending,
            target,
            parameters: HashMap::new(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            approved: false,
            error: None,
        }
    }

    /// Mark operation as executing.
    pub fn mark_executing(&mut self) {
        self.status = OperationStatus::Executing;
        self.started_at = Some(Utc::now());
    }

    /// Mark operation as successful.
    pub fn mark_success(&mut self) {
        self.status = OperationStatus::Success;
        self.completed_at = Some(Utc::now());
    }

    /// Mark operation as failed.
    pub fn mark_failed(&mut self, error: String) {
        self.status = OperationStatus::Failed;
        self.completed_at = Some(Utc::now());
        self.error = Some(error);
    }
}

/// Result of a file write operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteResult {
    pub success: bool,
    pub file_path: String,
    pub created: bool,
    /// File size in bytes.
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// True if operation was interrupted.
    #[serde(default)]
    pub interrupted: bool,
}

/// Result of a file edit operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    pub success: bool,
    pub file_path: String,
    pub lines_added: u64,
    pub lines_removed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// Diff preview for the edit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// True if operation was interrupted.
    #[serde(default)]
    pub interrupted: bool,
}

/// Result of a bash command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BashResult {
    pub success: bool,
    pub command: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    /// Duration in seconds.
    pub duration: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    /// ID if running as background task.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_task_id: Option<String>,
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;
