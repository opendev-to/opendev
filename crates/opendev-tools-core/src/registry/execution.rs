//! Tool execution pipeline: resolution, validation, middleware, and dispatch.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::middleware::ToolMiddleware;
use crate::normalizer;
use crate::traits::{BaseTool, ToolContext, ToolResult};
use crate::validation;

use super::ToolRegistry;
use super::helpers::{camel_to_snake_name, edit_distance, make_dedup_key};

impl ToolRegistry {
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
        debug!(tool = %tool_name, params = ?normalized, "Normalized tool params");

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
