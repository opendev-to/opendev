//! list_todos tool — display the current todo list.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opendev_runtime::TodoManager;
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool that lists all todo items sorted by status.
#[derive(Debug)]
pub struct ListTodosTool {
    manager: Arc<Mutex<TodoManager>>,
}

impl ListTodosTool {
    pub fn new(manager: Arc<Mutex<TodoManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl BaseTool for ListTodosTool {
    fn name(&self) -> &str {
        "list_todos"
    }

    fn description(&self) -> &str {
        "List all todo items sorted by status (doing → todo → done)."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(
        &self,
        _args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let mgr = match self.manager.lock() {
            Ok(m) => m,
            Err(e) => return ToolResult::fail(format!("Lock error: {e}")),
        };

        if !mgr.has_todos() {
            return ToolResult::ok("No todos.");
        }

        ToolResult::ok(mgr.format_status_sorted())
    }
}

#[cfg(test)]
#[path = "list_todos_tests.rs"]
mod tests;
