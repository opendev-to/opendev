//! Tool registry for discovery and dispatch.
//!
//! Stores `Arc<dyn BaseTool>` instances and dispatches execution by tool name.
//! Supports middleware pipelines, parameter validation, per-tool timeouts,
//! and same-turn call deduplication.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use tracing::{info, warn};

use crate::middleware::ToolMiddleware;
use crate::normalizer;
use crate::sanitizer::ToolResultSanitizer;
use crate::traits::{BaseTool, ToolContext, ToolResult, ToolTimeoutConfig};
use crate::validation;

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
    tools: RwLock<HashMap<String, Arc<dyn BaseTool>>>,
    middleware: RwLock<Vec<Arc<dyn ToolMiddleware>>>,
    /// Per-tool timeout overrides keyed by tool name.
    tool_timeouts: RwLock<HashMap<String, ToolTimeoutConfig>>,
    /// Cache for same-turn deduplication. Keyed by hash of (tool_name, args).
    dedup_cache: Mutex<HashMap<String, ToolResult>>,
    /// Sanitizer that truncates large tool outputs before they enter LLM context.
    sanitizer: ToolResultSanitizer,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let tools = self.tools.read().expect("ToolRegistry lock poisoned");
        let middleware = self.middleware.read().expect("ToolRegistry lock poisoned");
        let tool_timeouts = self
            .tool_timeouts
            .read()
            .expect("ToolRegistry lock poisoned");
        f.debug_struct("ToolRegistry")
            .field("tools", &tools.keys().collect::<Vec<_>>())
            .field("middleware_count", &middleware.len())
            .field("tool_timeouts", &*tool_timeouts)
            .field(
                "dedup_cache_size",
                &self.dedup_cache.lock().map(|c| c.len()).unwrap_or(0),
            )
            .field("sanitizer", &"ToolResultSanitizer")
            .finish()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            middleware: RwLock::new(Vec::new()),
            tool_timeouts: RwLock::new(HashMap::new()),
            dedup_cache: Mutex::new(HashMap::new()),
            sanitizer: ToolResultSanitizer::new(),
        }
    }

    /// Create a registry with overflow storage for truncated tool output.
    ///
    /// When a tool's output exceeds its truncation limit, the full output is
    /// saved to `overflow_dir` for later retrieval. Files are retained for 7 days.
    pub fn with_overflow_dir(overflow_dir: std::path::PathBuf) -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            middleware: RwLock::new(Vec::new()),
            tool_timeouts: RwLock::new(HashMap::new()),
            dedup_cache: Mutex::new(HashMap::new()),
            sanitizer: ToolResultSanitizer::new().with_overflow_dir(overflow_dir),
        }
    }

    /// Register a tool. Replaces any existing tool with the same name.
    pub fn register(&self, tool: Arc<dyn BaseTool>) {
        let name = tool.name().to_string();
        info!(tool = %name, "Registered tool");
        self.tools
            .write()
            .expect("ToolRegistry lock poisoned")
            .insert(name, tool);
    }

    /// Unregister a tool by name. Returns the tool if it existed.
    pub fn unregister(&self, name: &str) -> Option<Arc<dyn BaseTool>> {
        self.tools
            .write()
            .expect("ToolRegistry lock poisoned")
            .remove(name)
    }

    /// Look up a tool by name (returns a cloned Arc).
    pub fn get(&self, name: &str) -> Option<Arc<dyn BaseTool>> {
        self.tools
            .read()
            .expect("ToolRegistry lock poisoned")
            .get(name)
            .cloned()
    }

    /// Check if a tool is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.tools
            .read()
            .expect("ToolRegistry lock poisoned")
            .contains_key(name)
    }

    /// Get all registered tool names.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools
            .read()
            .expect("ToolRegistry lock poisoned")
            .keys()
            .cloned()
            .collect()
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.read().expect("ToolRegistry lock poisoned").len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools
            .read()
            .expect("ToolRegistry lock poisoned")
            .is_empty()
    }

    // --- Middleware ---

    /// Add a middleware to the execution pipeline.
    ///
    /// Middleware are called in insertion order for `before_execute` and
    /// in the same order for `after_execute`.
    pub fn add_middleware(&self, mw: Box<dyn ToolMiddleware>) {
        self.middleware
            .write()
            .expect("ToolRegistry lock poisoned")
            .push(Arc::from(mw));
    }

    /// Get the number of registered middleware.
    pub fn middleware_count(&self) -> usize {
        self.middleware
            .read()
            .expect("ToolRegistry lock poisoned")
            .len()
    }

    // --- Per-tool timeouts ---

    /// Set a timeout configuration for a specific tool.
    pub fn set_tool_timeout(&self, tool_name: impl Into<String>, config: ToolTimeoutConfig) {
        self.tool_timeouts
            .write()
            .expect("ToolRegistry lock poisoned")
            .insert(tool_name.into(), config);
    }

    /// Set timeout configurations for multiple tools at once.
    pub fn set_tool_timeouts(&self, timeouts: HashMap<String, ToolTimeoutConfig>) {
        self.tool_timeouts
            .write()
            .expect("ToolRegistry lock poisoned")
            .extend(timeouts);
    }

    /// Get the timeout configuration for a tool (if any).
    pub fn get_tool_timeout(&self, tool_name: &str) -> Option<ToolTimeoutConfig> {
        self.tool_timeouts
            .read()
            .expect("ToolRegistry lock poisoned")
            .get(tool_name)
            .cloned()
    }

    // --- Deduplication ---

    /// Clear the deduplication cache. Call this between turns.
    pub fn clear_dedup_cache(&self) {
        if let Ok(mut cache) = self.dedup_cache.lock() {
            cache.clear();
        }
    }

    /// Get the number of entries in the dedup cache.
    pub fn dedup_cache_size(&self) -> usize {
        self.dedup_cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Get JSON schemas for all registered tools.
    ///
    /// Returns a list of tool schema objects suitable for LLM tool-use.
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

    /// Suggest similar tool names for a mistyped name (edit distance or substring match).
    fn suggest_tool_names(&self, name: &str) -> Vec<String> {
        let tools = self.tools.read().expect("ToolRegistry lock poisoned");
        let lower = name.to_lowercase();
        let mut suggestions: Vec<String> = Vec::new();
        for registered in tools.keys() {
            let reg_lower = registered.to_lowercase();
            // Substring match or short edit distance
            if reg_lower.contains(&lower)
                || lower.contains(&reg_lower)
                || edit_distance(&lower, &reg_lower) <= 3
            {
                suggestions.push(registered.clone());
            }
        }
        suggestions.sort();
        suggestions.truncate(5);
        suggestions
    }

    /// Try to find a tool by name with fuzzy matching fallback.
    ///
    /// If exact match fails, tries:
    /// 1. Case-insensitive match
    /// 2. Common name transformations (e.g., `ReadFile` -> `read_file`)
    ///
    /// Returns `(tool, resolved_name)` or `None`.
    fn resolve_tool(&self, name: &str) -> Option<(Arc<dyn BaseTool>, String)> {
        let tools = self.tools.read().expect("ToolRegistry lock poisoned");

        // Exact match (fast path)
        if let Some(t) = tools.get(name) {
            return Some((Arc::clone(t), name.to_string()));
        }

        // Case-insensitive match
        let lower = name.to_lowercase();
        for (registered_name, tool) in tools.iter() {
            if registered_name.to_lowercase() == lower {
                info!(
                    requested = %name,
                    resolved = %registered_name,
                    "Fuzzy tool name match (case-insensitive)"
                );
                return Some((Arc::clone(tool), registered_name.clone()));
            }
        }

        // CamelCase/PascalCase -> snake_case transformation
        let snake = camel_to_snake_name(name);
        if snake != name
            && let Some(t) = tools.get(&snake)
        {
            info!(
                requested = %name,
                resolved = %snake,
                "Fuzzy tool name match (camelCase -> snake_case)"
            );
            return Some((Arc::clone(t), snake));
        }

        None
    }

    /// Execute a tool by name with parameter normalization.
    ///
    /// Pipeline:
    /// 1. Look up tool (with fuzzy name matching)
    /// 2. Normalize parameters (camelCase -> snake_case, path resolution)
    /// 3. Check dedup cache — return cached result if identical call in same turn
    /// 4. Validate parameters against the tool's JSON Schema
    /// 5. Run `before_execute` middleware (abort on error)
    /// 6. Apply per-tool timeout config to context
    /// 7. Execute tool
    /// 8. Run `after_execute` middleware
    /// 9. Cache result for dedup
    /// 10. Attach duration_ms
    pub async fn execute(
        &self,
        tool_name: &str,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        // Clone Arc out of the read lock so we don't hold it during execution
        let (tool, resolved_name) = match self.resolve_tool(tool_name) {
            Some((t, name)) => (t, name),
            None => {
                // Build suggestion list for "did you mean?"
                let suggestions = self.suggest_tool_names(tool_name);
                let hint = if suggestions.is_empty() {
                    String::new()
                } else {
                    format!(". Did you mean: {}?", suggestions.join(", "))
                };
                warn!(tool = %tool_name, "Unknown tool");
                return ToolResult::fail(format!("Unknown tool: {tool_name}{hint}"));
            }
        };
        let tool_name = &resolved_name;

        // Normalize parameters
        let working_dir = ctx.working_dir.to_string_lossy().to_string();
        let normalized = normalizer::normalize_params(tool_name, args, Some(&working_dir));

        // Deduplication: check cache
        let dedup_key = make_dedup_key(tool_name, &normalized);
        if let Ok(cache) = self.dedup_cache.lock()
            && let Some(cached) = cache.get(&dedup_key)
        {
            info!(tool = %tool_name, "Returning cached result (dedup)");
            return cached.clone();
        }

        // Validate parameters against schema
        let schema = tool.parameter_schema();
        let validation_errors = validation::validate_args_detailed(&normalized, &schema);
        if !validation_errors.is_empty() {
            // Try tool-specific formatter first, then fall back to generic message
            let error_msg = tool
                .format_validation_error(&validation_errors)
                .unwrap_or_else(|| {
                    let details: Vec<String> =
                        validation_errors.iter().map(|e| e.to_string()).collect();
                    format!(
                        "The {} tool was called with invalid arguments:\n  - {}\nPlease fix the arguments and try again.",
                        tool_name,
                        details.join("\n  - ")
                    )
                });
            warn!(tool = %tool_name, error = %error_msg, "Parameter validation failed");
            return ToolResult::fail(error_msg);
        }

        // Clone middleware Arcs out of the lock so we can call async methods
        let middleware: Vec<Arc<dyn ToolMiddleware>> = {
            let mw = self.middleware.read().expect("ToolRegistry lock poisoned");
            mw.clone()
        };

        // Run before_execute middleware
        for mw in &middleware {
            if let Err(err) = mw.before_execute(tool_name, &normalized, ctx).await {
                warn!(tool = %tool_name, error = %err, "Middleware rejected execution");
                return ToolResult::fail(format!("Middleware error: {err}"));
            }
        }

        // Apply per-tool timeout config
        let exec_ctx = {
            let timeouts = self
                .tool_timeouts
                .read()
                .expect("ToolRegistry lock poisoned");
            if let Some(timeout_config) = timeouts.get(tool_name) {
                let mut new_ctx = ctx.clone();
                new_ctx.timeout_config = Some(timeout_config.clone());
                new_ctx
            } else {
                ctx.clone()
            }
        };

        // Execute
        let start = std::time::Instant::now();
        let mut result = tool.execute(normalized, &exec_ctx).await;
        result.duration_ms = Some(start.elapsed().as_millis() as u64);

        // Sanitize: truncate large outputs before they enter LLM context
        let sanitized = self.sanitizer.sanitize_with_mcp_fallback(
            tool_name,
            result.success,
            result.output.as_deref(),
            result.error.as_deref(),
        );
        if sanitized.was_truncated {
            result.output = sanitized.output;
            result.error = sanitized.error;
        }

        // Run after_execute middleware
        for mw in &middleware {
            if let Err(err) = mw.after_execute(tool_name, &result).await {
                warn!(tool = %tool_name, error = %err, "Middleware after_execute error");
                // after_execute errors are logged but don't change the result
            }
        }

        // Cache result for dedup
        if let Ok(mut cache) = self.dedup_cache.lock() {
            cache.insert(dedup_key, result.clone());
        }

        result
    }
}

/// Simple Levenshtein edit distance between two strings.
fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    #[allow(clippy::needless_range_loop)]
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

/// Convert a camelCase or PascalCase tool name to snake_case.
///
/// Examples: `ReadFile` -> `read_file`, `webFetch` -> `web_fetch`
fn camel_to_snake_name(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

/// Create a dedup cache key from tool name and normalized args.
///
/// Uses a deterministic JSON serialization of sorted keys + tool name.
fn make_dedup_key(tool_name: &str, args: &HashMap<String, serde_json::Value>) -> String {
    // Sort keys for deterministic hashing
    let mut sorted_args: Vec<(&String, &serde_json::Value)> = args.iter().collect();
    sorted_args.sort_by_key(|(k, _)| k.as_str());
    let args_str = serde_json::to_string(&sorted_args).unwrap_or_default();
    format!("{tool_name}:{args_str}")
}

#[cfg(test)]
mod tests {
    use super::*;
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
