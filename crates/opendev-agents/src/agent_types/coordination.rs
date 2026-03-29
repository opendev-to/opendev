//! Handoff messages, partial results, and tool parallelization.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::definitions::AgentDefinition;
use super::roles::AgentRole;

/// Message sent when one agent hands off work to another.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffMessage {
    /// Identifier of the agent handing off.
    pub from_agent: String,
    /// Identifier of the agent receiving the handoff.
    pub to_agent: String,
    /// High-level summary of what was accomplished.
    pub summary: String,
    /// Key findings discovered during execution.
    pub key_findings: Vec<String>,
    /// Actions that still need to be performed.
    pub pending_actions: Vec<String>,
}

impl HandoffMessage {
    /// Create a handoff from an agent definition and its conversation messages.
    pub fn create_handoff(from: &AgentDefinition, to_role: &AgentRole, messages: &[Value]) -> Self {
        let from_name = from.role.to_string();
        let to_name = to_role.to_string();

        let summary = messages
            .iter()
            .rev()
            .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("assistant"))
            .and_then(|m| m.get("content").and_then(|c| c.as_str()))
            .unwrap_or("No summary available")
            .to_string();

        let mut key_findings = Vec::new();
        let mut pending_actions = Vec::new();

        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            if role == "tool" {
                let tool_name = msg.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");

                if !content.starts_with("Error") && !content.is_empty() {
                    let finding = if content.len() > 200 {
                        format!("{tool_name}: {}...", &content[..200])
                    } else {
                        format!("{tool_name}: {content}")
                    };
                    key_findings.push(finding);
                }

                if content.starts_with("Error") {
                    pending_actions.push(format!("Retry {tool_name}: {content}"));
                }
            }
        }

        key_findings.truncate(10);
        pending_actions.truncate(10);

        HandoffMessage {
            from_agent: from_name,
            to_agent: to_name,
            summary,
            key_findings,
            pending_actions,
        }
    }

    /// Convert the handoff into a user message for the receiving agent.
    pub fn to_context_message(&self) -> Value {
        let findings = if self.key_findings.is_empty() {
            "None".to_string()
        } else {
            self.key_findings
                .iter()
                .map(|f| format!("- {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let pending = if self.pending_actions.is_empty() {
            "None".to_string()
        } else {
            self.pending_actions
                .iter()
                .map(|a| format!("- {a}"))
                .collect::<Vec<_>>()
                .join("\n")
        };

        serde_json::json!({
            "role": "user",
            "content": format!(
                "[HANDOFF from {from} agent]\n\n\
                 ## Summary\n{summary}\n\n\
                 ## Key Findings\n{findings}\n\n\
                 ## Pending Actions\n{pending}",
                from = self.from_agent,
                summary = self.summary,
            )
        })
    }
}

/// Group tool calls by file-path dependency for parallel execution.
pub fn can_parallelize(calls: &[Value]) -> Vec<Vec<Value>> {
    if calls.len() <= 1 {
        return vec![calls.to_vec()];
    }

    let mut groups: Vec<Vec<Value>> = Vec::new();
    let mut file_to_group: HashMap<String, usize> = HashMap::new();
    let mut no_path_group: Vec<Value> = Vec::new();

    for tc in calls {
        let path = extract_file_path(tc);

        match path {
            Some(p) => {
                if let Some(&group_idx) = file_to_group.get(&p) {
                    groups[group_idx].push(tc.clone());
                } else {
                    let idx = groups.len();
                    file_to_group.insert(p, idx);
                    groups.push(vec![tc.clone()]);
                }
            }
            None => {
                no_path_group.push(tc.clone());
            }
        }
    }

    if groups.is_empty() {
        return vec![no_path_group];
    }

    if !no_path_group.is_empty() {
        groups.push(no_path_group);
    }

    groups
}

/// Extract the file path from a tool call's arguments.
fn extract_file_path(tool_call: &Value) -> Option<String> {
    let args_str = tool_call
        .get("function")
        .and_then(|f| f.get("arguments"))
        .and_then(|a| a.as_str())
        .unwrap_or("{}");

    let args: Value = serde_json::from_str(args_str).unwrap_or_default();

    for key in &["path", "file_path", "file"] {
        if let Some(p) = args.get(*key).and_then(|v| v.as_str()) {
            return Some(p.to_string());
        }
    }
    None
}

/// Partial result data preserved when an agent is interrupted mid-execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialResult {
    /// Tool results that were successfully collected before the interrupt.
    pub completed_tool_results: Vec<Value>,
    /// The last assistant content chunk (may be incomplete).
    pub last_assistant_content: Option<String>,
    /// The iteration number at which the interrupt occurred.
    pub interrupted_at_iteration: usize,
    /// Number of tool calls that were completed in the interrupted batch.
    pub completed_tool_count: usize,
    /// Total tool calls that were requested in the interrupted batch.
    pub total_tool_count: usize,
}

impl PartialResult {
    /// Create a new partial result from interrupted execution state.
    pub fn from_interrupted_state(
        messages: &[Value],
        assistant_content: Option<&str>,
        iteration: usize,
        completed: usize,
        total: usize,
    ) -> Self {
        let completed_tool_results: Vec<Value> = messages
            .iter()
            .rev()
            .take_while(|m| m.get("role").and_then(|r| r.as_str()) == Some("tool"))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        Self {
            completed_tool_results,
            last_assistant_content: assistant_content.map(|s| s.to_string()),
            interrupted_at_iteration: iteration,
            completed_tool_count: completed,
            total_tool_count: total,
        }
    }

    /// Produce a human-readable summary of the partial result.
    pub fn summary(&self) -> String {
        format!(
            "Interrupted at iteration {} ({}/{} tool calls completed). {} tool result(s) preserved.",
            self.interrupted_at_iteration,
            self.completed_tool_count,
            self.total_tool_count,
            self.completed_tool_results.len(),
        )
    }
}

#[cfg(test)]
#[path = "coordination_tests.rs"]
mod tests;
