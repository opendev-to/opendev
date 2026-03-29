//! Todo tool — list, update, and manage plan execution todos.
//!
//! Works with the `TodoManager` from `opendev-runtime` to let the agent
//! query and update todo progress during plan execution.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opendev_tools_core::{BaseTool, ToolContext, ToolDisplayMeta, ToolResult};

/// Tool for managing plan execution todos.
#[derive(Debug)]
pub struct TodoTool {
    /// Shared reference to the todo manager.
    ///
    /// Uses `Arc<Mutex<_>>` so the tool can be registered in the tool registry
    /// while the manager is also accessed by the TUI and react loop.
    manager: Arc<Mutex<opendev_runtime::TodoManager>>,
}

impl TodoTool {
    /// Create a new todo tool with a shared manager.
    pub fn new(manager: Arc<Mutex<opendev_runtime::TodoManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl BaseTool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        "Manage plan execution todos. List current todos, mark items as \
         in-progress or completed, or add new items."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "start", "complete", "add"],
                    "description": "Action to perform on todos"
                },
                "id": {
                    "type": "integer",
                    "description": "Todo item ID (for start/complete)"
                },
                "title": {
                    "type": "string",
                    "description": "Title for a new todo item (for add)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let action = match args.get("action").and_then(|v| v.as_str()) {
            Some(a) => a,
            None => return ToolResult::fail("action is required"),
        };

        let mut mgr = match self.manager.lock() {
            Ok(m) => m,
            Err(e) => return ToolResult::fail(format!("Lock error: {e}")),
        };

        match action {
            "list" => {
                if !mgr.has_todos() {
                    return ToolResult::ok("No todos.");
                }
                ToolResult::ok(mgr.format_status())
            }
            "start" => {
                let id = match args.get("id").and_then(|v| v.as_u64()) {
                    Some(id) => id as usize,
                    None => return ToolResult::fail("id is required for start"),
                };
                if mgr.start(id) {
                    ToolResult::ok(format!(
                        "Todo {id} marked as in-progress.\n\n{}",
                        mgr.format_status()
                    ))
                } else {
                    ToolResult::fail(format!("Todo {id} not found"))
                }
            }
            "complete" => {
                let id = match args.get("id").and_then(|v| v.as_u64()) {
                    Some(id) => id as usize,
                    None => return ToolResult::fail("id is required for complete"),
                };
                if mgr.complete(id) {
                    let status = mgr.format_status();
                    if mgr.all_completed() {
                        ToolResult::ok(format!(
                            "Todo {id} completed. All todos are done!\n\n{status}"
                        ))
                    } else {
                        ToolResult::ok(format!("Todo {id} completed.\n\n{status}"))
                    }
                } else {
                    ToolResult::fail(format!("Todo {id} not found"))
                }
            }
            "add" => {
                let title = match args.get("title").and_then(|v| v.as_str()) {
                    Some(t) if !t.is_empty() => t,
                    _ => return ToolResult::fail("title is required for add"),
                };
                let id = mgr.add(title.to_string());
                ToolResult::ok(format!(
                    "Added todo {id}: {title}\n\n{}",
                    mgr.format_status()
                ))
            }
            _ => ToolResult::fail(format!(
                "Unknown action: {action}. Available: list, start, complete, add"
            )),
        }
    }

    fn display_meta(&self) -> Option<ToolDisplayMeta> {
        Some(ToolDisplayMeta {
            verb: "Todo",
            label: "task",
            category: "Plan",
            primary_arg_keys: &["action", "id", "title"],
        })
    }
}

#[cfg(test)]
#[path = "todo_tests.rs"]
mod tests;
