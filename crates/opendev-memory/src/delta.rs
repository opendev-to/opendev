//! Delta batch operations for playbook mutations.
//!
//! Mirrors `opendev/core/context_engineering/memory/delta.py`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Type of delta operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DeltaOperationType {
    Add,
    Update,
    Tag,
    Remove,
}

impl fmt::Display for DeltaOperationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Add => write!(f, "ADD"),
            Self::Update => write!(f, "UPDATE"),
            Self::Tag => write!(f, "TAG"),
            Self::Remove => write!(f, "REMOVE"),
        }
    }
}

/// Single mutation to apply to the playbook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaOperation {
    #[serde(rename = "type")]
    pub op_type: DeltaOperationType,
    pub section: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet_id: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, i64>,
}

impl DeltaOperation {
    /// Create a DeltaOperation from a JSON value.
    pub fn from_json(payload: &serde_json::Value) -> Option<Self> {
        let op_type_str = payload["type"].as_str()?.to_uppercase();
        let op_type = match op_type_str.as_str() {
            "ADD" => DeltaOperationType::Add,
            "UPDATE" => DeltaOperationType::Update,
            "TAG" => DeltaOperationType::Tag,
            "REMOVE" => DeltaOperationType::Remove,
            _ => return None,
        };

        let section = payload
            .get("section")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let content = payload
            .get("content")
            .and_then(|v| v.as_str())
            .map(String::from);

        let bullet_id = payload
            .get("bullet_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        let mut metadata = HashMap::new();
        if let Some(meta_obj) = payload.get("metadata").and_then(|v| v.as_object()) {
            let valid_tags: &[&str] = if op_type == DeltaOperationType::Tag {
                &["helpful", "harmful", "neutral"]
            } else {
                // For non-TAG operations, accept all keys
                &[]
            };

            for (k, v) in meta_obj {
                if op_type == DeltaOperationType::Tag && !valid_tags.contains(&k.as_str()) {
                    continue;
                }
                if let Some(n) = v.as_i64() {
                    metadata.insert(k.clone(), n);
                }
            }
        }

        Some(Self {
            op_type,
            section,
            content,
            bullet_id,
            metadata,
        })
    }

    /// Convert to JSON value.
    pub fn to_json(&self) -> serde_json::Value {
        let mut data = serde_json::json!({
            "type": self.op_type,
            "section": self.section,
        });
        if let Some(ref content) = self.content {
            data["content"] = serde_json::Value::String(content.clone());
        }
        if let Some(ref bullet_id) = self.bullet_id {
            data["bullet_id"] = serde_json::Value::String(bullet_id.clone());
        }
        if !self.metadata.is_empty() {
            data["metadata"] = serde_json::to_value(&self.metadata).unwrap_or_default();
        }
        data
    }
}

/// Bundle of curator reasoning and operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaBatch {
    pub reasoning: String,
    #[serde(default)]
    pub operations: Vec<DeltaOperation>,
}

impl DeltaBatch {
    /// Create a DeltaBatch from a JSON value.
    pub fn from_json(payload: &serde_json::Value) -> Self {
        let reasoning = payload
            .get("reasoning")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut operations = Vec::new();
        if let Some(ops_array) = payload.get("operations").and_then(|v| v.as_array()) {
            for item in ops_array {
                if let Some(op) = DeltaOperation::from_json(item) {
                    operations.push(op);
                }
            }
        }

        Self {
            reasoning,
            operations,
        }
    }

    /// Convert to JSON value.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "reasoning": self.reasoning,
            "operations": self.operations.iter().map(|op| op.to_json()).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
#[path = "delta_tests.rs"]
mod tests;
