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
#[path = "export_tests.rs"]
mod tests;
