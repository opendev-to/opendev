//! Tool registry for discovery and dispatch.
//!
//! Stores `Arc<dyn BaseTool>` instances and dispatches execution by tool name.
//! Supports middleware pipelines, parameter validation, per-tool timeouts,
//! and same-turn call deduplication.

mod execution;
mod helpers;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::middleware::ToolMiddleware;
use crate::sanitizer::ToolResultSanitizer;
use crate::traits::{BaseTool, ToolResult, ToolTimeoutConfig};

/// Registry that maps tool names to implementations and dispatches execution.
///
/// Features:
/// - Middleware pipeline (before/after hooks)
/// - JSON Schema parameter validation
/// - Per-tool timeout configuration
/// - Same-turn call deduplication
///
/// Uses interior mutability (`RwLock`) so tools can be registered via `&self`,
/// enabling late registration (e.g. `SpawnSubagentTool` after `Arc<ToolRegistry>` is created).
pub struct ToolRegistry {
    pub(super) tools: RwLock<HashMap<String, Arc<dyn BaseTool>>>,
    pub(super) middleware: RwLock<Vec<Arc<dyn ToolMiddleware>>>,
    /// Per-tool timeout overrides keyed by tool name.
    pub(super) tool_timeouts: RwLock<HashMap<String, ToolTimeoutConfig>>,
    /// Dedup cache for same-turn identical calls.
    pub(super) dedup_cache: Mutex<HashMap<String, ToolResult>>,
    /// Sanitizer for truncating oversized tool outputs.
    pub(super) sanitizer: ToolResultSanitizer,
    /// Optional directory for overflow file storage.
    #[allow(dead_code)]
    overflow_dir: Option<std::path::PathBuf>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tool_count = self.tools.read().map(|t| t.len()).unwrap_or(0);
        let mw_count = self.middleware.read().map(|m| m.len()).unwrap_or(0);
        f.debug_struct("ToolRegistry")
            .field("tool_count", &tool_count)
            .field("middleware_count", &mw_count)
            .finish()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            middleware: RwLock::new(Vec::new()),
            tool_timeouts: RwLock::new(HashMap::new()),
            dedup_cache: Mutex::new(HashMap::new()),
            sanitizer: ToolResultSanitizer::new(),
            overflow_dir: None,
        }
    }

    /// Create with an overflow directory for storing full tool outputs to disk
    /// when they exceed inline size limits.
    pub fn with_overflow_dir(overflow_dir: std::path::PathBuf) -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            middleware: RwLock::new(Vec::new()),
            tool_timeouts: RwLock::new(HashMap::new()),
            dedup_cache: Mutex::new(HashMap::new()),
            sanitizer: ToolResultSanitizer::new().with_overflow_dir(overflow_dir.clone()),
            overflow_dir: Some(overflow_dir),
        }
    }

    /// Register a tool. If a tool with the same name exists, it's replaced.
    pub fn register(&self, tool: Arc<dyn BaseTool>) {
        let name = tool.name().to_string();
        let mut tools = self.tools.write().expect("ToolRegistry lock poisoned");
        tools.insert(name, tool);
    }

    /// Remove a tool by name and return it, if found.
    pub fn unregister(&self, name: &str) -> Option<Arc<dyn BaseTool>> {
        let mut tools = self.tools.write().expect("ToolRegistry lock poisoned");
        tools.remove(name)
    }

    /// Get a tool by exact name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn BaseTool>> {
        let tools = self.tools.read().expect("ToolRegistry lock poisoned");
        tools.get(name).cloned()
    }

    /// Check if a tool is registered.
    pub fn contains(&self, name: &str) -> bool {
        let tools = self.tools.read().expect("ToolRegistry lock poisoned");
        tools.contains_key(name)
    }

    /// Get sorted list of all registered tool names.
    pub fn tool_names(&self) -> Vec<String> {
        let tools = self.tools.read().expect("ToolRegistry lock poisoned");
        let mut names: Vec<String> = tools.keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.read().expect("ToolRegistry lock poisoned").len()
    }

    /// Whether no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools
            .read()
            .expect("ToolRegistry lock poisoned")
            .is_empty()
    }

    /// Add a middleware to the pipeline.
    pub fn add_middleware(&self, mw: Box<dyn ToolMiddleware>) {
        let mut middleware = self.middleware.write().expect("ToolRegistry lock poisoned");
        middleware.push(Arc::from(mw));
    }

    /// Number of registered middleware.
    pub fn middleware_count(&self) -> usize {
        self.middleware
            .read()
            .expect("ToolRegistry lock poisoned")
            .len()
    }

    /// Set a per-tool timeout override.
    pub fn set_tool_timeout(&self, tool_name: impl Into<String>, config: ToolTimeoutConfig) {
        let mut timeouts = self
            .tool_timeouts
            .write()
            .expect("ToolRegistry lock poisoned");
        timeouts.insert(tool_name.into(), config);
    }

    /// Set multiple per-tool timeouts at once (bulk).
    pub fn set_tool_timeouts(&self, timeouts: HashMap<String, ToolTimeoutConfig>) {
        let mut current = self
            .tool_timeouts
            .write()
            .expect("ToolRegistry lock poisoned");
        current.extend(timeouts);
    }

    /// Get the timeout config for a specific tool, if set.
    pub fn get_tool_timeout(&self, tool_name: &str) -> Option<ToolTimeoutConfig> {
        self.tool_timeouts
            .read()
            .expect("ToolRegistry lock poisoned")
            .get(tool_name)
            .cloned()
    }

    /// Clear the dedup cache (call at each turn boundary).
    pub fn clear_dedup_cache(&self) {
        if let Ok(mut cache) = self.dedup_cache.lock() {
            cache.clear();
        }
    }

    /// Number of entries in the dedup cache.
    pub fn dedup_cache_size(&self) -> usize {
        self.dedup_cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Get OpenAI-compatible function schemas for all registered tools.
    pub fn get_schemas(&self) -> Vec<serde_json::Value> {
        let tools = self.tools.read().expect("ToolRegistry lock poisoned");
        tools
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
}

#[cfg(test)]
mod tests {
    use super::helpers::{camel_to_snake_name, edit_distance, make_dedup_key};
    use super::*;
    use crate::traits::ToolContext;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    /// A tool that counts how many times it's been executed.
    #[derive(Debug)]
    struct CounterTool {
        call_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl BaseTool for CounterTool {
        fn name(&self) -> &str {
            "counter"
        }

        fn description(&self) -> &str {
            "Counts calls"
        }

        fn parameter_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "value": {"type": "string"}
                },
                "required": []
            })
        }

        async fn execute(
            &self,
            _args: HashMap<String, serde_json::Value>,
            _ctx: &ToolContext,
        ) -> ToolResult {
            let count = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
            ToolResult::ok(format!("call #{count}"))
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
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        assert!(reg.contains("echo"));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("echo").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_unregister() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        assert!(reg.contains("echo"));

        let removed = reg.unregister("echo");
        assert!(removed.is_some());
        assert!(!reg.contains("echo"));
        assert!(reg.is_empty());
    }

    #[test]
    fn test_tool_names() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let names = reg.tool_names();
        assert_eq!(names, vec!["echo"]);
    }

    #[test]
    fn test_get_schemas() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let schemas = reg.get_schemas();
        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0]["type"], "function");
        assert_eq!(schemas[0]["function"]["name"], "echo");
        assert!(schemas[0]["function"]["parameters"]["properties"]["message"].is_object());
    }

    #[tokio::test]
    async fn test_execute_success() {
        let reg = ToolRegistry::new();
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
        let reg = ToolRegistry::new();
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
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        reg.register(Arc::new(EchoTool)); // Same name
        assert_eq!(reg.len(), 1); // Not duplicated
    }

    // --- Middleware tests ---

    #[derive(Debug)]
    struct TrackingMiddleware {
        before_count: Arc<AtomicUsize>,
        after_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ToolMiddleware for TrackingMiddleware {
        async fn before_execute(
            &self,
            _name: &str,
            _args: &HashMap<String, serde_json::Value>,
            _ctx: &ToolContext,
        ) -> Result<(), String> {
            self.before_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn after_execute(&self, _name: &str, _result: &ToolResult) -> Result<(), String> {
            self.after_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Debug)]
    struct RejectMiddleware;

    #[async_trait::async_trait]
    impl ToolMiddleware for RejectMiddleware {
        async fn before_execute(
            &self,
            name: &str,
            _args: &HashMap<String, serde_json::Value>,
            _ctx: &ToolContext,
        ) -> Result<(), String> {
            Err(format!("Blocked: {name}"))
        }

        async fn after_execute(&self, _name: &str, _result: &ToolResult) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_middleware_called_on_execute() {
        let before = Arc::new(AtomicUsize::new(0));
        let after = Arc::new(AtomicUsize::new(0));

        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        reg.add_middleware(Box::new(TrackingMiddleware {
            before_count: Arc::clone(&before),
            after_count: Arc::clone(&after),
        }));

        let mut args = HashMap::new();
        args.insert("message".into(), serde_json::json!("test"));
        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("echo", args, &ctx).await;
        assert!(result.success);
        assert_eq!(before.load(Ordering::SeqCst), 1);
        assert_eq!(after.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_middleware_rejects_execution() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));
        reg.add_middleware(Box::new(RejectMiddleware));

        let mut args = HashMap::new();
        args.insert("message".into(), serde_json::json!("test"));
        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("echo", args, &ctx).await;
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Middleware error"));
        assert!(result.error.as_ref().unwrap().contains("Blocked: echo"));
    }

    #[test]
    fn test_middleware_count() {
        let reg = ToolRegistry::new();
        assert_eq!(reg.middleware_count(), 0);
        reg.add_middleware(Box::new(TrackingMiddleware {
            before_count: Arc::new(AtomicUsize::new(0)),
            after_count: Arc::new(AtomicUsize::new(0)),
        }));
        assert_eq!(reg.middleware_count(), 1);
    }

    // --- Validation tests ---

    #[tokio::test]
    async fn test_validation_rejects_missing_required() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        // EchoTool requires "message"
        let args = HashMap::new();
        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("echo", args, &ctx).await;
        assert!(!result.success);
        let err = result.error.as_ref().unwrap();
        assert!(err.contains("invalid arguments") || err.contains("Validation error"));
        assert!(err.contains("message"));
    }

    #[tokio::test]
    async fn test_validation_rejects_wrong_type() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let mut args = HashMap::new();
        args.insert("message".into(), serde_json::json!(42)); // Should be string
        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("echo", args, &ctx).await;
        assert!(!result.success);
        let err = result.error.as_ref().unwrap();
        assert!(err.contains("invalid arguments") || err.contains("Validation error"));
    }

    #[tokio::test]
    async fn test_validation_uses_custom_formatter() {
        /// A tool with a custom validation error formatter.
        #[derive(Debug)]
        struct CustomValidTool;

        #[async_trait::async_trait]
        impl BaseTool for CustomValidTool {
            fn name(&self) -> &str {
                "custom_valid"
            }
            fn description(&self) -> &str {
                "Test"
            }
            fn parameter_schema(&self) -> serde_json::Value {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string"}
                    },
                    "required": ["path"]
                })
            }
            async fn execute(
                &self,
                _args: HashMap<String, serde_json::Value>,
                _ctx: &ToolContext,
            ) -> ToolResult {
                ToolResult::ok("ok")
            }
            fn format_validation_error(
                &self,
                errors: &[crate::traits::ValidationError],
            ) -> Option<String> {
                Some(format!("CUSTOM: {} issues found", errors.len()))
            }
        }

        let reg = ToolRegistry::new();
        reg.register(Arc::new(CustomValidTool));

        let args = HashMap::new(); // missing "path"
        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("custom_valid", args, &ctx).await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.starts_with("CUSTOM: 1 issues found"));
    }

    // --- Per-tool timeout tests ---

    #[test]
    fn test_set_tool_timeout() {
        let reg = ToolRegistry::new();
        reg.set_tool_timeout(
            "bash",
            ToolTimeoutConfig {
                idle_timeout_secs: 30,
                max_timeout_secs: 120,
            },
        );
        let config = reg.get_tool_timeout("bash");
        assert!(config.is_some());
        let config = config.unwrap();
        assert_eq!(config.idle_timeout_secs, 30);
        assert_eq!(config.max_timeout_secs, 120);
    }

    #[test]
    fn test_set_tool_timeouts_bulk() {
        let reg = ToolRegistry::new();
        let mut timeouts = HashMap::new();
        timeouts.insert(
            "bash".into(),
            ToolTimeoutConfig {
                idle_timeout_secs: 30,
                max_timeout_secs: 120,
            },
        );
        timeouts.insert(
            "run_command".into(),
            ToolTimeoutConfig {
                idle_timeout_secs: 10,
                max_timeout_secs: 60,
            },
        );
        reg.set_tool_timeouts(timeouts);
        assert!(reg.get_tool_timeout("bash").is_some());
        assert!(reg.get_tool_timeout("run_command").is_some());
        assert!(reg.get_tool_timeout("echo").is_none());
    }

    #[tokio::test]
    async fn test_per_tool_timeout_applied() {
        // Tool that captures its context timeout
        #[derive(Debug)]
        struct TimeoutCaptureTool;

        #[async_trait::async_trait]
        impl BaseTool for TimeoutCaptureTool {
            fn name(&self) -> &str {
                "timeout_capture"
            }
            fn description(&self) -> &str {
                "Captures timeout config"
            }
            fn parameter_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {}})
            }
            async fn execute(
                &self,
                _args: HashMap<String, serde_json::Value>,
                ctx: &ToolContext,
            ) -> ToolResult {
                if let Some(tc) = &ctx.timeout_config {
                    ToolResult::ok(format!(
                        "idle={},max={}",
                        tc.idle_timeout_secs, tc.max_timeout_secs
                    ))
                } else {
                    ToolResult::ok("no timeout config")
                }
            }
        }

        let reg = ToolRegistry::new();
        reg.register(Arc::new(TimeoutCaptureTool));
        reg.set_tool_timeout(
            "timeout_capture",
            ToolTimeoutConfig {
                idle_timeout_secs: 15,
                max_timeout_secs: 45,
            },
        );

        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("timeout_capture", HashMap::new(), &ctx).await;
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("idle=15,max=45"));
    }

    #[tokio::test]
    async fn test_no_per_tool_timeout_uses_context() {
        #[derive(Debug)]
        struct TimeoutCaptureTool2;

        #[async_trait::async_trait]
        impl BaseTool for TimeoutCaptureTool2 {
            fn name(&self) -> &str {
                "timeout_capture2"
            }
            fn description(&self) -> &str {
                "Captures timeout config"
            }
            fn parameter_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object", "properties": {}})
            }
            async fn execute(
                &self,
                _args: HashMap<String, serde_json::Value>,
                ctx: &ToolContext,
            ) -> ToolResult {
                if let Some(tc) = &ctx.timeout_config {
                    ToolResult::ok(format!(
                        "idle={},max={}",
                        tc.idle_timeout_secs, tc.max_timeout_secs
                    ))
                } else {
                    ToolResult::ok("no timeout config")
                }
            }
        }

        let reg = ToolRegistry::new();
        reg.register(Arc::new(TimeoutCaptureTool2));
        // No per-tool timeout set, context has a global one
        let ctx = ToolContext::new("/tmp/test").with_timeout_config(ToolTimeoutConfig {
            idle_timeout_secs: 60,
            max_timeout_secs: 600,
        });
        let result = reg.execute("timeout_capture2", HashMap::new(), &ctx).await;
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("idle=60,max=600"));
    }

    // --- Deduplication tests ---

    #[tokio::test]
    async fn test_dedup_same_call_returns_cached() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let reg = ToolRegistry::new();
        reg.register(Arc::new(CounterTool {
            call_count: Arc::clone(&call_count),
        }));

        let ctx = ToolContext::new("/tmp/test");

        // First call
        let result1 = reg.execute("counter", HashMap::new(), &ctx).await;
        assert!(result1.success);
        assert_eq!(result1.output.as_deref(), Some("call #1"));
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Second identical call — should return cached
        let result2 = reg.execute("counter", HashMap::new(), &ctx).await;
        assert!(result2.success);
        assert_eq!(result2.output.as_deref(), Some("call #1"));
        // Tool should NOT have been called again
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_dedup_different_args_not_cached() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let reg = ToolRegistry::new();
        reg.register(Arc::new(CounterTool {
            call_count: Arc::clone(&call_count),
        }));

        let ctx = ToolContext::new("/tmp/test");

        let mut args1 = HashMap::new();
        args1.insert("value".into(), serde_json::json!("a"));
        let result1 = reg.execute("counter", args1, &ctx).await;
        assert!(result1.success);

        let mut args2 = HashMap::new();
        args2.insert("value".into(), serde_json::json!("b"));
        let result2 = reg.execute("counter", args2, &ctx).await;
        assert!(result2.success);

        // Both calls should have executed
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_dedup_clear_between_turns() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let reg = ToolRegistry::new();
        reg.register(Arc::new(CounterTool {
            call_count: Arc::clone(&call_count),
        }));

        let ctx = ToolContext::new("/tmp/test");

        // First call
        reg.execute("counter", HashMap::new(), &ctx).await;
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Clear cache (simulating turn boundary)
        reg.clear_dedup_cache();
        assert_eq!(reg.dedup_cache_size(), 0);

        // Same call again — should execute since cache was cleared
        let result = reg.execute("counter", HashMap::new(), &ctx).await;
        assert!(result.success);
        assert_eq!(result.output.as_deref(), Some("call #2"));
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_dedup_key_deterministic() {
        let mut args1 = HashMap::new();
        args1.insert("a".into(), serde_json::json!(1));
        args1.insert("b".into(), serde_json::json!(2));

        let mut args2 = HashMap::new();
        args2.insert("b".into(), serde_json::json!(2));
        args2.insert("a".into(), serde_json::json!(1));

        let key1 = make_dedup_key("test", &args1);
        let key2 = make_dedup_key("test", &args2);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_dedup_key_different_tool_names() {
        let args = HashMap::new();
        let key1 = make_dedup_key("tool_a", &args);
        let key2 = make_dedup_key("tool_b", &args);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_registry_debug() {
        let reg = ToolRegistry::new();
        let debug = format!("{reg:?}");
        assert!(debug.contains("ToolRegistry"));
    }

    // --- Fuzzy tool name resolution tests ---

    #[tokio::test]
    async fn test_execute_case_insensitive_match() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let mut args = HashMap::new();
        args.insert("message".into(), serde_json::json!("hello"));

        let ctx = ToolContext::new("/tmp/test");
        // "Echo" should match "echo" case-insensitively
        let result = reg.execute("Echo", args, &ctx).await;
        assert!(
            result.success,
            "Case-insensitive match should work: {:?}",
            result.error
        );
        assert_eq!(result.output.as_deref(), Some("Echo: hello"));
    }

    #[tokio::test]
    async fn test_execute_camel_case_to_snake() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        // "echo" is already snake_case, let's test with a PascalCase-registered tool
        // We'll register with snake_case name and call with PascalCase
        // Since EchoTool returns "echo", "Echo" -> case insensitive match covers this.
        // Instead test camel_to_snake_name directly
        assert_eq!(camel_to_snake_name("ReadFile"), "read_file");
        assert_eq!(camel_to_snake_name("webFetch"), "web_fetch");
        assert_eq!(camel_to_snake_name("echo"), "echo");
        assert_eq!(camel_to_snake_name("SpawnSubagent"), "spawn_subagent");
    }

    #[tokio::test]
    async fn test_execute_unknown_suggests_similar() {
        let reg = ToolRegistry::new();
        reg.register(Arc::new(EchoTool));

        let ctx = ToolContext::new("/tmp/test");
        let result = reg.execute("ech", HashMap::new(), &ctx).await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(
            err.contains("Unknown tool: ech"),
            "Error should mention unknown tool"
        );
        assert!(err.contains("echo"), "Error should suggest 'echo': {}", err);
    }

    #[test]
    fn test_edit_distance() {
        assert_eq!(edit_distance("echo", "echo"), 0);
        assert_eq!(edit_distance("echo", "ech"), 1);
        assert_eq!(edit_distance("echo", "Echo"), 1);
        assert_eq!(edit_distance("read", "write"), 4);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
    }

    #[test]
    fn test_camel_to_snake_name() {
        assert_eq!(camel_to_snake_name("readFile"), "read_file");
        assert_eq!(camel_to_snake_name("ReadFile"), "read_file");
        assert_eq!(camel_to_snake_name("read_file"), "read_file");
        assert_eq!(camel_to_snake_name("webFetch"), "web_fetch");
        assert_eq!(camel_to_snake_name("HTMLParser"), "h_t_m_l_parser");
    }
}
