//! Batch tool — execute multiple tool invocations in parallel.
//!
//! Dispatches multiple tool calls through the registry concurrently
//! using `tokio::spawn`, matching OpenCode's BatchTool behavior.
//! Limited to 25 concurrent calls; self-recursion is blocked.

use std::collections::HashMap;
use std::sync::Arc;

use opendev_tools_core::{BaseTool, ToolContext, ToolRegistry, ToolResult};
use tracing::{debug, warn};

/// Maximum number of concurrent tool calls in a single batch.
const MAX_BATCH_SIZE: usize = 25;

/// Tools that cannot be called from within a batch (prevent recursion).
const DISALLOWED_TOOLS: &[&str] = &["batch_tool", "task_complete"];

/// Tool for batch-executing multiple tool invocations in parallel.
#[derive(Debug)]
pub struct BatchTool {
    /// Tool registry for dispatching calls.
    registry: Arc<ToolRegistry>,
}

impl BatchTool {
    /// Create a new batch tool backed by the given registry.
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait::async_trait]
impl BaseTool for BatchTool {
    fn name(&self) -> &str {
        "batch_tool"
    }

    fn description(&self) -> &str {
        "Execute multiple tool calls in parallel. Use this when you need to run \
         several independent operations simultaneously (e.g., reading multiple files, \
         running multiple searches). Each tool call runs concurrently and results \
         are returned together. Maximum 25 calls per batch."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "invocations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": {
                                "type": "string",
                                "description": "Name of the tool to invoke"
                            },
                            "input": {
                                "type": "object",
                                "description": "Tool input parameters"
                            }
                        },
                        "required": ["tool"]
                    },
                    "minItems": 1,
                    "maxItems": 25,
                    "description": "List of tool invocations to execute in parallel"
                }
            },
            "required": ["invocations"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let invocations = match args.get("invocations").and_then(|v| v.as_array()) {
            Some(arr) => arr.clone(),
            None => return ToolResult::fail("invocations array is required"),
        };

        if invocations.is_empty() {
            return ToolResult::fail("invocations array must not be empty");
        }

        // Parse invocations
        let mut parsed: Vec<(usize, String, HashMap<String, serde_json::Value>)> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for (i, inv) in invocations.iter().enumerate() {
            let tool_name = match inv.get("tool").and_then(|v| v.as_str()) {
                Some(name) => name.to_string(),
                None => {
                    errors.push(format!("[{i}] missing 'tool' field"));
                    continue;
                }
            };

            // Block disallowed tools
            if DISALLOWED_TOOLS.contains(&tool_name.as_str()) {
                errors.push(format!("[{i}] tool '{tool_name}' cannot be used in batch"));
                continue;
            }

            // Verify tool exists
            if !self.registry.contains(&tool_name) {
                errors.push(format!("[{i}] unknown tool '{tool_name}'"));
                continue;
            }

            let tool_input: HashMap<String, serde_json::Value> = inv
                .get("input")
                .and_then(|v| v.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();

            parsed.push((i, tool_name, tool_input));
        }

        // Enforce size limit
        let discarded = if parsed.len() > MAX_BATCH_SIZE {
            let overflow = parsed.len() - MAX_BATCH_SIZE;
            parsed.truncate(MAX_BATCH_SIZE);
            warn!(
                overflow,
                "Batch tool: discarding {} invocations beyond limit of {}",
                overflow,
                MAX_BATCH_SIZE
            );
            overflow
        } else {
            0
        };

        if parsed.is_empty() {
            return ToolResult::fail(format!(
                "No valid invocations to execute. Errors:\n{}",
                errors.join("\n")
            ));
        }

        debug!(
            count = parsed.len(),
            "Batch tool: executing tool calls in parallel"
        );

        // Spawn all tool calls concurrently
        let mut handles = Vec::with_capacity(parsed.len());
        for (idx, tool_name, tool_input) in parsed {
            let registry = Arc::clone(&self.registry);
            let tool_ctx = ctx.clone();
            let name = tool_name.clone();

            handles.push(tokio::spawn(async move {
                let start = std::time::Instant::now();
                let result = registry.execute(&name, tool_input, &tool_ctx).await;
                let duration_ms = start.elapsed().as_millis() as u64;
                (idx, name, result, duration_ms)
            }));
        }

        // Collect results
        let mut results: Vec<serde_json::Value> = Vec::with_capacity(handles.len());
        let mut success_count = 0usize;
        let mut fail_count = 0usize;

        for handle in handles {
            match handle.await {
                Ok((idx, tool_name, result, duration_ms)) => {
                    let status = if result.success {
                        success_count += 1;
                        "success"
                    } else {
                        fail_count += 1;
                        "error"
                    };

                    let output = if result.success {
                        result.output.unwrap_or_default()
                    } else {
                        result.error.unwrap_or_else(|| "Unknown error".to_string())
                    };

                    results.push(serde_json::json!({
                        "index": idx,
                        "tool": tool_name,
                        "status": status,
                        "output": output,
                        "duration_ms": duration_ms,
                    }));
                }
                Err(e) => {
                    fail_count += 1;
                    results.push(serde_json::json!({
                        "status": "error",
                        "output": format!("Task panicked: {e}"),
                    }));
                }
            }
        }

        // Sort results by original index
        results.sort_by(|a, b| {
            let ai = a.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            let bi = b.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            ai.cmp(&bi)
        });

        // Build output summary
        let mut output_parts: Vec<String> = Vec::new();
        output_parts.push(format!(
            "Batch completed: {success_count} succeeded, {fail_count} failed"
        ));
        if discarded > 0 {
            output_parts.push(format!("({discarded} invocations discarded — exceeds limit of {MAX_BATCH_SIZE})"));
        }
        if !errors.is_empty() {
            output_parts.push(format!("Validation errors:\n{}", errors.join("\n")));
        }

        // Append individual results
        for r in &results {
            let tool = r.get("tool").and_then(|v| v.as_str()).unwrap_or("?");
            let status = r.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            let output = r.get("output").and_then(|v| v.as_str()).unwrap_or("");
            let duration = r.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);

            // Truncate individual outputs for the summary
            let truncated = if output.len() > 500 {
                format!("{}...(truncated)", &output[..500])
            } else {
                output.to_string()
            };

            output_parts.push(format!(
                "\n--- [{tool}] {status} ({duration}ms) ---\n{truncated}"
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert("results".into(), serde_json::json!(results));
        metadata.insert("success_count".into(), serde_json::json!(success_count));
        metadata.insert("fail_count".into(), serde_json::json!(fail_count));

        ToolResult::ok_with_metadata(output_parts.join("\n"), metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_args(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    fn make_registry_with_tools() -> Arc<ToolRegistry> {
        let registry = Arc::new(ToolRegistry::new());

        // Register a simple echo tool for testing
        #[derive(Debug)]
        struct EchoTool;

        #[async_trait::async_trait]
        impl BaseTool for EchoTool {
            fn name(&self) -> &str {
                "echo"
            }
            fn description(&self) -> &str {
                "Echo input"
            }
            fn parameter_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {"message": {"type": "string"}}})
            }
            async fn execute(
                &self,
                args: HashMap<String, serde_json::Value>,
                _ctx: &ToolContext,
            ) -> ToolResult {
                let msg = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no message)");
                ToolResult::ok(msg)
            }
        }

        // Register a tool that always fails
        #[derive(Debug)]
        struct FailTool;

        #[async_trait::async_trait]
        impl BaseTool for FailTool {
            fn name(&self) -> &str {
                "fail_tool"
            }
            fn description(&self) -> &str {
                "Always fails"
            }
            fn parameter_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {}})
            }
            async fn execute(
                &self,
                _args: HashMap<String, serde_json::Value>,
                _ctx: &ToolContext,
            ) -> ToolResult {
                ToolResult::fail("intentional failure")
            }
        }

        registry.register(Arc::new(EchoTool));
        registry.register(Arc::new(FailTool));
        registry
    }

    #[tokio::test]
    async fn test_batch_missing_invocations() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let result = tool.execute(HashMap::new(), &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("invocations"));
    }

    #[tokio::test]
    async fn test_batch_empty_invocations() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[("invocations", serde_json::json!([]))]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_batch_parallel_execution() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "invocations",
            serde_json::json!([
                {"tool": "echo", "input": {"message": "hello"}},
                {"tool": "echo", "input": {"message": "world"}}
            ]),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("2 succeeded, 0 failed"));
        assert!(output.contains("hello"));
        assert!(output.contains("world"));
    }

    #[tokio::test]
    async fn test_batch_mixed_success_and_failure() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "invocations",
            serde_json::json!([
                {"tool": "echo", "input": {"message": "ok"}},
                {"tool": "fail_tool", "input": {}}
            ]),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let output = result.output.unwrap();
        assert!(output.contains("1 succeeded, 1 failed"));
        assert_eq!(
            result.metadata.get("success_count"),
            Some(&serde_json::json!(1))
        );
        assert_eq!(
            result.metadata.get("fail_count"),
            Some(&serde_json::json!(1))
        );
    }

    #[tokio::test]
    async fn test_batch_blocks_self_recursion() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "invocations",
            serde_json::json!([
                {"tool": "batch_tool", "input": {}}
            ]),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("cannot be used in batch"));
    }

    #[tokio::test]
    async fn test_batch_blocks_task_complete() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "invocations",
            serde_json::json!([
                {"tool": "task_complete", "input": {}}
            ]),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("cannot be used in batch"));
    }

    #[tokio::test]
    async fn test_batch_unknown_tool() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "invocations",
            serde_json::json!([
                {"tool": "nonexistent_tool", "input": {}}
            ]),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown tool"));
    }

    #[tokio::test]
    async fn test_batch_missing_tool_field() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "invocations",
            serde_json::json!([
                {"input": {"message": "no tool name"}}
            ]),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("missing 'tool' field"));
    }

    #[tokio::test]
    async fn test_batch_results_sorted_by_index() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        let args = make_args(&[(
            "invocations",
            serde_json::json!([
                {"tool": "echo", "input": {"message": "first"}},
                {"tool": "echo", "input": {"message": "second"}},
                {"tool": "echo", "input": {"message": "third"}}
            ]),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        let results = result
            .metadata
            .get("results")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0]["index"], 0);
        assert_eq!(results[1]["index"], 1);
        assert_eq!(results[2]["index"], 2);
    }

    #[tokio::test]
    async fn test_batch_default_empty_input() {
        let registry = make_registry_with_tools();
        let tool = BatchTool::new(registry);
        let ctx = ToolContext::new("/tmp");
        // No "input" field — should default to empty HashMap
        let args = make_args(&[(
            "invocations",
            serde_json::json!([
                {"tool": "echo"}
            ]),
        )]);
        let result = tool.execute(args, &ctx).await;
        assert!(result.success);
        assert!(result.output.unwrap().contains("1 succeeded"));
    }
}
