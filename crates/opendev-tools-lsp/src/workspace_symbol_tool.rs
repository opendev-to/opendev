//! Workspace symbol search tool.
//!
//! Exposes the LSP `workspace/symbol` request as an OpenDev tool.
//! The [`WorkspaceSymbolTool`] implements [`BaseTool`] so it can be
//! registered in the tool registry and invoked by the agent.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use opendev_tools_core::{BaseTool, ToolContext, ToolResult};

use crate::wrapper::LspWrapper;

/// Tool that performs workspace-wide symbol searches via LSP.
///
/// Delegates to [`LspWrapper::find_symbols`] and formats the results
/// as a readable string for the agent.
#[derive(Debug)]
pub struct WorkspaceSymbolTool {
    /// Shared LSP wrapper (behind a mutex because `find_symbols` takes `&mut self`).
    lsp: Arc<Mutex<LspWrapper>>,
}

impl WorkspaceSymbolTool {
    /// Create a new workspace symbol tool with the given LSP wrapper.
    pub fn new(lsp: Arc<Mutex<LspWrapper>>) -> Self {
        Self { lsp }
    }
}

#[async_trait]
impl BaseTool for WorkspaceSymbolTool {
    fn name(&self) -> &str {
        "workspace_symbol"
    }

    fn description(&self) -> &str {
        "Search for symbols (functions, types, constants) across the workspace using the \
         language server's workspace/symbol request. Returns matching symbol names, kinds, \
         and locations."
    }

    fn parameter_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The symbol name or pattern to search for"
                },
                "file_hint": {
                    "type": "string",
                    "description": "Optional path to a file in the workspace (used to select the correct language server)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        args: HashMap<String, serde_json::Value>,
        ctx: &ToolContext,
    ) -> ToolResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q,
            None => return ToolResult::fail("Missing required parameter: query"),
        };

        let workspace_root = ctx.working_dir.clone();

        // Use a file hint if provided, otherwise pick a placeholder.
        let file_hint = args
            .get("file_hint")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace_root.join("dummy.rs"));

        let mut lsp = self.lsp.lock().await;
        match lsp.find_symbols(query, &file_hint, &workspace_root).await {
            Ok(symbols) => {
                if symbols.is_empty() {
                    return ToolResult::ok(format!("No symbols found matching '{}'", query));
                }

                let mut output = format!(
                    "Found {} symbol(s) matching '{}':\n\n",
                    symbols.len(),
                    query
                );
                for sym in &symbols {
                    output.push_str(&format!(
                        "  {} ({:?}) — {}:{}\n",
                        sym.name,
                        sym.kind,
                        sym.file_path.display(),
                        sym.range.start.line + 1,
                    ));
                }

                let mut metadata = HashMap::new();
                metadata.insert("count".to_string(), serde_json::json!(symbols.len()));
                ToolResult::ok_with_metadata(output, metadata)
            }
            Err(e) => ToolResult::fail(format!("workspace/symbol request failed: {}", e)),
        }
    }
}

#[cfg(test)]
#[path = "workspace_symbol_tool_tests.rs"]
mod tests;
