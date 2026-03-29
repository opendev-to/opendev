//! clear_todos tool — remove all todo items.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opendev_runtime::TodoManager;
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool that clears all todo items.
#[derive(Debug)]
pub struct ClearTodosTool {
    manager: Arc<Mutex<TodoManager>>,
}

impl ClearTodosTool {
    pub fn new(manager: Arc<Mutex<TodoManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl BaseTool for ClearTodosTool {
    fn name(&self) -> &str {
        "clear_todos"
    }

    fn description(&self) -> &str {
        "Clear all todo items from the list."
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
        let mut mgr = match self.manager.lock() {
            Ok(m) => m,
            Err(e) => return ToolResult::fail(format!("Lock error: {e}")),
        };

        mgr.clear();
        ToolResult::ok("All todos cleared.")
    }
}

#[cfg(test)]
#[path = "clear_todos_tests.rs"]
mod tests;
