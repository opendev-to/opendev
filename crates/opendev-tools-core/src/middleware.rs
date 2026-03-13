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
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[derive(Debug)]
    struct CountingMiddleware {
        before_count: Arc<AtomicUsize>,
        after_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ToolMiddleware for CountingMiddleware {
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
    struct BlockingMiddleware;

    #[async_trait::async_trait]
    impl ToolMiddleware for BlockingMiddleware {
        async fn before_execute(
            &self,
            name: &str,
            _args: &HashMap<String, serde_json::Value>,
            _ctx: &ToolContext,
        ) -> Result<(), String> {
            Err(format!("Tool '{name}' is blocked by middleware"))
        }

        async fn after_execute(&self, _name: &str, _result: &ToolResult) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn test_counting_middleware_trait() {
        let mw = CountingMiddleware {
            before_count: Arc::new(AtomicUsize::new(0)),
            after_count: Arc::new(AtomicUsize::new(0)),
        };
        // Just verify we can construct it and it implements Debug
        assert_eq!(format!("{mw:?}").is_empty(), false);
    }

    #[tokio::test]
    async fn test_middleware_before_ok() {
        let mw = CountingMiddleware {
            before_count: Arc::new(AtomicUsize::new(0)),
            after_count: Arc::new(AtomicUsize::new(0)),
        };
        let ctx = ToolContext::new("/tmp");
        let result = mw.before_execute("test", &HashMap::new(), &ctx).await;
        assert!(result.is_ok());
        assert_eq!(mw.before_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_middleware_before_blocks() {
        let mw = BlockingMiddleware;
        let ctx = ToolContext::new("/tmp");
        let result = mw.before_execute("danger", &HashMap::new(), &ctx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("blocked"));
    }

    #[tokio::test]
    async fn test_middleware_after_execute() {
        let mw = CountingMiddleware {
            before_count: Arc::new(AtomicUsize::new(0)),
            after_count: Arc::new(AtomicUsize::new(0)),
        };
        let tool_result = ToolResult::ok("done");
        let result = mw.after_execute("test", &tool_result).await;
        assert!(result.is_ok());
        assert_eq!(mw.after_count.load(Ordering::SeqCst), 1);
    }
}
