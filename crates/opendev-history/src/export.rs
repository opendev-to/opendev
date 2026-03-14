//! Export a session as readable Markdown.
//!
//! Renders the conversation with `##` headers for each turn, fenced code
//! blocks for tool outputs, and a YAML-style metadata block at the top.

use opendev_models::{Role, Session};

/// Export a session as a Markdown string.
///
/// The output includes:
/// - A metadata block with session ID, timestamps, and title
/// - `## User` / `## Assistant` / `## System` headers for each message
/// - Fenced code blocks for tool call results
pub fn export_markdown(session: &Session) -> String {
    let mut out = String::new();

    // Metadata header
    out.push_str("# Session Export\n\n");

    let title = session
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled");
    out.push_str(&format!("- **Title:** {}\n", title));
    out.push_str(&format!("- **Session ID:** {}\n", session.id));
    out.push_str(&format!(
        "- **Created:** {}\n",
        session.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    out.push_str(&format!(
        "- **Updated:** {}\n",
        session.updated_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    out.push_str(&format!("- **Messages:** {}\n", session.messages.len()));

    if let Some(ref wd) = session.working_directory {
        out.push_str(&format!("- **Working Directory:** {}\n", wd));
    }
    out.push('\n');
    out.push_str("---\n\n");

    // Messages
    for (i, msg) in session.messages.iter().enumerate() {
        let role_label = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => "System",
        };

        out.push_str(&format!("## {} (Turn {})\n\n", role_label, i + 1));

        // Thinking trace (if present)
        if let Some(ref trace) = msg.thinking_trace {
            out.push_str("<details>\n<summary>Thinking</summary>\n\n");
            out.push_str(trace.trim());
            out.push_str("\n\n</details>\n\n");
        }

        // Message content
        let content = msg.content.trim();
        if !content.is_empty() {
            out.push_str(content);
            out.push_str("\n\n");
        }

        // Tool calls
        for tc in &msg.tool_calls {
            out.push_str(&format!("### Tool: `{}`\n\n", tc.name));

            // Parameters
            if !tc.parameters.is_empty() {
                out.push_str("**Parameters:**\n\n");
                out.push_str("```json\n");
                let params_json = serde_json::to_string_pretty(&tc.parameters).unwrap_or_default();
                out.push_str(&params_json);
                out.push_str("\n```\n\n");
            }

            // Result
            if let Some(ref result) = tc.result {
                let result_str = match result {
                    serde_json::Value::String(s) => s.clone(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                };
                if !result_str.is_empty() {
                    out.push_str("**Result:**\n\n");
                    out.push_str("```\n");
                    out.push_str(&result_str);
                    out.push_str("\n```\n\n");
                }
            }

            // Error
            if let Some(ref error) = tc.error {
                out.push_str(&format!("**Error:** {}\n\n", error));
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use opendev_models::ChatMessage;
    use std::collections::HashMap;

    fn make_msg(role: Role, content: &str) -> ChatMessage {
        ChatMessage {
            role,
            content: content.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
            tool_calls: vec![],
            tokens: None,
            thinking_trace: None,
            reasoning_content: None,
            token_usage: None,
            provenance: None,
        }
    }

    #[test]
    fn test_export_empty_session() {
        let session = Session::new();
        let md = export_markdown(&session);
        assert!(md.contains("# Session Export"));
        assert!(md.contains("Untitled"));
        assert!(md.contains("Messages:** 0"));
    }

    #[test]
    fn test_export_with_messages() {
        let mut session = Session::new();
        session
            .metadata
            .insert("title".to_string(), serde_json::json!("Test Session"));
        session.messages.push(make_msg(Role::User, "Hello there"));
        session
            .messages
            .push(make_msg(Role::Assistant, "Hi! How can I help?"));

        let md = export_markdown(&session);
        assert!(md.contains("**Title:** Test Session"));
        assert!(md.contains("## User (Turn 1)"));
        assert!(md.contains("Hello there"));
        assert!(md.contains("## Assistant (Turn 2)"));
        assert!(md.contains("Hi! How can I help?"));
    }

    #[test]
    fn test_export_with_thinking_trace() {
        let mut session = Session::new();
        let mut msg = make_msg(Role::Assistant, "The answer is 42.");
        msg.thinking_trace = Some("Let me think about this...".to_string());
        session.messages.push(msg);

        let md = export_markdown(&session);
        assert!(md.contains("<details>"));
        assert!(md.contains("<summary>Thinking</summary>"));
        assert!(md.contains("Let me think about this..."));
        assert!(md.contains("</details>"));
        assert!(md.contains("The answer is 42."));
    }

    #[test]
    fn test_export_with_tool_calls() {
        let mut session = Session::new();
        let mut msg = make_msg(Role::Assistant, "I'll read the file.");
        msg.tool_calls.push(opendev_models::ToolCall {
            id: "tc-1".to_string(),
            name: "read_file".to_string(),
            parameters: {
                let mut p = HashMap::new();
                p.insert("path".to_string(), serde_json::json!("/src/main.rs"));
                p
            },
            result: Some(serde_json::json!("fn main() {}")),
            result_summary: None,
            timestamp: Utc::now(),
            approved: true,
            error: None,
            nested_tool_calls: vec![],
        });
        session.messages.push(msg);

        let md = export_markdown(&session);
        assert!(md.contains("### Tool: `read_file`"));
        assert!(md.contains("```json"));
        assert!(md.contains("/src/main.rs"));
        assert!(md.contains("**Result:**"));
        assert!(md.contains("fn main() {}"));
    }

    #[test]
    fn test_export_with_tool_error() {
        let mut session = Session::new();
        let mut msg = make_msg(Role::Assistant, "Trying to write.");
        msg.tool_calls.push(opendev_models::ToolCall {
            id: "tc-2".to_string(),
            name: "write_file".to_string(),
            parameters: HashMap::new(),
            result: None,
            result_summary: None,
            timestamp: Utc::now(),
            approved: false,
            error: Some("Permission denied".to_string()),
            nested_tool_calls: vec![],
        });
        session.messages.push(msg);

        let md = export_markdown(&session);
        assert!(md.contains("**Error:** Permission denied"));
    }

    #[test]
    fn test_export_with_working_directory() {
        let mut session = Session::new();
        session.working_directory = Some("/home/user/project".to_string());

        let md = export_markdown(&session);
        assert!(md.contains("**Working Directory:** /home/user/project"));
    }

    #[test]
    fn test_export_system_message() {
        let mut session = Session::new();
        session
            .messages
            .push(make_msg(Role::System, "Context loaded."));

        let md = export_markdown(&session);
        assert!(md.contains("## System (Turn 1)"));
        assert!(md.contains("Context loaded."));
    }

    #[test]
    fn test_export_multi_turn_conversation() {
        let mut session = Session::new();
        session.messages.push(make_msg(Role::User, "Question 1"));
        session.messages.push(make_msg(Role::Assistant, "Answer 1"));
        session.messages.push(make_msg(Role::User, "Question 2"));
        session.messages.push(make_msg(Role::Assistant, "Answer 2"));

        let md = export_markdown(&session);
        assert!(md.contains("## User (Turn 1)"));
        assert!(md.contains("## Assistant (Turn 2)"));
        assert!(md.contains("## User (Turn 3)"));
        assert!(md.contains("## Assistant (Turn 4)"));
    }
}
