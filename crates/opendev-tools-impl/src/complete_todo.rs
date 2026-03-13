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
mod tests {
    use super::*;
    use opendev_runtime::TodoStatus;

    fn make_tool() -> (CompleteTodoTool, Arc<Mutex<TodoManager>>) {
        let mgr = Arc::new(Mutex::new(TodoManager::from_steps(&[
            "Step A".into(),
            "Step B".into(),
        ])));
        let tool = CompleteTodoTool::new(Arc::clone(&mgr));
        (tool, mgr)
    }

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_complete_by_id() {
        let (tool, mgr) = make_tool();
        let ctx = ToolContext::new("/tmp");
        let result = tool
            .execute(make_args(&[("id", serde_json::json!("1"))]), &ctx)
            .await;
        assert!(result.success);
        assert_eq!(
            mgr.lock().unwrap().get(1).unwrap().status,
            TodoStatus::Completed
        );
    }

    #[tokio::test]
    async fn test_complete_all_message() {
        let (tool, _mgr) = make_tool();
        let ctx = ToolContext::new("/tmp");
        tool.execute(make_args(&[("id", serde_json::json!("1"))]), &ctx)
            .await;
        let result = tool
            .execute(make_args(&[("id", serde_json::json!("2"))]), &ctx)
            .await;
        assert!(result.output.unwrap().contains("All todos are done"));
    }

    #[tokio::test]
    async fn test_complete_not_found() {
        let (tool, _mgr) = make_tool();
        let ctx = ToolContext::new("/tmp");
        let result = tool
            .execute(make_args(&[("id", serde_json::json!("999"))]), &ctx)
            .await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_complete_by_numeric_value() {
        let (tool, mgr) = make_tool();
        let ctx = ToolContext::new("/tmp");
        let result = tool
            .execute(make_args(&[("id", serde_json::json!(2))]), &ctx)
            .await;
        assert!(result.success);
        assert_eq!(
            mgr.lock().unwrap().get(2).unwrap().status,
            TodoStatus::Completed
        );
    }
}
