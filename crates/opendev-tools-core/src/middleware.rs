//! Tool execution middleware pipeline.
//!
//! Middleware hooks that run before and after tool execution, allowing
//! cross-cutting concerns like logging, rate limiting, and auditing.

use std::collections::HashMap;

use crate::traits::{ToolContext, ToolResult};

/// Middleware that can intercept tool execution.
///
/// Implementations can inspect/modify tool calls before execution and
/// observe results after execution. If `before_execute` returns an error,
/// the tool is not executed and the error is returned as a failed `ToolResult`.
#[async_trait::async_trait]
pub trait ToolMiddleware: Send + Sync + std::fmt::Debug {
    /// Called before tool execution. Return `Err` to abort execution.
    async fn before_execute(
        &self,
        name: &str,
        args: &HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> Result<(), String>;

    /// Called after tool execution with the result.
    async fn after_execute(&self, name: &str, result: &ToolResult) -> Result<(), String>;
}

#[cfg(test)]
#[path = "middleware_tests.rs"]
mod tests;
