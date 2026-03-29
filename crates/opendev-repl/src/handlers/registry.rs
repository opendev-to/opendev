//! Handler registry — dispatches to tool-specific handlers.
//!
//! Mirrors the Python handler registry pattern in `handlers/__init__.py`.

use std::collections::HashMap;

use serde_json::Value;

use super::traits::{HandlerResult, PreCheckResult, ToolHandler};

/// Registry of tool handlers.
///
/// Maps tool names to their handlers. Falls back to pass-through
/// behavior for tools without a registered handler.
pub struct HandlerRegistry {
    handlers: Vec<Box<dyn ToolHandler>>,
}

impl HandlerRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            handlers: Vec::new(),
        }
    }

    /// Register a handler.
    pub fn register(&mut self, handler: Box<dyn ToolHandler>) {
        self.handlers.push(handler);
    }

    /// Find the handler for a given tool name.
    fn find_handler(&self, tool_name: &str) -> Option<&dyn ToolHandler> {
        self.handlers
            .iter()
            .find(|h| h.handles().contains(&tool_name))
            .map(|h| h.as_ref())
    }

    /// Run pre-execution checks for a tool.
    pub fn pre_check(&self, tool_name: &str, args: &HashMap<String, Value>) -> PreCheckResult {
        match self.find_handler(tool_name) {
            Some(handler) => handler.pre_check(tool_name, args),
            None => PreCheckResult::Allow,
        }
    }

    /// Run post-execution processing for a tool.
    pub fn post_process(
        &self,
        tool_name: &str,
        args: &HashMap<String, Value>,
        output: Option<&str>,
        error: Option<&str>,
        success: bool,
    ) -> HandlerResult {
        match self.find_handler(tool_name) {
            Some(handler) => handler.post_process(tool_name, args, output, error, success),
            None => HandlerResult {
                output: output.map(|s| s.to_string()),
                error: error.map(|s| s.to_string()),
                success,
                meta: Default::default(),
            },
        }
    }
}

impl Default for HandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for HandlerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandlerRegistry")
            .field("handler_count", &self.handlers.len())
            .finish()
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
