//! Artifact index for tracking files touched during a session.

use std::collections::HashMap;

use chrono::Local;
use serde::{Deserialize, Serialize};

/// Tracks files touched during a session, surviving compaction.
///
/// Records file operations (create, modify, read, delete) with metadata
/// so the agent retains awareness of workspace state post-compaction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArtifactIndex {
    pub entries: HashMap<String, ArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    pub file_path: String,
    pub last_operation: String,
    pub last_details: String,
    pub created_at: String,
    pub updated_at: String,
    pub operation_count: u32,
    pub operations_seen: Vec<String>,
}

impl ArtifactIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a file operation.
    pub fn record(&mut self, file_path: &str, operation: &str, details: &str) {
        let now = Local::now().to_rfc3339();
        if let Some(existing) = self.entries.get_mut(file_path) {
            existing.last_operation.clear();
            existing.last_operation.push_str(operation);
            existing.last_details.clear();
            existing.last_details.push_str(details);
            existing.updated_at = now;
            existing.operation_count += 1;
            if !existing.operations_seen.iter().any(|s| s == operation) {
                existing.operations_seen.push(operation.to_owned());
            }
        } else {
            let op = operation.to_owned();
            self.entries.insert(
                file_path.to_owned(),
                ArtifactEntry {
                    file_path: file_path.to_owned(),
                    last_operation: op.clone(),
                    last_details: details.to_owned(),
                    created_at: now.clone(),
                    updated_at: now,
                    operation_count: 1,
                    operations_seen: vec![op],
                },
            );
        }
    }

    /// Format the artifact index as a compact summary for injection into compaction.
    pub fn as_summary(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut lines = vec!["## Artifact Index (files touched this session)".to_string()];
        for (path, entry) in &self.entries {
            let ops = entry.operations_seen.join(", ");
            let detail = if entry.last_details.is_empty() {
                String::new()
            } else {
                format!(" — {}", entry.last_details)
            };
            lines.push(format!("- `{path}` [{ops}]{detail}"));
        }
        lines.join("\n")
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize the artifact index to a JSON value for session persistence.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// Deserialize an artifact index from a JSON value (loaded from session metadata).
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }
}

#[cfg(test)]
#[path = "artifacts_tests.rs"]
mod tests;
