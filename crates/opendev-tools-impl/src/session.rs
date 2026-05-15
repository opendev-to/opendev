//! Past sessions tool — browse and search historical conversation sessions.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use opendev_config::Paths;
use opendev_history::SessionManager;
use opendev_models::SessionMetadata;
use opendev_runtime::redact_secrets;
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool for browsing and searching past conversation sessions.
///
/// Uses project-scoped session directories via `opendev_config::Paths`
/// and the `SessionManager` API from `opendev-history`.
#[derive(Debug)]
pub struct PastSessionsTool;

#[async_trait::async_trait]
impl BaseTool for PastSessionsTool {
    fn name(&self) -> &str {
        "past_sessions"
    }

    fn description(&self) -> &str {
        "Browse and search past conversation sessions for this project. \
         NOT for checking subagent status — subagent results arrive automatically."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "read", "search", "info"],
                    "description": "Action to perform"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID (for read/info)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results (default: 20 list, 50 read, 10 search)"
                },
                "offset": {
                    "type": "integer",
                    "description": "Skip N items for pagination"
                },
                "include_archived": {
                    "type": "boolean",
                    "description": "Include archived sessions (default: false)"
                }
            },
            "required": ["action"]
        })
    }

    fn is_read_only(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }

    fn is_concurrent_safe(&self, _args: &HashMap<String, serde_json::Value>) -> bool {
        true
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Session
    }

    fn truncation_rule(&self) -> Option<opendev_tools_core::TruncationRule> {
        Some(opendev_tools_core::TruncationRule::tail(15000))
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        // Guard: subagents cannot access past sessions
        if ctx.is_subagent {
            return ToolResult::fail(
                "past_sessions is not available to subagents. \
                 Focus on completing your assigned task.",
            );
        }

        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::fail("action is required"),
        };

        // Resolve project-scoped session directory
        let paths = Paths::new(None);
        let session_dir = paths.project_sessions_dir(&ctx.working_dir);

        // Guard: don't create directories as a side effect
        if !session_dir.exists() {
            return ToolResult::ok("No past sessions found for this project.".to_string());
        }

        // Construct SessionManager (dir already exists so create_dir_all is a no-op)
        let manager = match SessionManager::new(session_dir) {
            Ok(m) => m,
            Err(e) => return ToolResult::fail(format!("Failed to open session store: {e}")),
        };

        let current_session_id = ctx.session_id.as_deref();

        match action {
            "list" => action_list(&manager, &args, current_session_id),
            "read" => action_read(&manager, &args, current_session_id),
            "search" => action_search(&manager, &args),
            "info" => action_info(&manager, &args, current_session_id),
            _ => ToolResult::fail(format!(
                "Unknown action: {action}. Available: list, read, search, info"
            )),
        }
    }
}

/// Validate session_id: reject path traversal characters.
/// Returns `Some(ToolResult)` on failure, `None` on success.
fn validate_session_id(id: &str) -> Option<ToolResult> {
    if id.is_empty() || id.contains("..") || id.contains('/') || id.contains('\\') {
        Some(ToolResult::fail("Invalid session ID"))
    } else {
        None
    }
}

/// Guard: reject reads of the current session.
/// Returns `Some(ToolResult)` if blocked, `None` if allowed.
fn guard_current_session(session_id: &str, current: Option<&str>) -> Option<ToolResult> {
    if let Some(current_id) = current
        && session_id == current_id
    {
        Some(ToolResult::ok(
            "This is your current session — its messages are already in your context.".to_string(),
        ))
    } else {
        None
    }
}

fn format_timestamp(dt: &DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

// --- Action implementations ---

fn action_list(
    manager: &SessionManager,
    args: &HashMap<String, serde_json::Value>,
    current_session_id: Option<&str>,
) -> ToolResult {
    let include_archived = args
        .get("include_archived")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let mut sessions: Vec<SessionMetadata> = manager.list_sessions(include_archived);

    // Exclude current session
    if let Some(current_id) = current_session_id {
        sessions.retain(|s| s.id != current_id);
    }

    // Sort by most recent first
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    if sessions.is_empty() {
        return ToolResult::ok("No past sessions found.".to_string());
    }

    let total = sessions.len();
    let page: Vec<&SessionMetadata> = sessions.iter().skip(offset).take(limit).collect();

    if page.is_empty() {
        return ToolResult::ok(format!("No sessions at offset {offset} (total: {total})."));
    }

    let mut output = format!(
        "Past sessions ({total} total, showing {}-{}):\n\n",
        offset + 1,
        offset + page.len(),
    );
    output.push_str(&format!(
        "{:<14} {:<40} {:>5} {:<17} {:>12}\n",
        "ID", "Title", "Msgs", "Updated", "Changes"
    ));
    output.push_str(&"-".repeat(90));
    output.push('\n');

    for meta in &page {
        let title = meta
            .title
            .as_deref()
            .unwrap_or("(untitled)")
            .chars()
            .take(38)
            .collect::<String>();
        let changes = format!("{}+/{}", meta.summary_additions, meta.summary_deletions);
        output.push_str(&format!(
            "{:<14} {:<40} {:>5} {:<17} {:>10}-\n",
            meta.id,
            title,
            meta.message_count,
            format_timestamp(&meta.updated_at),
            changes,
        ));
    }

    if total > offset + page.len() {
        output.push_str(&format!(
            "\nUse offset={} to see more.",
            offset + page.len()
        ));
    }

    let mut metadata = HashMap::new();
    metadata.insert("total".into(), serde_json::json!(total));
    ToolResult::ok_with_metadata(output, metadata)
}

fn action_read(
    manager: &SessionManager,
    args: &HashMap<String, serde_json::Value>,
    current_session_id: Option<&str>,
) -> ToolResult {
    let session_id = match args.get("session_id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolResult::fail("session_id is required for read"),
    };

    if let Some(r) = validate_session_id(session_id) {
        return r;
    }
    if let Some(r) = guard_current_session(session_id, current_session_id) {
        return r;
    }

    let session = match manager.load_session(session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::fail(format!("Session not found or corrupted: {e}")),
    };

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let total_messages = session.messages.len();

    // Default offset: show the last `limit` messages
    let default_offset = total_messages.saturating_sub(limit);
    let offset = args
        .get("offset")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(default_offset);

    let page: Vec<_> = session.messages.iter().skip(offset).take(limit).collect();

    if page.is_empty() {
        return ToolResult::ok(format!(
            "Session {session_id}: no messages at offset {offset} (total: {total_messages})."
        ));
    }

    let title = session
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("(untitled)");

    let mut output = format!(
        "Session: {session_id} — \"{title}\"\n\
         Messages {}-{} of {total_messages}:\n\n",
        offset + 1,
        offset + page.len(),
    );

    for (i, msg) in page.iter().enumerate() {
        let idx = offset + i + 1;
        let role = &msg.role;
        // Truncate content to 500 chars
        let content = &msg.content;
        let truncated: String = if content.chars().count() > 500 {
            let s: String = content.chars().take(500).collect();
            format!("{s}...[truncated]")
        } else {
            content.to_string()
        };
        output.push_str(&format!("[{idx}] {role}:\n{truncated}\n\n"));
    }

    if offset + page.len() < total_messages {
        output.push_str(&format!(
            "Use offset={} to see more messages.",
            offset + page.len()
        ));
    }

    // Redact secrets from the entire output
    let redacted = redact_secrets(&output);

    let mut metadata = HashMap::new();
    metadata.insert("total_messages".into(), serde_json::json!(total_messages));
    ToolResult::ok_with_metadata(redacted, metadata)
}

fn action_search(
    manager: &SessionManager,
    args: &HashMap<String, serde_json::Value>,
) -> ToolResult {
    let query = match args.get("query").and_then(|v| v.as_str()) {
        Some(q) if !q.trim().is_empty() => q,
        _ => return ToolResult::fail("query is required and must be non-empty for search"),
    };

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    let results = manager.search_sessions(query);

    if results.is_empty() {
        return ToolResult::ok(format!("No sessions matching \"{query}\"."));
    }

    let total = results.len();
    let shown = results.iter().take(limit);

    let mut output = format!("Search results for \"{query}\" ({total} sessions matched):\n\n");

    for (session_id, match_indices) in shown {
        let match_count = match_indices.len();

        // Try to load session for a context snippet
        let snippet = if let Ok(session) = manager.load_session(session_id) {
            if let Some(&first_idx) = match_indices.first() {
                if let Some(msg) = session.messages.get(first_idx) {
                    let content = &msg.content;
                    let preview: String = content.chars().take(100).collect();
                    format!("  {preview}...")
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        output.push_str(&format!("• {session_id} ({match_count} matches)\n"));
        if !snippet.is_empty() {
            output.push_str(&redact_secrets(&snippet));
            output.push('\n');
        }
        output.push('\n');
    }

    if total > limit {
        output.push_str(&format!("...and {} more sessions.", total - limit));
    }

    ToolResult::ok(output)
}

fn action_info(
    manager: &SessionManager,
    args: &HashMap<String, serde_json::Value>,
    current_session_id: Option<&str>,
) -> ToolResult {
    let session_id = match args.get("session_id").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => return ToolResult::fail("session_id is required for info"),
    };

    if let Some(r) = validate_session_id(session_id) {
        return r;
    }
    if let Some(r) = guard_current_session(session_id, current_session_id) {
        return r;
    }

    let session = match manager.load_session(session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::fail(format!("Session not found or corrupted: {e}")),
    };

    let title = session
        .metadata
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("(untitled)");
    let working_dir = session.working_directory.as_deref().unwrap_or("(unknown)");
    let archived = if session.is_archived() { "yes" } else { "no" };
    let parent = session.parent_id.as_deref().unwrap_or("none");
    let subagent_count = session.subagent_sessions.len();
    let file_changes = session.file_changes.len();
    let summary = session.get_file_changes_summary();

    let output = format!(
        "Session: {session_id}\n\
         Title: {title}\n\
         Created: {}\n\
         Updated: {}\n\
         Messages: {}\n\
         Working directory: {working_dir}\n\
         File changes: {file_changes} (+{} lines, -{} lines across {} files)\n\
         Parent session: {parent}\n\
         Subagent sessions: {subagent_count}\n\
         Archived: {archived}",
        format_timestamp(&session.created_at),
        format_timestamp(&session.updated_at),
        session.messages.len(),
        summary.total_lines_added,
        summary.total_lines_removed,
        summary.total,
    );

    ToolResult::ok(output)
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
