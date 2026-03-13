//! write_todos tool — replace the entire todo list.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opendev_runtime::{TodoManager, TodoStatus, parse_status, strip_markdown};
use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool that replaces the entire todo list.
#[derive(Debug)]
pub struct WriteTodosTool {
    manager: Arc<Mutex<TodoManager>>,
}

impl WriteTodosTool {
    pub fn new(manager: Arc<Mutex<TodoManager>>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl BaseTool for WriteTodosTool {
    fn name(&self) -> &str {
        "write_todos"
    }

    fn description(&self) -> &str {
        "Replace the entire todo list with new items. Each item can be a string \
         or an object with content, status, and activeForm fields."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "List of todo items. Each can be a string or an object with 'content' (required), 'status' (optional: pending/in_progress/completed), and 'activeForm' (optional: present continuous text for spinner).",
                    "items": {
                        "oneOf": [
                            { "type": "string" },
                            {
                                "type": "object",
                                "properties": {
                                    "content": { "type": "string" },
                                    "status": { "type": "string" },
                                    "activeForm": { "type": "string" }
                                },
                                "required": ["content"]
                            }
                        ]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let todos_val = match args.get("todos") {
            Some(v) if v.is_array() => v.as_array().unwrap(),
            _ => return ToolResult::fail("todos array is required"),
        };

        let mut items = Vec::new();
        for item in todos_val {
            if let Some(s) = item.as_str() {
                let title = strip_markdown(s);
                items.push((title, TodoStatus::Pending, String::new()));
            } else if let Some(obj) = item.as_object() {
                let content = match obj.get("content").and_then(|v| v.as_str()) {
                    Some(c) => strip_markdown(c),
                    None => return ToolResult::fail("Each todo object requires a 'content' field"),
                };
                let status = obj
                    .get("status")
                    .and_then(|v| v.as_str())
                    .and_then(parse_status)
                    .unwrap_or(TodoStatus::Pending);
                let active_form = obj
                    .get("activeForm")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                items.push((content, status, active_form));
            } else {
                return ToolResult::fail("Each todo must be a string or object");
            }
        }

        let mut mgr = match self.manager.lock() {
            Ok(m) => m,
            Err(e) => return ToolResult::fail(format!("Lock error: {e}")),
        };

        mgr.write_todos(items);
        ToolResult::ok(mgr.format_status_sorted())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tool() -> (WriteTodosTool, Arc<Mutex<TodoManager>>) {
        let mgr = Arc::new(Mutex::new(TodoManager::new()));
        let tool = WriteTodosTool::new(Arc::clone(&mgr));
        (tool, mgr)
    }

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[tokio::test]
    async fn test_write_todos_strings() {
        let (tool, mgr) = make_tool();
        let ctx = ToolContext::new("/tmp");
        let result = tool
            .execute(
                make_args(&[("todos", serde_json::json!(["Step A", "Step B", "Step C"]))]),
                &ctx,
            )
            .await;
        assert!(result.success);
        assert_eq!(mgr.lock().unwrap().total(), 3);
    }

    #[tokio::test]
    async fn test_write_todos_objects() {
        let (tool, mgr) = make_tool();
        let ctx = ToolContext::new("/tmp");
        let result = tool
            .execute(
                make_args(&[(
                    "todos",
                    serde_json::json!([
                        {"content": "First", "status": "in_progress", "activeForm": "Working on first"},
                        {"content": "Second"}
                    ]),
                )]),
                &ctx,
            )
            .await;
        assert!(result.success);
        let m = mgr.lock().unwrap();
        assert_eq!(m.total(), 2);
        assert_eq!(m.get(1).unwrap().status, TodoStatus::InProgress);
        assert_eq!(m.get(1).unwrap().active_form, "Working on first");
    }

    #[tokio::test]
    async fn test_write_todos_replaces() {
        let (tool, mgr) = make_tool();
        let ctx = ToolContext::new("/tmp");
        // Write initial
        tool.execute(make_args(&[("todos", serde_json::json!(["Old"]))]), &ctx)
            .await;
        assert_eq!(mgr.lock().unwrap().total(), 1);

        // Replace
        tool.execute(
            make_args(&[("todos", serde_json::json!(["New A", "New B"]))]),
            &ctx,
        )
        .await;
        assert_eq!(mgr.lock().unwrap().total(), 2);
        assert_eq!(mgr.lock().unwrap().get(1).unwrap().title, "New A");
    }

    #[tokio::test]
    async fn test_write_todos_missing_arg() {
        let (tool, _mgr) = make_tool();
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
    }
}
