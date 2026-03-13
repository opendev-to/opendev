//! Tool registry for discovery and dispatch.
//!
//! Stores `Arc<dyn BaseTool>` instances and dispatches execution by tool name.

use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

use crate::normalizer;
use crate::traits::{BaseTool, ToolContext, ToolResult};

/// Registry that maps tool names to implementations and dispatches execution.
#[derive(Debug, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn BaseTool>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Replaces any existing tool with the same name.
    pub fn register(&mut self, tool: Arc<dyn BaseTool>) {
        let name = tool.name().to_string();
        info!(tool = %name, "Registered tool");
        self.tools.insert(name, tool);
    }

    /// Unregister a tool by name. Returns the tool if it existed.
    pub fn unregister(&mut self, name: &str) -> Option<Arc<dyn BaseTool>> {
        self.tools.remove(name)
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn BaseTool>> {
        self.tools.get(name)
    }

    /// Check if a tool is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all registered tool names.
    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Get JSON schemas for all registered tools.
    ///
    /// Returns a list of tool schema objects suitable for LLM tool-use.
    pub fn get_schemas(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.parameter_schema()
                    }
                })
            })
            .collect()
    }

    /// Execute a tool by name with parameter normalization.
    ///
    /// Normalizes parameters (camelCase -> snake_case, path resolution) before
    /// passing them to the tool's execute method. Automatically measures
    /// execution time and attaches it to the result as `duration_ms`.
    pub async fn execute(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let tool = match self.tools.get(tool_name) {
            Some(t) => t,
            None => {
                warn!(tool = %tool_name, "Unknown tool");
                return ToolResult::fail(format!("Unknown tool: {tool_name}"));
            }
        };

        // Normalize parameters
        let working_dir = ctx.working_dir.to_string_lossy().to_string();
        let normalized = normalizer::normalize_params(tool_name, args, Some(&working_dir));

        let start = std::time::Instant::now();
        let mut result = tool.execute(normalized, ctx).await;
        result.duration_ms = Some(start.elapsed().as_millis() as u64);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A simple test tool for verifying registry behavior.
    #[derive(Debug)]
    struct EchoTool;

    #[async_trait::async_trait]
    impl BaseTool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes back the input"
        }

        fn parameter_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string", "description": "Message to echo"}
                },
                "required": ["message"]
            })
        }

        async fn execute(
            &self,
            args: HashMap<String, serde_json::Value>,
            _ctx: &ToolContext,
        ) -> ToolResult {
            let message = args
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("(no message)");
            ToolResult::ok(format!("Echo: {message}"))
        }
    }

    #[test]
    fn test_registry_new() {
        let reg = ToolRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        assert!(reg.contains("echo"));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("echo").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_unregister() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        assert!(reg.contains("echo"));

        let removed = reg.unregister("echo");
        assert!(removed.is_some());
        assert!(!reg.contains("echo"));
        assert!(reg.is_empty());
    }

    #[test]
    fn test_tool_names() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let names = reg.tool_names();
        assert_eq!(names, vec!["echo"]);
    }

    #[test]
    fn test_get_schemas() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let schemas = reg.get_schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0]["type"], "function");
        assert_eq!(schemas[0]["function"]["name"], "echo");
        assert!(schemas[0]["function"]["parameters"]["properties"]["message"].is_object());
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let mut args = HashMap::new();
        args.insert("message".into(), serde_json::json!("hello"));

        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("echo", args, &ctx).await;
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("Echo: hello"));
    }

    #[tokio::test]
    async fn test_execute_populates_duration_ms() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let mut args = HashMap::new();
        args.insert("message".into(), serde_json::json!("timing"));

        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("echo", args, &ctx).await;
        assert!(result.success);
        // duration_ms should be populated by the registry
        assert!(result.duration_ms.is_some());
        // Execution should be near-instant (< 100ms for an echo)
        assert!(result.duration_ms.unwrap() < 100);
    }

    #[tokio::test]
    async fn test_execute_unknown_tool() {
        let reg = ToolRegistry::new();
        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("nonexistent", HashMap::new(), &ctx).await;
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Unknown tool"));
    }

    #[test]
    fn test_register_replaces_existing() {
        let mut reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        reg.register(Arc::new(EchoTool)); // Same name
        assert_eq!(reg.len(), 1); // Not duplicated
    }
}
