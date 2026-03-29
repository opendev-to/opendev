//! ACE role data models (Reflector, Curator outputs).
//!
//! Mirrors `opendev/core/context_engineering/memory/roles.py`.
//!
//! Note: The actual LLM-calling Reflector and Curator classes from the Python
//! code depend on the LLM client and prompt system. This module provides the
//! data models and JSON parsing utilities used by those roles.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::delta::DeltaBatch;

/// Safely parse JSON from LLM output, handling markdown code fences.
pub fn safe_json_loads(text: &str) -> Result<serde_json::Value, String> {
    let mut text = text.trim().to_string();

    // Strip markdown code blocks
    if text.starts_with("```json") {
        text = text[7..].trim().to_string();
    } else if text.starts_with("```") {
        text = text[3..].trim().to_string();
    }
    if text.ends_with("```") {
        text = text[..text.len() - 3].trim().to_string();
    }

    match serde_json::from_str::<serde_json::Value>(&text) {
        Ok(val) => {
            if val.is_object() {
                Ok(val)
            } else {
                Err("Expected a JSON object from LLM.".to_string())
            }
        }
        Err(e) => {
            // Check for truncation
            let open_braces = text.chars().filter(|&c| c == '{').count();
            let close_braces = text.chars().filter(|&c| c == '}').count();
            if open_braces > close_braces || text.trim_end().ends_with('"') {
                Err(format!(
                    "LLM response appears to be truncated JSON. Original error: {e}"
                ))
            } else {
                Err(format!("LLM response is not valid JSON: {e}"))
            }
        }
    }
}

/// Main agent response for ACE analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<serde_json::Value>,
}

impl AgentResponse {
    /// Create a new agent response.
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string(),
            reasoning: None,
            tool_calls: Vec::new(),
        }
    }
}

/// Bullet tagging information from Reflector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulletTag {
    pub id: String,
    pub tag: String,
}

/// Output from the Reflector role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectorOutput {
    pub reasoning: String,
    pub error_identification: String,
    pub root_cause_analysis: String,
    pub correct_approach: String,
    pub key_insight: String,
    pub bullet_tags: Vec<BulletTag>,
    #[serde(default)]
    pub raw: HashMap<String, serde_json::Value>,
}

impl ReflectorOutput {
    /// Parse reflector output from LLM JSON response.
    pub fn from_json(data: &serde_json::Value) -> Self {
        let mut bullet_tags = Vec::new();
        if let Some(tags_arr) = data.get("bullet_tags").and_then(|v| v.as_array()) {
            for item in tags_arr {
                if let (Some(id), Some(tag)) = (
                    item.get("id").and_then(|v| v.as_str()),
                    item.get("tag").and_then(|v| v.as_str()),
                ) {
                    bullet_tags.push(BulletTag {
                        id: id.to_string(),
                        tag: tag.to_lowercase(),
                    });
                }
            }
        }

        let raw = data
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        Self {
            reasoning: data
                .get("reasoning")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            error_identification: data
                .get("error_identification")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            root_cause_analysis: data
                .get("root_cause_analysis")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            correct_approach: data
                .get("correct_approach")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            key_insight: data
                .get("key_insight")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            bullet_tags,
            raw,
        }
    }
}

/// Output from the Curator role.
#[derive(Debug, Clone)]
pub struct CuratorOutput {
    pub delta: DeltaBatch,
    pub raw: HashMap<String, serde_json::Value>,
}

impl CuratorOutput {
    /// Parse curator output from LLM JSON response.
    pub fn from_json(data: &serde_json::Value) -> Self {
        let delta = DeltaBatch::from_json(data);
        let raw = data
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        Self { delta, raw }
    }
}

#[cfg(test)]
#[path = "roles_tests.rs"]
mod tests;
