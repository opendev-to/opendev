//! update_todo tool — update a single todo item's status or title.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opendev_runtime::{TodoManager, parse_status};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool that updates a single todo item by fuzzy ID.
#[derive(Debug)]
pub struct UpdateTodoTool {
    manager: Arc<Mutex<TodoManager>>,
}

impl UpdateTodoTool {
    pub fn new(manager: Arc<Mutex<TodoManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl BaseTool for UpdateTodoTool {
    fn name(&self) -> &str {
        "update_todo"
    }

    fn description(&self) -> &str {
        "Update a todo item's status or title. Supports fuzzy ID matching \
         (e.g., '3', 'todo-3', 'todo_3', or partial title)."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Todo item ID (e.g., '3', 'todo-3', 'todo_3', or partial title)"
                },
                "status": {
                    "type": "string",
                    "description": "New status: pending, in_progress, completed (or aliases: todo, doing, done)"
                },
                "title": {
                    "type": "string",
                    "description": "New title for the todo item"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let id_str = match args.get("id").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                // Also try numeric
                match args.get("id").and_then(|v| v.as_u64()) {
                    Some(n) => return self.do_update(&n.to_string(), &args),
                    None => return ToolResult::fail("id is required"),
                }
            }
        };

        self.do_update(id_str, &args)
    }
}

impl UpdateTodoTool {
    fn do_update(&self, id_str: &str, args: &HashMap<String, serde_json::Value>) -> ToolResult {
        let mut mgr = match self.manager.lock() {
            Ok(m) => m,
            Err(e) => return ToolResult::fail(format!("Lock error: {e}")),
        };

        let (id, _) = match mgr.find_todo(id_str) {
            Some(found) => found,
            None => return ToolResult::fail(format!("Todo not found: {id_str}")),
        };

        let mut changed = false;

        // Update status if provided
        if let Some(status_str) = args.get("status").and_then(|v| v.as_str()) {
            if let Some(status) = parse_status(status_str) {
                mgr.set_status(id, status);
                changed = true;
            } else {
                return ToolResult::fail(format!(
                    "Unknown status: {status_str}. Use: pending, in_progress, completed (or aliases: todo, doing, done)"
                ));
            }
        }

        // Update title if provided
        if let Some(title) = args.get("title").and_then(|v| v.as_str())
            && let Some(item) = mgr.todos_mut().get_mut(&id)
        {
            item.title = title.to_string();
            changed = true;
        }

        if !changed {
            return ToolResult::fail("No updates provided. Specify status and/or title.");
        }

        ToolResult::ok(mgr.format_status_sorted())
    }
}

#[cfg(test)]
#[path = "update_todo_tests.rs"]
mod tests;
