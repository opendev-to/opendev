//! Open browser tool — open a URL in the system's default browser.

use std::collections::HashMap;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

/// Tool for opening URLs in the default browser.
#[derive(Debug)]
pub struct OpenBrowserTool;

#[async_trait::async_trait]
impl BaseTool for OpenBrowserTool {
    fn name(&self) -> &str {
        "open_browser"
    }

    fn description(&self) -> &str {
        "Open a URL in the system's default web browser."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to open in the browser"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        _ctx: &ToolContext,
    ) -> ToolResult {
        let url = match args.get("url").and_then(|v| v.as_str()) {
            Some(u) => u,
            None => return ToolResult::fail("url is required"),
        };

        // Basic validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ToolResult::fail("URL must start with http:// or https://");
        }

        match open::that(url) {
            Ok(_) => ToolResult::ok(format!("Opened {url} in default browser")),
            Err(e) => ToolResult::fail(format!("Failed to open browser: {e}")),
        }
    }
}

#[cfg(test)]
#[path = "open_browser_tests.rs"]
mod tests;
