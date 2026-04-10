//! complete_todo tool — mark a todo item as completed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opendev_runtime::TodoManager;
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool that marks a todo item as completed via fuzzy ID.
#[derive(Debug)]
pub struct CompleteTodoTool {
    manager: Arc<Mutex<TodoManager>>,
}

impl CompleteTodoTool {
    pub fn new(manager: Arc<Mutex<TodoManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl BaseTool for CompleteTodoTool {
    fn name(&self) -> &str {
        "complete_todo"
    }

    fn description(&self) -> &str {
        "Mark a todo item as completed. Supports fuzzy ID matching \
         (e.g., '3', 'todo-3', 'todo_3', or partial title)."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "Todo item ID (e.g., '3', 'todo-3', 'todo_3', or partial title)"
                }
            },
            "required": ["id"]
        })
    }

    fn category(&self) -> opendev_tools_core::ToolCategory {
        opendev_tools_core::ToolCategory::Meta
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let id_str = match args.get("id").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => match args.get("id").and_then(|v| v.as_u64()) {
                Some(n) => n.to_string(),
                None => return ToolResult::fail("id is required"),
            },
        };

        let mut mgr = match self.manager.lock() {
            Ok(m) => m,
            Err(e) => return ToolResult::fail(format!("Lock error: {e}")),
        };

        let (id, _) = match mgr.find_todo(&id_str) {
            Some(found) => found,
            None => return ToolResult::fail(format!("Todo not found: {id_str}")),
        };

        mgr.complete(id);

        if mgr.all_completed() {
            ToolResult::ok(format!(
                "Todo {id} completed. All todos are done!\n\n{}",
                mgr.format_status_sorted()
            ))
        } else {
            ToolResult::ok(format!(
                "Todo {id} completed.\n\n{}",
                mgr.format_status_sorted()
            ))
        }
    }
}

#[cfg(test)]
#[path = "complete_todo_tests.rs"]
mod tests;
