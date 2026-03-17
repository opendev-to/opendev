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
mod tests {
    use super::*;

    // --- HandoffMessage tests ---

    #[test]
    fn test_handoff_create() {
        let from = AgentDefinition::from_role(AgentRole::Code);
        let messages = vec![
            serde_json::json!({"role": "user", "content": "implement feature X"}),
            serde_json::json!({"role": "assistant", "content": "I read the file and found..."}),
            serde_json::json!({
                "role": "tool",
                "name": "read_file",
                "content": "fn main() { println!(\"hello\"); }",
                "tool_call_id": "tc-1"
            }),
            serde_json::json!({"role": "assistant", "content": "Done with initial analysis."}),
        ];
        let handoff = HandoffMessage::create_handoff(&from, &AgentRole::Test, &messages);
        assert_eq!(handoff.from_agent, "Code");
        assert_eq!(handoff.to_agent, "Test");
        assert_eq!(handoff.summary, "Done with initial analysis.");
        assert!(!handoff.key_findings.is_empty());
        assert!(handoff.pending_actions.is_empty());
    }

    #[test]
    fn test_handoff_with_errors() {
        let from = AgentDefinition::from_role(AgentRole::Build);
        let messages = vec![
            serde_json::json!({"role": "assistant", "content": "Trying to fix..."}),
            serde_json::json!({
                "role": "tool",
                "name": "bash",
                "content": "Error in bash: compilation failed",
                "tool_call_id": "tc-1"
            }),
        ];
        let handoff = HandoffMessage::create_handoff(&from, &AgentRole::Code, &messages);
        assert!(!handoff.pending_actions.is_empty());
        assert!(handoff.pending_actions[0].contains("Retry bash"));
    }

    #[test]
    fn test_handoff_empty_messages() {
        let from = AgentDefinition::from_role(AgentRole::Code);
        let handoff = HandoffMessage::create_handoff(&from, &AgentRole::Plan, &[]);
        assert_eq!(handoff.summary, "No summary available");
        assert!(handoff.key_findings.is_empty());
        assert!(handoff.pending_actions.is_empty());
    }

    #[test]
    fn test_handoff_to_context_message() {
        let handoff = HandoffMessage {
            from_agent: "Code".into(),
            to_agent: "Test".into(),
            summary: "Implemented feature X".into(),
            key_findings: vec!["Found bug in parser".into()],
            pending_actions: vec!["Write unit tests".into()],
        };
        let msg = handoff.to_context_message();
        assert_eq!(msg["role"], "user");
        let content = msg["content"].as_str().unwrap();
        assert!(content.contains("[HANDOFF from Code agent]"));
        assert!(content.contains("Implemented feature X"));
        assert!(content.contains("Found bug in parser"));
        assert!(content.contains("Write unit tests"));
    }

    #[test]
    fn test_handoff_to_context_message_empty_findings() {
        let handoff = HandoffMessage {
            from_agent: "Plan".into(),
            to_agent: "Code".into(),
            summary: "Plan complete".into(),
            key_findings: vec![],
            pending_actions: vec![],
        };
        let msg = handoff.to_context_message();
        let content = msg["content"].as_str().unwrap();
        assert!(content.contains("None"));
    }

    #[test]
    fn test_handoff_message_serialization() {
        let handoff = HandoffMessage {
            from_agent: "Code".into(),
            to_agent: "Test".into(),
            summary: "Done".into(),
            key_findings: vec!["found a bug".into()],
            pending_actions: vec!["write tests".into()],
        };
        let json = serde_json::to_string(&handoff).unwrap();
        let roundtrip: HandoffMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.from_agent, "Code");
        assert_eq!(roundtrip.key_findings.len(), 1);
    }

    // --- can_parallelize tests ---

    #[test]
    fn test_can_parallelize_single_call() {
        let calls = vec![serde_json::json!({
            "function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}
        })];
        let groups = can_parallelize(&calls);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 1);
    }

    #[test]
    fn test_can_parallelize_different_files() {
        let calls = vec![
            serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}}),
            serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"b.rs\"}"}}),
            serde_json::json!({"function": {"name": "edit_file", "arguments": "{\"path\": \"c.rs\"}"}}),
        ];
        let groups = can_parallelize(&calls);
        assert_eq!(groups.len(), 3);
        assert!(groups.iter().all(|g| g.len() == 1));
    }

    #[test]
    fn test_can_parallelize_same_file() {
        let calls = vec![
            serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}}),
            serde_json::json!({"function": {"name": "edit_file", "arguments": "{\"path\": \"a.rs\"}"}}),
        ];
        let groups = can_parallelize(&calls);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_can_parallelize_mixed() {
        let calls = vec![
            serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}}),
            serde_json::json!({"function": {"name": "edit_file", "arguments": "{\"path\": \"a.rs\"}"}}),
            serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"b.rs\"}"}}),
        ];
        let groups = can_parallelize(&calls);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_can_parallelize_no_path() {
        let calls = vec![
            serde_json::json!({"function": {"name": "bash", "arguments": "{\"command\": \"cargo test\"}"}}),
            serde_json::json!({"function": {"name": "bash", "arguments": "{\"command\": \"cargo build\"}"}}),
        ];
        let groups = can_parallelize(&calls);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn test_can_parallelize_mixed_path_and_no_path() {
        let calls = vec![
            serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"a.rs\"}"}}),
            serde_json::json!({"function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}}),
        ];
        let groups = can_parallelize(&calls);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn test_can_parallelize_empty() {
        let groups = can_parallelize(&[]);
        assert_eq!(groups.len(), 1);
        assert!(groups[0].is_empty());
    }

    #[test]
    fn test_can_parallelize_file_path_key() {
        let calls = vec![
            serde_json::json!({"function": {"name": "write_file", "arguments": "{\"file_path\": \"x.rs\"}"}}),
            serde_json::json!({"function": {"name": "write_file", "arguments": "{\"file_path\": \"y.rs\"}"}}),
        ];
        let groups = can_parallelize(&calls);
        assert_eq!(groups.len(), 2);
    }

    // --- extract_file_path tests ---

    #[test]
    fn test_extract_file_path_with_path() {
        let tc = serde_json::json!({"function": {"name": "read_file", "arguments": "{\"path\": \"src/main.rs\"}"}});
        assert_eq!(extract_file_path(&tc), Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_extract_file_path_with_file_path() {
        let tc = serde_json::json!({"function": {"name": "write_file", "arguments": "{\"file_path\": \"out.txt\"}"}});
        assert_eq!(extract_file_path(&tc), Some("out.txt".to_string()));
    }

    #[test]
    fn test_extract_file_path_with_file() {
        let tc = serde_json::json!({"function": {"name": "edit", "arguments": "{\"file\": \"lib.rs\"}"}});
        assert_eq!(extract_file_path(&tc), Some("lib.rs".to_string()));
    }

    #[test]
    fn test_extract_file_path_none() {
        let tc =
            serde_json::json!({"function": {"name": "bash", "arguments": "{\"command\": \"ls\"}"}});
        assert_eq!(extract_file_path(&tc), None);
    }

    // --- PartialResult tests ---

    #[test]
    fn test_partial_result_from_interrupted_state() {
        let messages = vec![
            serde_json::json!({"role": "user", "content": "do stuff"}),
            serde_json::json!({
                "role": "assistant",
                "content": "I'll read the files.",
                "tool_calls": [{"id": "tc-1", "function": {"name": "read_file", "arguments": "{}"}}]
            }),
            serde_json::json!({"role": "tool", "name": "read_file", "content": "file contents", "tool_call_id": "tc-1"}),
            serde_json::json!({"role": "tool", "name": "search", "content": "search results", "tool_call_id": "tc-2"}),
        ];
        let partial =
            PartialResult::from_interrupted_state(&messages, Some("I was analyzing..."), 3, 2, 5);
        assert_eq!(partial.completed_tool_results.len(), 2);
        assert_eq!(
            partial.last_assistant_content.as_deref(),
            Some("I was analyzing...")
        );
        assert_eq!(partial.interrupted_at_iteration, 3);
        assert_eq!(partial.completed_tool_count, 2);
        assert_eq!(partial.total_tool_count, 5);
    }

    #[test]
    fn test_partial_result_summary() {
        let partial = PartialResult {
            completed_tool_results: vec![serde_json::json!({"role": "tool", "content": "ok"})],
            last_assistant_content: None,
            interrupted_at_iteration: 5,
            completed_tool_count: 1,
            total_tool_count: 3,
        };
        let summary = partial.summary();
        assert!(summary.contains("iteration 5"));
        assert!(summary.contains("1/3"));
        assert!(summary.contains("1 tool result(s) preserved"));
    }

    #[test]
    fn test_partial_result_empty() {
        let partial = PartialResult::from_interrupted_state(&[], None, 1, 0, 0);
        assert!(partial.completed_tool_results.is_empty());
        assert!(partial.last_assistant_content.is_none());
        assert_eq!(
            partial.summary(),
            "Interrupted at iteration 1 (0/0 tool calls completed). 0 tool result(s) preserved."
        );
    }

    #[test]
    fn test_partial_result_serialization() {
        let partial = PartialResult {
            completed_tool_results: vec![serde_json::json!({"role": "tool"})],
            last_assistant_content: Some("partial".into()),
            interrupted_at_iteration: 2,
            completed_tool_count: 1,
            total_tool_count: 3,
        };
        let json = serde_json::to_string(&partial).unwrap();
        let roundtrip: PartialResult = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.interrupted_at_iteration, 2);
        assert_eq!(roundtrip.last_assistant_content.as_deref(), Some("partial"));
    }
}
